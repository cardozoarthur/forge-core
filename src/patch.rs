use crate::artifact::{hex_sha256, write_json_artifact};
use crate::identity::ensure_workflow_policy;
use crate::storage::ForgeStore;
use crate::workflow::attach_workflow_artifact;
use crate::workflow::ArtifactAttachReport;
use crate::worktree::{
    evaluate_worktree_modification_guard, resolve_bound_worktree_root,
    WorktreeModificationGuardReport, WorktreeModificationGuardRequest,
};
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree_guard: Option<WorktreeModificationGuardReport>,
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
    ensure_workflow_policy(store, workflow_id, "patch plan")?;
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

    let bound_worktree_root = resolve_bound_worktree_root(store, workflow_id, Some(task_id))?;
    let worktree_guard = bound_worktree_root
        .as_ref()
        .map(|root| {
            evaluate_worktree_modification_guard(
                store,
                WorktreeModificationGuardRequest {
                    worktree: root.display().to_string(),
                    operation: "modify".to_string(),
                    paths: allowed_paths.clone(),
                    reason: intent.to_string(),
                    workflow_id: Some(workflow_id.to_string()),
                    task_id: Some(task_id.to_string()),
                },
            )
        })
        .transpose()?;
    if let Some(guard) = worktree_guard.as_ref().filter(|guard| !guard.allowed) {
        let denied = guard.blocked_paths.to_vec();
        allowed_paths.retain(|path| !denied.contains(path));
        blocked_paths.extend(denied);
        blocked_paths.sort();
        blocked_paths.dedup();
    }

    let allowed_root = bound_worktree_root
        .clone()
        .unwrap_or(std::env::current_dir()?);
    let file_snapshots = allowed_paths
        .iter()
        .map(|path| snapshot_file_at(&allowed_root, path))
        .collect::<Result<Vec<_>>>()?;

    let status = if allowed_paths.is_empty() || !blocked_paths.is_empty() {
        "patch_plan_blocked"
    } else {
        "patch_plan_ready"
    };
    let cwd = allowed_root.display().to_string();
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
        worktree_guard,
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

    if !allowed_paths.is_empty() && blocked_paths.is_empty() {
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
    snapshot_file_at(Path::new("."), path)
}

fn snapshot_file_at(root: &Path, path: &str) -> Result<PatchFileSnapshot> {
    let path_buf = root.join(path);
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

const PATCH_REVIEW_SCHEMA_VERSION: &str = "forge.patch_review.v1";
const PATCH_DIFF_SCHEMA_VERSION: &str = "forge.patch_diff.v1";
const PATCH_APPLY_SCHEMA_VERSION: &str = "forge.patch_apply.v1";
const PATCH_REVERT_SCHEMA_VERSION: &str = "forge.patch_revert.v1";
const PATCH_RESTORE_SCHEMA_VERSION: &str = "forge.patch_restore.v1";

#[derive(Debug, Clone, Serialize)]
pub struct PatchReviewReport {
    pub schema_version: String,
    pub status: String,
    pub workflow_id: String,
    pub task_id: String,
    pub origin: String,
    pub applies_changes: bool,
    pub external_resources_mutated: bool,
    pub requires_human_approval: bool,
    pub plan_artifact: Option<PatchApplyArtifactRef>,
    pub summary: PatchReviewSummary,
    pub path_reviews: Vec<PatchPathReview>,
    pub commands: Vec<PatchReviewCommandResult>,
    pub artifact: Option<PatchApplyArtifactRef>,
    pub safety_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatchReviewSummary {
    pub changed_path_count: usize,
    pub diff_present: bool,
    pub diff_check_passed: bool,
    pub blocked_paths: Vec<String>,
    pub approval_recommendation: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatchPathReview {
    pub path: String,
    pub allowed: bool,
    pub exists: bool,
    pub bytes: u64,
    pub sha256: Option<String>,
    pub changed: bool,
    pub status_line: Option<String>,
    pub diff_excerpt: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatchDiffReport {
    pub schema_version: String,
    pub status: String,
    pub workflow_id: String,
    pub task_id: String,
    pub origin: String,
    pub applies_changes: bool,
    pub external_resources_mutated: bool,
    pub requires_human_approval: bool,
    pub summary: PatchDiffSummary,
    pub selection: PatchDiffSelection,
    pub navigation: PatchDiffNavigation,
    pub files: Vec<PatchDiffFile>,
    pub commands: Vec<PatchReviewCommandResult>,
    pub artifact: Option<PatchApplyArtifactRef>,
    pub safety_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatchDiffSummary {
    pub requested_path_count: usize,
    pub changed_file_count: usize,
    pub hunk_count: usize,
    pub line_count: usize,
    pub context_lines: usize,
    pub blocked_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatchDiffSelection {
    pub selected_file_index: usize,
    pub selected_hunk_index: usize,
    pub selected_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatchDiffNavigation {
    pub has_previous_file: bool,
    pub has_next_file: bool,
    pub has_previous_hunk: bool,
    pub has_next_hunk: bool,
    pub previous_file_command: Option<String>,
    pub next_file_command: Option<String>,
    pub previous_hunk_command: Option<String>,
    pub next_hunk_command: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatchDiffFile {
    pub file_index: usize,
    pub path: String,
    pub changed: bool,
    pub additions: usize,
    pub deletions: usize,
    pub hunks: Vec<PatchDiffHunk>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatchDiffHunk {
    pub hunk_index: usize,
    pub header: String,
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub lines: Vec<PatchDiffLine>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatchDiffLine {
    pub kind: String,
    pub old_lineno: Option<usize>,
    pub new_lineno: Option<usize>,
    pub content: String,
}

#[derive(Debug, Clone, Copy)]
pub struct PatchDiffOptions<'a> {
    pub file_index: usize,
    pub hunk_index: usize,
    pub context_lines: usize,
    pub origin: &'a str,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatchReviewCommandResult {
    pub command: String,
    pub status: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

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

pub fn build_patch_review(
    store: &ForgeStore,
    workflow_id: &str,
    task_id: &str,
    paths: Vec<String>,
    origin: &str,
    plan_artifact_path: Option<&str>,
) -> Result<PatchReviewReport> {
    ensure_workflow_policy(store, workflow_id, "patch review")?;
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

    let mut commands = Vec::new();
    if !allowed_paths.is_empty() {
        commands.push(run_git_review_command(
            "git diff --stat",
            &["diff", "--stat"],
            &allowed_paths,
        ));
        commands.push(run_git_review_command(
            "git diff --check",
            &["diff", "--check"],
            &allowed_paths,
        ));
        commands.push(run_git_review_command(
            "git status --short",
            &["status", "--short"],
            &allowed_paths,
        ));
    }

    let status_output = commands
        .iter()
        .find(|command| command.command.starts_with("git status --short"))
        .map(|command| command.stdout.as_str())
        .unwrap_or("");
    let path_reviews = allowed_paths
        .iter()
        .map(|path| build_path_review(path, status_output))
        .collect::<Result<Vec<_>>>()?;

    let changed_path_count = path_reviews.iter().filter(|review| review.changed).count();
    let diff_present = changed_path_count > 0;
    let diff_check_passed = commands
        .iter()
        .find(|command| command.command.starts_with("git diff --check"))
        .map(|command| command.exit_code == Some(0))
        .unwrap_or(false);
    let approval_recommendation = if allowed_paths.is_empty() {
        "blocked_paths_only"
    } else if !diff_check_passed {
        "fix_diff_check_before_review"
    } else if diff_present {
        "ready_for_human_review"
    } else {
        "no_changes_to_review"
    }
    .to_string();
    let status = if allowed_paths.is_empty() {
        "patch_review_blocked"
    } else if diff_present {
        "patch_review_ready"
    } else {
        "patch_review_no_changes"
    }
    .to_string();

    let mut report = PatchReviewReport {
        schema_version: PATCH_REVIEW_SCHEMA_VERSION.to_string(),
        status,
        workflow_id: workflow_id.to_string(),
        task_id: task_id.to_string(),
        origin: origin.to_string(),
        applies_changes: false,
        external_resources_mutated: false,
        requires_human_approval: true,
        plan_artifact: plan_ref,
        summary: PatchReviewSummary {
            changed_path_count,
            diff_present,
            diff_check_passed,
            blocked_paths,
            approval_recommendation,
        },
        path_reviews,
        commands,
        artifact: None,
        safety_notes: vec![
            "This command reviews local file diffs only; it does not edit source files."
                .to_string(),
            "Patch review must precede human approval for apply or revert execution."
                .to_string(),
            "Git commands run with path arguments separated from the command to avoid shell expansion."
                .to_string(),
        ],
    };

    if !allowed_paths.is_empty() {
        let payload = serde_json::to_value(&report)?;
        let relative_path = format!("tmp/{workflow_id}-{task_id}-patch-review.json");
        let (path, _) = write_json_artifact(&store.base_dir(), &relative_path, &payload)?;
        let attached = attach_workflow_artifact(store, workflow_id, &path, "patch_review", origin)?;
        report.artifact = Some(PatchApplyArtifactRef::from_artifact(&attached));
    }

    Ok(report)
}

fn build_path_review(path: &str, status_output: &str) -> Result<PatchPathReview> {
    let snapshot = snapshot_file(path)?;
    let diff = run_git_review_command("git diff", &["diff"], &[path.to_string()]);
    let status_line = status_output
        .lines()
        .find(|line| line.split_whitespace().last() == Some(path))
        .map(str::to_string);
    let diff_excerpt = bounded_command_text(&diff.stdout);
    let changed = !diff_excerpt.trim().is_empty() || status_line.is_some();

    Ok(PatchPathReview {
        path: path.to_string(),
        allowed: true,
        exists: snapshot.exists,
        bytes: snapshot.bytes,
        sha256: snapshot.sha256,
        changed,
        status_line,
        diff_excerpt,
    })
}

pub fn build_patch_diff(
    store: &ForgeStore,
    workflow_id: &str,
    task_id: &str,
    paths: Vec<String>,
    options: PatchDiffOptions<'_>,
) -> Result<PatchDiffReport> {
    ensure_workflow_policy(store, workflow_id, "patch diff")?;
    if paths.is_empty() {
        bail!("at least one patch path is required");
    }

    let workflow = store.load_workflow(workflow_id)?;
    if !workflow.tasks.iter().any(|task| task.id == task_id) {
        bail!("task {task_id} not found in workflow {workflow_id}");
    }

    let requested_path_count = paths.len();
    let mut allowed_paths = Vec::new();
    let mut blocked_paths = Vec::new();
    for path in paths {
        let normalized = path.trim().to_string();
        if normalized.is_empty() || !is_repo_relative_path(&normalized) {
            blocked_paths.push(normalized);
        } else if !allowed_paths.contains(&normalized) {
            allowed_paths.push(normalized);
        }
    }
    blocked_paths.sort();
    blocked_paths.dedup();

    let context_lines = options.context_lines.min(50);
    let mut commands = Vec::new();
    let mut files = Vec::new();
    if !allowed_paths.is_empty() {
        let unified_arg = format!("--unified={context_lines}");
        let label = format!("git diff {unified_arg}");
        let diff_command =
            run_git_review_command(&label, &["diff", unified_arg.as_str()], &allowed_paths);
        files = parse_git_diff_files(&diff_command.stdout);
        files = order_patch_diff_files(files, &allowed_paths);
        commands.push(diff_command);
    }

    for (idx, file) in files.iter_mut().enumerate() {
        file.file_index = idx;
        for (hunk_idx, hunk) in file.hunks.iter_mut().enumerate() {
            hunk.hunk_index = hunk_idx;
        }
    }

    let changed_file_count = files.iter().filter(|file| file.changed).count();
    let hunk_count = files.iter().map(|file| file.hunks.len()).sum();
    let line_count = files
        .iter()
        .flat_map(|file| file.hunks.iter())
        .map(|hunk| hunk.lines.len())
        .sum();
    let selected_file_index = if files.is_empty() {
        0
    } else {
        options.file_index.min(files.len() - 1)
    };
    let selected_hunk_index = files
        .get(selected_file_index)
        .and_then(|file| file.hunks.len().checked_sub(1))
        .map(|last| options.hunk_index.min(last))
        .unwrap_or(0);
    let selected_path = files.get(selected_file_index).map(|file| file.path.clone());
    let status = if allowed_paths.is_empty() {
        "patch_diff_blocked"
    } else if changed_file_count == 0 {
        "patch_diff_no_changes"
    } else if commands
        .iter()
        .any(|command| command.exit_code != Some(0) && command.status != "passed")
    {
        "patch_diff_failed"
    } else {
        "patch_diff_ready"
    }
    .to_string();
    let navigation = build_patch_diff_navigation(
        workflow_id,
        task_id,
        &allowed_paths,
        selected_file_index,
        selected_hunk_index,
        context_lines,
        &files,
    );

    let mut report = PatchDiffReport {
        schema_version: PATCH_DIFF_SCHEMA_VERSION.to_string(),
        status,
        workflow_id: workflow_id.to_string(),
        task_id: task_id.to_string(),
        origin: options.origin.to_string(),
        applies_changes: false,
        external_resources_mutated: false,
        requires_human_approval: true,
        summary: PatchDiffSummary {
            requested_path_count,
            changed_file_count,
            hunk_count,
            line_count,
            context_lines,
            blocked_paths,
        },
        selection: PatchDiffSelection {
            selected_file_index,
            selected_hunk_index,
            selected_path,
        },
        navigation,
        files,
        commands,
        artifact: None,
        safety_notes: vec![
            "This command builds a multi-file diff navigation model and does not edit source files."
                .to_string(),
            "File and hunk indexes are clamped to the available diff model so TUI/MCP callers can page safely."
                .to_string(),
            "Git diff runs with separated path arguments to avoid shell expansion.".to_string(),
        ],
    };

    if !allowed_paths.is_empty() {
        let payload = serde_json::to_value(&report)?;
        let relative_path = format!("tmp/{workflow_id}-{task_id}-patch-diff.json");
        let (path, _) = write_json_artifact(&store.base_dir(), &relative_path, &payload)?;
        let attached =
            attach_workflow_artifact(store, workflow_id, &path, "patch_diff", options.origin)?;
        report.artifact = Some(PatchApplyArtifactRef::from_artifact(&attached));
    }

    Ok(report)
}

fn parse_git_diff_files(diff: &str) -> Vec<PatchDiffFile> {
    let mut files = Vec::new();
    let mut current_file: Option<PatchDiffFile> = None;
    let mut current_hunk: Option<PatchDiffHunk> = None;
    let mut old_lineno = 0usize;
    let mut new_lineno = 0usize;

    for line in diff.lines() {
        if let Some(path) = parse_diff_git_path(line) {
            push_current_hunk(&mut current_file, &mut current_hunk);
            if let Some(file) = current_file.take() {
                files.push(file);
            }
            current_file = Some(PatchDiffFile {
                file_index: files.len(),
                path,
                changed: true,
                additions: 0,
                deletions: 0,
                hunks: Vec::new(),
            });
            continue;
        }

        if line.starts_with("@@") {
            push_current_hunk(&mut current_file, &mut current_hunk);
            let (old_start, old_lines, new_start, new_lines) = parse_hunk_header(line);
            old_lineno = old_start;
            new_lineno = new_start;
            current_hunk = Some(PatchDiffHunk {
                hunk_index: 0,
                header: line.to_string(),
                old_start,
                old_lines,
                new_start,
                new_lines,
                lines: Vec::new(),
            });
            continue;
        }

        if let Some(hunk) = current_hunk.as_mut() {
            if let Some(rest) = line.strip_prefix('+') {
                hunk.lines.push(PatchDiffLine {
                    kind: "addition".to_string(),
                    old_lineno: None,
                    new_lineno: Some(new_lineno),
                    content: rest.to_string(),
                });
                new_lineno += 1;
                if let Some(file) = current_file.as_mut() {
                    file.additions += 1;
                }
            } else if let Some(rest) = line.strip_prefix('-') {
                hunk.lines.push(PatchDiffLine {
                    kind: "deletion".to_string(),
                    old_lineno: Some(old_lineno),
                    new_lineno: None,
                    content: rest.to_string(),
                });
                old_lineno += 1;
                if let Some(file) = current_file.as_mut() {
                    file.deletions += 1;
                }
            } else if let Some(rest) = line.strip_prefix(' ') {
                hunk.lines.push(PatchDiffLine {
                    kind: "context".to_string(),
                    old_lineno: Some(old_lineno),
                    new_lineno: Some(new_lineno),
                    content: rest.to_string(),
                });
                old_lineno += 1;
                new_lineno += 1;
            } else if line.starts_with('\\') {
                hunk.lines.push(PatchDiffLine {
                    kind: "metadata".to_string(),
                    old_lineno: None,
                    new_lineno: None,
                    content: line.to_string(),
                });
            }
        }
    }

    push_current_hunk(&mut current_file, &mut current_hunk);
    if let Some(file) = current_file {
        files.push(file);
    }
    files
}

fn push_current_hunk(
    current_file: &mut Option<PatchDiffFile>,
    current_hunk: &mut Option<PatchDiffHunk>,
) {
    if let (Some(file), Some(hunk)) = (current_file.as_mut(), current_hunk.take()) {
        file.hunks.push(hunk);
    }
}

fn parse_diff_git_path(line: &str) -> Option<String> {
    let mut parts = line.split_whitespace();
    if parts.next() != Some("diff") || parts.next() != Some("--git") {
        return None;
    }
    let _old_path = parts.next()?;
    let new_path = parts.next()?;
    Some(new_path.strip_prefix("b/").unwrap_or(new_path).to_string())
}

fn parse_hunk_header(header: &str) -> (usize, usize, usize, usize) {
    let mut old_start = 0;
    let mut old_lines = 0;
    let mut new_start = 0;
    let mut new_lines = 0;
    let parts = header.split_whitespace().collect::<Vec<_>>();
    for part in parts {
        if let Some(spec) = part.strip_prefix('-') {
            let (start, lines) = parse_hunk_range(spec);
            old_start = start;
            old_lines = lines;
        } else if let Some(spec) = part.strip_prefix('+') {
            let (start, lines) = parse_hunk_range(spec);
            new_start = start;
            new_lines = lines;
        }
    }
    (old_start, old_lines, new_start, new_lines)
}

fn parse_hunk_range(spec: &str) -> (usize, usize) {
    if let Some((start, lines)) = spec.split_once(',') {
        (
            start.parse::<usize>().unwrap_or(0),
            lines.parse::<usize>().unwrap_or(1),
        )
    } else {
        (spec.parse::<usize>().unwrap_or(0), 1)
    }
}

fn order_patch_diff_files(
    mut files: Vec<PatchDiffFile>,
    ordered_paths: &[String],
) -> Vec<PatchDiffFile> {
    let mut ordered = Vec::new();
    for path in ordered_paths {
        if let Some(index) = files.iter().position(|file| &file.path == path) {
            ordered.push(files.remove(index));
        }
    }
    ordered.extend(files);
    ordered
}

fn build_patch_diff_navigation(
    workflow_id: &str,
    task_id: &str,
    paths: &[String],
    selected_file_index: usize,
    selected_hunk_index: usize,
    context_lines: usize,
    files: &[PatchDiffFile],
) -> PatchDiffNavigation {
    let has_previous_file = selected_file_index > 0 && !files.is_empty();
    let has_next_file = selected_file_index + 1 < files.len();
    let selected_hunk_count = files
        .get(selected_file_index)
        .map(|file| file.hunks.len())
        .unwrap_or(0);
    let has_previous_hunk = selected_hunk_index > 0 && selected_hunk_count > 0;
    let has_next_hunk = selected_hunk_index + 1 < selected_hunk_count;

    PatchDiffNavigation {
        has_previous_file,
        has_next_file,
        has_previous_hunk,
        has_next_hunk,
        previous_file_command: has_previous_file.then(|| {
            patch_diff_navigation_command(
                workflow_id,
                task_id,
                paths,
                selected_file_index - 1,
                0,
                context_lines,
            )
        }),
        next_file_command: has_next_file.then(|| {
            patch_diff_navigation_command(
                workflow_id,
                task_id,
                paths,
                selected_file_index + 1,
                0,
                context_lines,
            )
        }),
        previous_hunk_command: has_previous_hunk.then(|| {
            patch_diff_navigation_command(
                workflow_id,
                task_id,
                paths,
                selected_file_index,
                selected_hunk_index - 1,
                context_lines,
            )
        }),
        next_hunk_command: has_next_hunk.then(|| {
            patch_diff_navigation_command(
                workflow_id,
                task_id,
                paths,
                selected_file_index,
                selected_hunk_index + 1,
                context_lines,
            )
        }),
    }
}

fn patch_diff_navigation_command(
    workflow_id: &str,
    task_id: &str,
    paths: &[String],
    file_index: usize,
    hunk_index: usize,
    context_lines: usize,
) -> String {
    let path_args = paths
        .iter()
        .map(|path| format!("--path {path}"))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "forge patch diff --workflow {workflow_id} --task {task_id} {path_args} --file-index {file_index} --hunk-index {hunk_index} --context-lines {context_lines} --output json"
    )
}

fn run_git_review_command(
    label: &str,
    git_args: &[&str],
    paths: &[String],
) -> PatchReviewCommandResult {
    let mut command = Command::new("git");
    command.args(git_args);
    command.arg("--");
    command.args(paths);
    match command.output() {
        Ok(output) => {
            let passed = output.status.success();
            PatchReviewCommandResult {
                command: format!("{label} -- {}", paths.join(" ")),
                status: if passed { "passed" } else { "failed" }.to_string(),
                exit_code: output.status.code(),
                stdout: bounded_command_text(&String::from_utf8_lossy(&output.stdout)),
                stderr: bounded_command_text(&String::from_utf8_lossy(&output.stderr)),
            }
        }
        Err(error) => PatchReviewCommandResult {
            command: format!("{label} -- {}", paths.join(" ")),
            status: "error".to_string(),
            exit_code: None,
            stdout: String::new(),
            stderr: format!("failed to execute git command: {error}"),
        },
    }
}

fn bounded_command_text(text: &str) -> String {
    const MAX_BYTES: usize = 12_000;
    if text.len() <= MAX_BYTES {
        return text.to_string();
    }
    let mut end = MAX_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...[truncated]", &text[..end])
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
    pub restore_executed: bool,
    pub requires_human_approval: bool,
    pub external_resources_mutated: bool,
    pub approval_command: Option<String>,
    pub apply_artifact: PatchApplyArtifactRef,
    pub restored_snapshots: Vec<PatchFileSnapshot>,
    pub validation: PatchValidationSummary,
    pub artifact: Option<PatchApplyArtifactRef>,
    pub safety_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PatchRestoreReport {
    pub schema_version: String,
    pub status: String,
    pub workflow_id: String,
    pub task_id: String,
    pub origin: String,
    pub restore_executed: bool,
    pub requires_human_approval: bool,
    pub approved_by: String,
    pub confirm_restore: bool,
    pub external_resources_mutated: bool,
    pub revert_artifact: PatchApplyArtifactRef,
    pub apply_artifact: PatchApplyArtifactRef,
    pub restored_paths: Vec<String>,
    pub pre_restore_snapshots: Vec<PatchFileSnapshot>,
    pub post_restore_snapshots: Vec<PatchFileSnapshot>,
    pub restore_command: PatchReviewCommandResult,
    pub validation: PatchValidationSummary,
    pub artifact: Option<PatchApplyArtifactRef>,
    pub safety_notes: Vec<String>,
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
    ensure_workflow_policy(store, workflow_id, "patch apply")?;
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
            "Use `forge patch revert` with this apply artifact to create a guarded rollback proposal."
                .to_string(),
            "A human must approve any destructive file restore outside this record-only apply step."
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

/// Record a guarded rollback proposal for a previously applied patch.
///
/// This does not restore files by itself. Forge records the apply artifact,
/// affected paths, approval command and safety notes so a human approval node
/// or future TUI diff review can decide whether to run a destructive restore.
pub fn build_patch_revert(
    store: &ForgeStore,
    workflow_id: &str,
    task_id: &str,
    apply_artifact_path: &str,
    origin: &str,
    _validation_commands: Option<&[String]>,
) -> Result<PatchRevertReport> {
    ensure_workflow_policy(store, workflow_id, "patch revert")?;
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

    let approval_command = format!("git checkout -- {}", paths.join(" "));
    let validation = PatchValidationSummary {
        passed: false,
        commands: Vec::new(),
    };

    let report = PatchRevertReport {
        schema_version: PATCH_REVERT_SCHEMA_VERSION.to_string(),
        status: "patch_revert_proposed".to_string(),
        workflow_id: workflow_id.to_string(),
        task_id: task_id.to_string(),
        origin: origin.to_string(),
        restore_executed: false,
        requires_human_approval: true,
        external_resources_mutated: false,
        approval_command: Some(approval_command),
        apply_artifact: apply_artifact_ref,
        restored_snapshots: Vec::new(),
        validation,
        artifact: None,
        safety_notes: vec![
            "Forge did not execute git checkout or restore files automatically.".to_string(),
            "Human approval is required before destructive rollback commands are run.".to_string(),
            format!(
                "If approved, run validation after restore: {}",
                DEFAULT_REVERT_VALIDATION_COMMANDS.join(" && ")
            ),
        ],
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

pub fn build_patch_restore(
    store: &ForgeStore,
    workflow_id: &str,
    task_id: &str,
    revert_artifact_path: &str,
    approved_by: &str,
    confirm_restore: bool,
    origin: &str,
) -> Result<PatchRestoreReport> {
    ensure_workflow_policy(store, workflow_id, "patch restore")?;
    let approved_by = approved_by.trim();
    if approved_by.is_empty() {
        bail!("--approved-by is required for patch restore");
    }
    if !confirm_restore {
        bail!("--confirm-restore is required before Forge restores files");
    }

    let workflow = store.load_workflow(workflow_id)?;
    if !workflow.tasks.iter().any(|task| task.id == task_id) {
        bail!("task {task_id} not found in workflow {workflow_id}");
    }

    let revert_bytes = fs::read(revert_artifact_path)
        .with_context(|| format!("failed to read revert artifact: {revert_artifact_path}"))?;
    let revert_artifact_ref = PatchApplyArtifactRef {
        kind: "patch_revert".to_string(),
        path: revert_artifact_path.to_string(),
        sha256: hex_sha256(&revert_bytes),
        bytes: revert_bytes.len() as u64,
    };
    let revert_report: serde_json::Value = serde_json::from_slice(&revert_bytes)?;
    ensure_artifact_matches_workflow_task(&revert_report, workflow_id, task_id, "revert")?;
    if revert_report["restore_executed"].as_bool().unwrap_or(false) {
        bail!("revert artifact already reports restore_executed=true");
    }

    let apply_artifact_path = revert_report["apply_artifact"]["path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("revert artifact is missing apply_artifact.path"))?;
    let apply_artifact_path = resolve_artifact_path(&store.base_dir(), apply_artifact_path);
    let apply_bytes = fs::read(&apply_artifact_path).with_context(|| {
        format!(
            "failed to read apply artifact: {}",
            apply_artifact_path.display()
        )
    })?;
    let apply_artifact_ref = PatchApplyArtifactRef {
        kind: "patch_apply".to_string(),
        path: apply_artifact_path.display().to_string(),
        sha256: hex_sha256(&apply_bytes),
        bytes: apply_bytes.len() as u64,
    };
    let apply_report: serde_json::Value = serde_json::from_slice(&apply_bytes)?;
    ensure_artifact_matches_workflow_task(&apply_report, workflow_id, task_id, "apply")?;

    let mut paths: Vec<String> = apply_report["file_snapshots"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|value| value["path"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    paths.retain(|path| is_repo_relative_path(path));
    paths.sort();
    paths.dedup();
    if paths.is_empty() {
        bail!("apply artifact contains no repo-relative file paths to restore");
    }

    let pre_restore_snapshots = paths
        .iter()
        .map(|path| snapshot_file(path))
        .collect::<Result<Vec<_>>>()?;
    let restore_command = run_git_review_command("git checkout", &["checkout"], &paths);
    let post_restore_snapshots = paths
        .iter()
        .map(|path| snapshot_file(path))
        .collect::<Result<Vec<_>>>()?;
    let restore_executed = restore_command.exit_code == Some(0);
    let status = if restore_executed {
        "patch_restored"
    } else {
        "patch_restore_failed"
    };
    let validation = PatchValidationSummary {
        passed: restore_executed,
        commands: Vec::new(),
    };

    let mut report = PatchRestoreReport {
        schema_version: PATCH_RESTORE_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        workflow_id: workflow_id.to_string(),
        task_id: task_id.to_string(),
        origin: origin.to_string(),
        restore_executed,
        requires_human_approval: true,
        approved_by: approved_by.to_string(),
        confirm_restore,
        external_resources_mutated: false,
        revert_artifact: revert_artifact_ref,
        apply_artifact: apply_artifact_ref,
        restored_paths: paths,
        pre_restore_snapshots,
        post_restore_snapshots,
        restore_command,
        validation,
        artifact: None,
        safety_notes: vec![
            "Forge executed an approved repo-local restore using git checkout with separated path arguments.".to_string(),
            "The restore was allowed only because --confirm-restore and --approved-by were present.".to_string(),
            "External resources, Docker, Kubernetes, Knative and device interfaces were not touched.".to_string(),
        ],
    };

    let payload = serde_json::to_value(&report)?;
    let relative_path = format!("tmp/{workflow_id}-{task_id}-patch-restore.json");
    let (path, _) = write_json_artifact(&store.base_dir(), &relative_path, &payload)?;
    let attached = attach_workflow_artifact(store, workflow_id, &path, "patch_restore", origin)?;
    report.artifact = Some(PatchApplyArtifactRef::from_artifact(&attached));

    Ok(report)
}

fn ensure_artifact_matches_workflow_task(
    artifact: &serde_json::Value,
    workflow_id: &str,
    task_id: &str,
    artifact_kind: &str,
) -> Result<()> {
    if artifact["workflow_id"].as_str() != Some(workflow_id) {
        bail!("{artifact_kind} artifact workflow_id does not match {workflow_id}");
    }
    if artifact["task_id"].as_str() != Some(task_id) {
        bail!("{artifact_kind} artifact task_id does not match {task_id}");
    }
    Ok(())
}

fn resolve_artifact_path(base_dir: &Path, artifact_path: &str) -> PathBuf {
    let path = PathBuf::from(artifact_path);
    if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    }
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
