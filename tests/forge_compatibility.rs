#![allow(deprecated)]
// Explicit 0.6.x compatibility coverage. foundry-brand-allow: legacy-compat

use assert_cmd::Command;
use foundry_core::brand::{env_var, project_config_path_for_read};
use foundry_core::cli_integration::{
    resolve_harness_forge_first_source, // foundry-brand-allow: legacy-compat
    resolve_harness_forge_first_source_for_project, // foundry-brand-allow: legacy-compat
};
use foundry_core::interactive::{
    build_forge_first_harness_smoke, // foundry-brand-allow: legacy-compat
    render_forge_first_harness_smoke, // foundry-brand-allow: legacy-compat
    ForgeFirstHarnessSmokeReport,    // foundry-brand-allow: legacy-compat
};
use foundry_core::mcp::call_mcp_tool;
use foundry_core::opencode_tui::{
    build_forge_tui,  // foundry-brand-allow: legacy-compat
    render_forge_tui, // foundry-brand-allow: legacy-compat
    run_forge_tui,    // foundry-brand-allow: legacy-compat
    ForgeTuiBenchmarkSnapshot,
    ForgeTuiCapability,
    ForgeTuiOrchestrator,
    ForgeTuiPrompt, // foundry-brand-allow: legacy-compat
    ForgeTuiRendererStrategy,
    ForgeTuiReport,
    ForgeTuiShell,
    ForgeTuiStatusBar,     // foundry-brand-allow: legacy-compat
    ForgeTuiVisualization, // foundry-brand-allow: legacy-compat
};
use foundry_core::storage::{ForgeStore, FoundryStore}; // foundry-brand-allow: legacy-compat
use std::sync::Mutex;
use tempfile::tempdir;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn assert_type<T>() {}

#[test]
fn deprecated_rust_type_aliases_remain_available_for_the_transition_cycle() {
    assert_type::<ForgeStore>(); // foundry-brand-allow: legacy-compat
    assert_type::<ForgeFirstHarnessSmokeReport>(); // foundry-brand-allow: legacy-compat
    assert_type::<ForgeTuiReport>(); // foundry-brand-allow: legacy-compat
    assert_type::<ForgeTuiOrchestrator>(); // foundry-brand-allow: legacy-compat
    assert_type::<ForgeTuiRendererStrategy>(); // foundry-brand-allow: legacy-compat
    assert_type::<ForgeTuiPrompt>(); // foundry-brand-allow: legacy-compat
    assert_type::<ForgeTuiShell>(); // foundry-brand-allow: legacy-compat
    assert_type::<ForgeTuiStatusBar>(); // foundry-brand-allow: legacy-compat
    assert_type::<ForgeTuiVisualization>(); // foundry-brand-allow: legacy-compat
    assert_type::<ForgeTuiCapability>(); // foundry-brand-allow: legacy-compat
    assert_type::<ForgeTuiBenchmarkSnapshot>(); // foundry-brand-allow: legacy-compat
}

#[test]
fn deprecated_rust_function_aliases_remain_available_for_the_transition_cycle() {
    let _ = resolve_harness_forge_first_source; // foundry-brand-allow: legacy-compat
    let _ = resolve_harness_forge_first_source_for_project; // foundry-brand-allow: legacy-compat
    let _ = build_forge_tui; // foundry-brand-allow: legacy-compat
    let _ = render_forge_tui; // foundry-brand-allow: legacy-compat
    let _ = run_forge_tui; // foundry-brand-allow: legacy-compat
    let _ = build_forge_first_harness_smoke; // foundry-brand-allow: legacy-compat
    let _ = render_forge_first_harness_smoke; // foundry-brand-allow: legacy-compat
}

#[test]
fn legacy_binary_warns_on_stderr_and_preserves_json_stdout() {
    let output = Command::cargo_bin("forge") // foundry-brand-allow: legacy-compat
        .expect("legacy compatibility binary")
        .args(["squad", "catalog", "--output", "json"])
        .output()
        .expect("run legacy compatibility binary");
    assert!(output.status.success());
    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    assert_eq!(stdout["schema_version"], "foundry.squad.catalog.v1");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("deprecated"));
    assert!(stderr.contains("use `foundry`"));
}

#[test]
fn foundry_environment_wins_by_presence_and_legacy_value_is_only_a_fallback() {
    let _guard = ENV_LOCK.lock().expect("environment test lock");
    const CANONICAL: &str = "FOUNDRY_COMPAT_TEST_VALUE";
    const LEGACY: &str = "FORGE_COMPAT_TEST_VALUE"; // foundry-brand-allow: legacy-compat
    std::env::remove_var(CANONICAL);
    std::env::remove_var(LEGACY);
    std::env::set_var(LEGACY, "legacy");
    assert_eq!(env_var(CANONICAL).as_deref(), Ok("legacy"));
    std::env::set_var(CANONICAL, "");
    assert_eq!(env_var(LEGACY).as_deref(), Ok("")); // foundry-brand-allow: legacy-compat
    std::env::remove_var(CANONICAL);
    std::env::remove_var(LEGACY);
}

#[test]
fn canonical_project_config_wins_and_legacy_config_is_read_only_fallback() {
    let root = tempdir().expect("project root");
    let legacy_dir = root.path().join(".forge"); // foundry-brand-allow: legacy-compat
    std::fs::create_dir_all(&legacy_dir).expect("legacy config directory");
    let legacy = legacy_dir.join("harness.json");
    std::fs::write(&legacy, "{}").expect("legacy config");
    assert_eq!(
        project_config_path_for_read(root.path(), "harness.json"),
        legacy
    );

    let canonical_dir = root.path().join(".foundry");
    std::fs::create_dir_all(&canonical_dir).expect("canonical config directory");
    let canonical = canonical_dir.join("harness.json");
    std::fs::write(&canonical, "not-json").expect("canonical config presence");
    assert_eq!(
        project_config_path_for_read(root.path(), "harness.json"),
        canonical
    );
}

#[test]
fn legacy_mcp_tool_name_is_accepted_but_output_is_canonical() {
    let root = tempdir().expect("store root");
    let store = FoundryStore::open(root.path().join("foundry.sqlite")).expect("store");
    let report = call_mcp_tool(
        &store,
        "forge.workflow.list", // foundry-brand-allow: legacy-compat
        serde_json::json!({}),
    )
    .expect("legacy MCP alias");
    assert_eq!(report.tool_name, "foundry.workflow.list");
    assert_eq!(report.schema_version, "foundry.mcp.call.v1");
}
