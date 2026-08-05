use assert_cmd::Command;
use chrono::Utc;
use foundry_core::artifact::hex_sha256;
use foundry_core::executor::ExecutorState;
use foundry_core::graph::ValidationRule;
use foundry_core::security::{sanitize_prompt_secrets, SecretSanitizationOptions};
use foundry_core::storage::FoundryStore;
use serde_json::Value;
use tempfile::tempdir;

fn foundry() -> Command {
    Command::cargo_bin("foundry").expect("foundry binary should build")
}

fn plan_workflow(store: &std::path::Path, goal: &str) -> Value {
    let output = foundry()
        .arg("--store")
        .arg(store)
        .args(["plan", "--goal", goal, "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).expect("plan output should be JSON")
}

fn task_id(workflow: &Value, title: &str) -> String {
    workflow["tasks"]
        .as_array()
        .expect("workflow tasks should be an array")
        .iter()
        .find(|task| task["title"] == title)
        .and_then(|task| task["id"].as_str())
        .unwrap_or_else(|| panic!("missing task {title}"))
        .to_string()
}

fn authorize_codex_fixture(store: &FoundryStore) {
    let command_path = std::env::current_exe()
        .expect("test executable should have a path")
        .display()
        .to_string();
    let executor = ExecutorState {
        id: "codex".to_string(),
        display_name: "Codex test fixture".to_string(),
        command: "codex".to_string(),
        installed: true,
        configured: true,
        command_path: Some(command_path),
        config_evidence: vec!["test-only configured executor fixture".to_string()],
        non_interactive_ready: true,
        probe_evidence: vec!["test-only non-interactive probe".to_string()],
        foundry_first_ready: false,
        foundry_first_entrypoint: None,
        harness_status: None,
        allowed: true,
        decision_source: "compact_secret_contract_fixture".to_string(),
        synced_at: Utc::now().to_rfc3339(),
    };
    store
        .save_executor_state("codex", &serde_json::to_value(executor).unwrap())
        .unwrap();
}

#[test]
fn compact_context_redacts_predecessor_human_fields_before_bounding_and_hashing() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("foundry.sqlite");
    let planned = plan_workflow(&store_path, "Exercise compact predecessor secret redaction");
    let workflow_id = planned["workflow_id"].as_str().unwrap().to_string();
    let predecessor_id = task_id(&planned, "Parse intent");
    let target_id = task_id(&planned, "Extract requirements");

    let title_secret = "sk-proj-titleabcdefghijklmnopqrstuvwxyzABCDEF1234567890";
    let goal_secret = "sk-proj-goalabcdefghijklmnopqrstuvwxyzABCDEF1234567890";
    let output_secret = "sk-proj-outputabcdefghijklmnopqrstuvwxyzABCDEF1234567890";
    let command_secret = "sk-proj-commandabcdefghijklmnopqrstuvwxyzABCDEF1234567890";
    let expected_secret = "sk-proj-expectedabcdefghijklmnopqrstuvwxyzABCDEF1234567890";
    let long_goal_prefix = "g".repeat(230);

    let store = FoundryStore::open(&store_path).unwrap();
    let mut workflow = store.load_workflow(&workflow_id).unwrap();
    let predecessor = workflow
        .tasks
        .iter_mut()
        .find(|task| task.id == predecessor_id)
        .unwrap();
    predecessor.title = format!("Prepare {title_secret}");
    predecessor.goal = format!("{long_goal_prefix}{goal_secret}");
    predecessor.expected_output = format!("Evidence {output_secret}");
    predecessor.validation_rules = vec![ValidationRule {
        kind: "command".to_string(),
        command: Some(format!("verify {command_secret}")),
        expected: format!("pass {expected_secret}"),
    }];
    store.save_workflow(&workflow).unwrap();
    drop(store);

    let output = foundry()
        .arg("--store")
        .arg(&store_path)
        .args([
            "context",
            "--workflow",
            &workflow_id,
            "--task",
            &target_id,
            "--budget",
            "1200",
            "--view",
            "compact",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let serialized = String::from_utf8(output).unwrap();
    for secret in [
        title_secret,
        goal_secret,
        output_secret,
        command_secret,
        expected_secret,
    ] {
        assert!(
            !serialized.contains(secret),
            "compact context serialized raw secret {secret}"
        );
    }

    let compact: Value = serde_json::from_str(&serialized).unwrap();
    assert_eq!(compact["schema_version"], "foundry.context.compact.v2");
    assert!(
        compact["secret_redaction_count"].as_u64().unwrap() >= 5,
        "every predecessor human field should contribute a redaction"
    );
    let predecessor = compact["guardrail"]["predecessor_tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["task_id"] == predecessor_id)
        .unwrap();
    let marker = "{{vault:project.openai.default}}";
    assert!(predecessor["title"].as_str().unwrap().contains(marker));
    assert!(predecessor["expected_output"]
        .as_str()
        .unwrap()
        .contains(marker));
    assert!(predecessor["validation_rules"][0]["command"]
        .as_str()
        .unwrap()
        .contains(marker));
    assert!(predecessor["validation_rules"][0]["expected"]
        .as_str()
        .unwrap()
        .contains(marker));

    let raw_goal = format!("{long_goal_prefix}{goal_secret}");
    let sanitized_goal =
        sanitize_prompt_secrets(&raw_goal, SecretSanitizationOptions::default()).sanitized_text;
    let compact_goal = predecessor["goal"].as_str().unwrap();
    assert!(compact_goal.ends_with('…'));
    let compact_goal_boundary = compact_goal.len() - "…".len();
    let expected_omitted_material = vec![sanitized_goal[compact_goal_boundary..].to_string()];
    let expected_omitted_sha256 =
        hex_sha256(&serde_json::to_vec(&expected_omitted_material).unwrap());
    let goal_omission = compact["omissions"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|field| field["field"] == "guardrail.predecessor_goals")
        .unwrap();
    assert_eq!(goal_omission["omitted_sha256"], expected_omitted_sha256);
}

#[test]
fn compact_handoff_redacts_expected_output_and_validation_rules_in_complete_json() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("foundry.sqlite");
    let planned = plan_workflow(&store_path, "Exercise compact handoff secret redaction");
    let workflow_id = planned["workflow_id"].as_str().unwrap().to_string();
    let task_id = task_id(&planned, "Parse intent");

    let output_secret = "sk-proj-handoffoutputabcdefghijklmnopqrstuvwxyzABCD1234567890";
    let command_secret = "sk-proj-handoffcommandabcdefghijklmnopqrstuvwxyzABCD1234567890";
    let expected_secret = "sk-proj-handoffexpectedabcdefghijklmnopqrstuvwxyzABCD1234567890";

    let store = FoundryStore::open(&store_path).unwrap();
    authorize_codex_fixture(&store);
    let mut workflow = store.load_workflow(&workflow_id).unwrap();
    let task = workflow
        .tasks
        .iter_mut()
        .find(|task| task.id == task_id)
        .unwrap();
    task.expected_output = format!("Deliver {output_secret}");
    task.validation_rules = vec![ValidationRule {
        kind: "command".to_string(),
        command: Some(format!("run {command_secret}")),
        expected: format!("produce {expected_secret}"),
    }];
    store.save_workflow(&workflow).unwrap();
    drop(store);

    let output = foundry()
        .arg("--store")
        .arg(&store_path)
        .args([
            "task",
            "handoff",
            "--workflow",
            &workflow_id,
            "--task",
            &task_id,
            "--executor",
            "codex",
            "--budget",
            "1200",
            "--view",
            "compact",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let serialized = String::from_utf8(output).unwrap();
    for secret in [output_secret, command_secret, expected_secret] {
        assert!(
            !serialized.contains(secret),
            "compact handoff serialized raw secret {secret}"
        );
    }

    let handoff: Value = serde_json::from_str(&serialized).unwrap();
    assert_eq!(
        handoff["schema_version"],
        "foundry.executor_handoff.compact.v1"
    );
    let marker = "{{vault:project.openai.default}}";
    assert!(handoff["execution"]["expected_output"]
        .as_str()
        .unwrap()
        .contains(marker));
    assert!(handoff["execution"]["validation_rules"][0]["command"]
        .as_str()
        .unwrap()
        .contains(marker));
    assert!(handoff["execution"]["validation_rules"][0]["expected"]
        .as_str()
        .unwrap()
        .contains(marker));
    let redaction_count = handoff["secret_redaction_count"].as_u64().unwrap();
    let marker_count = serialized.matches("{{vault:").count() as u64;
    assert!(redaction_count >= 3);
    assert_eq!(redaction_count, marker_count);
}
