use anyhow::{bail, Result};
use serde::Serialize;

const STATUS_SCHEMA_VERSION: &str = "forge.multimodal.status.v1";
const INSTALL_PLAN_SCHEMA_VERSION: &str = "forge.multimodal.install_plan.v1";
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
    let capability = capability_inventory(enable_experimental)
        .into_iter()
        .find(|capability| capability.id == capability_id)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "unknown multimodal capability: {capability_id}; run forge multimodal status"
            )
        })?;

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
