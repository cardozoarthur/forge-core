use assert_cmd::Command;
use serde_json::Value;
use std::fs;
use tempfile::tempdir;

fn forge() -> Command {
    Command::cargo_bin("forge").expect("forge binary should build")
}

#[test]
fn plan_from_human_goal_creates_persistent_atomic_graph() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Create a delivery platform",
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
    assert!(json["workflow_id"].as_str().unwrap().starts_with("wf_"));
    assert!(json["tasks"].as_array().unwrap().len() >= 7);
    assert!(json["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|task| task["title"] == "Extract requirements"));
    assert!(json["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|task| task["title"] == "Validate build"));
    assert!(store.exists());

    let workflow_id = json["workflow_id"].as_str().unwrap();
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "status",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"pending\""));
}

#[test]
fn plan_supports_autonomous_mixed_workflow_with_cron_non_ai_step_and_email_cost_report() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Execute research now, continue every Friday at 09:00, calculate costs without AI, and email the final workflow cost to finance@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    let tasks = json["tasks"].as_array().unwrap();

    assert!(tasks.iter().any(|task| task["executor"] == "ai"));
    assert!(tasks.iter().any(|task| task["executor"] == "wait"));
    assert!(tasks.iter().any(|task| task["executor"] == "command"));
    assert!(tasks.iter().any(|task| task["executor"] == "notification"));
    assert!(tasks.iter().all(|task| task["human_required"] == false));

    let wait_task = tasks
        .iter()
        .find(|task| task["executor"] == "wait")
        .unwrap();
    assert_eq!(wait_task["schedule"]["cron"], "0 9 * * 5");

    let email_task = tasks
        .iter()
        .find(|task| task["executor"] == "notification")
        .unwrap();
    assert_eq!(email_task["notification"]["channel"], "email");
    assert_eq!(email_task["notification"]["to"], "finance@example.com");
    assert_eq!(email_task["notification"]["include_cost_report"], true);
}

#[test]
fn validation_blocks_promotion_until_required_gates_pass() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Create a reliable workflow runtime",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();

    let validation_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "validate",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();

    let validation: Value = serde_json::from_slice(&validation_output).unwrap();
    assert_eq!(validation["status"], "blocked");
    assert_eq!(validation["promotable"], false);
    assert!(validation["failed_rules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|rule| rule["kind"] == "task_status"));
}

#[test]
fn simulated_run_generates_cost_report_and_email_notification_payload() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Do an AI summary now, wait on cron 0 18 * * 1, run a shell-free deterministic cost calculation, and email the workflow costs to ops@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();

    let run_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "run",
            "--workflow",
            workflow_id,
            "--simulate",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let run: Value = serde_json::from_slice(&run_output).unwrap();
    assert_eq!(run["status"], "completed");
    assert!(
        run["cost_report"]["total_estimated_cost_usd"]
            .as_f64()
            .unwrap()
            > 0.0
    );
    assert_eq!(run["notifications"].as_array().unwrap().len(), 1);
    assert_eq!(run["notifications"][0]["channel"], "email");
    assert_eq!(run["notifications"][0]["to"], "ops@example.com");
    assert!(run["notifications"][0]["body"]
        .as_str()
        .unwrap()
        .contains("total_estimated_cost_usd"));
}

#[test]
fn context_controller_returns_minimal_task_local_context() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build an authenticated dashboard with docs",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let task_id = json["tasks"][0]["id"].as_str().unwrap();

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--budget",
            "900",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let context: Value = serde_json::from_slice(&context_output).unwrap();
    assert_eq!(context["workflow_id"], workflow_id);
    assert_eq!(context["task_id"], task_id);
    assert!(context["context_bytes"].as_u64().unwrap() <= 900);
    assert!(context["included_sections"]
        .as_array()
        .unwrap()
        .contains(&Value::String("local_objective".to_string())));
    assert!(!context["included_sections"]
        .as_array()
        .unwrap()
        .contains(&Value::String("entire_history".to_string())));
}

#[test]
fn improve_creates_controlled_experiment_and_never_promotes_without_validation() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Optimize agent execution reliability",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();

    let improve_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "improve",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let improvement: Value = serde_json::from_slice(&improve_output).unwrap();
    assert_eq!(improvement["status"], "experiment_generated");
    assert_eq!(improvement["auto_promoted"], false);
    assert_eq!(
        improvement["promotion_gate"],
        "benchmark_and_validation_required"
    );
    assert!(improvement["artifact_path"]
        .as_str()
        .unwrap()
        .ends_with(".json"));
    assert!(temp
        .path()
        .join(improvement["artifact_path"].as_str().unwrap())
        .exists());
}

#[test]
fn artifacts_command_lists_persistent_outputs_for_workflow() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Create reusable operational memory artifacts",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "improve",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success();

    let artifacts_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "artifacts",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let artifacts: Value = serde_json::from_slice(&artifacts_output).unwrap();
    assert_eq!(artifacts["workflow_id"], workflow_id);
    assert_eq!(artifacts["artifacts"].as_array().unwrap().len(), 1);
    assert!(artifacts["artifacts"][0]["path"]
        .as_str()
        .unwrap()
        .contains("improvement-"));
    assert!(artifacts["artifacts"][0]["sha256"].as_str().unwrap().len() >= 64);
}

#[test]
fn simulated_run_completes_graph_then_validation_allows_improvement_cycle() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Create a validated CLI and skill",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "run",
            "--workflow",
            workflow_id,
            "--simulate",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"completed\""));

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "validate",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"promotable\": true"));

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "improve",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "benchmark_and_validation_required",
        ));
}

#[test]
fn skill_install_creates_codex_and_opencode_compatible_skill_files() {
    let temp = tempdir().unwrap();

    forge()
        .args([
            "skill",
            "install",
            "--home",
            temp.path().to_str().unwrap(),
            "--target",
            "codex",
            "--target",
            "opencode",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"forge-core\""));

    let codex_skill = temp.path().join(".codex/skills/forge-core/SKILL.md");
    let opencode_skill = temp
        .path()
        .join(".config/opencode/skills/forge-core/SKILL.md");
    let shared_skill = temp.path().join(".agents/skills/forge-core/SKILL.md");
    assert!(codex_skill.exists());
    assert!(opencode_skill.exists());
    assert!(shared_skill.exists());

    let skill = fs::read_to_string(opencode_skill).unwrap();
    assert!(skill.starts_with("---\nname: forge-core\n"));
    assert!(skill.contains("description:"));
    assert!(skill.contains("forge plan"));
    assert!(skill.contains("forge validate"));
}
