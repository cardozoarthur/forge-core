use crate::artifact::hex_sha256;
use crate::intent::OperatingContextSpec;
use crate::storage::{ForgeStore, HeadroomBlobWrite, StoredHeadroomBlobRecord};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const TOKEN_HEADROOM_SCHEMA_VERSION: &str = "forge.harness.token_headroom.v1";
pub const CLI_WRAPPER_PLAN_SCHEMA_VERSION: &str = "forge.harness.cli_wrapper_plan.v1";
pub const HEADROOM_RETRIEVAL_SCHEMA_VERSION: &str = "forge.harness.headroom_retrieval.v1";
pub const CLI_HARNESS_EXEC_SCHEMA_VERSION: &str = "forge.harness.exec_receipt.v1";
pub const CLI_HARNESS_EXEC_EVENT_SCHEMA_VERSION: &str = "forge.harness.exec_event.v1";
pub const CLI_HARNESS_MODE_SCHEMA_VERSION: &str = "forge.harness.mode.v1";
pub const CLI_SHIM_INSTALL_SCHEMA_VERSION: &str = "forge.harness.shim_install.v1";
pub const CLI_SHIM_STATUS_SCHEMA_VERSION: &str = "forge.harness.shim_status.v1";
const CLI_SHIM_MARKER: &str = "# forge-harness-shim:v1";

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
    pub token_headroom_enabled: bool,
    pub env: Vec<CliWrapperEnvVar>,
    pub launch_command: Vec<String>,
    pub harness_checks: Vec<String>,
    pub notes: Vec<String>,
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
    pub token_headroom: bool,
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
    pub precedence: Vec<String>,
    pub safety_checks: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct HarnessModeOptions<'a> {
    pub forge_first: bool,
    pub observe_only: bool,
    pub project_root: Option<&'a Path>,
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
    pub checks: Vec<String>,
    pub instructions: Vec<String>,
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
    pub token_headroom: bool,
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
    let mut safety_checks = vec![
        "mode report is read-only and never launches child processes".to_string(),
        "exec policy should be inspected before running external brain CLIs".to_string(),
    ];
    if project_exec_policy.require_lineage_for_exec {
        safety_checks.push("project_require_lineage_for_exec".to_string());
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
        token_headroom,
    } = options;
    let executor = normalize_executor(executor);
    let forge_first_source = normalize_harness_mode_source(forge_first_source, forge_first);
    let command = if command.is_empty() {
        vec![executor.clone()]
    } else {
        command.to_vec()
    };
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
    launch_command.push("--".to_string());
    launch_command.extend(command.clone());

    CliWrapperPlanReport {
        schema_version: CLI_WRAPPER_PLAN_SCHEMA_VERSION.to_string(),
        status: "cli_wrapper_plan_ready".to_string(),
        executor,
        command,
        forge_first,
        forge_first_source,
        workflow_id: normalize_optional_text(workflow_id),
        task_id: normalize_optional_text(task_id),
        run_id: normalize_optional_text(run_id),
        wrapper_strategy: "env_overlay_with_forge_context_and_token_headroom".to_string(),
        context_budget,
        token_headroom_enabled: token_headroom,
        env,
        launch_command,
        harness_checks: vec![
            "resolve real CLI before PATH shim precedence".to_string(),
            "prepend Forge shim directory only for the child process".to_string(),
            "record argv, cwd, workflow/task/run lineage, token-headroom metrics and timeline event evidence".to_string(),
            "persist reversible headroom blobs in the Forge store when compression is applied".to_string(),
            "fall back to observe_only when Forge context is unavailable".to_string(),
        ],
        notes: vec![
            "Headroom-inspired ideas absorbed: local-first compression, reversible retrieval refs, CLI wrapper env shaping, tool-search preservation and shim-based harness tests".to_string(),
            "This plan is non-destructive; actual exec remains a separate guarded harness action".to_string(),
        ],
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
        token_headroom,
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
        token_headroom,
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
    let global_event_id = store.record_global_event(
        "forge_harness",
        &source_id,
        workflow_id,
        &receipt.status,
        "forge_harness",
        harness_event_status(&receipt.status),
        &data,
        &tenant_context,
    )?;
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
