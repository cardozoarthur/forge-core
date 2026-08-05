use assert_cmd::Command;
use serde_json::Value;
use tempfile::tempdir;

#[test]
fn json_output_returns_a_stable_machine_readable_error() {
    let directory = tempdir().expect("tempdir");
    let store = directory.path().join("foundry.sqlite");
    let output = Command::cargo_bin("foundry")
        .expect("foundry binary")
        .args([
            "--store",
            store.to_str().expect("store path"),
            "mcp",
            "call",
            "foundry.tool.that.does.not.exist",
            "--input",
            "{}",
            "--output",
            "json",
        ])
        .output()
        .expect("run foundry");

    assert!(!output.status.success());
    let response: Value =
        serde_json::from_slice(&output.stderr).expect("stderr must be one JSON document");
    assert_eq!(response["schema_version"], "foundry.cli.error.v1");
    assert_eq!(response["status"], "error");
    assert!(response["error"]["code"].is_string());
    assert!(response["error"]["category"].is_string());
    assert!(response["error"]["message"].is_string());
    assert!(response["error"]["retryable"].is_boolean());
    assert!(response["error"]["remediation"].is_string());
}

#[test]
fn production_mode_rejects_relative_default_store() {
    let output = Command::cargo_bin("foundry")
        .expect("foundry binary")
        .env("FOUNDRY_PRODUCTION_MODE", "1")
        .args(["list", "--output", "json"])
        .output()
        .expect("run foundry");

    assert!(!output.status.success());
    let response: Value = serde_json::from_slice(&output.stderr).expect("JSON error");
    assert_eq!(response["error"]["code"], "invalid_argument");
    assert!(response["error"]["message"]
        .as_str()
        .expect("message")
        .contains("absolute --store"));
}
