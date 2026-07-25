use assert_cmd::Command;
use chrono::{Duration, Utc};
use forge_core::graph;
use forge_core::intent::parse_intent;
use forge_core::request::{create_run_record, load_run_record, save_run_record};
use forge_core::storage::ForgeStore;
use serde_json::Value;
use std::path::Path;
use tempfile::tempdir;

fn forge() -> Command {
    Command::cargo_bin("forge").expect("forge binary should build")
}

fn seed_request_run(store_path: &Path, goal: &str, live_supervisor_lease: bool) -> String {
    let store = ForgeStore::open(store_path).unwrap();
    let workflow = graph::create_workflow(parse_intent(goal));
    store.save_workflow(&workflow).unwrap();
    let mut run = create_run_record(&workflow, "test", "accepted");
    if live_supervisor_lease {
        run.supervisor_instance_id = Some("live-supervisor".to_string());
        run.supervisor_lease_expires_at = Some(Utc::now() + Duration::minutes(5));
        run.supervisor_fencing_token = 11;
    }
    save_run_record(&store, &run).unwrap();
    run.run_id
}

fn assert_live_supervisor_lease_unchanged(store_path: &Path, run_id: &str) {
    let store = ForgeStore::open(store_path).unwrap();
    let run = load_run_record(&store, run_id).unwrap();
    assert_eq!(run.status, "accepted");
    assert_eq!(
        run.supervisor_instance_id.as_deref(),
        Some("live-supervisor")
    );
    assert_eq!(run.supervisor_fencing_token, 11);
    assert!(run
        .supervisor_lease_expires_at
        .is_some_and(|expires_at| expires_at > Utc::now()));
}

#[test]
fn request_supervisor_cli_reports_an_empty_store() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .arg("--store")
        .arg(&store)
        .args(["request", "supervise", "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let report: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["schema_version"], "forge.request_supervisor.v1");
    assert_eq!(report["status"], "request_supervisor_completed");
    assert_eq!(report["counts"]["scanned"], 0);
    assert_eq!(report["counts"]["failures"], 0);
}

#[test]
fn continuous_request_supervisor_rejects_zero_interval() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    forge()
        .arg("--store")
        .arg(&store)
        .args([
            "request",
            "supervise",
            "--continuous",
            "--interval-seconds",
            "0",
            "--output",
            "json",
        ])
        .assert()
        .failure();
}

#[test]
fn drive_loop_persists_manual_handoff_as_needs_attention() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let started = forge()
        .arg("--store")
        .arg(&store)
        .args([
            "request",
            "start",
            "--goal",
            "Research and implement a bounded production runtime review",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let started: Value = serde_json::from_slice(&started).unwrap();
    let run_id = started["run_id"].as_str().unwrap();

    forge()
        .arg("--store")
        .arg(&store)
        .args([
            "request",
            "drive-loop",
            "--run",
            run_id,
            "--executor",
            "drive-loop-test",
        ])
        .assert()
        .success();

    let status = forge()
        .arg("--store")
        .arg(&store)
        .args(["request", "status", "--run", run_id, "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let status: Value = serde_json::from_slice(&status).unwrap();
    assert_eq!(status["status"], "needs_attention");
    assert_eq!(status["workflow_status"], "needs_attention");
    assert_eq!(status["activity"]["heartbeat_status"], "needs_attention");
    assert_eq!(status["activity"]["executor"], Value::Null);
    assert_eq!(status["activity"]["pid"], Value::Null);
}

#[test]
fn request_cli_mutators_enforce_live_supervisor_lease_and_keep_normal_paths() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let live_cancel = seed_request_run(&store, "CLI cancel must honor fencing", true);
    let live_resume = seed_request_run(&store, "CLI resume must honor fencing", true);
    let live_switch = seed_request_run(&store, "CLI switch must honor fencing", true);

    for (command, run_id) in [("cancel", &live_cancel), ("resume", &live_resume)] {
        let stderr = forge()
            .arg("--store")
            .arg(&store)
            .args([
                "request", command, "--run", run_id, "--origin", "cli-test", "--output", "json",
            ])
            .assert()
            .failure()
            .get_output()
            .stderr
            .clone();
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(stderr.contains("live supervisor lease"));
        assert!(stderr.contains("recover or reconcile the run after the lease expires"));
        assert_live_supervisor_lease_unchanged(&store, run_id);
    }

    let switch_stderr = forge()
        .arg("--store")
        .arg(&store)
        .args([
            "request",
            "switch-executor",
            "--run",
            &live_switch,
            "--executor",
            "codex",
            "--fallback-executor",
            "agy",
            "--origin",
            "cli-test",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let switch_stderr = String::from_utf8(switch_stderr).unwrap();
    assert!(switch_stderr.contains("live supervisor lease"));
    assert!(switch_stderr.contains("recover or reconcile the run after the lease expires"));
    assert_live_supervisor_lease_unchanged(&store, &live_switch);

    let normal_cancel = seed_request_run(&store, "CLI cancel without fencing", false);
    let cancelled = forge()
        .arg("--store")
        .arg(&store)
        .args([
            "request",
            "cancel",
            "--run",
            &normal_cancel,
            "--origin",
            "cli-test",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let cancelled: Value = serde_json::from_slice(&cancelled).unwrap();
    assert_eq!(cancelled["status"], "cancelled");

    let normal_resume = seed_request_run(&store, "CLI resume without fencing", false);
    let resumed = forge()
        .arg("--store")
        .arg(&store)
        .args([
            "request",
            "resume",
            "--run",
            &normal_resume,
            "--origin",
            "cli-test",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let resumed: Value = serde_json::from_slice(&resumed).unwrap();
    assert_eq!(resumed["status"], "resumed");

    let normal_switch = seed_request_run(&store, "CLI switch without fencing", false);
    let switched = forge()
        .arg("--store")
        .arg(&store)
        .args([
            "request",
            "switch-executor",
            "--run",
            &normal_switch,
            "--executor",
            "codex",
            "--fallback-executor",
            "agy",
            "--origin",
            "cli-test",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let switched: Value = serde_json::from_slice(&switched).unwrap();
    assert_eq!(switched["status"], "running");
    assert_eq!(switched["new_executor"], "codex");
}

#[test]
fn request_mcp_mutators_enforce_live_supervisor_lease_and_keep_normal_paths() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let live_cancel = seed_request_run(&store, "MCP cancel must honor fencing", true);
    let live_resume = seed_request_run(&store, "MCP resume must honor fencing", true);
    let live_switch = seed_request_run(&store, "MCP switch must honor fencing", true);

    for (tool, run_id) in [
        ("forge.request.cancel", &live_cancel),
        ("forge.run.resume", &live_resume),
    ] {
        let input = serde_json::json!({
            "run_id": run_id,
            "origin": "mcp-test",
        })
        .to_string();
        let stderr = forge()
            .arg("--store")
            .arg(&store)
            .args(["mcp", "call", tool, "--input", &input, "--output", "json"])
            .assert()
            .failure()
            .get_output()
            .stderr
            .clone();
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(stderr.contains("live supervisor lease"));
        assert!(stderr.contains("recover or reconcile the run after the lease expires"));
        assert_live_supervisor_lease_unchanged(&store, run_id);
    }

    let switch_input = serde_json::json!({
        "run_id": &live_switch,
        "executor": "codex",
        "fallback_executors": ["agy"],
        "origin": "mcp-test",
    })
    .to_string();
    let switch_stderr = forge()
        .arg("--store")
        .arg(&store)
        .args([
            "mcp",
            "call",
            "forge.run.switch_executor",
            "--input",
            &switch_input,
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let switch_stderr = String::from_utf8(switch_stderr).unwrap();
    assert!(switch_stderr.contains("live supervisor lease"));
    assert!(switch_stderr.contains("recover or reconcile the run after the lease expires"));
    assert_live_supervisor_lease_unchanged(&store, &live_switch);

    for (tool, goal, expected_status) in [
        (
            "forge.request.cancel",
            "MCP cancel without fencing",
            "cancelled",
        ),
        ("forge.run.resume", "MCP resume without fencing", "resumed"),
    ] {
        let run_id = seed_request_run(&store, goal, false);
        let input = serde_json::json!({
            "run_id": run_id,
            "origin": "mcp-test",
        })
        .to_string();
        let output = forge()
            .arg("--store")
            .arg(&store)
            .args(["mcp", "call", tool, "--input", &input, "--output", "json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let output: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(output["status"], "ok");
        assert_eq!(output["result"]["status"], expected_status);
    }

    let normal_switch = seed_request_run(&store, "MCP switch without fencing", false);
    let switch_input = serde_json::json!({
        "run_id": normal_switch,
        "executor": "codex",
        "fallback_executors": ["agy"],
        "origin": "mcp-test",
    })
    .to_string();
    let switched = forge()
        .arg("--store")
        .arg(&store)
        .args([
            "mcp",
            "call",
            "forge.run.switch_executor",
            "--input",
            &switch_input,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let switched: Value = serde_json::from_slice(&switched).unwrap();
    assert_eq!(switched["status"], "ok");
    assert_eq!(switched["result"]["status"], "running");
    assert_eq!(switched["result"]["new_executor"], "codex");
}
