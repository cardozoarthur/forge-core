use assert_cmd::Command;
use foundry_core::executor::ExecutorState;
use foundry_core::graph::TaskStatus;
use foundry_core::storage::FoundryStore;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use tempfile::{tempdir, TempDir};

#[path = "support/mission_toolchain.rs"]
mod mission_toolchain;
use mission_toolchain::{
    gate_evidence_envelope, materialize_real_host_toolchain, write_gate_evidence_command,
};

fn foundry() -> Command {
    Command::cargo_bin("foundry").expect("foundry binary should build")
}

fn json_stdout(assertion: assert_cmd::assert::Assert) -> Value {
    serde_json::from_slice(&assertion.get_output().stdout)
        .expect("command should return JSON on stdout")
}

fn foundry_json(store: &Path, args: &[&str]) -> Value {
    json_stdout(
        foundry()
            .arg("--store")
            .arg(store)
            .args(args)
            .assert()
            .success(),
    )
}

fn foundry_json_failure(store: &Path, args: &[&str]) -> Value {
    json_stdout(
        foundry()
            .arg("--store")
            .arg(store)
            .args(args)
            .assert()
            .failure(),
    )
}

fn git_repository(path: &Path) {
    fs::create_dir_all(path).unwrap();
    let init = ProcessCommand::new("git")
        .args(["init", "--initial-branch=main"])
        .arg(path)
        .output()
        .unwrap();
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    let commit = ProcessCommand::new("git")
        .args([
            "-C",
            path.to_str().unwrap(),
            "-c",
            "user.name=Foundry Mission Workflow Projection",
            "-c",
            "user.email=foundry-mission-workflow-projection@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "initial",
        ])
        .output()
        .unwrap();
    assert!(
        commit.status.success(),
        "{}",
        String::from_utf8_lossy(&commit.stderr)
    );
}

struct MissionFixture {
    _temp: TempDir,
    store: PathBuf,
    repository: PathBuf,
    cargo_command: String,
    evidence_command: String,
}

fn mission_fixture() -> MissionFixture {
    let temp = tempdir().unwrap();
    let store = temp.path().join("foundry.sqlite");
    let repository = temp.path().join("repository");
    git_repository(&repository);

    let registered = foundry_json(
        &store,
        &[
            "worktree",
            "register",
            "--path",
            repository.to_str().unwrap(),
            "--output",
            "json",
        ],
    );
    let worktree_id = registered["worktree"]["id"].as_str().unwrap();
    foundry_json(
        &store,
        &[
            "worktree",
            "init",
            "--worktree",
            worktree_id,
            "--allow-worktree-write",
            "--output",
            "json",
        ],
    );

    let cargo_command = materialize_real_host_toolchain(&repository);
    let evidence_command = write_gate_evidence_command(&repository);

    let config_path = repository.join(".foundry/worktree.toml");
    let mut config: toml::Value =
        toml::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config["guardrails"]["allowed_commands"] = toml::Value::Array(vec![
        toml::Value::String(cargo_command.clone()),
        toml::Value::String(evidence_command.clone()),
    ]);
    config["guardrails"]["max_command_seconds"] = toml::Value::Integer(30);
    config["sandbox"]["runtime"] = toml::Value::String("bubblewrap".to_string());
    config["sandbox"]["network"] = toml::Value::String("deny".to_string());
    fs::write(&config_path, toml::to_string_pretty(&config).unwrap()).unwrap();
    foundry_json(
        &store,
        &[
            "worktree",
            "approve-config",
            "--worktree",
            worktree_id,
            "--allow-guardrail-update",
            "--approved-by",
            "mission-workflow-projection-test",
            "--output",
            "json",
        ],
    );

    let foundry_store = FoundryStore::open(&store).unwrap();
    let executor = ExecutorState {
        id: "codex".to_string(),
        display_name: "Codex CLI".to_string(),
        command: "codex".to_string(),
        installed: true,
        configured: true,
        command_path: Some("codex".to_string()),
        config_evidence: vec!["mission workflow projection test fixture".to_string()],
        non_interactive_ready: true,
        probe_evidence: vec!["test fixture does not invoke the model".to_string()],
        foundry_first_ready: true,
        foundry_first_entrypoint: None,
        harness_status: None,
        allowed: true,
        decision_source: "mission-workflow-projection-test".to_string(),
        synced_at: "2026-01-01T00:00:00Z".to_string(),
    };
    foundry_store
        .save_executor_state(&executor.id, &serde_json::to_value(&executor).unwrap())
        .unwrap();

    MissionFixture {
        _temp: temp,
        store,
        repository,
        cargo_command,
        evidence_command,
    }
}

fn start_mission(fixture: &MissionFixture, goal: &str) -> Value {
    foundry_json(
        &fixture.store,
        &[
            "mission",
            "start",
            "--goal",
            goal,
            "--worktree",
            fixture.repository.to_str().unwrap(),
            "--output",
            "json",
        ],
    )
}

fn context_for(fixture: &MissionFixture, workflow_id: &str, task_id: &str) -> Value {
    foundry_json(
        &fixture.store,
        &[
            "context",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--project-root",
            fixture.repository.to_str().unwrap(),
            "--budget",
            "4096",
            "--strict",
            "--view",
            "compact",
            "--output",
            "json",
        ],
    )
}

fn drive_assignment(fixture: &MissionFixture, mission_id: &str) -> (String, String, Value) {
    let driven = foundry_json(
        &fixture.store,
        &["mission", "drive", mission_id, "--output", "json"],
    );
    assert_eq!(driven["action"], "assignment_created");
    let task_id = driven["assignment"]["task"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let agent_id = driven["assignment"]["agent"]["instance_id"]
        .as_str()
        .unwrap()
        .to_string();
    (task_id, agent_id, driven)
}

fn execute_receipt(
    fixture: &MissionFixture,
    mission_id: &str,
    task_id: &str,
    agent_id: &str,
    idempotency_key: &str,
    cargo_argument: &str,
    evidence: &[&str],
) -> String {
    let (purpose, command, command_arguments) = if cargo_argument == "test" {
        (
            "test",
            fixture.cargo_command.as_str(),
            vec![
                "test".to_string(),
                "--offline".to_string(),
                "--".to_string(),
                "--nocapture".to_string(),
            ],
        )
    } else {
        (
            "preview",
            fixture.evidence_command.as_str(),
            vec![gate_evidence_envelope(evidence)],
        )
    };
    let mut args = vec![
        "mission".to_string(),
        "execute".to_string(),
        mission_id.to_string(),
        "--task".to_string(),
        task_id.to_string(),
        "--agent".to_string(),
        agent_id.to_string(),
        "--idempotency-key".to_string(),
        idempotency_key.to_string(),
        "--purpose".to_string(),
        purpose.to_string(),
        "--approved-by".to_string(),
        "mission-workflow-projection-test".to_string(),
        "--command".to_string(),
        command.to_string(),
    ];
    for argument in command_arguments {
        args.extend(["--command".to_string(), argument]);
    }
    for evidence_kind in evidence {
        args.extend(["--evidence".to_string(), (*evidence_kind).to_string()]);
    }
    args.extend(["--output".to_string(), "json".to_string()]);
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();
    let executed = foundry_json(&fixture.store, &args);
    assert_eq!(executed["replayed"], false);
    assert_eq!(executed["receipt"]["status"], "completed");
    assert_eq!(executed["receipt"]["exit_code"], 0);
    executed["receipt"]["receipt_id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[allow(clippy::too_many_arguments)]
fn submit_receipt(
    fixture: &MissionFixture,
    mission_id: &str,
    task_id: &str,
    agent_id: &str,
    idempotency_key: &str,
    receipt_id: &str,
    summary: &str,
    risk: Option<&str>,
) -> Value {
    let mut args = vec![
        "mission",
        "submit",
        mission_id,
        "--task",
        task_id,
        "--agent",
        agent_id,
        "--idempotency-key",
        idempotency_key,
        "--receipt-id",
        receipt_id,
        "--summary",
        summary,
    ];
    if let Some(risk) = risk {
        args.extend(["--risk", risk]);
    }
    args.extend(["--output", "json"]);
    foundry_json(&fixture.store, &args)
}

#[allow(clippy::too_many_arguments)]
fn submit_assignment(
    fixture: &MissionFixture,
    mission_id: &str,
    task_id: &str,
    agent_id: &str,
    idempotency_prefix: &str,
    cargo_argument: &str,
    evidence: &[&str],
    risk: Option<&str>,
) {
    let execution_key = format!("{idempotency_prefix}-execution-v1");
    let receipt_id = execute_receipt(
        fixture,
        mission_id,
        task_id,
        agent_id,
        &execution_key,
        cargo_argument,
        evidence,
    );
    let submission_key = format!("{idempotency_prefix}-submission-v1");
    let submitted = submit_receipt(
        fixture,
        mission_id,
        task_id,
        agent_id,
        &submission_key,
        &receipt_id,
        &format!("Validated delivery for {task_id}"),
        risk,
    );
    assert_eq!(submitted["status"], "queued");
}

fn blocked_validation(fixture: &MissionFixture, workflow_id: &str) -> Value {
    let validation = foundry_json_failure(
        &fixture.store,
        &["validate", "--workflow", workflow_id, "--output", "json"],
    );
    assert_eq!(validation["status"], "blocked");
    assert_eq!(validation["promotable"], false);
    validation
}

fn failed_rule_exists(validation: &Value, task_id: &str, kind: &str) -> bool {
    validation["failed_rules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|rule| rule["task_id"] == task_id && rule["kind"] == kind)
}

#[test]
fn mission_start_projects_exact_task_ids_and_routes_initial_context() {
    let fixture = mission_fixture();
    let started = start_mission(
        &fixture,
        "Project one operational mission DAG into workflow context",
    );
    let workflow_id = started["mission"]["workflow_id"].as_str().unwrap();
    let expected_ids = ["mission-task-001", "mission-task-002", "mission-task-003"];
    let mission_ids = started["mission"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(mission_ids, expected_ids);

    let store = FoundryStore::open(&fixture.store).unwrap();
    let workflow = store.load_workflow(workflow_id).unwrap();
    let workflow_ids = workflow
        .tasks
        .iter()
        .map(|task| task.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(workflow_ids, expected_ids);
    assert_eq!(
        workflow.tasks[1].dependencies,
        vec!["mission-task-001".to_string()]
    );
    assert_eq!(
        workflow.tasks[2].dependencies,
        vec!["mission-task-002".to_string()]
    );
    drop(store);

    let context = context_for(&fixture, workflow_id, "mission-task-001");
    assert_eq!(context["workflow_id"], workflow_id);
    assert_eq!(context["task_id"], "mission-task-001");
    assert_eq!(context["handoff_ready"], true);
    assert_eq!(context["handoff_status"], "ready");
    assert_eq!(context["guardrail"]["status"], "ready");
}

#[test]
fn repair_stays_non_promotable_until_every_mission_gate_passes() {
    let fixture = mission_fixture();
    let started = start_mission(
        &fixture,
        "Project gate readiness and validate only after bounded repair",
    );
    let mission_id = started["mission"]["id"].as_str().unwrap();
    let workflow_id = started["mission"]["workflow_id"].as_str().unwrap();
    blocked_validation(&fixture, workflow_id);

    let (intake_task, intake_agent, _) = drive_assignment(&fixture, mission_id);
    assert_eq!(intake_task, "mission-task-001");
    submit_assignment(
        &fixture,
        mission_id,
        &intake_task,
        &intake_agent,
        "projection-intake",
        "--version",
        &["requirements_summary", "acceptance_criteria"],
        None,
    );
    let intake_consumed = foundry_json(
        &fixture.store,
        &["mission", "resume", mission_id, "--output", "json"],
    );
    assert_eq!(intake_consumed["action"], "handoff_consumed");

    let store = FoundryStore::open(&fixture.store).unwrap();
    let workflow = store.load_workflow(workflow_id).unwrap();
    assert_eq!(workflow.tasks[0].status, TaskStatus::Completed);
    assert!(
        workflow.tasks[0]
            .work_item
            .goal_validation
            .definitively_ready
    );
    assert_eq!(workflow.tasks[1].status, TaskStatus::Pending);
    assert!(
        !workflow.tasks[1]
            .work_item
            .goal_validation
            .definitively_ready
    );
    drop(store);

    let task_two_context = context_for(&fixture, workflow_id, "mission-task-002");
    assert_eq!(task_two_context["handoff_ready"], true);
    assert_eq!(task_two_context["handoff_status"], "ready");

    let (delivery_task, delivery_agent, _) = drive_assignment(&fixture, mission_id);
    assert_eq!(delivery_task, "mission-task-002");
    submit_assignment(
        &fixture,
        mission_id,
        &delivery_task,
        &delivery_agent,
        "projection-delivery",
        "test",
        &[],
        None,
    );
    let delivery_consumed = foundry_json(
        &fixture.store,
        &["mission", "resume", mission_id, "--output", "json"],
    );
    assert_eq!(delivery_consumed["action"], "handoff_consumed");

    let store = FoundryStore::open(&fixture.store).unwrap();
    let workflow = store.load_workflow(workflow_id).unwrap();
    assert_eq!(workflow.tasks[1].status, TaskStatus::Completed);
    assert_eq!(
        workflow.tasks[1].work_item.backlog_state,
        "validation_pending"
    );
    assert!(
        !workflow.tasks[1]
            .work_item
            .goal_validation
            .definitively_ready
    );
    assert_eq!(workflow.tasks[2].status, TaskStatus::Pending);
    assert!(
        !workflow.tasks[2]
            .work_item
            .goal_validation
            .definitively_ready
    );
    drop(store);

    let task_three_context = context_for(&fixture, workflow_id, "mission-task-003");
    assert_eq!(task_three_context["handoff_ready"], true);
    assert_eq!(task_three_context["handoff_status"], "ready");
    let pre_review_validation = blocked_validation(&fixture, workflow_id);
    assert!(failed_rule_exists(
        &pre_review_validation,
        "mission-task-002",
        "goal_readiness"
    ));
    assert!(failed_rule_exists(
        &pre_review_validation,
        "mission-task-003",
        "task_status"
    ));

    let (review_task, review_agent, review_drive) = drive_assignment(&fixture, mission_id);
    assert_eq!(review_task, "mission-task-003");
    assert_eq!(review_drive["mission"]["status"], "reviewing");
    submit_assignment(
        &fixture,
        mission_id,
        &review_task,
        &review_agent,
        "projection-risky-review",
        "test",
        &["review_passed", "structured_delivery"],
        Some("promotion risk remains unresolved"),
    );
    let failed_review = foundry_json(
        &fixture.store,
        &["mission", "resume", mission_id, "--output", "json"],
    );
    assert_eq!(failed_review["action"], "repair_created");
    assert_eq!(failed_review["mission"]["status"], "repairing");
    assert_ne!(failed_review["mission"]["status"], "completed");

    let store = FoundryStore::open(&fixture.store).unwrap();
    let workflow = store.load_workflow(workflow_id).unwrap();
    assert_eq!(workflow.tasks[1].status, TaskStatus::Completed);
    assert!(
        workflow.tasks[1]
            .work_item
            .goal_validation
            .definitively_ready
    );
    assert_eq!(workflow.tasks[2].status, TaskStatus::Pending);
    assert!(
        !workflow.tasks[2]
            .work_item
            .goal_validation
            .definitively_ready
    );
    drop(store);
    blocked_validation(&fixture, workflow_id);

    let (repair_task, repair_agent, repair_drive) = drive_assignment(&fixture, mission_id);
    assert_eq!(repair_task, "mission-task-003");
    assert_eq!(repair_drive["assignment"]["task"]["attempt"], 2);
    submit_assignment(
        &fixture,
        mission_id,
        &repair_task,
        &repair_agent,
        "projection-repaired-review",
        "test",
        &[
            "review_passed",
            "structured_delivery",
            "no_unresolved_risks",
        ],
        None,
    );
    let completed = foundry_json(
        &fixture.store,
        &["mission", "resume", mission_id, "--output", "json"],
    );
    assert_eq!(completed["action"], "mission_completed");
    assert_eq!(completed["mission"]["status"], "completed");

    let validation = foundry_json(
        &fixture.store,
        &["validate", "--workflow", workflow_id, "--output", "json"],
    );
    assert_eq!(validation["status"], "passed");
    assert_eq!(validation["promotable"], true);
    assert!(validation["failed_rules"].as_array().unwrap().is_empty());
    assert!(validation["rework_tasks"].as_array().unwrap().is_empty());

    let store = FoundryStore::open(&fixture.store).unwrap();
    let workflow = store.load_workflow(workflow_id).unwrap();
    assert_eq!(workflow.status, "completed");
    assert!(workflow.tasks.iter().all(|task| {
        task.status == TaskStatus::Completed && task.work_item.goal_validation.definitively_ready
    }));
}
