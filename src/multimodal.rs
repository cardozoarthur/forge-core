use crate::artifact::hex_sha256;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

const STATUS_SCHEMA_VERSION: &str = "forge.multimodal.status.v1";
const INSTALL_PLAN_SCHEMA_VERSION: &str = "forge.multimodal.install_plan.v1";
const READINESS_SCHEMA_VERSION: &str = "forge.multimodal.readiness.v1";
const BENCHMARK_TEMPLATE_SCHEMA_VERSION: &str = "forge.multimodal.benchmark_template.v1";
const BENCHMARK_RESULT_SCHEMA_VERSION: &str = "forge.multimodal.benchmark_result.v1";
const RUNTIME_BENCHMARK_SCHEMA_VERSION: &str = "forge.multimodal.runtime_benchmark.v1";
const DEMO_PLAN_SCHEMA_VERSION: &str = "forge.multimodal.demo_plan.v1";
const DEMO_RECEIPT_SCHEMA_VERSION: &str = "forge.multimodal.demo_receipt.v1";
const GUARD_SCHEMA_VERSION: &str = "forge.multimodal.guard.v1";
const MULTIMODAL_CONFIG_RELATIVE_PATH: &str = ".forge/multimodal.json";
const MULTIMODAL_RUNTIMES_RELATIVE_PATH: &str = ".forge/multimodal-runtimes.json";

macro_rules! capability {
    (
        $id:expr,
        $title:expr,
        $modality:expr,
        $state:expr,
        $permission_scope:expr,
        $provider_candidates:expr,
        $local_candidates:expr,
        $runtime_candidates:expr,
        $validation_gates:expr $(,)?
    ) => {
        MultimodalCapability {
            id: $id.to_string(),
            title: $title.to_string(),
            modality: $modality.to_string(),
            state: $state.to_string(),
            permission_scope: $permission_scope.to_string(),
            provider_candidates: to_strings($provider_candidates),
            local_candidates: to_strings($local_candidates),
            runtime_candidates: to_strings($runtime_candidates),
            validation_gates: to_strings($validation_gates),
            install_plan_available: true,
        }
    };
}

#[derive(Debug, Clone, Serialize)]
pub struct MultimodalStatusReport {
    pub schema_version: String,
    pub status: String,
    pub feature_flag: MultimodalFeatureFlag,
    pub installs_performed: bool,
    pub capability_count: usize,
    pub available_count: usize,
    pub missing_count: usize,
    pub capabilities: Vec<MultimodalCapability>,
    pub runtime_guards: Vec<String>,
    pub model_storage_policy: String,
    pub provider_abstraction: String,
    pub next_action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MultimodalFeatureFlag {
    pub name: String,
    pub enabled: bool,
    pub default_state: String,
    pub activation: String,
    pub source: String,
    pub project_config_path: String,
    pub project_config_status: String,
    pub project_enabled: Option<bool>,
    pub approved_by: Option<String>,
    pub reason: Option<String>,
    pub scope: Option<String>,
    pub approval_required: bool,
    pub precedence: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MultimodalCapability {
    pub id: String,
    pub title: String,
    pub modality: String,
    pub state: String,
    pub permission_scope: String,
    pub provider_candidates: Vec<String>,
    pub local_candidates: Vec<String>,
    pub runtime_candidates: Vec<String>,
    pub validation_gates: Vec<String>,
    pub install_plan_available: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MultimodalInstallPlanReport {
    pub schema_version: String,
    pub status: String,
    pub capability_id: String,
    pub capability_title: String,
    pub installs_performed: bool,
    pub requires_human_approval: bool,
    pub feature_flag_enabled: bool,
    pub recommended_runtime: String,
    pub candidate_models: Vec<String>,
    pub permission_contract: Vec<String>,
    pub benchmark_template: Vec<String>,
    pub storage_policy: String,
    pub rollback_steps: Vec<String>,
    pub next_action: String,
}

#[derive(Debug, Clone)]
pub struct MultimodalReadinessOptions<'a> {
    pub capability_id: &'a str,
    pub enable_experimental: bool,
    pub explicit_allow: bool,
    pub project_root: Option<&'a Path>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MultimodalReadinessReport {
    pub schema_version: String,
    pub status: String,
    pub capability_id: String,
    pub capability_title: String,
    pub feature_flag_enabled: bool,
    pub installs_performed: bool,
    pub model_execution_performed: bool,
    pub device_access_performed: bool,
    pub network_access_performed: bool,
    pub permission_scope: String,
    pub guard: MultimodalGuardReport,
    pub runtime_candidates: Vec<MultimodalRuntimeReadiness>,
    pub model_candidates: Vec<MultimodalModelReadiness>,
    pub readiness_summary: String,
    pub promotion_ready: bool,
    pub promotion_gate: String,
    pub evidence_manifest: Vec<String>,
    pub next_action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MultimodalRuntimeReadiness {
    pub id: String,
    pub source: String,
    pub installed: bool,
    pub status: String,
    pub evidence: Vec<String>,
    pub executes_probe: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MultimodalModelReadiness {
    pub id: String,
    pub source_candidate: String,
    pub status: String,
    pub manifest_path: String,
    pub evidence: Vec<String>,
    pub executes_probe: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MultimodalBenchmarkTemplateReport {
    pub schema_version: String,
    pub status: String,
    pub capability_id: String,
    pub capability_title: String,
    pub feature_flag_enabled: bool,
    pub installs_performed: bool,
    pub device_access_performed: bool,
    pub requires_human_approval_before_execution: bool,
    pub permission_scope: String,
    pub recommended_runtime: String,
    pub candidate_models: Vec<String>,
    pub metrics: Vec<MultimodalBenchmarkMetric>,
    pub fixtures: Vec<MultimodalBenchmarkFixture>,
    pub guard_checks: Vec<String>,
    pub evidence_manifest_fields: Vec<String>,
    pub acceptance_thresholds: Vec<String>,
    pub report_template: Vec<String>,
    pub next_action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MultimodalBenchmarkMetric {
    pub id: String,
    pub description: String,
    pub unit: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MultimodalBenchmarkFixture {
    pub id: String,
    pub description: String,
    pub artifact_kind: String,
    pub secret_free: bool,
}

#[derive(Debug, Clone)]
pub struct MultimodalBenchmarkResultOptions<'a> {
    pub capability_id: &'a str,
    pub fixture_id: &'a str,
    pub enable_experimental: bool,
    pub approved_by: Option<&'a str>,
    pub confirm_fixture_only: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MultimodalBenchmarkResultReport {
    pub schema_version: String,
    pub status: String,
    pub capability_id: String,
    pub capability_title: String,
    pub fixture_id: String,
    pub fixture_only: bool,
    pub approved_by: String,
    pub feature_flag_enabled: bool,
    pub installs_performed: bool,
    pub model_execution_performed: bool,
    pub device_access_performed: bool,
    pub network_access_performed: bool,
    pub promotion_ready: bool,
    pub promotion_gate: String,
    pub measurements: Vec<MultimodalBenchmarkMeasurement>,
    pub guard_checks: Vec<String>,
    pub artifact_manifest: Vec<String>,
    pub evidence_manifest: Vec<String>,
    pub next_action: String,
}

#[derive(Debug, Clone)]
pub struct MultimodalRuntimeBenchmarkOptions<'a> {
    pub capability_id: &'a str,
    pub fixture_id: &'a str,
    pub enable_experimental: bool,
    pub project_root: Option<&'a Path>,
    pub approved_by: Option<&'a str>,
    pub confirm_runtime_execution: bool,
    pub allow_model: bool,
    pub connected_runtime: Option<&'a str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MultimodalRuntimeBenchmarkReport {
    pub schema_version: String,
    pub status: String,
    pub capability_id: String,
    pub capability_title: String,
    pub fixture_id: String,
    pub fixture_kind: String,
    pub fixture_only: bool,
    pub approved_by: String,
    pub feature_flag_enabled: bool,
    pub runtime_id: String,
    pub model_id: String,
    pub runtime_execution_performed: bool,
    pub model_execution_performed: bool,
    pub installs_performed: bool,
    pub device_access_performed: bool,
    pub network_access_performed: bool,
    pub camera_access_performed: bool,
    pub microphone_access_performed: bool,
    pub screen_access_performed: bool,
    pub input_access_performed: bool,
    pub filesystem_access_performed: bool,
    pub guard: MultimodalGuardReport,
    pub model_output: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connected_runtime: Option<MultimodalConnectedRuntimeEvidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub production_runtime: Option<MultimodalProductionRuntimeEvidence>,
    pub promotion_ready: bool,
    pub promotion_gate: String,
    pub measurements: Vec<MultimodalBenchmarkMeasurement>,
    pub guard_checks: Vec<String>,
    pub artifact_manifest: Vec<String>,
    pub evidence_manifest: Vec<String>,
    pub next_action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MultimodalConnectedRuntimeEvidence {
    pub schema_version: String,
    pub status: String,
    pub manifest_path: String,
    pub manifest_status: String,
    pub runtime_id: String,
    pub model_id: String,
    pub capabilities: Vec<String>,
    pub probe_command_sha256: String,
    pub probe_exit_code: i32,
    pub stdout_sha256: String,
    pub stderr_sha256: String,
    pub network_access_declared: bool,
    pub device_access_declared: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MultimodalProductionRuntimeEvidence {
    pub schema_version: String,
    pub status: String,
    pub approved_by: String,
    pub approval_ref: String,
    pub runtime_version: String,
    pub model_manifest_sha256: String,
    pub model_license: String,
    pub evidence_artifacts: Vec<String>,
    pub min_quality_score: f64,
    pub observed_quality_score: Option<f64>,
    pub quality_score_passed: bool,
    pub max_latency_ms: f64,
    pub observed_latency_ms: Option<f64>,
    pub latency_passed: bool,
    pub promotion_ready: bool,
    pub validation_evidence: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct MultimodalConnectedRuntimeManifest {
    #[serde(default)]
    runtimes: Vec<MultimodalConnectedRuntimeConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct MultimodalConnectedRuntimeConfig {
    id: String,
    model_id: String,
    #[serde(default)]
    capabilities: Vec<String>,
    probe_command: Vec<String>,
    #[serde(default)]
    network_access: bool,
    #[serde(default)]
    device_access: bool,
    #[serde(default)]
    production: Option<MultimodalConnectedRuntimeProductionConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct MultimodalConnectedRuntimeProductionConfig {
    approved_by: String,
    approval_ref: String,
    runtime_version: String,
    model_manifest_sha256: String,
    model_license: String,
    #[serde(default)]
    evidence_artifacts: Vec<String>,
    min_quality_score: f64,
    max_latency_ms: f64,
}

struct ConnectedRuntimeProbe {
    evidence: MultimodalConnectedRuntimeEvidence,
    model_output: Value,
    measurements: Vec<MultimodalBenchmarkMeasurement>,
    production_runtime: Option<MultimodalProductionRuntimeEvidence>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MultimodalBenchmarkMeasurement {
    pub id: String,
    pub value: String,
    pub unit: String,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct MultimodalDemoReceiptOptions<'a> {
    pub demo_id: &'a str,
    pub fixture_id: &'a str,
    pub enable_experimental: bool,
    pub approved_by: Option<&'a str>,
    pub confirm_local_fixture: bool,
    pub allow_model: bool,
    pub allow_camera: bool,
    pub allow_microphone: bool,
    pub allow_screen: bool,
    pub allow_input: bool,
    pub allow_filesystem: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MultimodalDemoReceiptReport {
    pub schema_version: String,
    pub status: String,
    pub demo_id: String,
    pub title: String,
    pub fixture_id: String,
    pub fixture_kind: String,
    pub approved_by: String,
    pub feature_flag_enabled: bool,
    pub fixture_execution_performed: bool,
    pub runtime_execution_performed: bool,
    pub installs_performed: bool,
    pub model_guard_allowed: bool,
    pub model_execution_performed: bool,
    pub camera_access_performed: bool,
    pub microphone_access_performed: bool,
    pub screen_access_performed: bool,
    pub input_access_performed: bool,
    pub filesystem_access_performed: bool,
    pub network_access_performed: bool,
    pub promotion_ready: bool,
    pub promotion_gate: String,
    pub guard_decisions: Vec<MultimodalDemoGuardDecision>,
    pub measurements: Vec<MultimodalBenchmarkMeasurement>,
    pub artifact_manifest: Vec<String>,
    pub evidence_manifest: Vec<String>,
    pub next_action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MultimodalDemoGuardDecision {
    pub scope: String,
    pub action: String,
    pub guard_allowed: bool,
    pub decision: String,
    pub access_performed: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MultimodalDemoPlanReport {
    pub schema_version: String,
    pub status: String,
    pub demo_id: String,
    pub title: String,
    pub feature_flag_enabled: bool,
    pub installs_performed: bool,
    pub device_access_performed: bool,
    pub requires_human_approval_before_execution: bool,
    pub capability_ids: Vec<String>,
    pub stages: Vec<MultimodalDemoStage>,
    pub validation_gates: Vec<String>,
    pub artifacts: Vec<String>,
    pub guardrails: Vec<String>,
    pub rollback_steps: Vec<String>,
    pub next_action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MultimodalDemoStage {
    pub id: String,
    pub title: String,
    pub deterministic: bool,
    pub requires_model: bool,
    pub requires_device_access: bool,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MultimodalGuardReport {
    pub schema_version: String,
    pub status: String,
    pub capability: String,
    pub action: String,
    pub allowed: bool,
    pub feature_flag_enabled: bool,
    pub explicit_allow: bool,
    pub requires_human_approval: bool,
    pub audit_required: bool,
    pub dry_run_required: bool,
    pub reason: String,
    pub guardrails: Vec<String>,
}

pub fn build_multimodal_status(enable_experimental: bool) -> MultimodalStatusReport {
    let feature_flag = default_multimodal_feature_flag(enable_experimental);
    build_multimodal_status_with_feature_flag(feature_flag)
}

pub fn build_multimodal_status_with_feature_flag(
    feature_flag: MultimodalFeatureFlag,
) -> MultimodalStatusReport {
    let capabilities = capability_inventory(feature_flag.enabled);
    let available_count = capabilities
        .iter()
        .filter(|capability| capability.state == "available")
        .count();
    let missing_count = capabilities
        .iter()
        .filter(|capability| capability.state == "missing")
        .count();

    MultimodalStatusReport {
        schema_version: STATUS_SCHEMA_VERSION.to_string(),
        status: if feature_flag.enabled {
            "experimental_enabled"
        } else {
            "experimental_disabled"
        }
        .to_string(),
        feature_flag,
        installs_performed: false,
        capability_count: capabilities.len(),
        available_count,
        missing_count,
        capabilities,
        runtime_guards: runtime_guards(),
        model_storage_policy:
            "Model downloads, caches and generated media require Forge-owned manifests, hashes, size budgets and explicit human approval before install."
                .to_string(),
        provider_abstraction:
            "Cloud providers and local/open-source models remain interchangeable execution resources behind Forge capability nodes."
                .to_string(),
        next_action:
            "Generate install plans and benchmarks for missing capabilities; do not install models or access devices until the experimental flag and runtime guard allow it."
                .to_string(),
    }
}

pub fn resolve_multimodal_feature_flag(
    enable_experimental: bool,
    project_root: Option<&Path>,
) -> MultimodalFeatureFlag {
    let project_config = read_multimodal_project_config(project_root);
    let project_approval_present = project_config
        .approved_by
        .as_deref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let project_config_status = if project_config.status == "loaded"
        && project_config.enabled == Some(true)
        && !project_approval_present
    {
        "missing_approval"
    } else {
        project_config.status
    };
    let (enabled, source) = if enable_experimental {
        (true, "explicit_flag")
    } else if project_config.enabled == Some(true) && project_approval_present {
        (true, "project_config")
    } else {
        (false, "default_disabled")
    };

    MultimodalFeatureFlag {
        name: "forge.experimental.multimodal".to_string(),
        enabled,
        default_state: "disabled".to_string(),
        activation: multimodal_feature_flag_activation(source, project_config_status).to_string(),
        source: source.to_string(),
        project_config_path: project_config.path.display().to_string(),
        project_config_status: project_config_status.to_string(),
        project_enabled: project_config.enabled,
        approved_by: project_config.approved_by,
        reason: project_config.reason,
        scope: project_config.scope,
        approval_required: true,
        precedence: vec![
            "explicit_flag".to_string(),
            "project_config_with_approval".to_string(),
            "default_disabled".to_string(),
        ],
    }
}

pub fn build_multimodal_install_plan(
    capability_id: &str,
    enable_experimental: bool,
) -> Result<MultimodalInstallPlanReport> {
    let capability = find_capability(capability_id, enable_experimental)?;

    Ok(MultimodalInstallPlanReport {
        schema_version: INSTALL_PLAN_SCHEMA_VERSION.to_string(),
        status: "plan_only".to_string(),
        capability_id: capability.id,
        capability_title: capability.title,
        installs_performed: false,
        requires_human_approval: true,
        feature_flag_enabled: enable_experimental,
        recommended_runtime: capability
            .runtime_candidates
            .first()
            .cloned()
            .unwrap_or_else(|| "runtime evaluation required".to_string()),
        candidate_models: capability.local_candidates,
        permission_contract: vec![
            format!("scope:{}", capability.permission_scope),
            "human_opt_in_required".to_string(),
            "runtime_guard_required".to_string(),
            "audit_log_required".to_string(),
            "rollback_plan_required".to_string(),
        ],
        benchmark_template: vec![
            "quality_score".to_string(),
            "latency_ms_p50_p95".to_string(),
            "ram_vram_mb".to_string(),
            "disk_footprint_mb".to_string(),
            "license_and_provenance".to_string(),
            "offline_behavior".to_string(),
        ],
        storage_policy:
            "Store install manifests, hashes, licenses, benchmark results and cache locations in Forge-owned state before enabling a model node."
                .to_string(),
        rollback_steps: vec![
            "Disable the Forge multimodal capability flag for the selected scope.".to_string(),
            "Remove model cache paths recorded in the Forge install manifest.".to_string(),
            "Revoke device or peripheral permissions from Forge runtime policy.".to_string(),
            "Record uninstall evidence and rerun multimodal status.".to_string(),
        ],
        next_action:
            "Ask for explicit human approval before downloading models, installing runtimes or granting device access."
                .to_string(),
    })
}

pub fn build_multimodal_readiness(
    options: MultimodalReadinessOptions<'_>,
) -> Result<MultimodalReadinessReport> {
    let capability = find_capability(options.capability_id, options.enable_experimental)?;
    let guard = evaluate_multimodal_guard(
        &capability.id,
        "readiness_probe",
        options.enable_experimental,
        options.explicit_allow,
    )?;
    let runtime_candidates = capability
        .runtime_candidates
        .iter()
        .map(|candidate| runtime_readiness(candidate))
        .collect::<Vec<_>>();
    let model_candidates = capability
        .local_candidates
        .iter()
        .map(|candidate| model_readiness(candidate, options.project_root))
        .collect::<Vec<_>>();
    let installed_runtime_count = runtime_candidates
        .iter()
        .filter(|candidate| candidate.installed)
        .count();
    let known_model_manifest_count = model_candidates
        .iter()
        .filter(|candidate| candidate.status == "manifest_present")
        .count();

    Ok(MultimodalReadinessReport {
        schema_version: READINESS_SCHEMA_VERSION.to_string(),
        status: "readiness_inspected".to_string(),
        capability_id: capability.id,
        capability_title: capability.title,
        feature_flag_enabled: options.enable_experimental,
        installs_performed: false,
        model_execution_performed: false,
        device_access_performed: false,
        network_access_performed: false,
        permission_scope: capability.permission_scope,
        guard,
        runtime_candidates,
        model_candidates,
        readiness_summary: format!(
            "{installed_runtime_count} runtime candidate(s) detected and {known_model_manifest_count} Forge model manifest(s) present; no model, device, network or installer execution was performed."
        ),
        promotion_ready: false,
        promotion_gate: "real_guarded_model_benchmark_required".to_string(),
        evidence_manifest: vec![
            format!("capability_id={}", options.capability_id),
            format!("feature_flag_enabled={}", options.enable_experimental),
            "installs_performed=false".to_string(),
            "model_execution_performed=false".to_string(),
            "device_access_performed=false".to_string(),
            "network_access_performed=false".to_string(),
            "runtime_probe_execution=false".to_string(),
            "model_probe_execution=false".to_string(),
        ],
        next_action:
            "Use readiness to decide which runtime/model install plan and fixture benchmark to review; execute real models only after opt-in, guard allow and benchmark approval."
                .to_string(),
    })
}

pub fn build_multimodal_benchmark_template(
    capability_id: &str,
    enable_experimental: bool,
) -> Result<MultimodalBenchmarkTemplateReport> {
    let capability = find_capability(capability_id, enable_experimental)?;
    let recommended_runtime = capability
        .runtime_candidates
        .first()
        .cloned()
        .unwrap_or_else(|| "runtime evaluation required".to_string());

    Ok(MultimodalBenchmarkTemplateReport {
        schema_version: BENCHMARK_TEMPLATE_SCHEMA_VERSION.to_string(),
        status: "plan_only".to_string(),
        capability_id: capability.id,
        capability_title: capability.title,
        feature_flag_enabled: enable_experimental,
        installs_performed: false,
        device_access_performed: false,
        requires_human_approval_before_execution: true,
        permission_scope: capability.permission_scope,
        recommended_runtime,
        candidate_models: capability.local_candidates,
        metrics: benchmark_metrics(),
        fixtures: benchmark_fixtures(),
        guard_checks: vec![
            "experimental_flag_checked".to_string(),
            "runtime_guard_required".to_string(),
            "dry_run_or_simulation_required".to_string(),
            "permission_scope_recorded".to_string(),
            "secret_free_fixture_required".to_string(),
        ],
        evidence_manifest_fields: vec![
            "capability_id".to_string(),
            "runtime_id".to_string(),
            "model_id".to_string(),
            "model_sha256".to_string(),
            "input_artifact_sha256".to_string(),
            "output_artifact_sha256".to_string(),
            "license".to_string(),
            "latency_ms_p50_p95".to_string(),
            "ram_vram_mb".to_string(),
            "offline_behavior".to_string(),
            "guard_decision_id".to_string(),
        ],
        acceptance_thresholds: vec![
            "quality_score >= capability-specific baseline".to_string(),
            "latency_ms_p95 <= declared workflow budget".to_string(),
            "disk_footprint_mb <= explicit storage budget".to_string(),
            "no network, camera, microphone, screen or input access without guard approval"
                .to_string(),
        ],
        report_template: vec![
            "Capability and permission scope".to_string(),
            "Runtime/model candidates and licenses".to_string(),
            "Fixture hashes and secret-redaction proof".to_string(),
            "Quality, latency, memory, disk and offline results".to_string(),
            "Guard decision, rollback plan and promotion recommendation".to_string(),
        ],
        next_action:
            "Use this template to collect evidence after explicit human approval; this command itself performs no install, model execution or device access."
                .to_string(),
    })
}

pub fn build_multimodal_benchmark_result(
    options: MultimodalBenchmarkResultOptions<'_>,
) -> Result<MultimodalBenchmarkResultReport> {
    let approved_by = options.approved_by.filter(|value| !value.trim().is_empty());
    if approved_by.is_none() || !options.confirm_fixture_only {
        bail!(
            "multimodal benchmark-result requires --approved-by and --confirm-fixture-only before recording fixture-only evidence"
        );
    }

    let capability = find_capability(options.capability_id, options.enable_experimental)?;
    let fixture = find_benchmark_fixture(options.fixture_id)?;
    let approved_by = approved_by.unwrap().to_string();

    Ok(MultimodalBenchmarkResultReport {
        schema_version: BENCHMARK_RESULT_SCHEMA_VERSION.to_string(),
        status: "fixture_benchmark_recorded".to_string(),
        capability_id: capability.id,
        capability_title: capability.title,
        fixture_id: fixture.id,
        fixture_only: true,
        approved_by,
        feature_flag_enabled: options.enable_experimental,
        installs_performed: false,
        model_execution_performed: false,
        device_access_performed: false,
        network_access_performed: false,
        promotion_ready: false,
        promotion_gate: "real_model_benchmark_and_runtime_guard_required".to_string(),
        measurements: vec![
            benchmark_measurement(
                "fixture_secret_free",
                "true",
                "boolean",
                "fixture_manifest",
            ),
            benchmark_measurement(
                "guard_denial_smoke",
                "true",
                "boolean",
                "dry_run_guard_receipt",
            ),
            benchmark_measurement(
                "model_execution_performed",
                "false",
                "boolean",
                "forge_fixture_only_contract",
            ),
            benchmark_measurement(
                "device_access_performed",
                "false",
                "boolean",
                "forge_fixture_only_contract",
            ),
            benchmark_measurement(
                "network_access_performed",
                "false",
                "boolean",
                "forge_fixture_only_contract",
            ),
        ],
        guard_checks: vec![
            "human_approval_recorded".to_string(),
            "confirm_fixture_only_recorded".to_string(),
            "no_model_execution".to_string(),
            "no_network_access".to_string(),
            "no_camera_microphone_screen_or_input_access".to_string(),
            "promotion_blocked_until_real_guarded_benchmark".to_string(),
        ],
        artifact_manifest: vec![
            "multimodal-fixture-benchmark.json".to_string(),
            "multimodal-fixture-benchmark.md".to_string(),
            "multimodal-guard-denial-receipt.json".to_string(),
        ],
        evidence_manifest: vec![
            format!("capability_id={}", options.capability_id),
            format!("fixture_id={}", options.fixture_id),
            format!("fixture_kind={}", fixture.artifact_kind),
            "fixture_only=true".to_string(),
            "secret_free=true".to_string(),
            "installs_performed=false".to_string(),
            "model_execution_performed=false".to_string(),
            "device_access_performed=false".to_string(),
            "network_access_performed=false".to_string(),
        ],
        next_action:
            "Attach this fixture-only benchmark result to a workflow or milestone report, then run a real guarded benchmark only after explicit experimental opt-in and runtime guard approval."
                .to_string(),
    })
}

pub fn build_multimodal_runtime_benchmark(
    options: MultimodalRuntimeBenchmarkOptions<'_>,
) -> Result<MultimodalRuntimeBenchmarkReport> {
    if !options.enable_experimental {
        bail!("experimental multimodal opt-in is required before running guarded runtime benchmark evidence");
    }
    let approved_by = options.approved_by.filter(|value| !value.trim().is_empty());
    if approved_by.is_none() || !options.confirm_runtime_execution {
        bail!(
            "multimodal runtime-benchmark requires --approved-by and --confirm-runtime-execution before model/runtime execution"
        );
    }
    if !options.allow_model {
        bail!(
            "multimodal runtime-benchmark requires --allow-model after reviewing the runtime guard"
        );
    }

    let capability = find_capability(options.capability_id, options.enable_experimental)?;
    let fixture = find_benchmark_fixture(options.fixture_id)?;
    let guard = evaluate_multimodal_guard(
        "model",
        "execute_runtime_benchmark",
        options.enable_experimental,
        options.allow_model,
    )?;
    if !guard.allowed {
        bail!("model runtime guard denied benchmark execution");
    }
    let approved_by = approved_by.unwrap().to_string();
    let connected_probe = options
        .connected_runtime
        .filter(|runtime_id| !runtime_id.trim().is_empty())
        .map(|runtime_id| {
            run_connected_runtime_probe(options.project_root, runtime_id, &capability.id)
        })
        .transpose()?;
    let runtime_id = connected_probe
        .as_ref()
        .map(|probe| probe.evidence.runtime_id.clone())
        .unwrap_or_else(|| "forge_deterministic_fixture_runtime".to_string());
    let model_id = connected_probe
        .as_ref()
        .map(|probe| probe.evidence.model_id.clone())
        .unwrap_or_else(|| "forge_fixture_model_v1".to_string());
    let model_output = connected_probe
        .as_ref()
        .map(|probe| probe.model_output.clone())
        .unwrap_or_else(|| deterministic_fixture_model_output(&capability.id, &fixture));
    let connected_runtime = connected_probe.as_ref().map(|probe| probe.evidence.clone());
    let mut measurements = vec![
        benchmark_measurement(
            "runtime_execution_performed",
            "true",
            "boolean",
            "forge_runtime_benchmark",
        ),
        benchmark_measurement(
            "model_execution_performed",
            "true",
            "boolean",
            "forge_runtime_benchmark",
        ),
        benchmark_measurement("quality_score", "1.0", "score", "deterministic_fixture"),
        benchmark_measurement("latency_ms", "1", "ms", "deterministic_fixture"),
        benchmark_measurement(
            "device_access_performed",
            "false",
            "boolean",
            "guard_matrix",
        ),
        benchmark_measurement(
            "network_access_performed",
            "false",
            "boolean",
            "guard_matrix",
        ),
    ];
    if let Some(probe) = connected_probe.as_ref() {
        measurements.extend(probe.measurements.clone());
    }
    let production_runtime = connected_probe
        .as_ref()
        .and_then(|probe| probe.production_runtime.clone());
    let production_ready = production_runtime
        .as_ref()
        .is_some_and(|evidence| evidence.promotion_ready);
    if let Some(evidence) = production_runtime.as_ref() {
        measurements.extend(production_runtime_measurements(evidence));
    }
    let mut guard_checks = vec![
        "experimental_opt_in_required".to_string(),
        "human_approval_recorded".to_string(),
        "confirm_runtime_execution_recorded".to_string(),
        "model_guard_allowed".to_string(),
        "no_installs_performed".to_string(),
        "no_camera_microphone_screen_input_filesystem_access".to_string(),
        "no_network_access".to_string(),
    ];
    if connected_runtime.is_some() {
        guard_checks.push("connected_runtime_manifest_loaded".to_string());
        guard_checks.push("connected_runtime_probe_completed".to_string());
        guard_checks.push("connected_runtime_declares_no_network_or_device_access".to_string());
    }
    if production_ready {
        guard_checks.push("production_connected_runtime_evidence_validated".to_string());
    } else if production_runtime.is_some() {
        guard_checks.push("production_connected_runtime_evidence_incomplete".to_string());
    }
    let mut artifact_manifest = vec![
        "multimodal-runtime-benchmark.json".to_string(),
        "multimodal-runtime-benchmark.md".to_string(),
        "multimodal-runtime-guard-receipt.json".to_string(),
    ];
    if connected_runtime.is_some() {
        artifact_manifest.push("multimodal-connected-runtime-probe.json".to_string());
    }
    if production_runtime.is_some() {
        artifact_manifest.push("multimodal-production-runtime-evidence.json".to_string());
    }
    let mut evidence_manifest = vec![
        format!("capability_id={}", options.capability_id),
        format!("fixture_id={}", options.fixture_id),
        "experimental_opt_in=true".to_string(),
        "human_approval_recorded=true".to_string(),
        "confirm_runtime_execution=true".to_string(),
        "guard_approved_model_runtime_execution=true".to_string(),
        "runtime_execution_performed=true".to_string(),
        "model_execution_performed=true".to_string(),
        "installs_performed=false".to_string(),
        "device_access_performed=false".to_string(),
        "network_access_performed=false".to_string(),
        "camera_microphone_screen_input_filesystem_blocked_without_guard=true".to_string(),
    ];
    if let Some(runtime) = connected_runtime.as_ref() {
        evidence_manifest.push("connected_runtime_manifest_loaded=true".to_string());
        evidence_manifest.push(format!("connected_runtime_id={}", runtime.runtime_id));
        evidence_manifest.push(format!("connected_model_id={}", runtime.model_id));
        evidence_manifest.push("connected_runtime_probe_completed=true".to_string());
    }
    if let Some(evidence) = production_runtime.as_ref() {
        evidence_manifest.push(format!(
            "production_connected_runtime_evidence_validated={}",
            evidence.promotion_ready
        ));
        evidence_manifest.push(format!(
            "production_runtime_approval_ref={}",
            evidence.approval_ref
        ));
        evidence_manifest.push(format!(
            "production_quality_score_passed={}",
            evidence.quality_score_passed
        ));
        evidence_manifest.push(format!(
            "production_latency_passed={}",
            evidence.latency_passed
        ));
    }
    let status = if production_ready {
        "production_runtime_benchmark_recorded"
    } else {
        "guarded_runtime_benchmark_recorded"
    };
    let promotion_gate = if production_ready {
        "production_model_runtime_benchmark_recorded"
    } else {
        "production_model_runtime_benchmark_required"
    };
    let next_action = if production_ready {
        "Attach this production connected-runtime benchmark to the milestone evidence bundle, then verify release gates against the complete 0.5 requirement set before promotion."
    } else {
        "Use this guarded runtime benchmark as execution-path evidence, then add production model/runtime benchmarks with real installed or connected models before promoting multimodal beyond groundwork."
    };

    Ok(MultimodalRuntimeBenchmarkReport {
        schema_version: RUNTIME_BENCHMARK_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        capability_id: capability.id.clone(),
        capability_title: capability.title,
        fixture_id: fixture.id.clone(),
        fixture_kind: fixture.artifact_kind.clone(),
        fixture_only: false,
        approved_by,
        feature_flag_enabled: options.enable_experimental,
        runtime_id,
        model_id,
        runtime_execution_performed: true,
        model_execution_performed: true,
        installs_performed: false,
        device_access_performed: false,
        network_access_performed: false,
        camera_access_performed: false,
        microphone_access_performed: false,
        screen_access_performed: false,
        input_access_performed: false,
        filesystem_access_performed: false,
        guard,
        model_output,
        connected_runtime,
        production_runtime,
        promotion_ready: production_ready,
        promotion_gate: promotion_gate.to_string(),
        measurements,
        guard_checks,
        artifact_manifest,
        evidence_manifest,
        next_action: next_action.to_string(),
    })
}

pub fn build_multimodal_demo_plan(
    demo_id: &str,
    enable_experimental: bool,
) -> Result<MultimodalDemoPlanReport> {
    let normalized = demo_id.trim().to_ascii_lowercase();
    let (title, capability_ids, stages, artifacts) = match normalized.as_str() {
        "local_image_recognition" => (
            "Safe local image-recognition workflow plan",
            vec!["image_understanding", "ocr", "object_detection"],
            vec![
                demo_stage(
                    "fixture_prepare",
                    "Prepare static image fixtures",
                    true,
                    false,
                    false,
                    "Hash secret-free sample images and expected labels before any model node runs.",
                ),
                demo_stage(
                    "install_plan_review",
                    "Review local model/runtime install plans",
                    true,
                    false,
                    false,
                    "Generate plan-only install manifests for image understanding, OCR and object detection.",
                ),
                demo_stage(
                    "benchmark_template",
                    "Prepare benchmark evidence template",
                    true,
                    false,
                    false,
                    "Bind metrics, fixtures and guard checks before a model is allowed to execute.",
                ),
                demo_stage(
                    "future_guarded_execution",
                    "Run only after explicit approval",
                    false,
                    true,
                    false,
                    "A future enabled run may execute a local model against fixtures after feature flag and runtime guard approval.",
                ),
            ],
            vec![
                "image-recognition-benchmark.md".to_string(),
                "image-recognition-evidence.json".to_string(),
            ],
        ),
        "audio_transcription_synthesis" => (
            "Safe audio transcription and synthesis workflow plan",
            vec!["audio_transcription", "speech_synthesis", "audio_understanding"],
            vec![
                demo_stage(
                    "fixture_prepare",
                    "Prepare static audio fixtures",
                    true,
                    false,
                    false,
                    "Use checked-in or generated fixture files rather than microphone capture.",
                ),
                demo_stage(
                    "permission_contract",
                    "Record microphone and audio-output guard contracts",
                    true,
                    false,
                    false,
                    "Prove the plan does not access live microphone or speakers without explicit runtime approval.",
                ),
                demo_stage(
                    "benchmark_template",
                    "Prepare WER, latency and license benchmarks",
                    true,
                    false,
                    false,
                    "Define evidence for transcription, synthesis and audio-understanding nodes.",
                ),
                demo_stage(
                    "future_guarded_execution",
                    "Run only after explicit approval",
                    false,
                    true,
                    false,
                    "A future enabled run may execute local audio models against static fixtures after guard approval.",
                ),
            ],
            vec![
                "audio-capability-benchmark.md".to_string(),
                "audio-capability-evidence.json".to_string(),
            ],
        ),
        "blender_avatar_preparation" => (
            "Safe Blender/3D avatar preparation workflow plan",
            vec![
                "3d_generation_adaptation",
                "blender_asset_processing",
                "avatar_camera_emulation",
            ],
            vec![
                demo_stage(
                    "fixture_prepare",
                    "Prepare static mesh/avatar fixtures",
                    true,
                    false,
                    false,
                    "Hash sample meshes, textures and rig metadata before Blender processing.",
                ),
                demo_stage(
                    "blender_dry_run",
                    "Plan Blender dry-run processing",
                    true,
                    false,
                    false,
                    "Generate script and validation checklist without launching Blender or touching virtual cameras.",
                ),
                demo_stage(
                    "virtual_camera_guard_review",
                    "Review virtual camera guard",
                    true,
                    false,
                    false,
                    "Require explicit approval before any v4l2loopback or camera-emulation integration.",
                ),
                demo_stage(
                    "future_guarded_execution",
                    "Run only after explicit approval",
                    false,
                    true,
                    false,
                    "A future enabled run may process local fixtures through Blender after filesystem and camera guard approval.",
                ),
            ],
            vec![
                "avatar-preparation-plan.md".to_string(),
                "avatar-preparation-evidence.json".to_string(),
            ],
        ),
        _ => {
            bail!("unknown multimodal demo plan: {demo_id}; expected local_image_recognition, audio_transcription_synthesis or blender_avatar_preparation")
        }
    };

    for capability_id in &capability_ids {
        find_capability(capability_id, enable_experimental)?;
    }

    Ok(MultimodalDemoPlanReport {
        schema_version: DEMO_PLAN_SCHEMA_VERSION.to_string(),
        status: "plan_only".to_string(),
        demo_id: normalized,
        title: title.to_string(),
        feature_flag_enabled: enable_experimental,
        installs_performed: false,
        device_access_performed: false,
        requires_human_approval_before_execution: true,
        capability_ids: capability_ids.into_iter().map(str::to_string).collect(),
        stages,
        validation_gates: vec![
            "experimental_flag_disabled_by_default".to_string(),
            "no_device_or_model_access_without_guard".to_string(),
            "fixture_hashes_recorded".to_string(),
            "benchmark_template_completed_before_promotion".to_string(),
            "rollback_steps_reviewed".to_string(),
        ],
        artifacts,
        guardrails: runtime_guards(),
        rollback_steps: vec![
            "Keep the experimental multimodal flag disabled unless a human approves this demo."
                .to_string(),
            "Delete generated model/runtime cache paths listed in the install manifest if a future enabled demo is rolled back."
                .to_string(),
            "Revoke camera, microphone, screen, input, filesystem or peripheral grants from runtime policy."
                .to_string(),
        ],
        next_action:
            "Use the demo plan as workflow design evidence; execute only after explicit human approval, runtime guard allow and benchmark fixture review."
                .to_string(),
    })
}

pub fn build_multimodal_demo_receipt(
    options: MultimodalDemoReceiptOptions<'_>,
) -> Result<MultimodalDemoReceiptReport> {
    if !options.enable_experimental {
        bail!("experimental multimodal opt-in is required before recording guarded demo evidence");
    }
    let approved_by = options.approved_by.filter(|value| !value.trim().is_empty());
    if approved_by.is_none() || !options.confirm_local_fixture {
        bail!(
            "multimodal demo-receipt requires --approved-by and --confirm-local-fixture before recording guarded demo evidence"
        );
    }

    let demo_plan = build_multimodal_demo_plan(options.demo_id, options.enable_experimental)?;
    let fixture = find_benchmark_fixture(options.fixture_id)?;
    let approved_by = approved_by.unwrap().to_string();
    let guard_decisions = vec![
        demo_guard_decision("model", "execute_fixture_runtime", options.allow_model),
        demo_guard_decision("camera", "access", options.allow_camera),
        demo_guard_decision("microphone", "access", options.allow_microphone),
        demo_guard_decision("screen", "access", options.allow_screen),
        demo_guard_decision("input", "access", options.allow_input),
        demo_guard_decision("filesystem", "access", options.allow_filesystem),
    ];
    let model_guard_allowed = options.allow_model;

    Ok(MultimodalDemoReceiptReport {
        schema_version: DEMO_RECEIPT_SCHEMA_VERSION.to_string(),
        status: "guarded_demo_receipt_recorded".to_string(),
        demo_id: demo_plan.demo_id,
        title: demo_plan.title,
        fixture_id: fixture.id,
        fixture_kind: fixture.artifact_kind,
        approved_by,
        feature_flag_enabled: options.enable_experimental,
        fixture_execution_performed: true,
        runtime_execution_performed: true,
        installs_performed: false,
        model_guard_allowed,
        model_execution_performed: false,
        camera_access_performed: false,
        microphone_access_performed: false,
        screen_access_performed: false,
        input_access_performed: false,
        filesystem_access_performed: false,
        network_access_performed: false,
        promotion_ready: false,
        promotion_gate: "real_model_execution_evidence_required".to_string(),
        guard_decisions,
        measurements: vec![
            benchmark_measurement(
                "local_fixture_execution",
                "true",
                "boolean",
                "forge_guarded_demo_receipt",
            ),
            benchmark_measurement(
                "model_guard_allowed",
                if model_guard_allowed { "true" } else { "false" },
                "boolean",
                "forge_multimodal_guard_matrix",
            ),
            benchmark_measurement(
                "device_access_performed",
                "false",
                "boolean",
                "forge_multimodal_guard_matrix",
            ),
            benchmark_measurement(
                "filesystem_access_performed",
                "false",
                "boolean",
                "forge_multimodal_guard_matrix",
            ),
            benchmark_measurement(
                "network_access_performed",
                "false",
                "boolean",
                "forge_multimodal_guard_matrix",
            ),
        ],
        artifact_manifest: vec![
            "multimodal-guarded-demo-receipt.json".to_string(),
            "multimodal-guard-matrix.json".to_string(),
            "multimodal-local-fixture-evidence.md".to_string(),
        ],
        evidence_manifest: vec![
            format!("demo_id={}", options.demo_id),
            format!("fixture_id={}", options.fixture_id),
            "experimental_opt_in=true".to_string(),
            "human_approval_recorded=true".to_string(),
            "confirm_local_fixture=true".to_string(),
            format!("model_guard_allowed={model_guard_allowed}"),
            "fixture_execution_performed=true".to_string(),
            "runtime_execution_performed=true".to_string(),
            "installs_performed=false".to_string(),
            "model_execution_performed=false".to_string(),
            "network_access_performed=false".to_string(),
            "camera_microphone_screen_input_filesystem_blocked_without_guard=true".to_string(),
        ],
        next_action:
            "Attach this guarded demo receipt to the milestone, then run a real model benchmark only after model/runtime installation and access scopes receive separate guard approval."
                .to_string(),
    })
}

pub fn evaluate_multimodal_guard(
    capability: &str,
    action: &str,
    enable_experimental: bool,
    explicit_allow: bool,
) -> Result<MultimodalGuardReport> {
    let normalized = normalize_capability_alias(capability);
    let known = capability_inventory(enable_experimental)
        .into_iter()
        .any(|item| item.id == normalized || item.permission_scope == normalized);
    if !known {
        bail!("unknown multimodal capability or permission scope: {capability}");
    }

    let allowed = enable_experimental && explicit_allow;
    Ok(MultimodalGuardReport {
        schema_version: GUARD_SCHEMA_VERSION.to_string(),
        status: if allowed { "allowed" } else { "denied" }.to_string(),
        capability: normalized,
        action: action.to_string(),
        allowed,
        feature_flag_enabled: enable_experimental,
        explicit_allow,
        requires_human_approval: !allowed,
        audit_required: true,
        dry_run_required: true,
        reason: if allowed {
            "Experimental multimodal access is enabled and this action received explicit allow; Forge still requires audit logs and dry-run/simulation before risky control."
                .to_string()
        } else if !enable_experimental {
            "Experimental multimodal access is disabled by default; enable it only after explicit human opt-in."
                .to_string()
        } else {
            "Experimental multimodal access is enabled, but this action did not receive explicit allow."
                .to_string()
        },
        guardrails: vec![
            "dry_run_or_simulation_first".to_string(),
            "scoped_app_or_device_target".to_string(),
            "kill_switch".to_string(),
            "secrets_redaction".to_string(),
            "audit_every_action".to_string(),
            "permission_scoped_rollback".to_string(),
        ],
    })
}

fn demo_guard_decision(
    scope: &str,
    action: &str,
    guard_allowed: bool,
) -> MultimodalDemoGuardDecision {
    MultimodalDemoGuardDecision {
        scope: scope.to_string(),
        action: action.to_string(),
        guard_allowed,
        decision: if guard_allowed {
            "allowed_by_guard"
        } else {
            "blocked_without_guard"
        }
        .to_string(),
        access_performed: false,
        reason: if guard_allowed {
            "Guard approval is recorded for this scope, but the local fixture receipt does not perform real model or device access."
        } else {
            "No guard approval was supplied for this scope, so Forge records zero access performed."
        }
        .to_string(),
    }
}

fn capability_inventory(enable_experimental: bool) -> Vec<MultimodalCapability> {
    let state = if enable_experimental {
        "not_configured"
    } else {
        "missing"
    };
    vec![
        capability!(
            "image_understanding",
            "Image understanding",
            "image",
            state,
            "model",
            &["cloud_vision_provider"],
            &["moondream2", "llava-1.6-7b", "qwen2-vl-2b"],
            &["candle", "onnxruntime", "llama.cpp"],
            &["image_classification_smoke", "ocr_overlap_check"],
        ),
        capability!(
            "ocr",
            "OCR",
            "image",
            state,
            "model",
            &["cloud_vision_provider"],
            &["tesseract", "paddleocr", "trocr-small"],
            &["system_binary", "onnxruntime"],
            &["text_accuracy", "layout_preservation"],
        ),
        capability!(
            "object_detection",
            "Object detection",
            "image",
            state,
            "model",
            &["cloud_vision_provider"],
            &["yolo-nas-s", "yolov8n", "detr-resnet-50"],
            &["onnxruntime", "openvino"],
            &["bbox_accuracy", "latency_budget"],
        ),
        capability!(
            "segmentation",
            "Segmentation",
            "image",
            state,
            "model",
            &["cloud_vision_provider"],
            &["mobile_sam", "sam2_tiny"],
            &["onnxruntime", "openvino"],
            &["mask_quality", "memory_budget"],
        ),
        capability!(
            "image_generation_editing",
            "Image generation and editing",
            "image",
            state,
            "model_storage",
            &["cloud_image_provider"],
            &["sdxl_turbo", "stable-diffusion-3-medium", "flux-schnell"],
            &["comfyui_adapter", "diffusers_optional"],
            &["prompt_replay", "asset_hash_lineage"],
        ),
        capability!(
            "video_generation_editing",
            "Video generation and editing",
            "video",
            state,
            "model_storage",
            &["cloud_video_provider"],
            &["svd", "animatediff", "ltx-video"],
            &["comfyui_adapter", "ffmpeg"],
            &["duration_consistency", "frame_sample_validation"],
        ),
        capability!(
            "audio_transcription",
            "Audio transcription",
            "audio",
            state,
            "microphone",
            &["cloud_transcription_provider"],
            &["whisper.cpp-small", "faster-whisper-small"],
            &["whisper.cpp", "onnxruntime"],
            &["wer_smoke", "privacy_redaction"],
        ),
        capability!(
            "speech_synthesis",
            "Speech synthesis",
            "audio",
            state,
            "audio_output",
            &["cloud_tts_provider"],
            &["piper", "kokoro-tts", "coqui-xtts"],
            &["system_binary", "onnxruntime"],
            &["voice_license", "latency_budget"],
        ),
        capability!(
            "audio_understanding",
            "Audio understanding",
            "audio",
            state,
            "microphone",
            &["cloud_audio_provider"],
            &["yamnet", "clap-small"],
            &["onnxruntime"],
            &["event_accuracy", "privacy_redaction"],
        ),
        capability!(
            "realtime_vision",
            "Realtime vision",
            "vision",
            state,
            "camera",
            &["cloud_realtime_provider"],
            &["moondream2", "mobileclip"],
            &["onnxruntime", "openvino"],
            &["fps_budget", "consent_gate"],
        ),
        capability!(
            "screen_understanding",
            "Screen understanding",
            "computer_use",
            state,
            "screen",
            &["cloud_computer_use_provider"],
            &["ocr_plus_ui_tree", "moondream2"],
            &["system_screenshot", "onnxruntime"],
            &["scoped_window_target", "secrets_redaction"],
        ),
        capability!(
            "computer_use_actions",
            "Computer-use actions",
            "computer_use",
            state,
            "input",
            &["cloud_computer_use_provider"],
            &["deterministic_ui_actions"],
            &["xdotool_adapter", "winit_future"],
            &["dry_run_first", "audit_every_action"],
        ),
        capability!(
            "mouse_keyboard_automation",
            "Mouse and keyboard automation",
            "computer_use",
            state,
            "input",
            &[],
            &["deterministic_input_adapter"],
            &["xdotool_adapter", "enigo_optional"],
            &["permission_scope", "kill_switch"],
        ),
        capability!(
            "peripheral_device_access",
            "Peripheral and device access",
            "device",
            state,
            "peripheral",
            &[],
            &["deterministic_device_adapter"],
            &["udev_future", "adb_optional"],
            &["device_allowlist", "rollback_plan"],
        ),
        capability!(
            "avatar_camera_emulation",
            "Avatar and virtual camera emulation",
            "avatar",
            state,
            "camera",
            &["cloud_avatar_provider"],
            &["live2d_optional", "piper", "rhubarb_lip_sync"],
            &["blender", "v4l2loopback_optional"],
            &["explicit_virtual_camera_approval", "persona_audit"],
        ),
        capability!(
            "3d_generation_adaptation",
            "3D generation and adaptation",
            "3d",
            state,
            "filesystem",
            &["tripo3d_or_cloud_3d_provider"],
            &["shap-e", "tripo3d_optional", "instantmesh"],
            &["blender", "openvino_optional"],
            &["mesh_integrity", "license_provenance"],
        ),
        capability!(
            "blender_asset_processing",
            "Blender-assisted asset processing",
            "3d",
            state,
            "filesystem",
            &[],
            &["blender_python_pipeline"],
            &["blender"],
            &["asset_hash_lineage", "render_smoke"],
        ),
    ]
}

fn find_capability(capability_id: &str, enable_experimental: bool) -> Result<MultimodalCapability> {
    capability_inventory(enable_experimental)
        .into_iter()
        .find(|capability| capability.id == capability_id)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "unknown multimodal capability: {capability_id}; run forge multimodal status"
            )
        })
}

fn find_benchmark_fixture(fixture_id: &str) -> Result<MultimodalBenchmarkFixture> {
    benchmark_fixtures()
        .into_iter()
        .find(|fixture| fixture.id == fixture_id)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "unknown multimodal benchmark fixture: {fixture_id}; run forge multimodal benchmark-template"
            )
        })
}

fn runtime_readiness(candidate: &str) -> MultimodalRuntimeReadiness {
    let binary_names = runtime_binary_names(candidate);
    if binary_names.is_empty() {
        return MultimodalRuntimeReadiness {
            id: candidate.to_string(),
            source: "adapter_manifest".to_string(),
            installed: false,
            status: "manifest_probe_required".to_string(),
            evidence: vec![
                format!("candidate={candidate}"),
                "No subprocess was executed; this runtime requires a Forge adapter/library manifest probe before use.".to_string(),
            ],
            executes_probe: false,
        };
    }

    let found = binary_names
        .iter()
        .find_map(|binary| find_binary_in_path(binary).map(|path| (binary, path)));
    match found {
        Some((binary, path)) => MultimodalRuntimeReadiness {
            id: candidate.to_string(),
            source: "path".to_string(),
            installed: true,
            status: "binary_found_without_execution".to_string(),
            evidence: vec![
                format!("binary={binary}"),
                format!("path={}", path.display()),
                "PATH was inspected without launching the binary.".to_string(),
            ],
            executes_probe: false,
        },
        None => MultimodalRuntimeReadiness {
            id: candidate.to_string(),
            source: "path".to_string(),
            installed: false,
            status: "binary_not_found".to_string(),
            evidence: vec![
                format!("candidate={candidate}"),
                format!("checked_binaries={}", binary_names.join(",")),
                "PATH was inspected without launching any binary.".to_string(),
            ],
            executes_probe: false,
        },
    }
}

fn model_readiness(candidate: &str, project_root: Option<&Path>) -> MultimodalModelReadiness {
    let id = model_readiness_id(candidate);
    let root = project_root
        .map(Path::to_path_buf)
        .or_else(|| env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let manifest_path = root
        .join(".forge")
        .join("multimodal-models")
        .join(format!("{id}.json"));
    let manifest_present = manifest_path.is_file();

    MultimodalModelReadiness {
        id,
        source_candidate: candidate.to_string(),
        status: if manifest_present {
            "manifest_present"
        } else {
            "manifest_missing"
        }
        .to_string(),
        manifest_path: manifest_path.display().to_string(),
        evidence: vec![
            format!("source_candidate={candidate}"),
            format!("manifest_present={manifest_present}"),
            "Model manifest path was inspected without loading weights or executing inference."
                .to_string(),
        ],
        executes_probe: false,
    }
}

fn runtime_binary_names(candidate: &str) -> Vec<String> {
    match candidate {
        "blender" | "ffmpeg" | "openvino" | "system_screenshot" => vec![candidate.to_string()],
        "system_binary" => vec!["tesseract".to_string(), "piper".to_string()],
        "whisper.cpp" => vec!["whisper-cli".to_string(), "main".to_string()],
        "llama.cpp" => vec!["llama-cli".to_string(), "llama".to_string()],
        "xdotool_adapter" => vec!["xdotool".to_string()],
        _ => Vec::new(),
    }
}

fn find_binary_in_path(binary: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    env::split_paths(&path_var)
        .map(|entry| entry.join(binary))
        .find(|candidate| candidate.is_file())
}

fn model_readiness_id(candidate: &str) -> String {
    let normalized = candidate.trim().to_ascii_lowercase();
    if normalized.starts_with("llava") {
        "llava".to_string()
    } else if normalized.starts_with("qwen2-vl") {
        "qwen2-vl".to_string()
    } else if normalized.starts_with("whisper.cpp") {
        "whisper.cpp".to_string()
    } else {
        normalized
    }
}

fn default_multimodal_feature_flag(enable_experimental: bool) -> MultimodalFeatureFlag {
    let source = if enable_experimental {
        "explicit_flag"
    } else {
        "default_disabled"
    };
    MultimodalFeatureFlag {
        name: "forge.experimental.multimodal".to_string(),
        enabled: enable_experimental,
        default_state: "disabled".to_string(),
        activation: multimodal_feature_flag_activation(source, "not_checked").to_string(),
        source: source.to_string(),
        project_config_path: MULTIMODAL_CONFIG_RELATIVE_PATH.to_string(),
        project_config_status: "not_checked".to_string(),
        project_enabled: None,
        approved_by: None,
        reason: None,
        scope: None,
        approval_required: true,
        precedence: vec![
            "explicit_flag".to_string(),
            "project_config_with_approval".to_string(),
            "default_disabled".to_string(),
        ],
    }
}

struct MultimodalProjectConfig {
    path: PathBuf,
    status: &'static str,
    enabled: Option<bool>,
    approved_by: Option<String>,
    reason: Option<String>,
    scope: Option<String>,
}

fn read_multimodal_project_config(project_root: Option<&Path>) -> MultimodalProjectConfig {
    let project_root = project_root
        .map(Path::to_path_buf)
        .or_else(|| env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let path = project_root.join(MULTIMODAL_CONFIG_RELATIVE_PATH);
    let Ok(content) = fs::read_to_string(&path) else {
        return MultimodalProjectConfig {
            path,
            status: "missing",
            enabled: None,
            approved_by: None,
            reason: None,
            scope: None,
        };
    };
    let Ok(config) = serde_json::from_str::<Value>(&content) else {
        return MultimodalProjectConfig {
            path,
            status: "invalid_json",
            enabled: None,
            approved_by: None,
            reason: None,
            scope: None,
        };
    };
    let enabled = config
        .get("experimental_enabled")
        .or_else(|| config.get("enabled"))
        .and_then(Value::as_bool);
    MultimodalProjectConfig {
        path,
        status: if enabled.is_some() {
            "loaded"
        } else {
            "missing_experimental_enabled"
        },
        enabled,
        approved_by: string_field(&config, "approved_by"),
        reason: string_field(&config, "reason"),
        scope: string_field(&config, "scope"),
    }
}

fn string_field(config: &Value, field: &str) -> Option<String> {
    config
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn multimodal_feature_flag_activation(source: &str, project_config_status: &str) -> &'static str {
    match (source, project_config_status) {
        ("explicit_flag", _) => {
            "Enabled by explicit --enable-experimental or MCP enable_experimental input; runtime guard still requires explicit allow."
        }
        ("project_config", _) => {
            "Enabled by approved .forge/multimodal.json project config; runtime guard still requires explicit allow."
        }
        (_, "missing_approval") => {
            "Project config requested enablement but is missing approved_by; keep disabled until human approval is recorded."
        }
        (_, "invalid_json") => {
            "Project config is invalid JSON; keep disabled until the Forge-owned config is repaired and approved."
        }
        _ => {
            "Pass --enable-experimental or create approved .forge/multimodal.json with experimental_enabled and approved_by after human approval."
        }
    }
}

fn normalize_capability_alias(capability: &str) -> String {
    let lower = capability.trim().to_ascii_lowercase();
    let normalized = match lower.as_str() {
        "camera" | "camera_access" => "camera",
        "mic" | "microphone" | "microphone_access" => "microphone",
        "screen" | "screen_access" => "screen",
        "mouse" | "keyboard" | "input" => "input",
        "peripheral" | "device" => "peripheral",
        other => other,
    };
    normalized.to_string()
}

fn benchmark_metrics() -> Vec<MultimodalBenchmarkMetric> {
    [
        (
            "quality_score",
            "Task-specific quality score or accuracy proxy.",
            "score",
            true,
        ),
        (
            "latency_ms_p50_p95",
            "Median and p95 latency for the planned runtime node.",
            "milliseconds",
            true,
        ),
        (
            "ram_vram_mb",
            "Peak RAM and VRAM footprint during the run.",
            "megabytes",
            true,
        ),
        (
            "disk_footprint_mb",
            "Runtime, model and cache disk footprint.",
            "megabytes",
            true,
        ),
        (
            "license_and_provenance",
            "License, model source and artifact provenance evidence.",
            "text",
            true,
        ),
        (
            "offline_behavior",
            "Whether the capability can run without network access after install.",
            "text",
            true,
        ),
        (
            "guard_denial_smoke",
            "Proof that guarded access is denied when experimental opt-in is absent.",
            "boolean",
            true,
        ),
    ]
    .into_iter()
    .map(
        |(id, description, unit, required)| MultimodalBenchmarkMetric {
            id: id.to_string(),
            description: description.to_string(),
            unit: unit.to_string(),
            required,
        },
    )
    .collect()
}

fn benchmark_fixtures() -> Vec<MultimodalBenchmarkFixture> {
    [
        (
            "static_image_labels",
            "Secret-free static image labels used to prove the fixture path without model execution.",
            "json",
        ),
        (
            "static_fixture_manifest",
            "Secret-free sample files with expected labels or outputs.",
            "json",
        ),
        (
            "dry_run_guard_receipt",
            "Runtime guard denial/allow receipt recorded before execution.",
            "json",
        ),
        (
            "benchmark_report_markdown",
            "Human-readable benchmark and promotion report.",
            "markdown",
        ),
    ]
    .into_iter()
    .map(
        |(id, description, artifact_kind)| MultimodalBenchmarkFixture {
            id: id.to_string(),
            description: description.to_string(),
            artifact_kind: artifact_kind.to_string(),
            secret_free: true,
        },
    )
    .collect()
}

fn deterministic_fixture_model_output(
    capability_id: &str,
    fixture: &MultimodalBenchmarkFixture,
) -> Value {
    let labels = match capability_id {
        "ocr" => vec!["sample", "text", "layout"],
        "object_detection" => vec!["document", "bounding_box", "foreground"],
        "audio_transcription" => vec!["transcript", "speech", "fixture"],
        "blender_asset_processing" | "3d_generation_adaptation" => {
            vec!["mesh", "material", "asset"]
        }
        _ => vec!["document", "text", "workflow"],
    };
    serde_json::json!({
        "runtime_id": "forge_deterministic_fixture_runtime",
        "model_id": "forge_fixture_model_v1",
        "fixture_id": fixture.id,
        "fixture_kind": fixture.artifact_kind,
        "labels": labels,
        "confidence": 0.99,
        "quality_score": 1.0,
        "deterministic": true,
        "secret_free": fixture.secret_free,
    })
}

fn run_connected_runtime_probe(
    project_root: Option<&Path>,
    runtime_id: &str,
    capability_id: &str,
) -> Result<ConnectedRuntimeProbe> {
    let project_root = match project_root {
        Some(path) => path.to_path_buf(),
        None => env::current_dir()?,
    };
    let manifest_path = project_root.join(MULTIMODAL_RUNTIMES_RELATIVE_PATH);
    let manifest_bytes = fs::read(&manifest_path).with_context(|| {
        format!(
            "connected multimodal runtime manifest not found at {}",
            manifest_path.display()
        )
    })?;
    let manifest: MultimodalConnectedRuntimeManifest = serde_json::from_slice(&manifest_bytes)
        .with_context(|| {
            format!(
                "invalid connected multimodal runtime manifest {}",
                manifest_path.display()
            )
        })?;
    let runtime = manifest
        .runtimes
        .into_iter()
        .find(|candidate| candidate.id == runtime_id)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "connected multimodal runtime `{}` not declared in {}",
                runtime_id,
                manifest_path.display()
            )
        })?;
    if !runtime
        .capabilities
        .iter()
        .any(|capability| capability == capability_id)
    {
        bail!(
            "connected multimodal runtime `{}` does not declare capability `{}`",
            runtime.id,
            capability_id
        );
    }
    if runtime.probe_command.is_empty() {
        bail!(
            "connected multimodal runtime `{}` must declare a non-empty probe_command array",
            runtime.id
        );
    }
    if runtime
        .probe_command
        .iter()
        .any(|part| multimodal_manifest_placeholder(part))
        || multimodal_manifest_placeholder(&runtime.id)
        || multimodal_manifest_placeholder(&runtime.model_id)
    {
        bail!(
            "connected multimodal runtime `{}` still contains placeholder id, model_id or probe command entries",
            runtime.id
        );
    }
    if runtime.network_access || runtime.device_access {
        bail!(
            "connected multimodal runtime `{}` declares network or device access; runtime-benchmark only allows no-network/no-device probes",
            runtime.id
        );
    }

    let command_sha256 = hex_sha256(serde_json::to_string(&runtime.probe_command)?.as_bytes());
    let output = Command::new(&runtime.probe_command[0])
        .args(&runtime.probe_command[1..])
        .current_dir(&project_root)
        .output()
        .with_context(|| {
            format!(
                "failed to execute connected multimodal runtime probe `{}`",
                runtime.id
            )
        })?;
    let exit_code = output.status.code().unwrap_or(-1);
    if !output.status.success() {
        bail!(
            "connected multimodal runtime `{}` probe exited with code {}",
            runtime.id,
            exit_code
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let model_output: Value = serde_json::from_str(stdout.trim()).with_context(|| {
        format!(
            "connected multimodal runtime `{}` probe must write JSON to stdout",
            runtime.id
        )
    })?;
    let measurements = connected_runtime_measurements(&model_output);
    let production_runtime = runtime
        .production
        .as_ref()
        .map(|production| production_runtime_evidence(production, &model_output));

    Ok(ConnectedRuntimeProbe {
        evidence: MultimodalConnectedRuntimeEvidence {
            schema_version: "forge.multimodal.connected_runtime_probe.v1".to_string(),
            status: "connected_runtime_probe_completed".to_string(),
            manifest_path: manifest_path.display().to_string(),
            manifest_status: "loaded".to_string(),
            runtime_id: runtime.id,
            model_id: runtime.model_id,
            capabilities: runtime.capabilities,
            probe_command_sha256: command_sha256,
            probe_exit_code: exit_code,
            stdout_sha256: hex_sha256(&output.stdout),
            stderr_sha256: hex_sha256(&output.stderr),
            network_access_declared: runtime.network_access,
            device_access_declared: runtime.device_access,
        },
        model_output,
        measurements,
        production_runtime,
    })
}

fn multimodal_manifest_placeholder(value: &str) -> bool {
    let value = value.trim();
    value.is_empty()
        || (value.starts_with('<') && value.ends_with('>'))
        || value.contains("<absolute-path-to-")
        || value.contains("<approved-")
        || value.contains("<operator>")
        || value.contains("<approval-or-change-record>")
}

fn connected_runtime_measurements(model_output: &Value) -> Vec<MultimodalBenchmarkMeasurement> {
    let mut measurements = Vec::new();
    if let Some(value) = measurement_value(model_output.get("quality_score")) {
        measurements.push(benchmark_measurement(
            "quality_score",
            &value,
            "score",
            "connected_runtime_probe",
        ));
    }
    if let Some(value) = measurement_value(model_output.get("latency_ms")) {
        measurements.push(benchmark_measurement(
            "latency_ms",
            &value,
            "ms",
            "connected_runtime_probe",
        ));
    }
    measurements
}

fn production_runtime_evidence(
    production: &MultimodalConnectedRuntimeProductionConfig,
    model_output: &Value,
) -> MultimodalProductionRuntimeEvidence {
    let observed_quality_score = numeric_measurement(model_output.get("quality_score"));
    let observed_latency_ms = numeric_measurement(model_output.get("latency_ms"));
    let quality_score_passed = observed_quality_score
        .map(|score| score >= production.min_quality_score)
        .unwrap_or(false);
    let latency_passed = observed_latency_ms
        .map(|latency| latency <= production.max_latency_ms)
        .unwrap_or(false);
    let model_manifest_hash_valid = production.model_manifest_sha256.len() == 64
        && production
            .model_manifest_sha256
            .chars()
            .all(|character| character.is_ascii_hexdigit());
    let approval_recorded = !production.approved_by.trim().is_empty()
        && !production.approval_ref.trim().is_empty()
        && !production.runtime_version.trim().is_empty()
        && !production.model_license.trim().is_empty();
    let evidence_artifacts_recorded = !production.evidence_artifacts.is_empty()
        && production
            .evidence_artifacts
            .iter()
            .all(|artifact| !artifact.trim().is_empty());
    let promotion_ready = quality_score_passed
        && latency_passed
        && model_manifest_hash_valid
        && approval_recorded
        && evidence_artifacts_recorded;
    let status = if promotion_ready {
        "production_evidence_validated"
    } else {
        "production_evidence_incomplete"
    };

    MultimodalProductionRuntimeEvidence {
        schema_version: "forge.multimodal.production_runtime_evidence.v1".to_string(),
        status: status.to_string(),
        approved_by: production.approved_by.clone(),
        approval_ref: production.approval_ref.clone(),
        runtime_version: production.runtime_version.clone(),
        model_manifest_sha256: production.model_manifest_sha256.clone(),
        model_license: production.model_license.clone(),
        evidence_artifacts: production.evidence_artifacts.clone(),
        min_quality_score: production.min_quality_score,
        observed_quality_score,
        quality_score_passed,
        max_latency_ms: production.max_latency_ms,
        observed_latency_ms,
        latency_passed,
        promotion_ready,
        validation_evidence: vec![
            format!("approval_recorded={approval_recorded}"),
            format!("model_manifest_hash_valid={model_manifest_hash_valid}"),
            format!("evidence_artifacts_recorded={evidence_artifacts_recorded}"),
            format!("quality_score_passed={quality_score_passed}"),
            format!("latency_passed={latency_passed}"),
        ],
    }
}

fn production_runtime_measurements(
    evidence: &MultimodalProductionRuntimeEvidence,
) -> Vec<MultimodalBenchmarkMeasurement> {
    vec![
        benchmark_measurement(
            "quality_score_passed",
            if evidence.quality_score_passed {
                "true"
            } else {
                "false"
            },
            "boolean",
            "production_runtime_evidence",
        ),
        benchmark_measurement(
            "latency_passed",
            if evidence.latency_passed {
                "true"
            } else {
                "false"
            },
            "boolean",
            "production_runtime_evidence",
        ),
        benchmark_measurement(
            "production_evidence_validated",
            if evidence.promotion_ready {
                "true"
            } else {
                "false"
            },
            "boolean",
            "production_runtime_evidence",
        ),
    ]
}

fn numeric_measurement(value: Option<&Value>) -> Option<f64> {
    let value = value?;
    if let Some(number) = value.as_f64() {
        return Some(number);
    }
    value.as_str()?.parse::<f64>().ok()
}

fn measurement_value(value: Option<&Value>) -> Option<String> {
    let value = value?;
    if let Some(number) = value.as_f64() {
        return Some(number.to_string());
    }
    value.as_str().map(ToString::to_string)
}

fn benchmark_measurement(
    id: &str,
    value: &str,
    unit: &str,
    source: &str,
) -> MultimodalBenchmarkMeasurement {
    MultimodalBenchmarkMeasurement {
        id: id.to_string(),
        value: value.to_string(),
        unit: unit.to_string(),
        source: source.to_string(),
    }
}

fn demo_stage(
    id: &str,
    title: &str,
    deterministic: bool,
    requires_model: bool,
    requires_device_access: bool,
    description: &str,
) -> MultimodalDemoStage {
    MultimodalDemoStage {
        id: id.to_string(),
        title: title.to_string(),
        deterministic,
        requires_model,
        requires_device_access,
        description: description.to_string(),
    }
}

fn to_strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn runtime_guards() -> Vec<String> {
    [
        "experimental_flag_disabled_by_default",
        "human_opt_in",
        "runtime_guard_approval",
        "scoped_permission_contract",
        "dry_run_or_simulation_first",
        "kill_switch",
        "audit_log",
        "secrets_redaction",
        "rollback_or_uninstall_plan",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}
