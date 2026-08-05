#![cfg(unix)]

use foundry_core::executor::{sync_executors, ExecutorSyncOptions};
use foundry_core::mcp::{call_mcp_tool, mcp_tools_manifest};
use foundry_core::storage::FoundryStore;
use foundry_core::worktree::{
    approve_worktree_config, initialize_worktree, register_worktree, WorktreeRegisterOptions,
};
use serde_json::json;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

#[path = "support/mission_toolchain.rs"]
mod mission_toolchain;
use mission_toolchain::{gate_evidence_envelope, write_gate_evidence_command};

fn git_repository(path: &Path) {
    fs::create_dir_all(path).unwrap();
    assert!(Command::new("git")
        .args(["init", "--initial-branch=main"])
        .arg(path)
        .output()
        .unwrap()
        .status
        .success());
    assert!(Command::new("git")
        .args([
            "-C",
            path.to_str().unwrap(),
            "-c",
            "user.name=Foundry Mission MCP E2E",
            "-c",
            "user.email=foundry-mission-mcp@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "initial",
        ])
        .output()
        .unwrap()
        .status
        .success());
}

fn executable(path: &Path, source: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, source).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn mcp_mission_execution_is_receipt_backed_and_resumes_after_store_reopen() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("foundry.sqlite");
    let repository = temp.path().join("repository");
    let home = temp.path().join("home");
    let bin = temp.path().join("bin");
    executable(
        &bin.join("codex"),
        "#!/bin/sh\nif [ \"${1:-}\" = \"--version\" ]; then\n  echo 'codex-cli test'\n  exit 0\nfi\nexit 2\n",
    );
    fs::create_dir_all(home.join(".codex")).unwrap();
    fs::write(home.join(".codex/config.toml"), "model = \"test\"\n").unwrap();
    git_repository(&repository);
    let cargo_command = repository.join("fixture-bin/cargo");
    executable(
        &cargo_command,
        "#!/bin/sh\ncase \"${1:-}\" in\n  --version) printf '%s\\n' 'cargo mission-mcp-e2e 1.0.0' ;;\n  *) exit 2 ;;\nesac\n",
    );
    let cargo_command = cargo_command.to_str().unwrap().to_string();
    let evidence_command = write_gate_evidence_command(&repository);
    let store = FoundryStore::open(&store_path).unwrap();
    let synced = sync_executors(
        &store,
        ExecutorSyncOptions {
            home,
            executor_paths: vec![bin],
            shim_dirs: Vec::new(),
            allow: vec!["codex".to_string()],
            deny: Vec::new(),
            prompt: false,
        },
    )
    .unwrap();
    assert!(synced.usable.iter().any(|executor| executor == "codex"));
    let registered = register_worktree(
        &store,
        WorktreeRegisterOptions {
            path: repository.clone(),
            id: None,
            workflow_id: None,
            task_id: None,
            origin: "mission-mcp-e2e".to_string(),
            created_by_foundry: false,
        },
    )
    .unwrap();
    let worktree_id = registered.worktree.id;
    initialize_worktree(&store, &worktree_id, true, false, "mission-mcp-e2e").unwrap();
    let config_path = repository.join(".foundry/worktree.toml");
    let mut config: toml::Value =
        toml::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config["guardrails"]["allowed_commands"] = toml::Value::Array(vec![
        toml::Value::String(cargo_command.clone()),
        toml::Value::String(evidence_command.clone()),
    ]);
    config["sandbox"]["runtime"] = toml::Value::String("bubblewrap".to_string());
    config["sandbox"]["network"] = toml::Value::String("deny".to_string());
    fs::write(&config_path, toml::to_string_pretty(&config).unwrap()).unwrap();
    approve_worktree_config(
        &store,
        &worktree_id,
        true,
        "mission-mcp-e2e",
        "mission-mcp-e2e",
    )
    .unwrap();

    let manifest = mcp_tools_manifest();
    for name in [
        "foundry.mission.start",
        "foundry.mission.drive",
        "foundry.mission.execute",
        "foundry.mission.submit",
        "foundry.mission.resume",
        "foundry.mission.execution.list",
        "foundry.mission.execution.inspect",
        "foundry.mission.execution.reconcile",
    ] {
        assert!(
            manifest.tools.iter().any(|tool| tool.name == name),
            "missing MCP tool {name}"
        );
    }
    let submit_tool = manifest
        .tools
        .iter()
        .find(|tool| tool.name == "foundry.mission.submit")
        .unwrap();
    assert!(submit_tool.input_schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|field| field == "receipt_id"));
    assert!(submit_tool.input_schema["properties"]
        .get("validations")
        .is_none());
    let execute_tool = manifest
        .tools
        .iter()
        .find(|tool| tool.name == "foundry.mission.execute")
        .unwrap();
    assert!(!execute_tool.input_schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|field| field == "approved_by"));
    assert!(execute_tool.mutates_workflow);
    assert!(execute_tool
        .description
        .contains("tool metadata remains mutating"));
    assert_eq!(
        execute_tool.input_schema["properties"]["evidence"]["type"],
        "array"
    );

    let started = call_mcp_tool(
        &store,
        "foundry.mission.start",
        json!({
            "goal": "Run a receipt-backed MCP mission assignment",
            "worktree": repository,
        }),
    )
    .unwrap();
    let mission_id = started.result["mission"]["id"].as_str().unwrap();
    let driven = call_mcp_tool(
        &store,
        "foundry.mission.drive",
        json!({"mission_id": mission_id}),
    )
    .unwrap();
    let task_id = driven.result["assignment"]["task"]["id"].as_str().unwrap();
    let agent_id = driven.result["assignment"]["agent"]["instance_id"]
        .as_str()
        .unwrap();
    let planned = call_mcp_tool(
        &store,
        "foundry.mission.execute",
        json!({
            "mission_id": mission_id,
            "task_id": task_id,
            "agent_id": agent_id,
            "idempotency_key": "mcp-execution-dry-run",
            "purpose": "test",
            "command": [
                evidence_command.clone(),
                gate_evidence_envelope(&["requirements_summary"])
            ],
            "evidence": ["requirements_summary"],
            "dry_run": true,
        }),
    )
    .unwrap();
    assert_eq!(planned.result["status"], "planned");
    assert_eq!(planned.result["persisted"], false);
    assert_eq!(
        planned.result["plan"]["requested_evidence"],
        json!(["requirements_summary"])
    );
    assert_eq!(
        planned.result["plan"]["gate_evidence_contract"][0]["gate_ids"],
        json!(["requirements_ready"])
    );

    let failed = call_mcp_tool(
        &store,
        "foundry.mission.execute",
        json!({
            "mission_id": mission_id,
            "task_id": task_id,
            "agent_id": agent_id,
            "idempotency_key": "mcp-execution-failed",
            "purpose": "test",
            "command": [cargo_command.clone(), "--definitely-invalid-foundry-test"],
            "approved_by": "mission-mcp-e2e",
        }),
    )
    .unwrap();
    assert_eq!(failed.result["receipt"]["status"], "failed");
    let failed_receipt_id = failed.result["receipt"]["receipt_id"].as_str().unwrap();

    let execution_input = json!({
        "mission_id": mission_id,
        "task_id": task_id,
        "agent_id": agent_id,
        "idempotency_key": "mcp-exec-test",
        "purpose": "test",
        "command": [
            evidence_command,
            gate_evidence_envelope(&["requirements_summary", "acceptance_criteria"])
        ],
        "evidence": ["requirements_summary", "acceptance_criteria"],
        "approved_by": "mission-mcp-e2e",
    });
    let protected =
        call_mcp_tool(&store, "foundry.mission.execute", execution_input.clone()).unwrap_err();
    assert!(format!("{protected:#}").contains("already has protected execution"));
    let unconfirmed = call_mcp_tool(
        &store,
        "foundry.mission.execution.reconcile",
        json!({
            "receipt_id": failed_receipt_id,
            "outcome": "no_effect_retry",
            "approved_by": "mission-mcp-e2e",
            "reason": "invalid cargo argument exited before any repository mutation",
            "confirm_no_effect_retry": false,
        }),
    )
    .unwrap_err();
    assert!(format!("{unconfirmed:#}").contains("explicit no-effect retry confirmation"));
    let reconciled = call_mcp_tool(
        &store,
        "foundry.mission.execution.reconcile",
        json!({
            "receipt_id": failed_receipt_id,
            "outcome": "no_effect_retry",
            "approved_by": "mission-mcp-e2e",
            "reason": "invalid cargo argument exited before any repository mutation",
            "confirm_no_effect_retry": true,
        }),
    )
    .unwrap();
    assert_eq!(reconciled.result["status"], "reconciled_no_effect_retry");

    let executed =
        call_mcp_tool(&store, "foundry.mission.execute", execution_input.clone()).unwrap();
    assert_eq!(executed.result["replayed"], false);
    assert_eq!(executed.result["receipt"]["status"], "completed");
    assert!(executed.result["receipt"]["claims"]
        .as_array()
        .unwrap()
        .iter()
        .any(|claim| {
            claim["kind"] == "gate_evidence"
                && claim["evidence_kind"] == "requirements_summary"
                && claim["gate_ids"] == json!(["requirements_ready"])
        }));
    let receipt_id = executed.result["receipt"]["receipt_id"].as_str().unwrap();
    let replayed = call_mcp_tool(&store, "foundry.mission.execute", execution_input).unwrap();
    assert_eq!(replayed.result["replayed"], true);
    assert_eq!(replayed.result["receipt"]["receipt_id"], receipt_id);

    let listed = call_mcp_tool(
        &store,
        "foundry.mission.execution.list",
        json!({"mission_id": mission_id, "task_id": task_id}),
    )
    .unwrap();
    assert_eq!(listed.result["records"].as_array().unwrap().len(), 2);
    let inspected = call_mcp_tool(
        &store,
        "foundry.mission.execution.inspect",
        json!({"receipt_id": receipt_id}),
    )
    .unwrap();
    assert_eq!(inspected.result["receipt"]["receipt_id"], receipt_id);

    let tampered = call_mcp_tool(
        &store,
        "foundry.mission.submit",
        json!({
            "mission_id": mission_id,
            "task_id": task_id,
            "agent_id": agent_id,
            "idempotency_key": "mcp-tampered-validation",
            "receipt_id": receipt_id,
            "summary": "tampered validation must be rejected",
            "validations": ["requirements_summary"],
        }),
    )
    .unwrap_err();
    assert!(format!("{tampered:#}").contains("unknown field `validations`"));

    let submitted = call_mcp_tool(
        &store,
        "foundry.mission.submit",
        json!({
            "mission_id": mission_id,
            "task_id": task_id,
            "agent_id": agent_id,
            "idempotency_key": "mcp-submission-v1",
            "receipt_id": receipt_id,
            "summary": "receipt-backed execution is ready",
        }),
    )
    .unwrap();
    assert_eq!(submitted.result["status"], "queued");
    assert_eq!(submitted.result["accepted"], false);
    drop(store);

    let reopened = FoundryStore::open(&store_path).unwrap();
    let resumed = call_mcp_tool(
        &reopened,
        "foundry.mission.resume",
        json!({"mission_id": mission_id}),
    )
    .unwrap();
    assert_eq!(resumed.result["action"], "handoff_consumed");
    assert_eq!(
        resumed.result["mission"]["handoffs"][0]["status"],
        "accepted"
    );
}
