use crate::artifact::hex_sha256;
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
pub const CLI_SHIM_INSTALL_SCHEMA_VERSION: &str = "forge.harness.shim_install.v1";
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
    pub wrapper_strategy: String,
    pub context_budget: usize,
    pub token_headroom_enabled: bool,
    pub env: Vec<CliWrapperEnvVar>,
    pub launch_command: Vec<String>,
    pub harness_checks: Vec<String>,
    pub notes: Vec<String>,
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
    pub forge_first: bool,
    pub dry_run: bool,
    pub allow_exec: bool,
    pub execution_mode: String,
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
    pub store_path: Option<String>,
    pub forge_binary: String,
    pub forge_first: bool,
    pub context_budget: usize,
    pub token_headroom: bool,
    pub status: String,
    pub script_sha256: Option<String>,
    pub argv_policy: String,
    pub safety_checks: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct CliShimInstallOptions<'a> {
    pub shim_dir: &'a Path,
    pub executor: &'a str,
    pub real_cmd: &'a str,
    pub store_path: Option<&'a Path>,
    pub forge_first: bool,
    pub workflow_id: Option<&'a str>,
    pub run_id: Option<&'a str>,
    pub context_budget: usize,
    pub token_headroom: bool,
    pub force: bool,
}

#[derive(Clone, Copy)]
pub struct CliHarnessExecOptions<'a> {
    pub store: Option<&'a ForgeStore>,
    pub executor: &'a str,
    pub command: &'a [String],
    pub forge_first: bool,
    pub workflow_id: Option<&'a str>,
    pub run_id: Option<&'a str>,
    pub context_budget: usize,
    pub token_headroom: bool,
    pub dry_run: bool,
    pub allow_exec: bool,
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

pub fn build_cli_wrapper_plan(
    executor: &str,
    command: &[String],
    forge_first: bool,
    workflow_id: Option<&str>,
    run_id: Option<&str>,
    context_budget: usize,
    token_headroom: bool,
) -> CliWrapperPlanReport {
    let executor = normalize_executor(executor);
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
        wrapper_strategy: "env_overlay_with_forge_context_and_token_headroom".to_string(),
        context_budget,
        token_headroom_enabled: token_headroom,
        env,
        launch_command,
        harness_checks: vec![
            "resolve real CLI before PATH shim precedence".to_string(),
            "prepend Forge shim directory only for the child process".to_string(),
            "record argv, cwd, workflow/run lineage and token-headroom metrics".to_string(),
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
        workflow_id,
        run_id,
        context_budget,
        token_headroom,
        force,
    } = options;
    let executor = normalize_executor(executor);
    let real_cmd = real_cmd.trim();
    if real_cmd.is_empty() {
        bail!("real CLI command cannot be empty");
    }
    fs::create_dir_all(shim_dir)
        .with_context(|| format!("failed to create shim dir `{}`", shim_dir.display()))?;
    let shim_dir = shim_dir
        .canonicalize()
        .unwrap_or_else(|_| shim_dir.to_path_buf());
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
        real_cmd,
        store_path,
        forge_first,
        workflow_id,
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
        real_command: real_cmd.to_string(),
        store_path: store_path.map(|path| path.display().to_string()),
        forge_binary: forge_binary.clone(),
        forge_first,
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

pub fn run_cli_harness_exec(options: CliHarnessExecOptions<'_>) -> Result<CliHarnessExecReceipt> {
    let CliHarnessExecOptions {
        store,
        executor,
        command,
        forge_first,
        workflow_id,
        run_id,
        context_budget,
        token_headroom,
        dry_run,
        allow_exec,
        cwd,
    } = options;
    let wrapper_plan = build_cli_wrapper_plan(
        executor,
        command,
        forge_first,
        workflow_id,
        run_id,
        context_budget,
        token_headroom,
    );
    let command = wrapper_plan.command.clone();
    let cwd_path = cwd
        .map(Path::to_path_buf)
        .unwrap_or(env::current_dir().context("failed to read current directory")?);
    let cwd_display = cwd_path.display().to_string();
    let (resolved_executable, resolution_status) = resolve_executable(command.first(), &cwd_path);
    let command_sha256 = hex_sha256(command.join("\0").as_bytes());
    let safety_checks = vec![
        "dry_run is the default; real execution requires --execute and --allow-exec".to_string(),
        "resolved executable is recorded before running the child process".to_string(),
        "Forge env overlay is applied only to the child process".to_string(),
        "stdout and stderr are summarized by bytes, sha256 and bounded excerpts".to_string(),
        "workflow/run lineage and token-headroom settings stay explicit in the receipt".to_string(),
    ];

    if dry_run {
        return Ok(exec_receipt(CliExecReceiptInput {
            wrapper_plan,
            command,
            command_sha256,
            cwd: cwd_display,
            forge_first,
            dry_run,
            allow_exec,
            execution_mode: "dry_run".to_string(),
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
        }));
    }
    if !allow_exec {
        return Ok(exec_receipt(CliExecReceiptInput {
            wrapper_plan,
            command,
            command_sha256,
            cwd: cwd_display,
            forge_first,
            dry_run,
            allow_exec,
            execution_mode: "blocked".to_string(),
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
        }));
    }
    let Some(executable) = resolved_executable.clone() else {
        return Ok(exec_receipt(CliExecReceiptInput {
            wrapper_plan,
            command,
            command_sha256,
            cwd: cwd_display,
            forge_first,
            dry_run,
            allow_exec,
            execution_mode: "blocked".to_string(),
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
        }));
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
    Ok(exec_receipt(CliExecReceiptInput {
        wrapper_plan,
        command,
        command_sha256,
        cwd: cwd_display,
        forge_first,
        dry_run,
        allow_exec,
        execution_mode: "guarded_exec".to_string(),
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
    }))
}

struct CliShimScriptOptions<'a> {
    forge_binary: &'a str,
    executor: &'a str,
    real_cmd: &'a str,
    store_path: Option<&'a Path>,
    forge_first: bool,
    workflow_id: Option<&'a str>,
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
    CliHarnessExecReceipt {
        schema_version: CLI_HARNESS_EXEC_SCHEMA_VERSION.to_string(),
        status: input.status,
        executor,
        command: input.command,
        command_sha256: input.command_sha256,
        cwd: input.cwd,
        forge_first: input.forge_first,
        dry_run: input.dry_run,
        allow_exec: input.allow_exec,
        execution_mode: input.execution_mode,
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
