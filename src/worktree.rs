use crate::artifact::hex_sha256;
use crate::graph::{task, ExecutorKind, TaskStatus, ValidationRule, WorkflowRevision};
use crate::identity::ensure_workflow_policy;
use crate::security::{sanitize_prompt_secrets, SecretSanitizationOptions};
use crate::storage::FoundryStore;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
#[cfg(windows)]
use std::{
    ffi::OsString,
    os::windows::ffi::{OsStrExt, OsStringExt},
};

pub const WORKTREE_CONFIG_SCHEMA_VERSION: &str = "foundry.worktree.config.v1";
pub const WORKTREE_RECORD_SCHEMA_VERSION: &str = "foundry.worktree.record.v1";
pub const WORKTREE_BINDING_SCHEMA_VERSION: &str = "foundry.worktree.binding.v1";
pub const WORKTREE_MUTATION_CLAIM_SCHEMA_VERSION: &str = "foundry.worktree.mutation_claim.v1";
pub const WORKTREE_DISCOVERY_SCHEMA_VERSION: &str = "foundry.worktree.discovery.v1";
pub const WORKTREE_SANDBOX_PLAN_SCHEMA_VERSION: &str = "foundry.worktree.sandbox_plan.v1";
pub const WORKTREE_SANDBOX_RECEIPT_SCHEMA_VERSION: &str = "foundry.worktree.sandbox_receipt.v1";
pub const WORKTREE_SANDBOX_LIFECYCLE_SCHEMA_VERSION: &str = "foundry.worktree.sandbox_lifecycle.v1";
pub const WORKTREE_MODIFICATION_GUARD_SCHEMA_VERSION: &str =
    "foundry.worktree.modification_guard.v1";
pub const WORKTREE_PREDECESSOR_TASK_SCHEMA_VERSION: &str = "foundry.worktree.predecessor_task.v1";

const DEFAULT_CONFIG_PATH: &str = ".foundry/worktree.toml";
const DEFAULT_SANDBOX_ROOT: &str = ".foundry/sandboxes/internal";
const BUBBLEWRAP_WORKTREE_ROOT: &str = "/workspace";
const BUBBLEWRAP_HOME: &str = "/home/foundry";
const MAX_SANDBOX_COMMAND_SECONDS: u64 = 3_600;
const MAX_SANDBOX_OUTPUT_BYTES: usize = 16 * 1_048_576;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeConfig {
    #[serde(default = "worktree_config_schema_version")]
    pub schema_version: String,
    #[serde(default)]
    pub guardrails: WorktreeGuardrails,
    #[serde(default)]
    pub sandbox: WorktreeSandboxConfig,
    #[serde(default)]
    pub settings: BTreeMap<String, toml::Value>,
}

impl Default for WorktreeConfig {
    fn default() -> Self {
        Self {
            schema_version: worktree_config_schema_version(),
            guardrails: WorktreeGuardrails::default(),
            sandbox: WorktreeSandboxConfig::default(),
            settings: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeGuardrails {
    #[serde(default)]
    pub require_clean: bool,
    #[serde(default)]
    pub allow_detached_head: bool,
    #[serde(default)]
    pub allowed_branches: Vec<String>,
    #[serde(default)]
    pub allowed_commands: Vec<String>,
    #[serde(default = "default_modifiable_paths")]
    pub modifiable_paths: Vec<String>,
    #[serde(default = "default_protected_paths")]
    pub protected_paths: Vec<String>,
    #[serde(default = "default_require_workflow_binding")]
    pub require_workflow_binding: bool,
    #[serde(default = "default_max_command_seconds")]
    pub max_command_seconds: u64,
    #[serde(default = "default_max_output_bytes")]
    pub max_output_bytes: usize,
}

impl Default for WorktreeGuardrails {
    fn default() -> Self {
        Self {
            require_clean: false,
            allow_detached_head: false,
            allowed_branches: Vec::new(),
            allowed_commands: Vec::new(),
            modifiable_paths: default_modifiable_paths(),
            protected_paths: default_protected_paths(),
            require_workflow_binding: true,
            max_command_seconds: default_max_command_seconds(),
            max_output_bytes: default_max_output_bytes(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeSandboxConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_sandbox_name")]
    pub name: String,
    #[serde(default = "default_sandbox_root")]
    pub root: String,
    #[serde(default = "default_sandbox_runtime")]
    pub runtime: String,
    #[serde(default = "default_working_directory")]
    pub working_directory: String,
    #[serde(default = "default_sandbox_purposes")]
    pub purposes: Vec<String>,
    #[serde(default = "default_network_policy")]
    pub network: String,
    #[serde(default)]
    pub commands: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    #[serde(default = "default_inherited_environment")]
    pub inherit_environment: Vec<String>,
}

impl Default for WorktreeSandboxConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            name: default_sandbox_name(),
            root: default_sandbox_root(),
            runtime: default_sandbox_runtime(),
            working_directory: default_working_directory(),
            purposes: default_sandbox_purposes(),
            network: default_network_policy(),
            commands: BTreeMap::new(),
            environment: BTreeMap::new(),
            inherit_environment: default_inherited_environment(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeConfigSnapshot {
    pub status: String,
    pub path: String,
    pub sha256: String,
    pub config: WorktreeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeBinding {
    #[serde(default = "worktree_binding_schema_version")]
    pub schema_version: String,
    pub workflow_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub origin: String,
    pub workflow_revision: u64,
    #[serde(default)]
    pub worktree_identity_sha256: String,
    #[serde(default)]
    pub head_at_binding: String,
    #[serde(default)]
    pub config_sha256_at_binding: String,
    pub bound_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorktreeMutationClaim {
    pub schema_version: String,
    pub mode: String,
    pub worktree_id: String,
    pub worktree_identity_sha256: String,
    pub repository_root: String,
    pub worktree_root: String,
    pub binding_scope: String,
    pub binding_workflow_revision: u64,
    pub head: String,
    pub config_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeRecord {
    #[serde(default = "worktree_record_schema_version")]
    pub schema_version: String,
    pub id: String,
    pub repository_root: String,
    pub worktree_root: String,
    pub git_common_dir: String,
    pub git_dir: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub head: String,
    pub detached: bool,
    pub dirty: bool,
    pub changed_path_count: usize,
    pub is_main_worktree: bool,
    #[serde(alias = "created_by_forge")] // foundry-brand-allow: legacy-compat
    pub created_by_foundry: bool,
    #[serde(default)]
    pub identity_sha256: String,
    pub config: WorktreeConfigSnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_config_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_approved_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_approved_at: Option<String>,
    #[serde(default)]
    pub bindings: Vec<WorktreeBinding>,
    pub registered_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredWorktree {
    pub path: String,
    pub head: String,
    pub branch: Option<String>,
    pub detached: bool,
    pub bare: bool,
    pub locked: bool,
    pub prunable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorktreeDiscoveryReport {
    pub schema_version: String,
    pub repository_root: String,
    pub git_common_dir: String,
    pub worktree_count: usize,
    pub worktrees: Vec<DiscoveredWorktree>,
}

#[derive(Debug, Clone)]
pub struct WorktreeRegisterOptions {
    pub path: PathBuf,
    pub id: Option<String>,
    pub workflow_id: Option<String>,
    pub task_id: Option<String>,
    pub origin: String,
    pub created_by_foundry: bool,
}

#[derive(Debug, Clone)]
pub struct WorktreeCreateOptions {
    pub repository: PathBuf,
    pub path: PathBuf,
    pub branch: String,
    pub start_point: Option<String>,
    pub allow_repository_mutation: bool,
    pub origin: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorktreeMutationReport {
    pub status: String,
    pub worktree: WorktreeRecord,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding: Option<WorktreeBinding>,
    pub repository_mutated: bool,
    pub worktree_files_written: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorktreeListReport {
    pub schema_version: String,
    pub count: usize,
    pub worktrees: Vec<WorktreeRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorktreeContextReport {
    pub schema_version: String,
    pub id: String,
    pub identity_sha256: String,
    pub repository_root: String,
    pub worktree_root: String,
    pub branch: Option<String>,
    pub head: String,
    pub dirty: bool,
    pub config_status: String,
    pub config_path: String,
    pub config_sha256: String,
    pub config_approved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_config_sha256: Option<String>,
    pub guardrails: WorktreeGuardrails,
    pub sandbox: WorktreeSandboxConfig,
    pub settings: BTreeMap<String, toml::Value>,
    pub bindings: Vec<WorktreeBinding>,
    pub binding_drifted: bool,
    pub binding_drift_reasons: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct WorktreeModificationGuardRequest {
    pub worktree: String,
    pub operation: String,
    pub paths: Vec<String>,
    pub reason: String,
    pub workflow_id: Option<String>,
    pub task_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorktreeModificationGuardReport {
    pub schema_version: String,
    pub status: String,
    pub allowed: bool,
    pub operation: String,
    pub worktree_id: String,
    pub worktree_root: String,
    pub workflow_id: Option<String>,
    pub task_id: Option<String>,
    pub reason: String,
    pub current_task_action: String,
    pub decisions: Vec<WorktreePathDecision>,
    pub blocked_paths: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_task_spec: Option<WorktreeRequiredTaskSpec>,
    pub next_command: Vec<String>,
    pub next_commands: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorktreePathDecision {
    pub path: String,
    pub scope_kind: String,
    pub decision: String,
    pub delegable_to_predecessor: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_modifiable_scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_protected_scope: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorktreeRequiredTaskSpec {
    pub title: String,
    pub goal: String,
    pub goal_template: String,
    pub paths: Vec<String>,
    pub blocked_paths: Vec<String>,
    pub dependency_direction: String,
    pub required_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorktreePredecessorTaskReport {
    pub schema_version: String,
    pub status: String,
    pub workflow_id: String,
    pub current_task_id: String,
    pub predecessor_task_id: String,
    pub blocked_paths: Vec<String>,
    pub current_task_action: String,
    pub dependency_added: bool,
    pub workflow_revision: u64,
    pub approved_by: String,
    pub origin: String,
}

#[derive(Debug, Clone)]
pub struct WorktreeSandboxRequest {
    pub worktree: String,
    pub purpose: String,
    pub workflow_id: Option<String>,
    pub task_id: Option<String>,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeSandboxPlan {
    pub schema_version: String,
    pub status: String,
    pub allowed: bool,
    pub worktree_id: String,
    pub worktree_root: String,
    pub repository_root: String,
    pub branch: Option<String>,
    pub head: String,
    pub dirty: bool,
    pub purpose: String,
    pub sandbox_name: String,
    pub sandbox_root: String,
    pub working_directory: String,
    pub runtime_worktree_root: String,
    pub runtime_sandbox_root: String,
    pub runtime_working_directory: String,
    pub runtime: String,
    pub network_policy: String,
    pub filesystem_isolation_enforced: bool,
    pub network_isolation_enforced: bool,
    pub command: Vec<String>,
    pub launch_command: Vec<String>,
    pub inherited_environment: Vec<String>,
    pub configured_environment_keys: Vec<String>,
    pub config_sha256: String,
    #[serde(alias = "forge_store_path")] // foundry-brand-allow: legacy-compat
    pub foundry_store_path: String,
    #[serde(alias = "forge_store_path_mounted")] // foundry-brand-allow: legacy-compat
    pub foundry_store_path_mounted: bool,
    pub max_command_seconds: u64,
    pub max_output_bytes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding: Option<WorktreeBinding>,
    pub guardrail_decisions: Vec<WorktreeGuardrailDecision>,
    pub blockers: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeGuardrailDecision {
    pub id: String,
    pub decision: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorktreeSandboxReceipt {
    pub schema_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox_id: Option<String>,
    pub status: String,
    pub allowed: bool,
    pub execution_attempted: bool,
    pub executed: bool,
    pub worktree_id: String,
    pub purpose: String,
    pub runtime: String,
    pub command_sha256: String,
    pub config_sha256: String,
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: u128,
    pub timed_out: bool,
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub stdout: BoundedStreamEvidence,
    pub stderr: BoundedStreamEvidence,
    pub sandbox_root: String,
    pub working_directory: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding: Option<WorktreeBinding>,
    pub plan: WorktreeSandboxPlan,
}

#[derive(Debug, Clone, Serialize)]
pub struct BoundedStreamEvidence {
    pub sha256: String,
    pub total_bytes: usize,
    pub captured_bytes: usize,
    pub truncated: bool,
    pub redaction_count: usize,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeSandboxLifecycleReport {
    pub schema_version: String,
    pub sandbox_id: String,
    pub status: String,
    pub worktree_id: String,
    pub worktree_root: String,
    pub purpose: String,
    pub workflow_id: Option<String>,
    pub task_id: Option<String>,
    pub command_sha256: String,
    pub config_sha256: String,
    pub supervisor_pid: Option<u32>,
    pub payload_pid: Option<u32>,
    #[serde(default)]
    pub payload_descendant_pids: Vec<u32>,
    pub created_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
    pub stop_requested_at: Option<String>,
    pub receipt_status: Option<String>,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub error: Option<String>,
    pub plan: WorktreeSandboxPlan,
}

struct GitWorktreeState {
    repository_root: PathBuf,
    worktree_root: PathBuf,
    git_common_dir: PathBuf,
    git_dir: PathBuf,
    branch: Option<String>,
    head: String,
    dirty: bool,
    changed_path_count: usize,
    is_main_worktree: bool,
}

pub fn discover_worktrees(repository: &Path) -> Result<WorktreeDiscoveryReport> {
    let state = inspect_git_worktree(repository)?;
    let output = git_output(&state.worktree_root, &["worktree", "list", "--porcelain"])?;
    let worktrees = parse_worktree_porcelain(&output);
    Ok(WorktreeDiscoveryReport {
        schema_version: WORKTREE_DISCOVERY_SCHEMA_VERSION.to_string(),
        repository_root: state.repository_root.display().to_string(),
        git_common_dir: state.git_common_dir.display().to_string(),
        worktree_count: worktrees.len(),
        worktrees,
    })
}

pub fn create_worktree(
    store: &FoundryStore,
    options: WorktreeCreateOptions,
) -> Result<WorktreeMutationReport> {
    if !options.allow_repository_mutation {
        bail!("creating a Git worktree requires --allow-repository-mutation");
    }
    if options.branch.trim().is_empty() || options.branch.starts_with('-') {
        bail!("worktree branch must be a non-empty Git branch name");
    }
    if let Some(start_point) = options.start_point.as_deref() {
        if start_point.trim().is_empty() || start_point.starts_with('-') {
            bail!("worktree start point must be a non-empty Git revision");
        }
    }
    let repository = inspect_git_worktree(&options.repository)?.worktree_root;
    ensure_valid_branch_name(&repository, &options.branch)?;
    let destination = absolute_path(&options.path)?;
    if destination.exists() {
        bail!(
            "worktree destination already exists: {}",
            destination.display()
        );
    }
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(&repository)
        .arg("worktree")
        .arg("add")
        .arg("-b")
        .arg(&options.branch)
        .arg(&destination);
    if let Some(start_point) = options.start_point.as_deref() {
        command.arg(start_point);
    }
    let output = command
        .output()
        .context("failed to invoke git worktree add")?;
    if !output.status.success() {
        bail!(
            "git worktree add failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let report = register_worktree(
        store,
        WorktreeRegisterOptions {
            path: destination,
            id: None,
            workflow_id: None,
            task_id: None,
            origin: options.origin,
            created_by_foundry: true,
        },
    )?;
    Ok(WorktreeMutationReport {
        status: "worktree_created".to_string(),
        worktree: report.worktree,
        binding: report.binding,
        repository_mutated: true,
        worktree_files_written: true,
    })
}

pub fn register_worktree(
    store: &FoundryStore,
    options: WorktreeRegisterOptions,
) -> Result<WorktreeMutationReport> {
    if options.task_id.is_some() && options.workflow_id.is_none() {
        bail!("task binding requires a workflow id when registering a worktree");
    }
    let state = inspect_git_worktree(&options.path)?;
    let existing = find_worktree_record_by_path(store, &state.worktree_root)?;
    let now = Utc::now().to_rfc3339();
    let id = existing
        .as_ref()
        .map(|record| record.id.clone())
        .or(options.id)
        .unwrap_or_else(|| stable_worktree_id(&state.worktree_root));
    validate_worktree_id(&id)?;
    if let Some(conflict) = load_all_worktree_records(store)?
        .into_iter()
        .find(|record| {
            record.id == id && !paths_equal(Path::new(&record.worktree_root), &state.worktree_root)
        })
    {
        bail!(
            "worktree id `{id}` is already registered for {}; refusing path replacement with {}",
            conflict.worktree_root,
            state.worktree_root.display()
        );
    }
    let bindings = existing
        .as_ref()
        .map(|record| record.bindings.clone())
        .unwrap_or_default();
    let registered_at = existing
        .as_ref()
        .map(|record| record.registered_at.clone())
        .unwrap_or_else(|| now.clone());
    let created_by_foundry = existing
        .as_ref()
        .map(|record| record.created_by_foundry)
        .unwrap_or(false)
        || options.created_by_foundry;
    let mut record =
        record_from_state(id, state, created_by_foundry, bindings, registered_at, now)?;
    if let Some(existing) = existing {
        if !existing.identity_sha256.is_empty()
            && existing.identity_sha256 != record.identity_sha256
        {
            bail!(
                "registered worktree identity changed at {}; re-register it explicitly with a new id",
                record.worktree_root
            );
        }
        record.approved_config_sha256 = existing.approved_config_sha256;
        record.config_approved_by = existing.config_approved_by;
        record.config_approved_at = existing.config_approved_at;
    }
    save_worktree_record(store, &record)?;
    store.record_event(
        "_system",
        "worktree_registered",
        &serde_json::to_value(&record)?,
    )?;

    let mut report = WorktreeMutationReport {
        status: "worktree_registered".to_string(),
        worktree: record,
        binding: None,
        repository_mutated: false,
        worktree_files_written: false,
    };
    if let Some(workflow_id) = options.workflow_id {
        report = bind_worktree(
            store,
            &report.worktree.id,
            &workflow_id,
            options.task_id.as_deref(),
            &options.origin,
        )?;
    }
    Ok(report)
}

pub fn bind_worktree(
    store: &FoundryStore,
    selector: &str,
    workflow_id: &str,
    task_id: Option<&str>,
    origin: &str,
) -> Result<WorktreeMutationReport> {
    let mut record = refresh_worktree_record(&load_worktree_record(store, selector)?)?;
    let mut workflow = store.load_workflow(workflow_id)?;
    if let Some(task_id) = task_id {
        if !workflow.tasks.iter().any(|task| task.id == task_id) {
            bail!("task not found in workflow {workflow_id}: {task_id}");
        }
    }
    let revision = workflow
        .revisions
        .last()
        .map(|revision| revision.revision + 1)
        .unwrap_or(1);
    let binding = WorktreeBinding {
        schema_version: worktree_binding_schema_version(),
        workflow_id: workflow_id.to_string(),
        task_id: task_id.map(str::to_string),
        origin: origin.to_string(),
        workflow_revision: revision,
        worktree_identity_sha256: record.identity_sha256.clone(),
        head_at_binding: record.head.clone(),
        config_sha256_at_binding: record.config.sha256.clone(),
        bound_at: Utc::now().to_rfc3339(),
    };
    record.bindings.retain(|candidate| {
        candidate.workflow_id != workflow_id || candidate.task_id.as_deref() != task_id
    });
    record.bindings.push(binding.clone());
    record.updated_at = Utc::now().to_rfc3339();
    workflow.revisions.push(WorkflowRevision {
        revision,
        origin: origin.to_string(),
        change_type: "worktree_binding_update".to_string(),
        summary: format!(
            "bound {} to worktree {}{}",
            workflow_id,
            record.id,
            task_id
                .map(|task| format!(" for task {task}"))
                .unwrap_or_default()
        ),
        created_at: Utc::now(),
    });
    store.with_transaction(|| {
        remove_scope_binding_from_other_worktrees(store, &record.id, workflow_id, task_id)?;
        save_worktree_record(store, &record)?;
        store.save_workflow(&workflow)?;
        store.record_event(
            workflow_id,
            "worktree_bound",
            &serde_json::json!({
                "worktree": record,
                "binding": binding,
            }),
        )?;
        Ok(())
    })?;
    Ok(WorktreeMutationReport {
        status: "worktree_bound".to_string(),
        worktree: record,
        binding: Some(binding),
        repository_mutated: false,
        worktree_files_written: false,
    })
}

pub fn initialize_worktree(
    store: &FoundryStore,
    selector: &str,
    allow_worktree_write: bool,
    force: bool,
    origin: &str,
) -> Result<WorktreeMutationReport> {
    if !allow_worktree_write {
        bail!("initializing worktree configuration requires --allow-worktree-write");
    }
    let mut record = load_worktree_record(store, selector)?;
    let root = fs::canonicalize(&record.worktree_root)
        .with_context(|| format!("failed to resolve {}", record.worktree_root))?;
    let current_state = inspect_git_worktree(&root)?;
    if worktree_identity_sha256(&current_state) != record.identity_sha256
        && !record.identity_sha256.is_empty()
    {
        bail!("worktree identity changed before initialization");
    }
    let config_path = resolve_relative_inside(&root, DEFAULT_CONFIG_PATH)?;
    if config_path.exists() && !force {
        bail!(
            "worktree configuration already exists: {}; use --force to replace it",
            config_path.display()
        );
    }
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let config = initial_worktree_config();
    let content = toml::to_string_pretty(&config)?;
    fs::write(&config_path, content)
        .with_context(|| format!("failed to write {}", config_path.display()))?;
    let sandbox_root = resolve_relative_inside(&root, DEFAULT_SANDBOX_ROOT)?;
    for directory in ["artifacts", "cache", "tmp", "home"] {
        fs::create_dir_all(sandbox_root.join(directory))?;
    }
    fs::write(sandbox_root.join(".gitignore"), "*\n!.gitignore\n")?;
    record = refresh_worktree_record(&record)?;
    record.approved_config_sha256 = Some(record.config.sha256.clone());
    record.config_approved_by = Some(origin.to_string());
    record.config_approved_at = Some(Utc::now().to_rfc3339());
    save_worktree_record(store, &record)?;
    store.record_event(
        "_system",
        "worktree_initialized",
        &serde_json::json!({
            "worktree_id": record.id,
            "origin": origin,
            "config_path": config_path,
            "sandbox_root": sandbox_root,
        }),
    )?;
    Ok(WorktreeMutationReport {
        status: "worktree_initialized".to_string(),
        worktree: record,
        binding: None,
        repository_mutated: false,
        worktree_files_written: true,
    })
}

pub fn approve_worktree_config(
    store: &FoundryStore,
    selector: &str,
    allow_guardrail_update: bool,
    approved_by: &str,
    origin: &str,
) -> Result<WorktreeMutationReport> {
    if !allow_guardrail_update {
        bail!("approving worktree configuration requires --allow-guardrail-update");
    }
    if approved_by.trim().is_empty() {
        bail!("approving worktree configuration requires --approved-by");
    }
    let mut record = refresh_worktree_record(&load_worktree_record(store, selector)?)?;
    if record.config.status != "configured" {
        bail!(
            "worktree configuration is {}; initialize it before approval",
            record.config.status
        );
    }
    record.approved_config_sha256 = Some(record.config.sha256.clone());
    record.config_approved_by = Some(approved_by.trim().to_string());
    record.config_approved_at = Some(Utc::now().to_rfc3339());
    save_worktree_record(store, &record)?;
    store.record_event(
        "_system",
        "worktree_config_approved",
        &serde_json::json!({
            "worktree_id": record.id,
            "config_sha256": record.config.sha256,
            "approved_by": approved_by.trim(),
            "origin": origin,
        }),
    )?;
    Ok(WorktreeMutationReport {
        status: "worktree_config_approved".to_string(),
        worktree: record,
        binding: None,
        repository_mutated: false,
        worktree_files_written: false,
    })
}

pub fn evaluate_worktree_modification_guard(
    store: &FoundryStore,
    request: WorktreeModificationGuardRequest,
) -> Result<WorktreeModificationGuardReport> {
    let operation = request.operation.trim().to_lowercase();
    if operation != "modify" {
        bail!("unsupported worktree guard operation `{operation}`; expected `modify`");
    }
    if request.paths.is_empty() {
        bail!("at least one --path is required for a worktree modification guard check");
    }
    if request.reason.trim().is_empty() {
        bail!("a concrete --reason is required for a worktree modification guard check");
    }
    if request.task_id.is_some() && request.workflow_id.is_none() {
        bail!("a task-scoped modification guard check requires --workflow");
    }

    let record = inspect_registered_worktree(store, &request.worktree)?;
    let config_snapshot = load_worktree_config(Path::new(&record.worktree_root))?;
    let config_approved =
        record.approved_config_sha256.as_deref() == Some(config_snapshot.sha256.as_str());
    let binding_matches = if let Some(workflow_id) = request.workflow_id.as_deref() {
        resolve_bound_worktree(store, workflow_id, request.task_id.as_deref())?
            .is_some_and(|bound| bound.id == record.id)
    } else {
        true
    };
    let root = Path::new(&record.worktree_root);
    let mut decisions = Vec::new();
    for requested in &request.paths {
        let requested_scope = normalize_guard_path(requested);
        let decision = match requested_scope {
            Err(error) => WorktreePathDecision {
                path: requested.trim().to_string(),
                scope_kind: "invalid".to_string(),
                decision: "blocked".to_string(),
                delegable_to_predecessor: false,
                matched_modifiable_scope: None,
                matched_protected_scope: None,
                reason: error.to_string(),
            },
            Ok(normalized) => {
                let scope_kind = guard_scope_kind(root, requested, &normalized);
                let modifiable = config_snapshot
                    .config
                    .guardrails
                    .modifiable_paths
                    .iter()
                    .find(|scope| guard_scope_allows(scope, &normalized, &scope_kind))
                    .cloned();
                let protected = config_snapshot
                    .config
                    .guardrails
                    .protected_paths
                    .iter()
                    .find(|scope| guard_scopes_overlap(scope, &normalized, &scope_kind))
                    .cloned();
                let scoped_path_safe = resolve_relative_inside(root, &normalized).is_ok();
                let allowed = config_approved
                    && binding_matches
                    && scoped_path_safe
                    && modifiable.is_some()
                    && protected.is_none();
                let reason = if !config_approved {
                    "the current worktree manifest hash is not approved".to_string()
                } else if !binding_matches {
                    "the requested workflow/task is bound to a different worktree".to_string()
                } else if !scoped_path_safe {
                    "the path is not safely contained below the worktree or traverses a symlink"
                        .to_string()
                } else if let Some(scope) = protected.as_deref() {
                    format!("protected scope `{scope}` takes precedence")
                } else if modifiable.is_none() {
                    "no modifiable path scope authorizes this path".to_string()
                } else {
                    "path is covered by an approved modifiable scope".to_string()
                };
                WorktreePathDecision {
                    path: normalized,
                    scope_kind,
                    decision: if allowed { "allowed" } else { "blocked" }.to_string(),
                    delegable_to_predecessor: config_approved
                        && binding_matches
                        && scoped_path_safe
                        && (protected.is_some() || modifiable.is_none()),
                    matched_modifiable_scope: modifiable,
                    matched_protected_scope: protected,
                    reason,
                }
            }
        };
        decisions.push(decision);
    }
    let blocked_paths = decisions
        .iter()
        .filter(|decision| decision.decision == "blocked")
        .map(|decision| decision.path.clone())
        .collect::<Vec<_>>();
    let allowed = blocked_paths.is_empty();
    let delegated_paths = decisions
        .iter()
        .filter(|decision| decision.delegable_to_predecessor)
        .map(|decision| decision.path.clone())
        .collect::<Vec<_>>();
    let required_task_spec = (!delegated_paths.is_empty()).then(|| WorktreeRequiredTaskSpec {
        title: format!(
            "Handle protected worktree scope{}",
            if delegated_paths.len() == 1 { "" } else { "s" }
        ),
        goal: format!(
            "Create an objective task that explains exactly why and how to change {} for this outcome: {}",
            delegated_paths.join(", "),
            request.reason.trim()
        ),
        goal_template: format!(
            "Modify {} to achieve: {}; include measurable validation evidence",
            delegated_paths.join(", "),
            request.reason.trim()
        ),
        paths: delegated_paths.clone(),
        blocked_paths: delegated_paths.clone(),
        dependency_direction: "current_task_depends_on_predecessor_task".to_string(),
        required_fields: vec![
            "goal".to_string(),
            "blocked_paths".to_string(),
            "validation_evidence".to_string(),
            "approved_by".to_string(),
        ],
    });
    let mut next_commands = Vec::new();
    if !config_approved {
        next_commands.push(vec![
            "foundry".to_string(),
            "--store".to_string(),
            absolute_store_path(store).display().to_string(),
            "worktree".to_string(),
            "approve-config".to_string(),
            "--worktree".to_string(),
            record.id.clone(),
            "--allow-guardrail-update".to_string(),
            "--approved-by".to_string(),
            "<approver>".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ]);
    } else if !delegated_paths.is_empty() {
        let mut command = vec![
            "foundry".to_string(),
            "--store".to_string(),
            absolute_store_path(store).display().to_string(),
            "worktree".to_string(),
            "guard".to_string(),
            "create-predecessor".to_string(),
            "--worktree".to_string(),
            record.id.clone(),
            "--workflow".to_string(),
            request
                .workflow_id
                .clone()
                .unwrap_or_else(|| "<workflow-id>".to_string()),
            "--task".to_string(),
            request
                .task_id
                .clone()
                .unwrap_or_else(|| "<current-task-id>".to_string()),
        ];
        for path in &delegated_paths {
            command.extend(["--path".to_string(), path.clone()]);
        }
        command.extend([
            "--goal".to_string(),
            "<objective, path-specific goal with validation evidence>".to_string(),
            "--allow-workflow-mutation".to_string(),
            "--approved-by".to_string(),
            "<approver>".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ]);
        next_commands.push(command);
    }
    let next_command = next_commands.first().cloned().unwrap_or_default();
    Ok(WorktreeModificationGuardReport {
        schema_version: WORKTREE_MODIFICATION_GUARD_SCHEMA_VERSION.to_string(),
        status: if allowed {
            "modification_allowed".to_string()
        } else {
            "modification_blocked".to_string()
        },
        allowed,
        operation,
        worktree_id: record.id,
        worktree_root: record.worktree_root,
        workflow_id: request.workflow_id,
        task_id: request.task_id,
        reason: request.reason.trim().to_string(),
        current_task_action: if allowed { "continue" } else { "blocked" }.to_string(),
        decisions,
        blocked_paths,
        required_task_spec,
        next_command,
        next_commands,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn create_worktree_guard_predecessor_task(
    store: &FoundryStore,
    worktree: &str,
    workflow_id: &str,
    current_task_id: &str,
    paths: Vec<String>,
    goal: &str,
    allow_workflow_mutation: bool,
    approved_by: &str,
    origin: &str,
) -> Result<WorktreePredecessorTaskReport> {
    if !allow_workflow_mutation {
        bail!("creating a predecessor task requires --allow-workflow-mutation");
    }
    if approved_by.trim().is_empty() {
        bail!("creating a predecessor task requires --approved-by");
    }
    let goal = goal.trim();
    if goal.len() < 20 {
        bail!("predecessor task goal must be objective and descriptive (at least 20 characters)");
    }
    ensure_workflow_policy(store, workflow_id, "worktree guard predecessor task")?;
    let guard = evaluate_worktree_modification_guard(
        store,
        WorktreeModificationGuardRequest {
            worktree: worktree.to_string(),
            operation: "modify".to_string(),
            paths,
            reason: goal.to_string(),
            workflow_id: Some(workflow_id.to_string()),
            task_id: Some(current_task_id.to_string()),
        },
    )?;
    if guard.allowed {
        bail!("all requested paths are already modifiable; no predecessor task is required");
    }
    let has_non_delegable_denial = guard
        .decisions
        .iter()
        .any(|decision| decision.decision == "blocked" && !decision.delegable_to_predecessor);
    if guard.required_task_spec.is_none() || has_non_delegable_denial {
        bail!(
            "every guard denial must be delegable before creating a predecessor task; follow the returned remediation command"
        );
    }
    if !guard.blocked_paths.iter().any(|path| {
        goal.contains(path)
            || Path::new(path)
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|name| goal.contains(name))
    }) {
        bail!("predecessor task goal must name at least one blocked file or directory");
    }

    let mut workflow = store.load_workflow(workflow_id)?;
    let current_index = workflow
        .tasks
        .iter()
        .position(|task| task.id == current_task_id)
        .with_context(|| format!("task not found in workflow {workflow_id}: {current_task_id}"))?;
    if workflow.tasks[current_index].status == TaskStatus::Completed {
        bail!("cannot block completed task {current_task_id} with a new predecessor");
    }
    let mut blocked_paths = guard.blocked_paths.clone();
    blocked_paths.sort();
    blocked_paths.dedup();
    let path_summary = blocked_paths.join(", ");
    let worktree_marker = format!("worktree_id={}", guard.worktree_id);
    let delegated_paths_marker = format!("delegated_protected_paths={path_summary}");
    if let Some(existing) = workflow.tasks.iter().find(|candidate| {
        candidate.status != TaskStatus::Completed
            && candidate.goal == goal
            && workflow.tasks[current_index]
                .dependencies
                .contains(&candidate.id)
            && candidate
                .context_requirements
                .iter()
                .any(|requirement| requirement == &worktree_marker)
            && candidate
                .context_requirements
                .iter()
                .any(|requirement| requirement == &delegated_paths_marker)
    }) {
        return Ok(WorktreePredecessorTaskReport {
            schema_version: WORKTREE_PREDECESSOR_TASK_SCHEMA_VERSION.to_string(),
            status: "worktree_guard_predecessor_reused".to_string(),
            workflow_id: workflow_id.to_string(),
            current_task_id: current_task_id.to_string(),
            predecessor_task_id: existing.id.clone(),
            blocked_paths,
            current_task_action: "blocked_by_predecessor_dependency".to_string(),
            dependency_added: false,
            workflow_revision: workflow
                .revisions
                .last()
                .map(|revision| revision.revision)
                .unwrap_or(0),
            approved_by: approved_by.trim().to_string(),
            origin: origin.to_string(),
        });
    }
    let id_material = format!(
        "{workflow_id}\n{current_task_id}\n{goal}\n{}\n{}",
        blocked_paths.join("\n"),
        Utc::now().to_rfc3339()
    );
    let predecessor_task_id = format!("task-guard-{}", &hex_sha256(id_material.as_bytes())[..12]);
    let dependency_ids = workflow.tasks[current_index].dependencies.clone();
    let dependency_refs = dependency_ids
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let title = format!("Modify protected worktree scope: {path_summary}");
    let expected_output = format!(
        "Validated change for {path_summary} with evidence that the requested outcome is ready"
    );
    let mut predecessor = task(
        &predecessor_task_id,
        &title,
        &dependency_refs,
        &["worktree guard decision", "protected path scope", "approval lineage"],
        vec![ValidationRule {
            kind: "worktree_path_guard".to_string(),
            command: None,
            expected: format!(
                "changes are limited to the explicitly delegated scope and validated: {path_summary}"
            ),
        }],
        &expected_output,
        (ExecutorKind::Ai, 0.0),
    );
    predecessor.goal = goal.to_string();
    predecessor.work_item.goal_validation.goal = goal.to_string();
    predecessor.context_requirements.push(worktree_marker);
    predecessor
        .context_requirements
        .push(delegated_paths_marker);

    let current = &mut workflow.tasks[current_index];
    if !current.dependencies.contains(&predecessor_task_id) {
        current.dependencies.push(predecessor_task_id.clone());
    }
    current.status = TaskStatus::Blocked;
    current.work_item.backlog_state = "blocked_by_worktree_guardrail".to_string();
    current.work_item.impediments.push(format!(
        "worktree guard predecessor {} must complete before modifying {}",
        predecessor_task_id, path_summary
    ));
    current.version = current.version.saturating_add(1);
    workflow.tasks.push(predecessor);
    let revision = workflow
        .revisions
        .last()
        .map(|revision| revision.revision.saturating_add(1))
        .unwrap_or(1);
    workflow.revisions.push(WorkflowRevision {
        revision,
        origin: origin.to_string(),
        change_type: "worktree_guard_predecessor_created".to_string(),
        summary: format!(
            "created predecessor {predecessor_task_id} for {current_task_id} covering {path_summary}"
        ),
        created_at: Utc::now(),
    });
    let report = WorktreePredecessorTaskReport {
        schema_version: WORKTREE_PREDECESSOR_TASK_SCHEMA_VERSION.to_string(),
        status: "worktree_guard_predecessor_created".to_string(),
        workflow_id: workflow_id.to_string(),
        current_task_id: current_task_id.to_string(),
        predecessor_task_id,
        blocked_paths,
        current_task_action: "blocked_by_predecessor_dependency".to_string(),
        dependency_added: true,
        workflow_revision: revision,
        approved_by: approved_by.trim().to_string(),
        origin: origin.to_string(),
    };
    store.with_transaction(|| {
        store.save_workflow(&workflow)?;
        store.record_event(
            workflow_id,
            "worktree_guard_predecessor_created",
            &serde_json::to_value(&report)?,
        )?;
        Ok(())
    })?;
    Ok(report)
}

pub fn list_registered_worktrees(
    store: &FoundryStore,
    repository: Option<&Path>,
    workflow_id: Option<&str>,
) -> Result<WorktreeListReport> {
    let repository = repository.map(absolute_path).transpose()?;
    let mut worktrees = load_all_worktree_records(store)?
        .into_iter()
        .filter(|record| {
            repository.as_ref().is_none_or(|repository| {
                paths_equal(Path::new(&record.repository_root), repository)
            })
        })
        .filter(|record| {
            workflow_id.is_none_or(|workflow_id| {
                record
                    .bindings
                    .iter()
                    .any(|binding| binding.workflow_id == workflow_id)
            })
        })
        .map(|record| refresh_worktree_record(&record).unwrap_or(record))
        .collect::<Vec<_>>();
    worktrees.sort_by(|left, right| left.worktree_root.cmp(&right.worktree_root));
    Ok(WorktreeListReport {
        schema_version: WORKTREE_RECORD_SCHEMA_VERSION.to_string(),
        count: worktrees.len(),
        worktrees,
    })
}

pub fn inspect_registered_worktree(store: &FoundryStore, selector: &str) -> Result<WorktreeRecord> {
    refresh_worktree_record(&load_worktree_record(store, selector)?)
}

pub fn resolve_worktree_selector_root(store: &FoundryStore, selector: &str) -> Result<PathBuf> {
    let candidate = PathBuf::from(selector);
    if candidate.exists() {
        return Ok(inspect_git_worktree(&candidate)?.worktree_root);
    }
    Ok(PathBuf::from(
        inspect_registered_worktree(store, selector)?.worktree_root,
    ))
}

pub fn worktree_context_for_project(
    project_root: Option<&Path>,
) -> Result<Option<WorktreeContextReport>> {
    let Some(root) = project_root else {
        return Ok(None);
    };
    let Some(state) = inspect_optional_git_worktree(root)? else {
        return Ok(None);
    };
    let config = redact_worktree_config_snapshot(load_worktree_config(&state.worktree_root)?);
    Ok(Some(WorktreeContextReport {
        schema_version: WORKTREE_RECORD_SCHEMA_VERSION.to_string(),
        id: stable_worktree_id(&state.worktree_root),
        identity_sha256: worktree_identity_sha256(&state),
        repository_root: state.repository_root.display().to_string(),
        worktree_root: state.worktree_root.display().to_string(),
        branch: state.branch,
        head: state.head,
        dirty: state.dirty,
        config_status: config.status,
        config_path: config.path,
        config_sha256: config.sha256,
        config_approved: false,
        approved_config_sha256: None,
        guardrails: config.config.guardrails,
        sandbox: config.config.sandbox,
        settings: config.config.settings,
        bindings: Vec::new(),
        binding_drifted: false,
        binding_drift_reasons: Vec::new(),
    }))
}

pub fn bound_worktree_context(
    store: &FoundryStore,
    workflow_id: &str,
    task_id: Option<&str>,
) -> Result<Option<WorktreeContextReport>> {
    let Some(record) = resolve_bound_worktree(store, workflow_id, task_id)? else {
        return Ok(None);
    };
    let selected_binding = select_binding(&record, Some(workflow_id), task_id);
    let mut context = context_from_record(record);
    if let Some(binding) = selected_binding {
        if !binding.worktree_identity_sha256.is_empty()
            && binding.worktree_identity_sha256 != context.identity_sha256
        {
            context
                .binding_drift_reasons
                .push("worktree_identity_changed".to_string());
        }
        if !binding.head_at_binding.is_empty() && binding.head_at_binding != context.head {
            context
                .binding_drift_reasons
                .push("head_changed_since_binding".to_string());
        }
        if !binding.config_sha256_at_binding.is_empty()
            && binding.config_sha256_at_binding != context.config_sha256
        {
            context
                .binding_drift_reasons
                .push("config_changed_since_binding".to_string());
        }
    }
    context.binding_drifted = !context.binding_drift_reasons.is_empty();
    Ok(Some(context))
}

pub fn bound_worktree_mutation_claim(
    store: &FoundryStore,
    workflow_id: &str,
    task_id: &str,
) -> Result<Option<WorktreeMutationClaim>> {
    let Some(record) = resolve_bound_worktree(store, workflow_id, Some(task_id))? else {
        return Ok(None);
    };
    let binding = select_binding(&record, Some(workflow_id), Some(task_id)).with_context(|| {
        format!(
            "resolved worktree {} has no binding for workflow {workflow_id} task {task_id}",
            record.id
        )
    })?;
    let repository_root =
        process_compatible_path(&fs::canonicalize(&record.repository_root).with_context(|| {
            format!(
                "failed to canonicalize repository root {} for worktree mutation claim",
                record.repository_root
            )
        })?);
    let worktree_root =
        process_compatible_path(&fs::canonicalize(&record.worktree_root).with_context(|| {
            format!(
                "failed to canonicalize worktree root {} for worktree mutation claim",
                record.worktree_root
            )
        })?);
    Ok(Some(WorktreeMutationClaim {
        schema_version: WORKTREE_MUTATION_CLAIM_SCHEMA_VERSION.to_string(),
        mode: "exclusive_mutation".to_string(),
        worktree_id: record.id,
        worktree_identity_sha256: record.identity_sha256,
        repository_root: repository_root.display().to_string(),
        worktree_root: worktree_root.display().to_string(),
        binding_scope: if binding.task_id.is_some() {
            "task".to_string()
        } else {
            "workflow".to_string()
        },
        binding_workflow_revision: binding.workflow_revision,
        head: record.head,
        config_sha256: record.config.sha256,
    }))
}

pub fn resolve_bound_worktree_root(
    store: &FoundryStore,
    workflow_id: &str,
    task_id: Option<&str>,
) -> Result<Option<PathBuf>> {
    Ok(resolve_bound_worktree(store, workflow_id, task_id)?
        .map(|record| PathBuf::from(record.worktree_root)))
}

pub fn resolve_effective_project_root(
    store: &FoundryStore,
    workflow_id: &str,
    task_id: Option<&str>,
    explicit_project_root: Option<&Path>,
) -> Result<Option<PathBuf>> {
    let bound_root = resolve_bound_worktree_root(store, workflow_id, task_id)?;
    let explicit_root = explicit_project_root
        .map(absolute_path)
        .transpose()?
        .map(|path| {
            fs::canonicalize(&path)
                .with_context(|| format!("failed to resolve project root {}", path.display()))
        })
        .transpose()?;
    if let (Some(bound), Some(explicit)) = (bound_root.as_ref(), explicit_root.as_ref()) {
        let bound = fs::canonicalize(bound)
            .with_context(|| format!("failed to resolve bound worktree {}", bound.display()))?;
        if bound != *explicit {
            bail!(
                "explicit project root {} conflicts with bound worktree {}; rebind the task/workflow before changing execution roots",
                explicit.display(),
                bound.display()
            );
        }
    }
    Ok(bound_root.or(explicit_root))
}

pub fn plan_worktree_sandbox(
    store: &FoundryStore,
    request: WorktreeSandboxRequest,
) -> Result<WorktreeSandboxPlan> {
    if request.task_id.is_some() && request.workflow_id.is_none() {
        bail!("a task-scoped sandbox requires a workflow id");
    }
    let record = inspect_registered_worktree(store, &request.worktree)?;
    let config_snapshot = load_worktree_config(Path::new(&record.worktree_root))?;
    let config = &config_snapshot.config;
    let mut decisions = Vec::new();
    let mut blockers = Vec::new();
    let purpose = request.purpose.trim().to_lowercase();

    decide(
        &mut decisions,
        &mut blockers,
        "config_present",
        record.config.status == "configured",
        format!("worktree config status is {}", record.config.status),
    );
    let config_approved =
        record.approved_config_sha256.as_deref() == Some(config_snapshot.sha256.as_str());
    decide(
        &mut decisions,
        &mut blockers,
        "config_approved",
        config_approved,
        "the current worktree manifest hash must be explicitly approved".to_string(),
    );
    decide(
        &mut decisions,
        &mut blockers,
        "sandbox_enabled",
        config.sandbox.enabled,
        format!("sandbox enabled={}", config.sandbox.enabled),
    );
    decide(
        &mut decisions,
        &mut blockers,
        "purpose_allowed",
        config.sandbox.purposes.iter().any(|item| item == &purpose),
        format!("requested sandbox purpose is {purpose}"),
    );

    let mut command = if request.command.is_empty() {
        config
            .sandbox
            .commands
            .get(&purpose)
            .cloned()
            .unwrap_or_default()
    } else {
        request.command.clone()
    };
    let command_redaction_count = command
        .iter_mut()
        .map(|argument| {
            let report = sanitize_prompt_secrets(
                argument,
                sandbox_secret_sanitization_options("sandbox_command"),
            );
            *argument = report.sanitized_text;
            report.detection_count
        })
        .sum::<usize>();
    decide(
        &mut decisions,
        &mut blockers,
        "command_present",
        !command.is_empty()
            && command
                .first()
                .is_some_and(|value| !value.trim().is_empty()),
        "a command must be supplied by the manifest or CLI override".to_string(),
    );
    decide(
        &mut decisions,
        &mut blockers,
        "command_secret_free",
        command_redaction_count == 0,
        if command_redaction_count == 0 {
            "command arguments contain no detected inline secrets".to_string()
        } else {
            format!(
                "redacted {command_redaction_count} inline secret value(s); inject authorized vault references instead"
            )
        },
    );
    let command_name = command
        .first()
        .and_then(|value| Path::new(value).file_name())
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let explicit_command_path = command
        .first()
        .is_some_and(|value| Path::new(value).components().count() > 1);
    let command_allowed = config.guardrails.allowed_commands.iter().any(|allowed| {
        allowed == "*"
            || (!explicit_command_path && allowed == command_name)
            || (explicit_command_path && command.first().is_some_and(|value| allowed == value))
    });
    decide(
        &mut decisions,
        &mut blockers,
        "command_allowlist",
        command_allowed,
        format!("command `{command_name}` must be listed in guardrails.allowed_commands"),
    );
    decide(
        &mut decisions,
        &mut blockers,
        "clean_worktree",
        !config.guardrails.require_clean || !record.dirty,
        format!(
            "require_clean={} dirty={}",
            config.guardrails.require_clean, record.dirty
        ),
    );
    let branch_allowed = match record.branch.as_deref() {
        Some(branch) => {
            config.guardrails.allowed_branches.is_empty()
                || config
                    .guardrails
                    .allowed_branches
                    .iter()
                    .any(|pattern| wildcard_matches(pattern, branch))
        }
        None => config.guardrails.allow_detached_head,
    };
    decide(
        &mut decisions,
        &mut blockers,
        "branch_policy",
        branch_allowed,
        format!(
            "branch={:?} detached_allowed={}",
            record.branch, config.guardrails.allow_detached_head
        ),
    );

    let expected_bound_record = request
        .workflow_id
        .as_deref()
        .map(|workflow_id| resolve_bound_worktree(store, workflow_id, request.task_id.as_deref()))
        .transpose()?
        .flatten();
    let binding_scope_matches = expected_bound_record
        .as_ref()
        .is_none_or(|expected| expected.id == record.id);
    decide(
        &mut decisions,
        &mut blockers,
        "binding_scope_matches",
        binding_scope_matches,
        expected_bound_record
            .as_ref()
            .map(|expected| {
                format!(
                    "workflow/task resolves to worktree {}; requested {}",
                    expected.id, record.id
                )
            })
            .unwrap_or_else(|| "no conflicting workflow/task binding exists".to_string()),
    );
    let binding = binding_scope_matches
        .then(|| {
            select_binding(
                &record,
                request.workflow_id.as_deref(),
                request.task_id.as_deref(),
            )
        })
        .flatten();
    let binding_allowed = !config.guardrails.require_workflow_binding || binding.is_some();
    decide(
        &mut decisions,
        &mut blockers,
        "workflow_binding",
        binding_allowed,
        "sandbox execution requires an unambiguous workflow/task binding".to_string(),
    );
    let binding_fingerprint_matches = binding.as_ref().is_none_or(|binding| {
        (binding.worktree_identity_sha256.is_empty()
            || binding.worktree_identity_sha256 == record.identity_sha256)
            && (binding.head_at_binding.is_empty() || binding.head_at_binding == record.head)
            && (binding.config_sha256_at_binding.is_empty()
                || binding.config_sha256_at_binding == config_snapshot.sha256)
    });
    decide(
        &mut decisions,
        &mut blockers,
        "binding_fingerprint",
        binding_fingerprint_matches,
        "bound worktree identity, HEAD and config hash must match; rebind after intentional drift"
            .to_string(),
    );

    let root = PathBuf::from(&record.worktree_root);
    let sandbox_root = resolve_relative_inside(&root, &config.sandbox.root)?;
    let working_directory = resolve_relative_inside(&root, &config.sandbox.working_directory)?;
    decide(
        &mut decisions,
        &mut blockers,
        "sandbox_root_scoped",
        sandbox_root != root,
        "sandbox root must be a dedicated directory inside the worktree".to_string(),
    );
    decide(
        &mut decisions,
        &mut blockers,
        "working_directory_available",
        working_directory.is_dir() || working_directory == sandbox_root,
        format!(
            "working directory must exist or equal the managed sandbox root: {}",
            working_directory.display()
        ),
    );
    let runtime = config.sandbox.runtime.trim().to_lowercase();
    let runtime_supported = matches!(runtime.as_str(), "process" | "bubblewrap");
    decide(
        &mut decisions,
        &mut blockers,
        "runtime_supported",
        runtime_supported,
        format!("sandbox runtime is {runtime}"),
    );
    let bubblewrap_path = trusted_system_command("bwrap");
    let bubblewrap_available = runtime != "bubblewrap" || bubblewrap_path.is_some();
    decide(
        &mut decisions,
        &mut blockers,
        "runtime_available",
        bubblewrap_available,
        "Bubblewrap runtime requires `bwrap` on PATH".to_string(),
    );
    let network_policy = config.sandbox.network.trim().to_lowercase();
    let network_supported = matches!(network_policy.as_str(), "inherit" | "deny");
    decide(
        &mut decisions,
        &mut blockers,
        "network_policy_supported",
        network_supported,
        format!("network policy is {network_policy}"),
    );
    let network_enforceable = network_policy != "deny" || runtime == "bubblewrap";
    decide(
        &mut decisions,
        &mut blockers,
        "network_policy_enforceable",
        network_enforceable,
        "network=deny requires the Bubblewrap runtime".to_string(),
    );
    decide(
        &mut decisions,
        &mut blockers,
        "timeout_within_administrative_limit",
        config.guardrails.max_command_seconds <= MAX_SANDBOX_COMMAND_SECONDS,
        format!("max_command_seconds must be <= {MAX_SANDBOX_COMMAND_SECONDS}"),
    );
    decide(
        &mut decisions,
        &mut blockers,
        "output_within_administrative_limit",
        config.guardrails.max_output_bytes <= MAX_SANDBOX_OUTPUT_BYTES,
        format!("max_output_bytes must be <= {MAX_SANDBOX_OUTPUT_BYTES}"),
    );

    let runtime_paths = sandbox_runtime_paths(&runtime, &root, &sandbox_root, &working_directory)?;
    let launch_command = if runtime == "bubblewrap" && !command.is_empty() {
        bubblewrap_command(
            bubblewrap_path
                .as_deref()
                .unwrap_or_else(|| Path::new("bwrap")),
            &root,
            &sandbox_root,
            &runtime_paths,
            &network_policy,
            &command,
        )
    } else {
        command.clone()
    };
    let mut inherited_environment = Vec::new();
    let mut unsafe_inherited_environment_count = 0usize;
    for name in &config.sandbox.inherit_environment {
        let Some(value) = crate::brand::env_var_os(name) else {
            continue;
        };
        let value = value.to_string_lossy();
        let report = sanitize_prompt_secrets(
            &value,
            sandbox_secret_sanitization_options("sandbox_inherited_environment"),
        );
        if report.detection_count == 0 {
            inherited_environment.push(name.clone());
        } else {
            unsafe_inherited_environment_count += 1;
        }
    }
    decide(
        &mut decisions,
        &mut blockers,
        "inherited_environment_secret_free",
        unsafe_inherited_environment_count == 0,
        if unsafe_inherited_environment_count == 0 {
            "inherited environment values contain no detected secrets".to_string()
        } else {
            format!(
                "{unsafe_inherited_environment_count} inherited environment value(s) contain detected secrets; use authorized vault injection"
            )
        },
    );
    let configured_environment_keys = config
        .sandbox
        .environment
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    let allowed = blockers.is_empty();
    Ok(WorktreeSandboxPlan {
        schema_version: WORKTREE_SANDBOX_PLAN_SCHEMA_VERSION.to_string(),
        status: if allowed {
            "sandbox_ready".to_string()
        } else {
            "sandbox_blocked".to_string()
        },
        allowed,
        worktree_id: record.id,
        worktree_root: record.worktree_root,
        repository_root: record.repository_root,
        branch: record.branch,
        head: record.head,
        dirty: record.dirty,
        purpose,
        sandbox_name: config.sandbox.name.clone(),
        sandbox_root: sandbox_root.display().to_string(),
        working_directory: working_directory.display().to_string(),
        runtime_worktree_root: runtime_paths.worktree_root.display().to_string(),
        runtime_sandbox_root: runtime_paths.sandbox_root.display().to_string(),
        runtime_working_directory: runtime_paths.working_directory.display().to_string(),
        runtime: runtime.clone(),
        network_policy: network_policy.clone(),
        filesystem_isolation_enforced: runtime == "bubblewrap",
        network_isolation_enforced: runtime == "bubblewrap" && network_policy == "deny",
        command,
        launch_command,
        inherited_environment,
        configured_environment_keys,
        config_sha256: config_snapshot.sha256,
        foundry_store_path: absolute_store_path(store).display().to_string(),
        foundry_store_path_mounted: runtime != "bubblewrap",
        max_command_seconds: config
            .guardrails
            .max_command_seconds
            .clamp(1, MAX_SANDBOX_COMMAND_SECONDS),
        max_output_bytes: config
            .guardrails
            .max_output_bytes
            .clamp(1, MAX_SANDBOX_OUTPUT_BYTES),
        binding,
        guardrail_decisions: decisions,
        blockers,
        notes: vec![
            "The process runtime scopes cwd, environment and evidence to the worktree but is not an OS security boundary.".to_string(),
            "The Bubblewrap runtime exposes only system runtime directories, mounts the worktree read-only, permits writes only below the managed sandbox root and optionally isolates the network.".to_string(),
            "Inside Bubblewrap, FOUNDRY_STORE_PATH is a host-side lineage locator; the central store is not mounted, so nested Foundry mutations must run outside the sandbox.".to_string(),
            "Docker, Kubernetes and Knative remain separately authorized async substrates and are never installed or mutated by this command.".to_string(),
        ],
    })
}

pub fn run_worktree_sandbox(
    store: &FoundryStore,
    request: WorktreeSandboxRequest,
    allow_exec: bool,
) -> Result<WorktreeSandboxReceipt> {
    run_worktree_sandbox_internal(store, request, allow_exec, None)
}

fn run_worktree_sandbox_internal(
    store: &FoundryStore,
    request: WorktreeSandboxRequest,
    allow_exec: bool,
    sandbox_id: Option<&str>,
) -> Result<WorktreeSandboxReceipt> {
    let plan = plan_worktree_sandbox(store, request)?;
    let started_at = Utc::now();
    let command_sha256 = hex_sha256(serde_json::to_string(&plan.command)?.as_bytes());
    if !allow_exec || !plan.allowed {
        let empty = empty_stream_evidence();
        let receipt = WorktreeSandboxReceipt {
            schema_version: WORKTREE_SANDBOX_RECEIPT_SCHEMA_VERSION.to_string(),
            sandbox_id: sandbox_id.map(str::to_string),
            status: if !allow_exec {
                "blocked_without_explicit_exec_approval".to_string()
            } else {
                "blocked_by_worktree_guardrails".to_string()
            },
            allowed: false,
            execution_attempted: false,
            executed: false,
            worktree_id: plan.worktree_id.clone(),
            purpose: plan.purpose.clone(),
            runtime: plan.runtime.clone(),
            command_sha256,
            config_sha256: plan.config_sha256.clone(),
            started_at: started_at.to_rfc3339(),
            finished_at: Utc::now().to_rfc3339(),
            duration_ms: 0,
            timed_out: false,
            exit_code: None,
            error: None,
            stdout: empty.clone(),
            stderr: empty,
            sandbox_root: plan.sandbox_root.clone(),
            working_directory: plan.working_directory.clone(),
            binding: plan.binding.clone(),
            plan,
        };
        record_sandbox_receipt(store, &receipt)?;
        return Ok(receipt);
    }

    let sandbox_root = PathBuf::from(&plan.sandbox_root);
    let setup = ["artifacts", "cache", "tmp", "home"]
        .into_iter()
        .try_for_each(|directory| {
            fs::create_dir_all(sandbox_root.join(directory)).with_context(|| {
                format!(
                    "failed to prepare managed sandbox directory {}",
                    sandbox_root.join(directory).display()
                )
            })
        });
    let execution = match setup {
        Err(error) => command_execution_failure(Instant::now(), false, error),
        Ok(()) => {
            if let Some(sandbox_id) = sandbox_id {
                execute_bounded_command_for_lifecycle(
                    &plan,
                    &sandbox_root,
                    Some((store, sandbox_id)),
                )
            } else {
                execute_bounded_command(&plan, &sandbox_root)
            }
        }
    };
    let finished_at = Utc::now();
    let lifecycle_was_stopped = sandbox_id
        .map(|sandbox_id| lifecycle_stop_requested(store, sandbox_id))
        .transpose()?
        .unwrap_or(false);
    let status = if execution.stop_requested || lifecycle_was_stopped {
        "sandbox_stopped"
    } else if execution.error.is_some() {
        "sandbox_execution_failed"
    } else if execution.timed_out {
        "sandbox_timed_out"
    } else if execution.exit_code == Some(0) {
        "sandbox_completed"
    } else {
        "sandbox_failed"
    };
    let receipt = WorktreeSandboxReceipt {
        schema_version: WORKTREE_SANDBOX_RECEIPT_SCHEMA_VERSION.to_string(),
        sandbox_id: sandbox_id.map(str::to_string),
        status: status.to_string(),
        allowed: true,
        execution_attempted: true,
        executed: execution.child_started,
        worktree_id: plan.worktree_id.clone(),
        purpose: plan.purpose.clone(),
        runtime: plan.runtime.clone(),
        command_sha256,
        config_sha256: plan.config_sha256.clone(),
        started_at: started_at.to_rfc3339(),
        finished_at: finished_at.to_rfc3339(),
        duration_ms: execution.duration_ms,
        timed_out: execution.timed_out,
        exit_code: execution.exit_code,
        error: execution.error,
        stdout: execution.stdout,
        stderr: execution.stderr,
        sandbox_root: plan.sandbox_root.clone(),
        working_directory: plan.working_directory.clone(),
        binding: plan.binding.clone(),
        plan,
    };
    record_sandbox_receipt(store, &receipt)?;
    Ok(receipt)
}

pub fn start_worktree_sandbox(
    store: &FoundryStore,
    request: WorktreeSandboxRequest,
    allow_exec: bool,
) -> Result<WorktreeSandboxLifecycleReport> {
    if !allow_exec {
        bail!("starting a persistent sandbox requires --allow-exec");
    }
    let plan = plan_worktree_sandbox(store, request)?;
    if !plan.allowed {
        bail!(
            "sandbox start is blocked by guardrails: {}",
            plan.blockers.join("; ")
        );
    }

    let created_at = Utc::now();
    let sandbox_id_material = format!(
        "{}\n{}\n{}\n{}",
        plan.worktree_id,
        plan.config_sha256,
        serde_json::to_string(&plan.command)?,
        created_at.to_rfc3339()
    );
    let sandbox_id = format!(
        "sandbox_{}",
        &hex_sha256(sandbox_id_material.as_bytes())[..16]
    );
    let mut report = WorktreeSandboxLifecycleReport {
        schema_version: WORKTREE_SANDBOX_LIFECYCLE_SCHEMA_VERSION.to_string(),
        sandbox_id: sandbox_id.clone(),
        status: "sandbox_starting".to_string(),
        worktree_id: plan.worktree_id.clone(),
        worktree_root: plan.worktree_root.clone(),
        purpose: plan.purpose.clone(),
        workflow_id: plan
            .binding
            .as_ref()
            .map(|binding| binding.workflow_id.clone()),
        task_id: plan
            .binding
            .as_ref()
            .and_then(|binding| binding.task_id.clone()),
        command_sha256: hex_sha256(serde_json::to_string(&plan.command)?.as_bytes()),
        config_sha256: plan.config_sha256.clone(),
        supervisor_pid: None,
        payload_pid: None,
        payload_descendant_pids: Vec::new(),
        created_at: created_at.to_rfc3339(),
        updated_at: created_at.to_rfc3339(),
        finished_at: None,
        stop_requested_at: None,
        receipt_status: None,
        exit_code: None,
        timed_out: false,
        error: None,
        plan,
    };
    save_worktree_sandbox_lifecycle(store, &report, Some("worktree_sandbox_starting"))?;

    let child = match std::env::current_exe()
        .context("failed to resolve Foundry executable")
        .and_then(|current_exe| {
            Command::new(current_exe)
                .arg("--store")
                .arg(absolute_store_path(store))
                .args([
                    "worktree",
                    "sandbox",
                    "supervise",
                    "--sandbox",
                    sandbox_id.as_str(),
                    "--allow-supervisor-exec",
                ])
                .current_dir(&report.worktree_root)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .context("failed to launch sandbox supervisor")
        }) {
        Ok(child) => child,
        Err(error) => {
            report.status = "sandbox_execution_failed".to_string();
            report.finished_at = Some(Utc::now().to_rfc3339());
            report.updated_at = report.finished_at.clone().unwrap_or_default();
            report.error = Some(
                sanitize_prompt_secrets(
                    &error.to_string(),
                    sandbox_secret_sanitization_options("sandbox_error"),
                )
                .sanitized_text,
            );
            save_worktree_sandbox_lifecycle(store, &report, Some("worktree_sandbox_start_failed"))?;
            return Ok(report);
        }
    };
    drop(child);

    for _ in 0..100 {
        let current = inspect_worktree_sandbox_lifecycle(store, &sandbox_id)?;
        if current.status != "sandbox_starting" {
            return Ok(current);
        }
        thread::sleep(Duration::from_millis(10));
    }
    inspect_worktree_sandbox_lifecycle(store, &sandbox_id)
}

pub fn supervise_worktree_sandbox(
    store: &FoundryStore,
    sandbox_id: &str,
    allow_supervisor_exec: bool,
) -> Result<WorktreeSandboxLifecycleReport> {
    if !allow_supervisor_exec {
        bail!("sandbox supervision requires --allow-supervisor-exec");
    }
    let lifecycle = inspect_worktree_sandbox_lifecycle(store, sandbox_id)?;
    if lifecycle.status != "sandbox_starting" {
        return Ok(lifecycle);
    }
    mark_lifecycle_supervisor_started(store, sandbox_id, std::process::id())?;
    let request = WorktreeSandboxRequest {
        worktree: lifecycle.plan.worktree_id.clone(),
        purpose: lifecycle.plan.purpose.clone(),
        workflow_id: lifecycle
            .plan
            .binding
            .as_ref()
            .map(|binding| binding.workflow_id.clone()),
        task_id: lifecycle
            .plan
            .binding
            .as_ref()
            .and_then(|binding| binding.task_id.clone()),
        command: lifecycle.plan.command.clone(),
    };
    match run_worktree_sandbox_internal(store, request, true, Some(sandbox_id)) {
        Ok(receipt) => finish_worktree_sandbox_lifecycle(store, sandbox_id, &receipt),
        Err(error) => fail_worktree_sandbox_lifecycle(store, sandbox_id, error),
    }
}

pub fn inspect_worktree_sandbox_lifecycle(
    store: &FoundryStore,
    sandbox_id: &str,
) -> Result<WorktreeSandboxLifecycleReport> {
    let report = load_worktree_sandbox_lifecycle_raw(store, sandbox_id)?;
    let Some(supervisor_pid) = report.supervisor_pid else {
        return Ok(report);
    };
    if sandbox_lifecycle_terminal(&report.status) || process_alive(supervisor_pid) {
        return Ok(report);
    }

    let mut termination_errors = terminate_persisted_sandbox_processes(&report, false);
    for _ in 0..50 {
        if persisted_sandbox_processes_alive(&report).is_empty() {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    termination_errors.extend(persisted_sandbox_processes_alive(&report));
    termination_errors.sort();
    termination_errors.dedup();
    mutate_worktree_sandbox_lifecycle(
        store,
        sandbox_id,
        Some("worktree_sandbox_supervisor_lost"),
        |current| {
            if sandbox_lifecycle_terminal(&current.status)
                || current.supervisor_pid != Some(supervisor_pid)
                || process_alive(supervisor_pid)
            {
                return Ok(false);
            }
            let now = Utc::now().to_rfc3339();
            current.status = "sandbox_execution_failed".to_string();
            current.finished_at = Some(now.clone());
            current.updated_at = now;
            current.error = Some(sanitize_lifecycle_error(format!(
                "sandbox supervisor {supervisor_pid} exited before lifecycle completion{}",
                format_termination_errors(&termination_errors)
            )));
            Ok(true)
        },
    )
}

fn load_worktree_sandbox_lifecycle_raw(
    store: &FoundryStore,
    sandbox_id: &str,
) -> Result<WorktreeSandboxLifecycleReport> {
    let value = store
        .load_worktree_sandbox_state(sandbox_id)?
        .with_context(|| format!("sandbox lifecycle not found: {sandbox_id}"))?;
    serde_json::from_value(value)
        .with_context(|| format!("invalid persisted sandbox lifecycle: {sandbox_id}"))
}

pub fn stop_worktree_sandbox(
    store: &FoundryStore,
    sandbox_id: &str,
    allow_stop: bool,
) -> Result<WorktreeSandboxLifecycleReport> {
    if !allow_stop {
        bail!("stopping a persistent sandbox requires --allow-stop");
    }
    let mut report = mutate_worktree_sandbox_lifecycle(
        store,
        sandbox_id,
        Some("worktree_sandbox_stop_requested"),
        |report| {
            if sandbox_lifecycle_terminal(&report.status) {
                return Ok(false);
            }
            let requested_at = Utc::now().to_rfc3339();
            report.status = "sandbox_stopping".to_string();
            report.stop_requested_at = Some(requested_at.clone());
            report.updated_at = requested_at;
            Ok(true)
        },
    )?;
    if sandbox_lifecycle_terminal(&report.status) {
        return Ok(report);
    }

    let mut termination_errors = terminate_persisted_sandbox_processes(&report, false);
    for _ in 0..150 {
        let current = inspect_worktree_sandbox_lifecycle(store, sandbox_id)?;
        if sandbox_lifecycle_terminal(&current.status) {
            return Ok(current);
        }
        thread::sleep(Duration::from_millis(20));
    }

    report = load_worktree_sandbox_lifecycle_raw(store, sandbox_id)?;
    if sandbox_lifecycle_terminal(&report.status) {
        return Ok(report);
    }
    termination_errors.extend(terminate_persisted_sandbox_processes(&report, true));
    for _ in 0..50 {
        if persisted_sandbox_processes_alive(&report).is_empty() {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    termination_errors.extend(persisted_sandbox_processes_alive(&report));
    termination_errors.sort();
    termination_errors.dedup();
    let event_kind = if termination_errors.is_empty() {
        "worktree_sandbox_stopped"
    } else {
        "worktree_sandbox_stop_failed"
    };
    mutate_worktree_sandbox_lifecycle(store, sandbox_id, Some(event_kind), |current| {
        if sandbox_lifecycle_terminal(&current.status) {
            return Ok(false);
        }
        current.status = if termination_errors.is_empty() {
            "sandbox_stopped".to_string()
        } else {
            "sandbox_stop_failed".to_string()
        };
        current.error = (!termination_errors.is_empty()).then(|| {
            sanitize_lifecycle_error(format!(
                "failed to stop every sandbox process{}",
                format_termination_errors(&termination_errors)
            ))
        });
        let finished_at = Utc::now().to_rfc3339();
        current.finished_at = Some(finished_at.clone());
        current.updated_at = finished_at;
        Ok(true)
    })
}

fn save_worktree_sandbox_lifecycle(
    store: &FoundryStore,
    report: &WorktreeSandboxLifecycleReport,
    event_kind: Option<&str>,
) -> Result<()> {
    store.with_transaction(|| save_worktree_sandbox_lifecycle_raw(store, report, event_kind))
}

fn save_worktree_sandbox_lifecycle_raw(
    store: &FoundryStore,
    report: &WorktreeSandboxLifecycleReport,
    event_kind: Option<&str>,
) -> Result<()> {
    let value = serde_json::to_value(report)?;
    store.save_worktree_sandbox_state(
        &report.sandbox_id,
        &report.worktree_id,
        &report.status,
        &value,
    )?;
    if let Some(event_kind) = event_kind {
        store.record_event(
            report.workflow_id.as_deref().unwrap_or("_system"),
            event_kind,
            &value,
        )?;
    }
    Ok(())
}

fn mutate_worktree_sandbox_lifecycle(
    store: &FoundryStore,
    sandbox_id: &str,
    event_kind: Option<&str>,
    mutation: impl FnOnce(&mut WorktreeSandboxLifecycleReport) -> Result<bool>,
) -> Result<WorktreeSandboxLifecycleReport> {
    store.with_transaction(|| {
        let mut report = load_worktree_sandbox_lifecycle_raw(store, sandbox_id)?;
        if mutation(&mut report)? {
            save_worktree_sandbox_lifecycle_raw(store, &report, event_kind)?;
        }
        Ok(report)
    })
}

fn mark_lifecycle_supervisor_started(
    store: &FoundryStore,
    sandbox_id: &str,
    supervisor_pid: u32,
) -> Result<()> {
    mutate_worktree_sandbox_lifecycle(store, sandbox_id, None, |report| {
        if report.stop_requested_at.is_some() || sandbox_lifecycle_terminal(&report.status) {
            bail!("sandbox stop was requested before its supervisor became ready");
        }
        report.supervisor_pid = Some(supervisor_pid);
        report.updated_at = Utc::now().to_rfc3339();
        Ok(true)
    })?;
    Ok(())
}

fn mark_lifecycle_payload_started(
    store: &FoundryStore,
    sandbox_id: &str,
    payload_pid: u32,
) -> Result<()> {
    mutate_worktree_sandbox_lifecycle(
        store,
        sandbox_id,
        Some("worktree_sandbox_running"),
        |report| {
            if report.stop_requested_at.is_some() || sandbox_lifecycle_terminal(&report.status) {
                bail!("sandbox stop was requested before its payload became ready");
            }
            report.status = "sandbox_running".to_string();
            report.payload_pid = Some(payload_pid);
            report.updated_at = Utc::now().to_rfc3339();
            Ok(true)
        },
    )?;
    Ok(())
}

fn mark_lifecycle_payload_descendants(
    store: &FoundryStore,
    sandbox_id: &str,
    payload_descendant_pids: &BTreeSet<u32>,
) -> Result<()> {
    let payload_descendant_pids = payload_descendant_pids.iter().copied().collect::<Vec<_>>();
    mutate_worktree_sandbox_lifecycle(store, sandbox_id, None, |report| {
        if sandbox_lifecycle_terminal(&report.status)
            || report.payload_descendant_pids == payload_descendant_pids
        {
            return Ok(false);
        }
        report.payload_descendant_pids = payload_descendant_pids;
        report.updated_at = Utc::now().to_rfc3339();
        Ok(true)
    })?;
    Ok(())
}

fn lifecycle_stop_requested(store: &FoundryStore, sandbox_id: &str) -> Result<bool> {
    let report = load_worktree_sandbox_lifecycle_raw(store, sandbox_id)?;
    Ok(report.stop_requested_at.is_some()
        || report.status == "sandbox_stopping"
        || sandbox_lifecycle_terminal(&report.status))
}

fn finish_worktree_sandbox_lifecycle(
    store: &FoundryStore,
    sandbox_id: &str,
    receipt: &WorktreeSandboxReceipt,
) -> Result<WorktreeSandboxLifecycleReport> {
    mutate_worktree_sandbox_lifecycle(
        store,
        sandbox_id,
        Some("worktree_sandbox_finished"),
        |report| {
            if sandbox_lifecycle_terminal(&report.status) {
                return Ok(false);
            }
            report.status = if report.stop_requested_at.is_some() {
                "sandbox_stopped".to_string()
            } else {
                receipt.status.clone()
            };
            report.receipt_status = Some(receipt.status.clone());
            report.exit_code = receipt.exit_code;
            report.timed_out = receipt.timed_out;
            report.error = receipt.error.clone();
            report.finished_at = Some(receipt.finished_at.clone());
            report.updated_at = receipt.finished_at.clone();
            Ok(true)
        },
    )
}

fn fail_worktree_sandbox_lifecycle(
    store: &FoundryStore,
    sandbox_id: &str,
    error: anyhow::Error,
) -> Result<WorktreeSandboxLifecycleReport> {
    let error = sanitize_lifecycle_error(error.to_string());
    mutate_worktree_sandbox_lifecycle(
        store,
        sandbox_id,
        Some("worktree_sandbox_finished"),
        |report| {
            if sandbox_lifecycle_terminal(&report.status) {
                return Ok(false);
            }
            report.status = if report.stop_requested_at.is_some() {
                "sandbox_stopped".to_string()
            } else {
                "sandbox_execution_failed".to_string()
            };
            report.error = Some(error);
            let finished_at = Utc::now().to_rfc3339();
            report.finished_at = Some(finished_at.clone());
            report.updated_at = finished_at;
            Ok(true)
        },
    )
}

fn sanitize_lifecycle_error(error: String) -> String {
    sanitize_prompt_secrets(&error, sandbox_secret_sanitization_options("sandbox_error"))
        .sanitized_text
}

fn format_termination_errors(errors: &[String]) -> String {
    if errors.is_empty() {
        String::new()
    } else {
        format!("; {}", errors.join("; "))
    }
}

fn sandbox_lifecycle_terminal(status: &str) -> bool {
    matches!(
        status,
        "sandbox_stopped"
            | "sandbox_completed"
            | "sandbox_failed"
            | "sandbox_timed_out"
            | "sandbox_execution_failed"
            | "sandbox_stop_failed"
            | "blocked_without_explicit_exec_approval"
            | "blocked_by_worktree_guardrails"
    )
}

fn resolve_bound_worktree(
    store: &FoundryStore,
    workflow_id: &str,
    task_id: Option<&str>,
) -> Result<Option<WorktreeRecord>> {
    let records = load_all_worktree_records(store)?;
    if let Some(task_id) = task_id {
        if let Some(record) = records.iter().find(|record| {
            record.bindings.iter().any(|binding| {
                binding.workflow_id == workflow_id && binding.task_id.as_deref() == Some(task_id)
            })
        }) {
            return Ok(Some(refresh_worktree_record(record)?));
        }
    }
    records
        .iter()
        .find(|record| {
            record
                .bindings
                .iter()
                .any(|binding| binding.workflow_id == workflow_id && binding.task_id.is_none())
        })
        .map(refresh_worktree_record)
        .transpose()
}

fn context_from_record(record: WorktreeRecord) -> WorktreeContextReport {
    let config_approved =
        record.approved_config_sha256.as_deref() == Some(record.config.sha256.as_str());
    WorktreeContextReport {
        schema_version: record.schema_version,
        id: record.id,
        identity_sha256: record.identity_sha256,
        repository_root: record.repository_root,
        worktree_root: record.worktree_root,
        branch: record.branch,
        head: record.head,
        dirty: record.dirty,
        config_status: record.config.status,
        config_path: record.config.path,
        config_sha256: record.config.sha256,
        config_approved,
        approved_config_sha256: record.approved_config_sha256,
        guardrails: record.config.config.guardrails,
        sandbox: record.config.config.sandbox,
        settings: record.config.config.settings,
        bindings: record.bindings,
        binding_drifted: false,
        binding_drift_reasons: Vec::new(),
    }
}

fn record_from_state(
    id: String,
    state: GitWorktreeState,
    created_by_foundry: bool,
    bindings: Vec<WorktreeBinding>,
    registered_at: String,
    updated_at: String,
) -> Result<WorktreeRecord> {
    let identity_sha256 = worktree_identity_sha256(&state);
    let config = redact_worktree_config_snapshot(load_worktree_config(&state.worktree_root)?);
    Ok(WorktreeRecord {
        schema_version: worktree_record_schema_version(),
        id,
        repository_root: state.repository_root.display().to_string(),
        worktree_root: state.worktree_root.display().to_string(),
        git_common_dir: state.git_common_dir.display().to_string(),
        git_dir: state.git_dir.display().to_string(),
        branch: state.branch.clone(),
        head: state.head,
        detached: state.branch.is_none(),
        dirty: state.dirty,
        changed_path_count: state.changed_path_count,
        is_main_worktree: state.is_main_worktree,
        created_by_foundry,
        identity_sha256,
        config,
        approved_config_sha256: None,
        config_approved_by: None,
        config_approved_at: None,
        bindings,
        registered_at,
        updated_at,
    })
}

fn refresh_worktree_record(record: &WorktreeRecord) -> Result<WorktreeRecord> {
    let state = inspect_git_worktree(Path::new(&record.worktree_root))?;
    let mut refreshed = record_from_state(
        record.id.clone(),
        state,
        record.created_by_foundry,
        record.bindings.clone(),
        record.registered_at.clone(),
        Utc::now().to_rfc3339(),
    )?;
    if !record.identity_sha256.is_empty() && refreshed.identity_sha256 != record.identity_sha256 {
        bail!(
            "registered worktree identity changed at {}; refusing to reuse ownership or bindings",
            record.worktree_root
        );
    }
    refreshed.approved_config_sha256 = record.approved_config_sha256.clone();
    refreshed.config_approved_by = record.config_approved_by.clone();
    refreshed.config_approved_at = record.config_approved_at.clone();
    Ok(refreshed)
}

fn load_worktree_config(root: &Path) -> Result<WorktreeConfigSnapshot> {
    let selected = crate::brand::project_config_path_for_read(root, "worktree.toml");
    let relative = selected
        .strip_prefix(root)
        .with_context(|| format!("worktree config escaped registered root {}", root.display()))?;
    let relative = relative
        .to_str()
        .context("worktree config path is not valid UTF-8")?;
    let path = resolve_relative_inside(root, relative)?;
    if !path.exists() {
        return Ok(WorktreeConfigSnapshot {
            status: "missing".to_string(),
            path: path.display().to_string(),
            sha256: hex_sha256(b"missing"),
            config: WorktreeConfig::default(),
        });
    }
    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let config: WorktreeConfig = toml::from_str(&content)
        .with_context(|| format!("invalid worktree config {}", path.display()))?;
    if !crate::brand::identifier_matches(&config.schema_version, WORKTREE_CONFIG_SCHEMA_VERSION) {
        bail!(
            "unsupported worktree config schema `{}` in {}; expected {}",
            config.schema_version,
            path.display(),
            WORKTREE_CONFIG_SCHEMA_VERSION
        );
    }
    validate_worktree_config(&config)?;
    Ok(WorktreeConfigSnapshot {
        status: "configured".to_string(),
        path: path.display().to_string(),
        sha256: hex_sha256(content.as_bytes()),
        config,
    })
}

fn validate_worktree_config(config: &WorktreeConfig) -> Result<()> {
    for scope in config
        .guardrails
        .modifiable_paths
        .iter()
        .chain(config.guardrails.protected_paths.iter())
    {
        normalize_guard_path(scope)
            .with_context(|| format!("invalid worktree guardrail path `{scope}`"))?;
    }
    for name in config
        .sandbox
        .environment
        .keys()
        .chain(config.sandbox.inherit_environment.iter())
    {
        if !valid_environment_name(name) {
            bail!("invalid sandbox environment variable name `{name}`");
        }
    }
    for name in config.sandbox.environment.keys() {
        if dangerous_environment_name(name) {
            bail!(
                "sandbox environment variable `{name}` can alter the launcher or load code and is not permitted"
            );
        }
    }
    for name in &config.sandbox.inherit_environment {
        if sensitive_name(name) {
            bail!(
                "inherited sandbox environment `{name}` looks sensitive; use the Foundry credential vault instead"
            );
        }
        if dangerous_environment_name(name) && !default_inherited_environment().contains(name) {
            bail!(
                "inherited sandbox environment `{name}` can alter the launcher or load code and is not permitted"
            );
        }
    }
    for (name, value) in &config.sandbox.environment {
        if sensitive_name(name) {
            bail!(
                "sandbox environment `{name}` looks sensitive; use the Foundry credential vault instead of a versioned worktree manifest"
            );
        }
        if value.contains(['\0', '\n', '\r']) {
            bail!("sandbox environment `{name}` contains unsupported control characters");
        }
        let secret_report = sanitize_prompt_secrets(
            value,
            sandbox_secret_sanitization_options("sandbox_configured_environment"),
        );
        if secret_report.detection_count > 0 {
            bail!(
                "sandbox environment `{name}` contains a detected secret; use the Foundry credential vault instead of a versioned worktree manifest"
            );
        }
    }
    for key in config.settings.keys() {
        if sensitive_name(key) {
            bail!(
                "worktree setting `{key}` looks sensitive; use the Foundry credential vault instead"
            );
        }
    }
    Ok(())
}

fn initial_worktree_config() -> WorktreeConfig {
    WorktreeConfig {
        schema_version: worktree_config_schema_version(),
        guardrails: WorktreeGuardrails {
            allowed_commands: vec![
                "cargo".to_string(),
                "npm".to_string(),
                "pnpm".to_string(),
                "yarn".to_string(),
                "bun".to_string(),
                "make".to_string(),
            ],
            ..WorktreeGuardrails::default()
        },
        sandbox: WorktreeSandboxConfig {
            enabled: true,
            ..WorktreeSandboxConfig::default()
        },
        settings: BTreeMap::new(),
    }
}

fn inspect_git_worktree(path: &Path) -> Result<GitWorktreeState> {
    let path = absolute_path(path)?;
    let worktree_root = PathBuf::from(git_output(&path, &["rev-parse", "--show-toplevel"])?);
    let worktree_root = process_compatible_path(
        &fs::canonicalize(&worktree_root)
            .with_context(|| format!("failed to resolve {}", worktree_root.display()))?,
    );
    let git_common_raw = git_output(&worktree_root, &["rev-parse", "--git-common-dir"])?;
    let git_common_dir = resolve_git_path(&worktree_root, &git_common_raw)?;
    let git_dir_raw = git_output(&worktree_root, &["rev-parse", "--git-dir"])?;
    let git_dir = resolve_git_path(&worktree_root, &git_dir_raw)?;
    let repository_root = git_common_dir
        .file_name()
        .filter(|name| *name == ".git")
        .and_then(|_| git_common_dir.parent())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| worktree_root.clone());
    let branch = git_optional_output(
        &worktree_root,
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
    )?;
    let head = git_output(&worktree_root, &["rev-parse", "HEAD"])?;
    let status = git_output(
        &worktree_root,
        &["status", "--porcelain=v1", "--untracked-files=normal"],
    )?;
    let changed_path_count = status
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    let is_main_worktree = paths_equal(&git_dir, &git_common_dir);
    Ok(GitWorktreeState {
        repository_root,
        worktree_root,
        git_common_dir,
        git_dir,
        branch,
        head,
        dirty: changed_path_count > 0,
        changed_path_count,
        is_main_worktree,
    })
}

fn inspect_optional_git_worktree(path: &Path) -> Result<Option<GitWorktreeState>> {
    let path = absolute_path(path)?;
    let probe = git_inspection_command(&path)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .with_context(|| format!("failed to invoke git in {}", path.display()))?;
    if probe.status.success() {
        return Ok(Some(inspect_git_worktree(&path)?));
    }
    let stderr = String::from_utf8_lossy(&probe.stderr).to_lowercase();
    if stderr.contains("not a git repository") {
        return Ok(None);
    }
    bail!(
        "git rev-parse --is-inside-work-tree failed in {}: {}",
        path.display(),
        String::from_utf8_lossy(&probe.stderr).trim()
    )
}

fn parse_worktree_porcelain(input: &str) -> Vec<DiscoveredWorktree> {
    let mut worktrees = Vec::new();
    let mut current: Option<DiscoveredWorktree> = None;
    for line in input.lines().chain(std::iter::once("")) {
        if line.is_empty() {
            if let Some(record) = current.take() {
                worktrees.push(record);
            }
            continue;
        }
        if let Some(value) = line.strip_prefix("worktree ") {
            if let Some(record) = current.take() {
                worktrees.push(record);
            }
            current = Some(DiscoveredWorktree {
                path: value.to_string(),
                head: String::new(),
                branch: None,
                detached: false,
                bare: false,
                locked: false,
                prunable: false,
            });
        } else if let Some(record) = current.as_mut() {
            if let Some(value) = line.strip_prefix("HEAD ") {
                record.head = value.to_string();
            } else if let Some(value) = line.strip_prefix("branch ") {
                record.branch = Some(value.trim_start_matches("refs/heads/").to_string());
            } else if line == "detached" {
                record.detached = true;
            } else if line == "bare" {
                record.bare = true;
            } else if line.starts_with("locked") {
                record.locked = true;
            } else if line.starts_with("prunable") {
                record.prunable = true;
            }
        }
    }
    worktrees
}

fn git_output(root: &Path, args: &[&str]) -> Result<String> {
    let output = git_inspection_command(root)
        .args(args)
        .output()
        .with_context(|| format!("failed to invoke git in {}", root.display()))?;
    if !output.status.success() {
        bail!(
            "git {} failed in {}: {}",
            args.join(" "),
            root.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_optional_output(root: &Path, args: &[&str]) -> Result<Option<String>> {
    let output = git_inspection_command(root)
        .args(args)
        .output()
        .with_context(|| format!("failed to invoke git in {}", root.display()))?;
    if output.status.success() {
        let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok((!value.is_empty()).then_some(value))
    } else if output.status.code() == Some(1) {
        Ok(None)
    } else {
        bail!(
            "git {} failed in {}: {}",
            args.join(" "),
            root.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )
    }
}

fn git_inspection_command(root: &Path) -> Command {
    let program = trusted_system_command("git").unwrap_or_else(|| PathBuf::from("git"));
    let mut command = Command::new(program);
    command
        .env_clear()
        .env(
            "PATH",
            crate::brand::env_var_os("PATH").unwrap_or_else(worktree_default_git_path),
        )
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_SYSTEM", worktree_git_null_config_path())
        .env("GIT_CONFIG_GLOBAL", worktree_git_null_config_path())
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .arg("-C")
        .arg(root);
    #[cfg(windows)]
    if let Some(system_root) = crate::brand::env_var_os("SystemRoot") {
        command.env("SystemRoot", system_root);
    }
    command
}

fn worktree_default_git_path() -> std::ffi::OsString {
    if cfg!(windows) {
        std::ffi::OsString::from("C:\\Windows\\System32")
    } else {
        std::ffi::OsString::from("/usr/local/bin:/usr/bin:/bin")
    }
}

fn worktree_git_null_config_path() -> &'static str {
    if cfg!(windows) {
        "NUL"
    } else {
        "/dev/null"
    }
}

fn resolve_git_path(root: &Path, value: &str) -> Result<PathBuf> {
    let path = PathBuf::from(value);
    let path = if path.is_absolute() {
        path
    } else {
        root.join(path)
    };
    Ok(process_compatible_path(
        &fs::canonicalize(&path)
            .with_context(|| format!("failed to resolve {}", path.display()))?,
    ))
}

fn ensure_valid_branch_name(repository: &Path, branch: &str) -> Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .arg("check-ref-format")
        .arg("--branch")
        .arg(branch)
        .output()?;
    if !output.status.success() {
        bail!("invalid Git branch name: {branch}");
    }
    Ok(())
}

fn save_worktree_record(store: &FoundryStore, record: &WorktreeRecord) -> Result<()> {
    store.save_worktree_state(
        &record.id,
        &record.repository_root,
        &record.worktree_root,
        record.branch.as_deref(),
        &record.head,
        &serde_json::to_value(record)?,
    )
}

fn load_all_worktree_records(store: &FoundryStore) -> Result<Vec<WorktreeRecord>> {
    store
        .load_worktree_states()?
        .into_iter()
        .map(|value| serde_json::from_value(value).map_err(Into::into))
        .collect()
}

fn load_worktree_record(store: &FoundryStore, selector: &str) -> Result<WorktreeRecord> {
    let selector_path = Path::new(selector);
    for record in load_all_worktree_records(store)? {
        if record.id == selector || paths_equal(Path::new(&record.worktree_root), selector_path) {
            return Ok(record);
        }
    }
    bail!("registered worktree not found: {selector}")
}

fn find_worktree_record_by_path(
    store: &FoundryStore,
    path: &Path,
) -> Result<Option<WorktreeRecord>> {
    Ok(load_all_worktree_records(store)?
        .into_iter()
        .find(|record| paths_equal(Path::new(&record.worktree_root), path)))
}

fn remove_scope_binding_from_other_worktrees(
    store: &FoundryStore,
    selected_id: &str,
    workflow_id: &str,
    task_id: Option<&str>,
) -> Result<()> {
    for mut record in load_all_worktree_records(store)? {
        if record.id == selected_id {
            continue;
        }
        let before = record.bindings.len();
        record.bindings.retain(|binding| {
            binding.workflow_id != workflow_id || binding.task_id.as_deref() != task_id
        });
        if record.bindings.len() != before {
            record.updated_at = Utc::now().to_rfc3339();
            save_worktree_record(store, &record)?;
        }
    }
    Ok(())
}

fn select_binding(
    record: &WorktreeRecord,
    workflow_id: Option<&str>,
    task_id: Option<&str>,
) -> Option<WorktreeBinding> {
    if let Some(workflow_id) = workflow_id {
        if let Some(task_id) = task_id {
            if let Some(binding) = record.bindings.iter().find(|binding| {
                binding.workflow_id == workflow_id && binding.task_id.as_deref() == Some(task_id)
            }) {
                return Some(binding.clone());
            }
        }
        return record
            .bindings
            .iter()
            .find(|binding| binding.workflow_id == workflow_id && binding.task_id.is_none())
            .cloned();
    }
    if record.bindings.len() == 1 {
        return record.bindings.first().cloned();
    }
    None
}

fn decide(
    decisions: &mut Vec<WorktreeGuardrailDecision>,
    blockers: &mut Vec<String>,
    id: &str,
    allowed: bool,
    detail: String,
) {
    decisions.push(WorktreeGuardrailDecision {
        id: id.to_string(),
        decision: if allowed { "allowed" } else { "blocked" }.to_string(),
        detail: detail.clone(),
    });
    if !allowed {
        blockers.push(format!("{id}: {detail}"));
    }
}

struct SandboxRuntimePaths {
    worktree_root: PathBuf,
    sandbox_root: PathBuf,
    working_directory: PathBuf,
}

fn sandbox_runtime_paths(
    runtime: &str,
    worktree_root: &Path,
    sandbox_root: &Path,
    working_directory: &Path,
) -> Result<SandboxRuntimePaths> {
    if runtime != "bubblewrap" {
        return Ok(SandboxRuntimePaths {
            worktree_root: worktree_root.to_path_buf(),
            sandbox_root: sandbox_root.to_path_buf(),
            working_directory: working_directory.to_path_buf(),
        });
    }

    let guest_root = PathBuf::from(BUBBLEWRAP_WORKTREE_ROOT);
    let sandbox_relative = sandbox_root.strip_prefix(worktree_root).with_context(|| {
        format!(
            "sandbox root {} is not inside worktree {}",
            sandbox_root.display(),
            worktree_root.display()
        )
    })?;
    let working_relative = working_directory
        .strip_prefix(worktree_root)
        .with_context(|| {
            format!(
                "working directory {} is not inside worktree {}",
                working_directory.display(),
                worktree_root.display()
            )
        })?;
    Ok(SandboxRuntimePaths {
        worktree_root: guest_root.clone(),
        sandbox_root: guest_root.join(sandbox_relative),
        working_directory: guest_root.join(working_relative),
    })
}

fn remap_bubblewrap_payload_argument(
    argument: &str,
    worktree_root: &Path,
    runtime_worktree_root: &Path,
) -> String {
    let path = Path::new(argument);
    if !path.is_absolute() {
        return argument.to_string();
    }
    let Ok(resolved_path) = fs::canonicalize(path) else {
        return argument.to_string();
    };
    let Ok(relative_path) = resolved_path.strip_prefix(worktree_root) else {
        return argument.to_string();
    };
    runtime_worktree_root
        .join(relative_path)
        .display()
        .to_string()
}

fn bubblewrap_command(
    bubblewrap_path: &Path,
    worktree_root: &Path,
    sandbox_root: &Path,
    runtime_paths: &SandboxRuntimePaths,
    network_policy: &str,
    command: &[String],
) -> Vec<String> {
    let mut args = vec![
        bubblewrap_path.display().to_string(),
        "--die-with-parent".to_string(),
        "--new-session".to_string(),
        "--unshare-pid".to_string(),
        "--unshare-ipc".to_string(),
        "--unshare-uts".to_string(),
        "--clearenv".to_string(),
    ];
    if network_policy == "deny" {
        args.push("--unshare-net".to_string());
    }
    args.extend([
        "--proc".to_string(),
        "/proc".to_string(),
        "--dev".to_string(),
        "/dev".to_string(),
    ]);
    for path in ["/usr", "/bin", "/sbin", "/lib", "/lib64"] {
        if Path::new(path).exists() {
            args.extend(["--ro-bind".to_string(), path.to_string(), path.to_string()]);
        }
    }
    for path in [
        "/etc/hosts",
        "/etc/alternatives",
        "/etc/resolv.conf",
        "/etc/nsswitch.conf",
        "/etc/ssl/certs",
        "/etc/ld.so.cache",
        "/etc/passwd",
        "/etc/group",
    ] {
        if Path::new(path).exists() {
            args.extend(["--ro-bind".to_string(), path.to_string(), path.to_string()]);
        }
    }
    args.extend([
        "--dir".to_string(),
        runtime_paths.worktree_root.display().to_string(),
        "--ro-bind".to_string(),
        worktree_root.display().to_string(),
        runtime_paths.worktree_root.display().to_string(),
        "--bind".to_string(),
        sandbox_root.display().to_string(),
        runtime_paths.sandbox_root.display().to_string(),
        "--bind".to_string(),
        sandbox_root.join("tmp").display().to_string(),
        "/tmp".to_string(),
        "--dir".to_string(),
        "/home".to_string(),
        "--dir".to_string(),
        BUBBLEWRAP_HOME.to_string(),
        "--bind".to_string(),
        sandbox_root.join("home").display().to_string(),
        BUBBLEWRAP_HOME.to_string(),
        "--setenv".to_string(),
        "HOME".to_string(),
        BUBBLEWRAP_HOME.to_string(),
        "--setenv".to_string(),
        "TMPDIR".to_string(),
        "/tmp".to_string(),
        "--setenv".to_string(),
        "PATH".to_string(),
        "/usr/local/bin:/usr/bin:/bin".to_string(),
        "--chdir".to_string(),
        runtime_paths.working_directory.display().to_string(),
        "--".to_string(),
    ]);
    let resolved_worktree_root =
        fs::canonicalize(worktree_root).unwrap_or_else(|_| worktree_root.to_path_buf());
    if let Some((executable, arguments)) = command.split_first() {
        args.push(remap_bubblewrap_payload_argument(
            executable,
            &resolved_worktree_root,
            &runtime_paths.worktree_root,
        ));
        args.extend(arguments.iter().cloned());
    }
    args
}

struct CommandExecution {
    duration_ms: u128,
    timed_out: bool,
    stop_requested: bool,
    exit_code: Option<i32>,
    child_started: bool,
    error: Option<String>,
    stdout: BoundedStreamEvidence,
    stderr: BoundedStreamEvidence,
}

fn execute_bounded_command(plan: &WorktreeSandboxPlan, sandbox_root: &Path) -> CommandExecution {
    execute_bounded_command_for_lifecycle(plan, sandbox_root, None)
}

fn execute_bounded_command_for_lifecycle(
    plan: &WorktreeSandboxPlan,
    sandbox_root: &Path,
    lifecycle: Option<(&FoundryStore, &str)>,
) -> CommandExecution {
    let started = Instant::now();
    let baseline_children = match prepare_descendant_tracking() {
        Ok(children) => children,
        Err(error) => return command_execution_failure(started, false, error),
    };
    let config_snapshot = match load_worktree_config(Path::new(&plan.worktree_root)) {
        Ok(snapshot) => snapshot,
        Err(error) => return command_execution_failure(started, false, error),
    };
    if config_snapshot.sha256 != plan.config_sha256 {
        return command_execution_failure(
            started,
            false,
            anyhow::anyhow!(
                "worktree configuration changed between sandbox planning and execution"
            ),
        );
    }
    let mut launch_command = plan.launch_command.clone();
    if plan.runtime == "bubblewrap" {
        let Some(separator) = launch_command.iter().position(|argument| argument == "--") else {
            return command_execution_failure(
                started,
                false,
                anyhow::anyhow!("Bubblewrap launch command is missing its payload separator"),
            );
        };
        let mut environment_arguments = Vec::new();
        for name in &plan.inherited_environment {
            if matches!(name.as_str(), "PATH" | "HOME" | "TMPDIR") {
                continue;
            }
            if let Some(value) = crate::brand::env_var_os(name) {
                if let Err(error) = ensure_inherited_environment_value_safe(name, &value) {
                    return command_execution_failure(started, false, error);
                }
                environment_arguments.extend([
                    "--setenv".to_string(),
                    name.clone(),
                    value.to_string_lossy().to_string(),
                ]);
            }
        }
        for (name, value) in &config_snapshot.config.sandbox.environment {
            environment_arguments.extend(["--setenv".to_string(), name.clone(), value.clone()]);
        }
        environment_arguments.extend([
            "--setenv".to_string(),
            "FOUNDRY_WORKTREE_ROOT".to_string(),
            plan.runtime_worktree_root.clone(),
            "--setenv".to_string(),
            "FOUNDRY_SANDBOX_ROOT".to_string(),
            plan.runtime_sandbox_root.clone(),
            "--setenv".to_string(),
            "FOUNDRY_SANDBOX_PURPOSE".to_string(),
            plan.purpose.clone(),
            "--setenv".to_string(),
            "FOUNDRY_STORE_PATH".to_string(),
            plan.foundry_store_path.clone(),
        ]);
        if let Some(binding) = &plan.binding {
            environment_arguments.extend([
                "--setenv".to_string(),
                "FOUNDRY_WORKFLOW_ID".to_string(),
                binding.workflow_id.clone(),
            ]);
            if let Some(task_id) = &binding.task_id {
                environment_arguments.extend([
                    "--setenv".to_string(),
                    "FOUNDRY_TASK_ID".to_string(),
                    task_id.clone(),
                ]);
            }
        }
        launch_command.splice(separator..separator, environment_arguments);
    }
    let Some(executable) = launch_command.first() else {
        return command_execution_failure(
            started,
            false,
            anyhow::anyhow!("sandbox launch command is empty"),
        );
    };
    let mut command = Command::new(executable);
    command
        .args(&launch_command[1..])
        .current_dir(&plan.working_directory)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if plan.runtime != "bubblewrap" {
        for name in &plan.inherited_environment {
            if let Some(value) = crate::brand::env_var_os(name) {
                if let Err(error) = ensure_inherited_environment_value_safe(name, &value) {
                    return command_execution_failure(started, false, error);
                }
                command.env(name, value);
            }
        }
        for (name, value) in &config_snapshot.config.sandbox.environment {
            command.env(name, value);
        }
    }
    let record = plan
        .binding
        .as_ref()
        .map(|binding| (binding.workflow_id.as_str(), binding.task_id.as_deref()));
    command.env("FOUNDRY_WORKTREE_ROOT", &plan.runtime_worktree_root);
    command.env("FOUNDRY_SANDBOX_ROOT", &plan.runtime_sandbox_root);
    command.env("FOUNDRY_SANDBOX_PURPOSE", &plan.purpose);
    command.env("FOUNDRY_STORE_PATH", &plan.foundry_store_path);
    command.env("TMPDIR", sandbox_root.join("tmp"));
    if let Some((workflow_id, task_id)) = record {
        command.env("FOUNDRY_WORKFLOW_ID", workflow_id);
        if let Some(task_id) = task_id {
            command.env("FOUNDRY_TASK_ID", task_id);
        }
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        let expected_parent = std::process::id() as libc::pid_t;
        // SAFETY: the closure only invokes async-signal-safe libc calls between fork and exec.
        unsafe {
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::getppid() != expected_parent {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Interrupted,
                        "sandbox supervisor exited before payload exec",
                    ));
                }
                Ok(())
            });
        }
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return command_execution_failure(
                started,
                false,
                anyhow::anyhow!(
                    "failed to launch sandbox command `{}`: {error}",
                    plan.launch_command.join(" ")
                ),
            )
        }
    };
    if let Some((store, sandbox_id)) = lifecycle {
        if let Err(error) = mark_lifecycle_payload_started(store, sandbox_id, child.id()) {
            let _ = terminate_process_tree(&mut child, false, &BTreeSet::new());
            let _ = child.wait();
            return command_execution_failure(started, true, error);
        }
    }
    let Some(stdout) = child.stdout.take() else {
        let _ = terminate_process_tree(&mut child, false, &BTreeSet::new());
        let _ = child.wait();
        return command_execution_failure(
            started,
            true,
            anyhow::anyhow!("sandbox stdout unavailable"),
        );
    };
    let Some(stderr) = child.stderr.take() else {
        let _ = terminate_process_tree(&mut child, false, &BTreeSet::new());
        let _ = child.wait();
        return command_execution_failure(
            started,
            true,
            anyhow::anyhow!("sandbox stderr unavailable"),
        );
    };
    let max_output = plan.max_output_bytes;
    let stdout_reader = thread::spawn(move || capture_stream(stdout, max_output));
    let stderr_reader = thread::spawn(move || capture_stream(stderr, max_output));
    let timeout = Duration::from_secs(plan.max_command_seconds.max(1));
    let mut direct_status = None;
    let mut timed_out = false;
    let mut stop_requested = false;
    let mut execution_error = None;
    let mut managed_descendants = BTreeSet::new();
    let mut persisted_descendants = BTreeSet::new();
    let mut last_lifecycle_poll = Instant::now();
    loop {
        if direct_status.is_none() {
            match child.try_wait() {
                Ok(status) => direct_status = status,
                Err(error) => {
                    execution_error = Some(anyhow::anyhow!(
                        "failed to inspect sandbox process status: {error}"
                    ));
                    let _ = terminate_process_tree(&mut child, false, &managed_descendants);
                    direct_status = child.wait().ok();
                    break;
                }
            }
        }
        match refresh_managed_descendants(child.id(), &baseline_children, &mut managed_descendants)
        {
            Ok(()) => {}
            Err(error) => {
                execution_error = Some(error);
                let _ = terminate_process_tree(
                    &mut child,
                    direct_status.is_some(),
                    &managed_descendants,
                );
                if direct_status.is_none() {
                    direct_status = child.wait().ok();
                }
                break;
            }
        }
        if let Some((store, sandbox_id)) = lifecycle {
            if managed_descendants != persisted_descendants {
                if let Err(error) =
                    mark_lifecycle_payload_descendants(store, sandbox_id, &managed_descendants)
                {
                    execution_error = Some(error);
                    let _ = terminate_process_tree(
                        &mut child,
                        direct_status.is_some(),
                        &managed_descendants,
                    );
                    if direct_status.is_none() {
                        direct_status = child.wait().ok();
                    }
                    break;
                }
                persisted_descendants = managed_descendants.clone();
            }
            if last_lifecycle_poll.elapsed() >= Duration::from_millis(100) {
                match lifecycle_stop_requested(store, sandbox_id) {
                    Ok(false) => {}
                    Ok(true) => {
                        stop_requested = true;
                        let _ = terminate_process_tree(
                            &mut child,
                            direct_status.is_some(),
                            &managed_descendants,
                        );
                        if direct_status.is_none() {
                            direct_status = child.wait().ok();
                        }
                        break;
                    }
                    Err(error) => {
                        execution_error = Some(error);
                        let _ = terminate_process_tree(
                            &mut child,
                            direct_status.is_some(),
                            &managed_descendants,
                        );
                        if direct_status.is_none() {
                            direct_status = child.wait().ok();
                        }
                        break;
                    }
                }
                last_lifecycle_poll = Instant::now();
            }
        }
        if direct_status.is_some()
            && stdout_reader.is_finished()
            && stderr_reader.is_finished()
            && !process_group_alive(child.id())
            && managed_descendants.is_empty()
        {
            break;
        }
        if started.elapsed() >= timeout {
            timed_out = true;
            if let Err(error) =
                terminate_process_tree(&mut child, direct_status.is_some(), &managed_descendants)
            {
                execution_error = Some(error);
            }
            if direct_status.is_none() {
                direct_status = child.wait().ok();
            }
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }
    if timed_out || execution_error.is_some() {
        let drain_started = Instant::now();
        while (!stdout_reader.is_finished() || !stderr_reader.is_finished())
            && drain_started.elapsed() < Duration::from_secs(1)
        {
            let _ = refresh_managed_descendants(
                child.id(),
                &baseline_children,
                &mut managed_descendants,
            );
            let _ =
                terminate_process_tree(&mut child, direct_status.is_some(), &managed_descendants);
            thread::sleep(Duration::from_millis(20));
        }
    }
    let stdout = if stdout_reader.is_finished() {
        stdout_reader
            .join()
            .map_err(|_| anyhow::anyhow!("sandbox stdout reader panicked"))
            .and_then(|result| result)
    } else {
        Err(anyhow::anyhow!(
            "sandbox stdout remained open after descendant termination"
        ))
    };
    let stderr = if stderr_reader.is_finished() {
        stderr_reader
            .join()
            .map_err(|_| anyhow::anyhow!("sandbox stderr reader panicked"))
            .and_then(|result| result)
    } else {
        Err(anyhow::anyhow!(
            "sandbox stderr remained open after descendant termination"
        ))
    };
    let stdout = match stdout {
        Ok(stdout) => stdout,
        Err(error) => {
            execution_error.get_or_insert(error);
            empty_stream_evidence()
        }
    };
    let stderr = match stderr {
        Ok(stderr) => stderr,
        Err(error) => {
            execution_error.get_or_insert(error);
            empty_stream_evidence()
        }
    };
    let error = execution_error.map(|error| {
        sanitize_prompt_secrets(
            &error.to_string(),
            sandbox_secret_sanitization_options("sandbox_error"),
        )
        .sanitized_text
    });
    CommandExecution {
        duration_ms: started.elapsed().as_millis(),
        timed_out,
        stop_requested,
        exit_code: direct_status.and_then(|status| status.code()),
        child_started: true,
        error,
        stdout,
        stderr,
    }
}

fn command_execution_failure(
    started: Instant,
    child_started: bool,
    error: anyhow::Error,
) -> CommandExecution {
    let error = sanitize_prompt_secrets(
        &error.to_string(),
        sandbox_secret_sanitization_options("sandbox_error"),
    )
    .sanitized_text;
    CommandExecution {
        duration_ms: started.elapsed().as_millis(),
        timed_out: false,
        stop_requested: false,
        exit_code: None,
        child_started,
        error: Some(error.clone()),
        stdout: empty_stream_evidence(),
        stderr: stream_evidence_from_text(&error),
    }
}

fn capture_stream(mut stream: impl Read, max_output: usize) -> Result<BoundedStreamEvidence> {
    let mut hasher = Sha256::new();
    let mut captured = Vec::new();
    let mut total = 0usize;
    let mut buffer = [0u8; 8192];
    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total += read;
        hasher.update(&buffer[..read]);
        let remaining = max_output.saturating_sub(captured.len());
        captured.extend_from_slice(&buffer[..read.min(remaining)]);
    }
    let raw_content = String::from_utf8_lossy(&captured).to_string();
    let sanitized = sanitize_prompt_secrets(
        &raw_content,
        sandbox_secret_sanitization_options("sandbox_output"),
    );
    Ok(BoundedStreamEvidence {
        sha256: format!("{:x}", hasher.finalize()),
        total_bytes: total,
        captured_bytes: captured.len(),
        truncated: total > captured.len(),
        redaction_count: sanitized.detection_count,
        content: sanitized.sanitized_text,
    })
}

fn stream_evidence_from_text(content: &str) -> BoundedStreamEvidence {
    let sanitized = sanitize_prompt_secrets(
        content,
        sandbox_secret_sanitization_options("sandbox_error"),
    );
    BoundedStreamEvidence {
        sha256: hex_sha256(content.as_bytes()),
        total_bytes: content.len(),
        captured_bytes: content.len(),
        truncated: false,
        redaction_count: sanitized.detection_count,
        content: sanitized.sanitized_text,
    }
}

fn prepare_descendant_tracking() -> Result<BTreeSet<u32>> {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: PR_SET_CHILD_SUBREAPER only changes orphan reparenting for this process.
        if unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1) } != 0 {
            return Err(std::io::Error::last_os_error())
                .context("failed to enable sandbox descendant tracking");
        }
        let supervisor_pid = std::process::id();
        Ok(linux_process_table()?
            .into_iter()
            .filter_map(|(pid, (parent_pid, state))| {
                (parent_pid == supervisor_pid && state != 'Z').then_some(pid)
            })
            .collect())
    }
    #[cfg(not(target_os = "linux"))]
    {
        Ok(BTreeSet::new())
    }
}

fn refresh_managed_descendants(
    root_pid: u32,
    baseline_children: &BTreeSet<u32>,
    managed: &mut BTreeSet<u32>,
) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let table = linux_process_table()?;
        let supervisor_pid = std::process::id();
        let mut lineage = BTreeSet::from([root_pid]);
        lineage.extend(
            managed
                .iter()
                .copied()
                .filter(|pid| table.get(pid).is_some_and(|(_, state)| *state != 'Z')),
        );
        lineage.extend(table.iter().filter_map(|(pid, (parent_pid, state))| {
            (*pid != root_pid
                && *parent_pid == supervisor_pid
                && *state != 'Z'
                && !baseline_children.contains(pid))
            .then_some(*pid)
        }));

        loop {
            let before = lineage.len();
            for (pid, (parent_pid, state)) in &table {
                if *state != 'Z' && lineage.contains(parent_pid) {
                    lineage.insert(*pid);
                }
            }
            if lineage.len() == before {
                break;
            }
        }
        lineage.remove(&root_pid);
        lineage.retain(|pid| table.get(pid).is_some_and(|(_, state)| *state != 'Z'));
        *managed = lineage;
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (root_pid, baseline_children);
        managed.clear();
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn linux_process_table() -> Result<BTreeMap<u32, (u32, char)>> {
    let mut table = BTreeMap::new();
    for entry in fs::read_dir("/proc").context("failed to inspect Linux process table")? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        let stat = match fs::read_to_string(entry.path().join("stat")) {
            Ok(stat) => stat,
            Err(_) => continue,
        };
        let Some((_, fields)) = stat.rsplit_once(") ") else {
            continue;
        };
        let mut fields = fields.split_whitespace();
        let Some(state) = fields.next().and_then(|value| value.chars().next()) else {
            continue;
        };
        let Some(parent_pid) = fields.next().and_then(|value| value.parse::<u32>().ok()) else {
            continue;
        };
        table.insert(pid, (parent_pid, state));
    }
    Ok(table)
}

fn process_group_alive(process_group_id: u32) -> bool {
    #[cfg(unix)]
    {
        let Ok(process_group_id) = i32::try_from(process_group_id) else {
            return false;
        };
        // SAFETY: signal 0 performs a read-only existence/permission probe.
        let result = unsafe { libc::kill(-process_group_id, 0) };
        if result == 0 {
            return true;
        }
        std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(not(unix))]
    {
        let _ = process_group_id;
        false
    }
}

fn process_alive(process_id: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        if let Ok(stat) = fs::read_to_string(format!("/proc/{process_id}/stat")) {
            if stat
                .rsplit_once(") ")
                .and_then(|(_, fields)| fields.split_whitespace().next())
                == Some("Z")
            {
                return false;
            }
        }
    }
    #[cfg(unix)]
    {
        let Ok(process_id) = i32::try_from(process_id) else {
            return false;
        };
        // SAFETY: signal 0 performs a read-only existence/permission probe.
        let result = unsafe { libc::kill(process_id, 0) };
        if result == 0 {
            return true;
        }
        std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(not(unix))]
    {
        let _ = process_id;
        true
    }
}

#[cfg(unix)]
fn signal_number(signal: &str) -> Result<i32> {
    match signal {
        "-KILL" => Ok(libc::SIGKILL),
        "-TERM" => Ok(libc::SIGTERM),
        _ => bail!("unsupported sandbox process signal `{signal}`"),
    }
}

fn signal_process_group(process_group_id: u32, signal: &str) -> Result<bool> {
    #[cfg(unix)]
    {
        let process_group_id = i32::try_from(process_group_id)
            .context("sandbox process group id exceeds platform range")?;
        // SAFETY: the negative pid scopes the requested signal to the payload process group.
        let result = unsafe { libc::kill(-process_group_id, signal_number(signal)?) };
        if result == 0 {
            return Ok(true);
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            return Ok(false);
        }
        Err(error).context("failed to signal sandbox process group")
    }
    #[cfg(not(unix))]
    {
        let _ = (process_group_id, signal);
        Ok(false)
    }
}

fn signal_process(process_id: u32, signal: &str) -> Result<bool> {
    #[cfg(unix)]
    {
        let process_id = i32::try_from(process_id)
            .context("sandbox descendant process id exceeds platform range")?;
        // SAFETY: the positive pid scopes the requested signal to one tracked descendant.
        let result = unsafe { libc::kill(process_id, signal_number(signal)?) };
        if result == 0 {
            return Ok(true);
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            return Ok(false);
        }
        Err(error).context("failed to signal sandbox descendant process")
    }
    #[cfg(not(unix))]
    {
        let _ = (process_id, signal);
        Ok(false)
    }
}

fn terminate_persisted_sandbox_processes(
    report: &WorktreeSandboxLifecycleReport,
    include_supervisor: bool,
) -> Vec<String> {
    let mut errors = Vec::new();
    if let Some(payload_pid) = report.payload_pid {
        if let Err(error) = signal_process_group(payload_pid, "-KILL") {
            errors.push(format!(
                "failed to signal sandbox payload process group {payload_pid}: {error}"
            ));
        }
    }
    for process_id in &report.payload_descendant_pids {
        if let Err(error) = signal_process(*process_id, "-KILL") {
            errors.push(format!(
                "failed to signal sandbox descendant process {process_id}: {error}"
            ));
        }
    }
    if include_supervisor {
        if let Some(supervisor_pid) = report.supervisor_pid {
            if let Err(error) = signal_process(supervisor_pid, "-KILL") {
                errors.push(format!(
                    "failed to signal sandbox supervisor process {supervisor_pid}: {error}"
                ));
            }
        }
    }
    errors
}

fn persisted_sandbox_processes_alive(report: &WorktreeSandboxLifecycleReport) -> Vec<String> {
    let mut alive = Vec::new();
    if let Some(payload_pid) = report.payload_pid.filter(|pid| process_group_alive(*pid)) {
        alive.push(format!(
            "sandbox payload process group {payload_pid} remained alive after termination"
        ));
    }
    alive.extend(
        report
            .payload_descendant_pids
            .iter()
            .filter(|process_id| process_alive(**process_id))
            .map(|process_id| {
                format!("sandbox descendant process {process_id} remained alive after termination")
            }),
    );
    if let Some(supervisor_pid) = report.supervisor_pid.filter(|pid| process_alive(*pid)) {
        alive.push(format!(
            "sandbox supervisor process {supervisor_pid} remained alive after termination"
        ));
    }
    alive
}

fn terminate_process_tree(
    child: &mut std::process::Child,
    direct_child_exited: bool,
    managed_descendants: &BTreeSet<u32>,
) -> Result<()> {
    let group_signalled = signal_process_group(child.id(), "-KILL")?;
    for process_id in managed_descendants {
        signal_process(*process_id, "-KILL")?;
    }
    if !group_signalled && !direct_child_exited {
        child
            .kill()
            .context("failed to terminate sandbox process")?;
    }
    Ok(())
}

fn empty_stream_evidence() -> BoundedStreamEvidence {
    BoundedStreamEvidence {
        sha256: hex_sha256(b""),
        total_bytes: 0,
        captured_bytes: 0,
        truncated: false,
        redaction_count: 0,
        content: String::new(),
    }
}

fn sandbox_secret_sanitization_options(scope: &str) -> SecretSanitizationOptions {
    SecretSanitizationOptions {
        scope: scope.to_string(),
        enable_regex: true,
        enable_entropy: true,
        enable_local_ai_fallback: false,
        allow_external_ai: false,
        entropy_threshold: 4.2,
    }
}

fn ensure_inherited_environment_value_safe(name: &str, value: &std::ffi::OsStr) -> Result<()> {
    let value = value.to_string_lossy();
    let report = sanitize_prompt_secrets(
        &value,
        sandbox_secret_sanitization_options("sandbox_inherited_environment"),
    );
    if report.detection_count > 0 {
        bail!(
            "inherited sandbox environment `{name}` contains a detected secret; use authorized vault injection"
        );
    }
    Ok(())
}

fn record_sandbox_receipt(store: &FoundryStore, receipt: &WorktreeSandboxReceipt) -> Result<()> {
    let workflow_id = receipt
        .binding
        .as_ref()
        .map(|binding| binding.workflow_id.as_str())
        .unwrap_or("_system");
    store.record_event(
        workflow_id,
        "worktree_sandbox_execution",
        &serde_json::to_value(receipt)?,
    )?;
    Ok(())
}

fn resolve_relative_inside(root: &Path, relative: &str) -> Result<PathBuf> {
    let relative = Path::new(relative);
    if relative.is_absolute() {
        bail!("worktree path must be relative: {}", relative.display());
    }
    let mut safe = PathBuf::new();
    for component in relative.components() {
        match component {
            Component::Normal(value) => safe.push(value),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("worktree path escapes its root: {}", relative.display())
            }
        }
    }
    let canonical_root = fs::canonicalize(root)
        .with_context(|| format!("failed to resolve worktree root {}", root.display()))?;
    let candidate = canonical_root.join(&safe);
    let mut cursor = canonical_root.clone();
    for component in safe.components() {
        let Component::Normal(value) = component else {
            continue;
        };
        cursor.push(value);
        match fs::symlink_metadata(&cursor) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!(
                    "worktree path contains a symlink and is rejected: {}",
                    cursor.display()
                )
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to inspect {}", cursor.display()))
            }
        }
    }
    if candidate.exists() {
        let resolved = fs::canonicalize(&candidate)
            .with_context(|| format!("failed to resolve {}", candidate.display()))?;
        if !resolved.starts_with(&canonical_root) {
            bail!("worktree path escapes its root: {}", candidate.display());
        }
    }
    Ok(candidate)
}

fn normalize_guard_path(value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("worktree guardrail path cannot be empty");
    }
    let path = Path::new(trimmed);
    if path.is_absolute() {
        bail!("worktree guardrail path must be relative: {trimmed}");
    }
    let wants_directory = trimmed.ends_with('/');
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => parts.push(value.to_string_lossy().to_string()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("worktree guardrail path escapes its root: {trimmed}")
            }
        }
    }
    if parts.is_empty() {
        return Ok(".".to_string());
    }
    let mut normalized = parts.join("/");
    if wants_directory {
        normalized.push('/');
    }
    Ok(normalized)
}

fn guard_scope_kind(root: &Path, original: &str, normalized: &str) -> String {
    if original.trim().ends_with('/') || root.join(normalized.trim_end_matches('/')).is_dir() {
        "directory".to_string()
    } else {
        "file".to_string()
    }
}

fn guard_scope_allows(scope: &str, requested: &str, requested_kind: &str) -> bool {
    let Ok(scope) = normalize_guard_path(scope) else {
        return false;
    };
    if scope == "." {
        return true;
    }
    let scope_directory = scope.ends_with('/');
    let scope_value = scope.trim_end_matches('/');
    let requested_value = requested.trim_end_matches('/');
    if scope_directory {
        requested_value == scope_value || requested_value.starts_with(&format!("{scope_value}/"))
    } else {
        requested_kind == "file" && requested_value == scope_value
    }
}

fn guard_scopes_overlap(protected_scope: &str, requested: &str, requested_kind: &str) -> bool {
    let Ok(protected) = normalize_guard_path(protected_scope) else {
        return true;
    };
    if protected == "." {
        return true;
    }
    let protected_value = protected.trim_end_matches('/');
    let requested_value = requested.trim_end_matches('/');
    let protected_directory = protected.ends_with('/');
    let requested_directory = requested_kind == "directory" || requested.ends_with('/');
    protected_value == requested_value
        || (protected_directory && requested_value.starts_with(&format!("{protected_value}/")))
        || (requested_directory && protected_value.starts_with(&format!("{requested_value}/")))
}

fn valid_environment_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn dangerous_environment_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "PATH"
            | "HOME"
            | "TMPDIR"
            | "BASH_ENV"
            | "ENV"
            | "RUSTC_WRAPPER"
            | "RUSTC_WORKSPACE_WRAPPER"
            | "CARGO_HOME"
            | "RUSTUP_HOME"
            | "PYTHONPATH"
            | "NODE_OPTIONS"
    ) || upper.starts_with("LD_")
        || upper.starts_with("DYLD_")
}

fn sensitive_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    [
        "TOKEN",
        "SECRET",
        "PASSWORD",
        "PASSWD",
        "API_KEY",
        "PRIVATE_KEY",
        "CREDENTIAL",
    ]
    .iter()
    .any(|marker| upper.contains(marker))
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn absolute_store_path(store: &FoundryStore) -> PathBuf {
    if store.path().is_absolute() {
        store.path().to_path_buf()
    } else {
        std::env::current_dir()
            .map(|directory| directory.join(store.path()))
            .unwrap_or_else(|_| store.path().to_path_buf())
    }
}

fn stable_worktree_id(path: &Path) -> String {
    let digest = hex_sha256(path.display().to_string().as_bytes());
    format!("wt_{}", &digest[..16])
}

fn validate_worktree_id(id: &str) -> Result<()> {
    if id.is_empty()
        || !id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
    {
        bail!("worktree id must contain only ASCII letters, numbers, `_` or `-`");
    }
    Ok(())
}

fn trusted_system_command(command: &str) -> Option<PathBuf> {
    ["/usr/bin", "/bin", "/usr/local/bin"]
        .into_iter()
        .map(|directory| Path::new(directory).join(command))
        .find(|candidate| candidate.is_file())
        .and_then(|candidate| fs::canonicalize(candidate).ok())
}

fn wildcard_matches(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    let parts = pattern.split('*').collect::<Vec<_>>();
    if parts.len() == 1 {
        return pattern == value;
    }
    let mut remainder = value;
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if index == 0 && !pattern.starts_with('*') {
            let Some(stripped) = remainder.strip_prefix(part) else {
                return false;
            };
            remainder = stripped;
            continue;
        }
        let Some(position) = remainder.find(part) else {
            return false;
        };
        remainder = &remainder[position + part.len()..];
    }
    pattern.ends_with('*') || remainder.is_empty()
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    let left =
        process_compatible_path(&fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf()));
    let right =
        process_compatible_path(&fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf()));
    left == right
}

#[cfg(windows)]
fn process_compatible_path(path: &Path) -> PathBuf {
    const VERBATIM_PREFIX: &[u16] = &[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16];
    const VERBATIM_UNC_PREFIX: &[u16] = &[
        b'\\' as u16,
        b'\\' as u16,
        b'?' as u16,
        b'\\' as u16,
        b'U' as u16,
        b'N' as u16,
        b'C' as u16,
        b'\\' as u16,
    ];
    let wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
    if let Some(rest) = wide.strip_prefix(VERBATIM_UNC_PREFIX) {
        let mut normalized = vec![b'\\' as u16, b'\\' as u16];
        normalized.extend_from_slice(rest);
        return PathBuf::from(OsString::from_wide(&normalized));
    }
    if let Some(rest) = wide.strip_prefix(VERBATIM_PREFIX) {
        return PathBuf::from(OsString::from_wide(rest));
    }
    path.to_path_buf()
}

#[cfg(not(windows))]
fn process_compatible_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}

fn worktree_config_schema_version() -> String {
    WORKTREE_CONFIG_SCHEMA_VERSION.to_string()
}

fn worktree_record_schema_version() -> String {
    WORKTREE_RECORD_SCHEMA_VERSION.to_string()
}

fn worktree_binding_schema_version() -> String {
    WORKTREE_BINDING_SCHEMA_VERSION.to_string()
}

fn default_require_workflow_binding() -> bool {
    true
}

fn default_modifiable_paths() -> Vec<String> {
    vec![".".to_string()]
}

fn default_protected_paths() -> Vec<String> {
    vec![
        ".git/".to_string(),
        ".foundry/worktree.toml".to_string(),
        format!("{}/worktree.toml", crate::brand::LEGACY_STATE_DIRECTORY),
    ]
}

fn default_max_command_seconds() -> u64 {
    900
}

fn default_max_output_bytes() -> usize {
    1_048_576
}

fn worktree_identity_sha256(state: &GitWorktreeState) -> String {
    hex_sha256(
        format!(
            "{}\n{}\n{}\n{}",
            state.repository_root.display(),
            state.worktree_root.display(),
            state.git_common_dir.display(),
            state.git_dir.display()
        )
        .as_bytes(),
    )
}

fn redact_worktree_config_snapshot(mut snapshot: WorktreeConfigSnapshot) -> WorktreeConfigSnapshot {
    for value in snapshot.config.sandbox.environment.values_mut() {
        *value = "<redacted>".to_string();
    }
    for value in snapshot.config.settings.values_mut() {
        *value = toml::Value::String("<redacted>".to_string());
    }
    snapshot
}

fn default_sandbox_name() -> String {
    "internal".to_string()
}

fn default_sandbox_root() -> String {
    DEFAULT_SANDBOX_ROOT.to_string()
}

fn default_sandbox_runtime() -> String {
    "process".to_string()
}

fn default_working_directory() -> String {
    ".".to_string()
}

fn default_sandbox_purposes() -> Vec<String> {
    vec!["preview".to_string(), "test".to_string()]
}

fn default_network_policy() -> String {
    "inherit".to_string()
}

fn default_inherited_environment() -> Vec<String> {
    [
        "PATH",
        "HOME",
        "LANG",
        "LC_ALL",
        "TERM",
        "CARGO_HOME",
        "RUSTUP_HOME",
        "XDG_CACHE_HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_branch_matching_is_deterministic() {
        assert!(wildcard_matches("feature/*", "feature/worktrees"));
        assert!(wildcard_matches("*-preview", "local-preview"));
        assert!(!wildcard_matches("release/*", "feature/worktrees"));
    }

    #[test]
    fn relative_sandbox_paths_cannot_escape_worktree() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        assert_eq!(
            resolve_relative_inside(root, ".foundry/sandboxes/internal").unwrap(),
            fs::canonicalize(root)
                .unwrap()
                .join(".foundry/sandboxes/internal")
        );
        assert!(resolve_relative_inside(root, "../outside").is_err());
        assert!(resolve_relative_inside(root, "/outside").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn relative_sandbox_paths_reject_symlink_components() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("worktree");
        let outside = temp.path().join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        symlink(&outside, root.join("linked")).unwrap();

        let error = resolve_relative_inside(&root, "linked/cache").unwrap_err();
        assert!(error.to_string().contains("contains a symlink"));
    }

    #[test]
    fn bubblewrap_uses_stable_guest_paths_and_minimal_writable_mounts() {
        let worktree = Path::new("/tmp/foundry-worktree-smoke/worktree");
        let sandbox = worktree.join(".foundry/sandboxes/internal");
        let working_directory = worktree.join("src");
        let runtime_paths =
            sandbox_runtime_paths("bubblewrap", worktree, &sandbox, &working_directory).unwrap();
        assert_eq!(
            runtime_paths.worktree_root,
            Path::new(BUBBLEWRAP_WORKTREE_ROOT)
        );
        assert_eq!(
            runtime_paths.sandbox_root,
            Path::new(BUBBLEWRAP_WORKTREE_ROOT).join(".foundry/sandboxes/internal")
        );
        assert_eq!(
            runtime_paths.working_directory,
            Path::new(BUBBLEWRAP_WORKTREE_ROOT).join("src")
        );

        let args = bubblewrap_command(
            Path::new("/usr/bin/bwrap"),
            worktree,
            &sandbox,
            &runtime_paths,
            "deny",
            &["sh".to_string(), "-c".to_string(), "true".to_string()],
        );
        let contains = |expected: &[String]| {
            args.windows(expected.len())
                .any(|window| window == expected)
        };
        assert!(contains(&[
            "--ro-bind".to_string(),
            worktree.display().to_string(),
            BUBBLEWRAP_WORKTREE_ROOT.to_string(),
        ]));
        assert!(contains(&[
            "--bind".to_string(),
            sandbox.display().to_string(),
            runtime_paths.sandbox_root.display().to_string(),
        ]));
        assert!(contains(&[
            "--bind".to_string(),
            sandbox.join("home").display().to_string(),
            BUBBLEWRAP_HOME.to_string(),
        ]));
        assert!(contains(&[
            "--chdir".to_string(),
            runtime_paths.working_directory.display().to_string(),
        ]));
        assert!(args.iter().any(|argument| argument == "--unshare-net"));
        if Path::new("/etc/alternatives").exists() {
            assert!(contains(&[
                "--ro-bind".to_string(),
                "/etc/alternatives".to_string(),
                "/etc/alternatives".to_string(),
            ]));
        }
        assert!(!contains(&[
            "--ro-bind".to_string(),
            "/".to_string(),
            "/".to_string(),
        ]));
    }

    #[cfg(unix)]
    #[test]
    fn bubblewrap_remaps_only_a_canonical_worktree_executable_to_guest_root() {
        let temp = tempfile::tempdir().unwrap();
        let worktree = temp.path().join("worktree");
        let sandbox = worktree.join(".foundry/sandboxes/internal");
        let executable = worktree.join("fixture-bin/cargo");
        let input = worktree.join("fixture-data/input.txt");
        let external = temp.path().join("external.txt");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::create_dir_all(input.parent().unwrap()).unwrap();
        fs::create_dir_all(&sandbox).unwrap();
        fs::write(&executable, "#!/bin/sh\n").unwrap();
        fs::write(&input, "input").unwrap();
        fs::write(&external, "external").unwrap();

        let runtime_paths =
            sandbox_runtime_paths("bubblewrap", &worktree, &sandbox, &worktree).unwrap();
        let args = bubblewrap_command(
            Path::new("/usr/bin/bwrap"),
            &worktree,
            &sandbox,
            &runtime_paths,
            "deny",
            &[
                executable.display().to_string(),
                input.display().to_string(),
                external.display().to_string(),
            ],
        );
        let separator = args.iter().position(|argument| argument == "--").unwrap();
        assert_eq!(
            &args[separator + 1..],
            &[
                "/workspace/fixture-bin/cargo".to_string(),
                input.display().to_string(),
                external.display().to_string(),
            ]
        );
    }

    #[test]
    fn persisted_worktree_configuration_redacts_values_but_keeps_policy_keys() {
        let mut snapshot = WorktreeConfigSnapshot {
            status: "configured".to_string(),
            path: "/worktree/.foundry/worktree.toml".to_string(),
            sha256: "config-hash".to_string(),
            config: WorktreeConfig::default(),
        };
        snapshot
            .config
            .sandbox
            .environment
            .insert("PUBLIC_SETTING".to_string(), "private-value".to_string());
        snapshot.config.settings.insert(
            "provider".to_string(),
            toml::Value::String("internal-value".to_string()),
        );

        let redacted = redact_worktree_config_snapshot(snapshot);
        assert_eq!(redacted.sha256, "config-hash");
        assert_eq!(
            redacted.config.sandbox.environment["PUBLIC_SETTING"],
            "<redacted>"
        );
        assert_eq!(
            redacted.config.settings["provider"],
            toml::Value::String("<redacted>".to_string())
        );
    }

    #[test]
    fn porcelain_parser_keeps_linked_worktree_state() {
        let parsed = parse_worktree_porcelain(
            "worktree /repo\nHEAD abc\nbranch refs/heads/main\n\nworktree /repo-wt\nHEAD def\ndetached\nlocked reason\n\n",
        );
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].branch.as_deref(), Some("main"));
        assert!(parsed[1].detached);
        assert!(parsed[1].locked);
    }
}
