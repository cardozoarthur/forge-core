use anyhow::{bail, Result};
use serde::Serialize;

const STATUS_SCHEMA_VERSION: &str = "forge.multimodal.status.v1";
const INSTALL_PLAN_SCHEMA_VERSION: &str = "forge.multimodal.install_plan.v1";
const BENCHMARK_TEMPLATE_SCHEMA_VERSION: &str = "forge.multimodal.benchmark_template.v1";
const DEMO_PLAN_SCHEMA_VERSION: &str = "forge.multimodal.demo_plan.v1";
const GUARD_SCHEMA_VERSION: &str = "forge.multimodal.guard.v1";

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
    let capabilities = capability_inventory(enable_experimental);
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
        status: if enable_experimental {
            "experimental_enabled"
        } else {
            "experimental_disabled"
        }
        .to_string(),
        feature_flag: MultimodalFeatureFlag {
            name: "forge.experimental.multimodal".to_string(),
            enabled: enable_experimental,
            default_state: "disabled".to_string(),
            activation:
                "Pass --enable-experimental or set the future Forge-owned config flag after human approval."
                    .to_string(),
        },
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
