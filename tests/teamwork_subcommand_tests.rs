use assert_cmd::Command;
use serde_json::Value;
use tempfile::tempdir;

fn forge() -> Command {
    Command::cargo_bin("forge").expect("forge binary should build")
}

#[test]
fn test_teamwork_subcommand_basic() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");

    let output = forge()
        .arg("--store")
        .arg(&store_path)
        .args([
            "teamwork",
            "--goal",
            "Build a lightweight Rust teamwork runtime",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["status"], "planned");
    assert_eq!(json["goal"], "Build a lightweight Rust teamwork runtime");
    assert!(json["workflow_id"].as_str().unwrap().starts_with("wf_"));
    assert!(json["roster"]["roles"].is_array());
    assert_eq!(json["roster"]["agent_count"], 3);
    assert_eq!(json["roster"]["max_parallel_agents"], 3);
    assert!(json["strategy"]["legacy_brains_invalidated"]
        .as_array()
        .unwrap()
        .iter()
        .any(|brain| brain == "gemini"));
    assert!(json["run_id"].is_null());
}

#[test]
fn test_teamwork_subcommand_detached() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");

    let output = forge()
        .arg("--store")
        .arg(&store_path)
        .args([
            "teamwork",
            "--goal",
            "Build a lightweight Rust teamwork runtime",
            "--detached",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["status"], "planned");
    assert_eq!(json["goal"], "Build a lightweight Rust teamwork runtime");
    assert!(json["workflow_id"].as_str().unwrap().starts_with("wf_"));
    assert!(json["roster"]["roles"].is_array());
    assert!(json["run_id"].as_str().unwrap().starts_with("run_"));
}
