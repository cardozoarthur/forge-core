use crate::artifact::{hex_sha256, write_json_artifact};
use crate::storage::ForgeStore;
use crate::workflow::attach_workflow_artifact;
use crate::workflow::ArtifactAttachReport;
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::Instant;

const PATCH_PLAN_SCHEMA_VERSION: &str = "forge.patch_plan.v1";
const DEFAULT_CONTEXT_BUDGET_BYTES: usize = 1200;

#[derive(Debug, Clone, Serialize)]
pub struct PatchPlanReport {
    pub schema_version: String,
    pub status: String,
    pub workflow_id: String,
    pub task_id: String,
    pub intent: String,
    pub origin: String,
    pub applies_changes: bool,
    pub external_resources_mutated: bool,
    pub requires_human_approval: bool,
    pub permission_gate: PatchPermissionGate,
    pub context_contract: PatchContextContract,
    pub diff_review: PatchDiffReview,
    pub file_snapshots: Vec<PatchFileSnapshot>,
    pub artifact: Option<PatchPlanArtifactRef>,
    pub safety_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatchPermissionGate {
    pub policy: String,
    pub risk_level: String,
    pub allowed_root: String,
    pub allowed_paths: Vec<String>,
    pub blocked_paths: Vec<String>,
    pub requires_explicit_allow_before_apply: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatchContextContract {
    pub required: bool,
    pub strict: bool,
    pub budget_bytes: usize,
    pub command: String,
    pub handoff_rule: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatchDiffReview {
    pub required_before_apply: bool,
    pub review_commands: Vec<String>,
    pub validation_commands: Vec<String>,
    pub rollback_plan: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatchFileSnapshot {
    pub path: String,
    pub exists: bool,
    pub bytes: u64,
    pub sha256: Option<String>,
    pub content_sampled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatchPlanArtifactRef {
    pub kind: String,
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

pub fn build_patch_plan(
    store: &ForgeStore,
    workflow_id: &str,
    task_id: &str,
    paths: Vec<String>,
    intent: &str,
    origin: &str,
) -> Result<PatchPlanReport> {
    let intent = intent.trim();
    if intent.is_empty() {
        bail!("patch intent is required");
    }
    if paths.is_empty() {
        bail!("at least one patch path is required");
    }

    let workflow = store.load_workflow(workflow_id)?;
    if !workflow.tasks.iter().any(|task| task.id == task_id) {
        bail!("task {task_id} not found in workflow {workflow_id}");
    }

    let mut allowed_paths = Vec::new();
    let mut blocked_paths = Vec::new();
    for path in paths {
        let normalized = path.trim().to_string();
        if normalized.is_empty() || !is_repo_relative_path(&normalized) {
            blocked_paths.push(normalized);
        } else {
            allowed_paths.push(normalized);
        }
    }
    allowed_paths.sort();
    allowed_paths.dedup();
    blocked_paths.sort();
    blocked_paths.dedup();

    let file_snapshots = allowed_paths
        .iter()
        .map(|path| snapshot_file(path))
        .collect::<Result<Vec<_>>>()?;

    let status = if allowed_paths.is_empty() {
        "patch_plan_blocked"
    } else {
        "patch_plan_ready"
    };
    let cwd = std::env::current_dir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| ".".to_string());
    let mut report = PatchPlanReport {
        schema_version: PATCH_PLAN_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        workflow_id: workflow_id.to_string(),
        task_id: task_id.to_string(),
        intent: intent.to_string(),
        origin: origin.to_string(),
        applies_changes: false,
        external_resources_mutated: false,
        requires_human_approval: true,
        permission_gate: PatchPermissionGate {
            policy: "repo_relative_paths_only_no_apply".to_string(),
            risk_level: if blocked_paths.is_empty() && allowed_paths.len() <= 2 {
                "medium"
            } else {
                "high"
            }
            .to_string(),
            allowed_root: cwd,
            allowed_paths: allowed_paths.clone(),
            blocked_paths: blocked_paths.clone(),
            requires_explicit_allow_before_apply: true,
        },
        context_contract: PatchContextContract {
            required: true,
            strict: true,
            budget_bytes: DEFAULT_CONTEXT_BUDGET_BYTES,
            command: format!(
                "forge context --workflow {workflow_id} --task {task_id} --budget {DEFAULT_CONTEXT_BUDGET_BYTES} --strict --output json"
            ),
            handoff_rule:
                "Executor must receive bounded context and return a diff/patch for human review; Forge does not apply changes during planning."
                    .to_string(),
        },
        diff_review: PatchDiffReview {
            required_before_apply: true,
            review_commands: diff_review_commands(&allowed_paths),
            validation_commands: vec![
                "cargo fmt --check".to_string(),
                "cargo clippy --all-targets --all-features -- -D warnings".to_string(),
                "cargo test".to_string(),
            ],
            rollback_plan: vec![
                "Keep pre-change file snapshots and SHA-256 hashes in the patch plan artifact."
                    .to_string(),
                "Require human diff approval before any future apply step.".to_string(),
                "If an apply step is rejected, archive the patch plan and leave source files unchanged."
                    .to_string(),
            ],
        },
        file_snapshots,
        artifact: None,
        safety_notes: vec![
            "This command creates a patch plan only; it does not edit source files.".to_string(),
            "Absolute paths, parent-directory traversal and .git internals are blocked.".to_string(),
            "External resources, Docker, Kubernetes, Knative and device interfaces are not touched."
                .to_string(),
        ],
    };

    if !allowed_paths.is_empty() {
        let payload = serde_json::to_value(&report)?;
        let relative_path = format!("tmp/{workflow_id}-{task_id}-patch-plan.json");
        let (path, _) = write_json_artifact(&store.base_dir(), &relative_path, &payload)?;
        let attached = attach_workflow_artifact(store, workflow_id, &path, "patch_plan", origin)?;
        report.artifact = Some(PatchPlanArtifactRef {
            kind: attached.artifact.kind,
            path: attached.artifact.path,
            sha256: attached.artifact.sha256,
            bytes: attached.artifact.bytes,
        });
    }

    Ok(report)
}

fn is_repo_relative_path(path: &str) -> bool {
    let parsed = Path::new(path);
    if parsed.is_absolute() || path.starts_with(".git/") || path == ".git" {
        return false;
    }

    parsed
        .components()
        .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

fn snapshot_file(path: &str) -> Result<PatchFileSnapshot> {
    let path_buf = PathBuf::from(path);
    if !path_buf.exists() {
        return Ok(PatchFileSnapshot {
            path: path.to_string(),
            exists: false,
            bytes: 0,
            sha256: None,
            content_sampled: false,
        });
    }
    if !path_buf.is_file() {
        return Ok(PatchFileSnapshot {
            path: path.to_string(),
            exists: true,
            bytes: 0,
            sha256: None,
            content_sampled: false,
        });
    }

    let bytes = fs::read(&path_buf)
        .with_context(|| format!("failed to read patch target {}", path_buf.display()))?;
    Ok(PatchFileSnapshot {
        path: path.to_string(),
        exists: true,
        bytes: bytes.len() as u64,
        sha256: Some(hex_sha256(&bytes)),
        content_sampled: true,
    })
}

fn diff_review_commands(paths: &[String]) -> Vec<String> {
    if paths.is_empty() {
        return Vec::new();
    }
    let path_args = paths.join(" ");
    vec![
        format!("git diff -- {path_args}"),
        format!("git diff --check -- {path_args}"),
        format!("git status --short -- {path_args}"),
    ]
}

// ---------------------------------------------------------------------------
// Patch apply
// ---------------------------------------------------------------------------

const PATCH_APPLY_SCHEMA_VERSION: &str = "forge.patch_apply.v1";
const PATCH_REVERT_SCHEMA_VERSION: &str = "forge.patch_revert.v1";

#[derive(Debug, Clone, Serialize)]
pub struct PatchApplyReport {
    pub schema_version: String,
    pub status: String,
    pub workflow_id: String,
    pub task_id: String,
    pub origin: String,
    pub plan_artifact: Option<PatchApplyArtifactRef>,
    pub file_snapshots: Vec<PatchFileSnapshot>,
    pub validation: PatchValidationSummary,
    pub artifact: Option<PatchApplyArtifactRef>,
    pub rollback_instructions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatchValidationSummary {
    pub passed: bool,
    pub commands: Vec<PatchValidationCommandResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatchValidationCommandResult {
    pub command: String,
    pub status: String,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<u64>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatchRevertReport {
    pub schema_version: String,
    pub status: String,
    pub workflow_id: String,
    pub task_id: String,
    pub origin: String,
    pub apply_artifact: PatchApplyArtifactRef,
    pub restored_snapshots: Vec<PatchFileSnapshot>,
    pub validation: PatchValidationSummary,
    pub artifact: Option<PatchApplyArtifactRef>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatchApplyArtifactRef {
    pub kind: String,
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

impl PatchApplyArtifactRef {
    fn from_artifact(attached: &ArtifactAttachReport) -> Self {
        Self {
            kind: attached.artifact.kind.clone(),
            path: attached.artifact.path.clone(),
            sha256: attached.artifact.sha256.clone(),
            bytes: attached.artifact.bytes,
        }
    }
}

/// Default validation commands for patch apply.
const DEFAULT_APPLY_VALIDATION_COMMANDS: [&str; 2] = [
    "cargo fmt --check",
    "cargo clippy --all-targets --all-features -- -D warnings",
];

/// Record that a patch has been applied: snapshot the current file state,
/// run validation commands, and persist an apply artifact with rollback
/// instructions.
///
/// `validation_commands` controls which shell commands to run for validation.
/// Pass `None` to use the default set (fmt + clippy, but NOT test – the
/// heavy `cargo test` belongs in the patch-plan's diff-review phase so it
/// does not cause recursive test hangs).
pub fn build_patch_apply(
    store: &ForgeStore,
    workflow_id: &str,
    task_id: &str,
    paths: Vec<String>,
    origin: &str,
    plan_artifact_path: Option<&str>,
    validation_commands: Option<&[String]>,
) -> Result<PatchApplyReport> {
    if paths.is_empty() {
        bail!("at least one patch path is required");
    }

    let workflow = store.load_workflow(workflow_id)?;
    if !workflow.tasks.iter().any(|task| task.id == task_id) {
        bail!("task {task_id} not found in workflow {workflow_id}");
    }

    let mut allowed_paths = Vec::new();
    let mut blocked_paths = Vec::new();
    for path in paths {
        let normalized = path.trim().to_string();
        if normalized.is_empty() || !is_repo_relative_path(&normalized) {
            blocked_paths.push(normalized);
        } else {
            allowed_paths.push(normalized);
        }
    }
    allowed_paths.sort();
    allowed_paths.dedup();

    if allowed_paths.is_empty() {
        bail!("all patch paths were blocked: {:?}", blocked_paths);
    }

    // Resolve optional plan artifact reference for lineage.
    let plan_ref = if let Some(plan_path) = plan_artifact_path {
        let plan_bytes = fs::read(plan_path)
            .with_context(|| format!("failed to read plan artifact: {plan_path}"))?;
        Some(PatchApplyArtifactRef {
            kind: "patch_plan".to_string(),
            path: plan_path.to_string(),
            sha256: hex_sha256(&plan_bytes),
            bytes: plan_bytes.len() as u64,
        })
    } else {
        None
    };

    // Snapshot current file state (after executor changes).
    let file_snapshots = allowed_paths
        .iter()
        .map(|path| snapshot_file(path))
        .collect::<Result<Vec<_>>>()?;

    // Run validation commands.
    let validation = if let Some(commands) = validation_commands {
        run_patch_validation(commands)?
    } else {
        let defaults: Vec<String> = DEFAULT_APPLY_VALIDATION_COMMANDS
            .iter()
            .map(|s| s.to_string())
            .collect();
        run_patch_validation(&defaults)?
    };

    let status = if validation.passed {
        "patch_applied"
    } else {
        "patch_applied_with_failures"
    };

    let report = PatchApplyReport {
        schema_version: PATCH_APPLY_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        workflow_id: workflow_id.to_string(),
        task_id: task_id.to_string(),
        origin: origin.to_string(),
        plan_artifact: plan_ref,
        file_snapshots: file_snapshots.clone(),
        validation: validation.clone(),
        artifact: None,
        rollback_instructions: vec![
            "Use `forge patch revert` with this apply artifact to restore pre-apply file state."
                .to_string(),
            "Alternatively, run `git checkout -- <path>` on each modified file to discard changes."
                .to_string(),
            "Pre-apply file metadata (SHA-256, size) is recorded in the associated patch plan if available."
                .to_string(),
        ],
    };

    let payload = serde_json::to_value(&report)?;
    let relative_path = format!("tmp/{workflow_id}-{task_id}-patch-apply.json");
    let (path, _) = write_json_artifact(&store.base_dir(), &relative_path, &payload)?;
    let attached = attach_workflow_artifact(store, workflow_id, &path, "patch_apply", origin)?;

    Ok(PatchApplyReport {
        artifact: Some(PatchApplyArtifactRef::from_artifact(&attached)),
        ..report
    })
}

/// Default validation commands for patch revert.
const DEFAULT_REVERT_VALIDATION_COMMANDS: [&str; 2] = [
    "cargo fmt --check",
    "cargo clippy --all-targets --all-features -- -D warnings",
];

/// Revert a previously applied patch: restore files to their pre-apply state
/// via git checkout, run validation, and record the revert artifact.
///
/// `validation_commands` controls which shell commands to run for validation.
/// Pass `None` to use the default set (fmt + clippy).
pub fn build_patch_revert(
    store: &ForgeStore,
    workflow_id: &str,
    task_id: &str,
    apply_artifact_path: &str,
    origin: &str,
    validation_commands: Option<&[String]>,
) -> Result<PatchRevertReport> {
    let apply_bytes = fs::read(apply_artifact_path)
        .with_context(|| format!("failed to read apply artifact: {apply_artifact_path}"))?;
    let apply_artifact_ref = PatchApplyArtifactRef {
        kind: "patch_apply".to_string(),
        path: apply_artifact_path.to_string(),
        sha256: hex_sha256(&apply_bytes),
        bytes: apply_bytes.len() as u64,
    };

    // Deserialize the original apply report to get file paths.
    let apply_report: serde_json::Value = serde_json::from_slice(&apply_bytes)?;
    let paths: Vec<String> = apply_report["file_snapshots"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v["path"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    if paths.is_empty() {
        bail!("apply artifact contains no file paths to revert");
    }

    let workflow = store.load_workflow(workflow_id)?;
    if !workflow.tasks.iter().any(|task| task.id == task_id) {
        bail!("task {task_id} not found in workflow {workflow_id}");
    }

    // Restore files via git checkout.
    for path in &paths {
        let status = Command::new("git")
            .args(["checkout", "--", path])
            .output()
            .with_context(|| format!("failed to run git checkout for {path}"))?;
        if !status.status.success() {
            let stderr = String::from_utf8_lossy(&status.stderr);
            bail!("git checkout failed for {path}: {stderr}");
        }
    }

    // Snapshot restored state.
    let restored_snapshots = paths
        .iter()
        .map(|path| snapshot_file(path))
        .collect::<Result<Vec<_>>>()?;

    // Run validation.
    let validation = if let Some(commands) = validation_commands {
        run_patch_validation(commands)?
    } else {
        let defaults: Vec<String> = DEFAULT_REVERT_VALIDATION_COMMANDS
            .iter()
            .map(|s| s.to_string())
            .collect();
        run_patch_validation(&defaults)?
    };

    let status = if validation.passed {
        "patch_reverted"
    } else {
        "patch_reverted_with_failures"
    };

    let report = PatchRevertReport {
        schema_version: PATCH_REVERT_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        workflow_id: workflow_id.to_string(),
        task_id: task_id.to_string(),
        origin: origin.to_string(),
        apply_artifact: apply_artifact_ref,
        restored_snapshots,
        validation: validation.clone(),
        artifact: None,
    };

    let payload = serde_json::to_value(&report)?;
    let relative_path = format!("tmp/{workflow_id}-{task_id}-patch-revert.json");
    let (path, _) = write_json_artifact(&store.base_dir(), &relative_path, &payload)?;
    let attached = attach_workflow_artifact(store, workflow_id, &path, "patch_revert", origin)?;

    Ok(PatchRevertReport {
        artifact: Some(PatchApplyArtifactRef::from_artifact(&attached)),
        ..report
    })
}

fn run_patch_validation(commands: &[String]) -> Result<PatchValidationSummary> {
    let mut command_results = Vec::new();
    let mut all_passed = true;

    for command in commands {
        let start = Instant::now();
        let output = Command::new("sh").args(["-c", command]).output();
        let duration_ms = start.elapsed().as_millis() as u64;

        match output {
            Ok(output) => {
                let passed = output.status.success();
                if !passed {
                    all_passed = false;
                }
                command_results.push(PatchValidationCommandResult {
                    command: command.clone(),
                    status: if passed { "passed" } else { "failed" }.to_string(),
                    exit_code: output.status.code(),
                    duration_ms: Some(duration_ms),
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                });
            }
            Err(e) => {
                all_passed = false;
                command_results.push(PatchValidationCommandResult {
                    command: command.clone(),
                    status: "error".to_string(),
                    exit_code: None,
                    duration_ms: Some(duration_ms),
                    stdout: String::new(),
                    stderr: format!("failed to execute command: {e}"),
                });
            }
        }
    }

    Ok(PatchValidationSummary {
        passed: all_passed,
        commands: command_results,
    })
}
