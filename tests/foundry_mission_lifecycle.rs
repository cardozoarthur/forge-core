use assert_cmd::Command;
use foundry_core::executor::ExecutorState;
use foundry_core::mission::{
    install_builtin_squads, install_squad, load_squad, QualityGateDefinition,
};
use foundry_core::storage::FoundryStore;
use predicates::prelude::*;
use rusqlite::Connection;
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
    serde_json::from_slice(&assertion.success().get_output().stdout)
        .expect("command should return JSON")
}

fn foundry_json(store: &Path, args: &[&str]) -> Value {
    json_stdout(foundry().arg("--store").arg(store).args(args).assert())
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
            "user.name=Foundry Mission Lifecycle",
            "-c",
            "user.email=foundry-mission-lifecycle@example.invalid",
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

    let repository_text = repository.to_str().unwrap();
    let registered = foundry_json(
        &store,
        &[
            "worktree",
            "register",
            "--path",
            repository_text,
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
            "mission-lifecycle-test",
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
        config_evidence: vec!["mission lifecycle test fixture".to_string()],
        non_interactive_ready: true,
        probe_evidence: vec!["test fixture does not invoke the model".to_string()],
        foundry_first_ready: true,
        foundry_first_entrypoint: None,
        harness_status: None,
        allowed: true,
        decision_source: "mission-lifecycle-test".to_string(),
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

fn install_extended_gate_squad(store_path: &Path) -> String {
    let store = FoundryStore::open(store_path).unwrap();
    install_builtin_squads(&store).unwrap();
    let mut squad = load_squad(&store, "software-factory", Some("1.0.0")).unwrap();
    squad.id = "software-factory-extended-gates".to_string();
    squad.name = "Software Factory Extended Gates".to_string();
    squad.version = "1.0.0-test".to_string();
    squad.distribution.origin = "test-fixture".to_string();
    squad.distribution.channel = "test".to_string();
    squad.distribution.signed = true;
    squad.distribution.signature = Some("test:software-factory-extended-gates".to_string());
    squad.distribution.trusted = true;
    squad.distribution.auto_update = false;
    squad.gates.extend([
        QualityGateDefinition {
            id: "security_ready".to_string(),
            trigger: "review_complete".to_string(),
            validator: "orchestrator_policy".to_string(),
            required_evidence: vec!["security_attestation".to_string()],
            approval_policy: "deterministic".to_string(),
            failure_action: "request_revision".to_string(),
            timeout_action: "block".to_string(),
        },
        QualityGateDefinition {
            id: "release_ready".to_string(),
            trigger: "review_complete".to_string(),
            validator: "orchestrator_policy".to_string(),
            required_evidence: vec!["release_manifest".to_string()],
            approval_policy: "deterministic".to_string(),
            failure_action: "request_revision".to_string(),
            timeout_action: "block".to_string(),
        },
    ]);
    let installed = install_squad(&store, &squad).unwrap();
    assert!(installed.validation.valid);
    squad.id
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
        "mission-lifecycle-test".to_string(),
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
    assert_eq!(executed["receipt"]["executor_id"], "codex");
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

#[test]
fn operational_handoff_survives_process_reopen_and_dead_letters_exhausted_lease() {
    let fixture = mission_fixture();
    let worktree = fixture.repository.to_str().unwrap();

    let started = foundry_json(
        &fixture.store,
        &[
            "mission",
            "start",
            "--goal",
            "Prove a durable operational mission lifecycle",
            "--worktree",
            worktree,
            "--output",
            "json",
        ],
    );
    assert_eq!(started["status"], "started");
    assert_eq!(started["mission"]["mode"], "workflow");
    assert_eq!(started["mission"]["status"], "running");
    assert_eq!(started["mission"]["worktree"], worktree);
    let mission_id = started["mission"]["id"].as_str().unwrap();

    let dispatched = foundry_json(
        &fixture.store,
        &["mission", "drive", mission_id, "--output", "json"],
    );
    assert_eq!(dispatched["action"], "assignment_created");
    let task_id = dispatched["assignment"]["task"]["id"].as_str().unwrap();
    let agent_id = dispatched["assignment"]["agent"]["instance_id"]
        .as_str()
        .unwrap();
    assert_eq!(dispatched["assignment"]["harness"]["task_id"], task_id);

    let receipt_id = execute_receipt(
        &fixture,
        mission_id,
        task_id,
        agent_id,
        "reopen-intake-execution-v1",
        "--version",
        &["requirements_summary", "acceptance_criteria"],
    );
    let submitted = submit_receipt(
        &fixture,
        mission_id,
        task_id,
        agent_id,
        "reopen-intake-v1",
        &receipt_id,
        "Intake and acceptance evidence are ready",
        None,
    );
    assert_eq!(submitted["status"], "queued");
    assert_eq!(submitted["accepted"], false);
    let handoff_id = submitted["handoff_id"].as_str().unwrap();

    let inspected = foundry_json(
        &fixture.store,
        &["mission", "inspect", mission_id, "--output", "json"],
    );
    let queued_handoff = inspected["handoffs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|handoff| handoff["id"] == handoff_id)
        .unwrap();
    assert_eq!(queued_handoff["status"], "queued");
    assert!(queued_handoff["accepted_at"].is_null());
    assert_eq!(inspected["inbox"][0]["status"], "pending");

    let duplicate = submit_receipt(
        &fixture,
        mission_id,
        task_id,
        agent_id,
        "reopen-intake-v1",
        &receipt_id,
        "Intake and acceptance evidence are ready",
        None,
    );
    assert_eq!(duplicate["status"], "deduplicated");
    assert_eq!(duplicate["handoff_id"], handoff_id);

    let resumed = foundry_json(
        &fixture.store,
        &["mission", "resume", mission_id, "--output", "json"],
    );
    assert_eq!(resumed["action"], "handoff_consumed");
    assert_eq!(resumed["mission"]["handoffs"][0]["status"], "accepted");
    assert_eq!(resumed["mission"]["inbox"][0]["status"], "consumed");
    assert!(resumed["mission"]["inbox"][0]["attempts"].as_u64().unwrap() >= 1);

    let connection = Connection::open(&fixture.store).unwrap();
    let handoff_rows: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM mission_handoffs WHERE idempotency_key = 'reopen-intake-v1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(handoff_rows, 1);
    let checkpoint_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM mission_runtime_checkpoints WHERE mission_id = ?1",
            [mission_id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        checkpoint_count >= 8,
        "every transition should be revisioned"
    );
    drop(connection);

    let second = foundry_json(
        &fixture.store,
        &["mission", "drive", mission_id, "--output", "json"],
    );
    let second_task = second["assignment"]["task"]["id"].as_str().unwrap();
    let second_agent = second["assignment"]["agent"]["instance_id"]
        .as_str()
        .unwrap();
    let second_receipt = execute_receipt(
        &fixture,
        mission_id,
        second_task,
        second_agent,
        "crashed-consumer-execution-v1",
        "test",
        &[],
    );
    let second_submit = submit_receipt(
        &fixture,
        mission_id,
        second_task,
        second_agent,
        "crashed-consumer-v1",
        &second_receipt,
        "Delivery waits behind a repeatedly crashed consumer",
        None,
    );
    let second_handoff = second_submit["handoff_id"].as_str().unwrap();
    let connection = Connection::open(&fixture.store).unwrap();
    connection
        .execute(
            "UPDATE mission_runtime_inbox SET status = 'leased', attempts = max_attempts, lease_owner = 'crashed-worker', lease_expires_at = '2000-01-01T00:00:00+00:00' WHERE handoff_id = ?1",
            [second_handoff],
        )
        .unwrap();
    drop(connection);

    let blocked = foundry_json(
        &fixture.store,
        &["mission", "resume", mission_id, "--output", "json"],
    );
    assert_eq!(blocked["status"], "blocked");
    assert_eq!(blocked["action"], "dead_letter_blocked");
    assert_eq!(blocked["mission"]["status"], "blocked");
}

#[test]
fn operational_gate_failure_repairs_and_revalidates_before_completion() {
    let fixture = mission_fixture();
    let started = foundry_json(
        &fixture.store,
        &[
            "mission",
            "start",
            "--goal",
            "Repair failed evidence and complete only after revalidation",
            "--worktree",
            fixture.repository.to_str().unwrap(),
            "--output",
            "json",
        ],
    );
    let mission_id = started["mission"]["id"].as_str().unwrap();

    let intake = foundry_json(
        &fixture.store,
        &["mission", "drive", mission_id, "--output", "json"],
    );
    let intake_task = intake["assignment"]["task"]["id"].as_str().unwrap();
    let intake_agent = intake["assignment"]["agent"]["instance_id"]
        .as_str()
        .unwrap();
    let intake_receipt = execute_receipt(
        &fixture,
        mission_id,
        intake_task,
        intake_agent,
        "intake-execution-v1",
        "--version",
        &["requirements_summary", "acceptance_criteria"],
    );
    submit_receipt(
        &fixture,
        mission_id,
        intake_task,
        intake_agent,
        "intake-v1",
        &intake_receipt,
        "Intake and acceptance evidence are ready",
        None,
    );
    let intake_consumed = foundry_json(
        &fixture.store,
        &["mission", "resume", mission_id, "--output", "json"],
    );
    assert_eq!(intake_consumed["action"], "handoff_consumed");
    assert_eq!(intake_consumed["mission"]["gates"][0]["status"], "passed");

    let delivery = foundry_json(
        &fixture.store,
        &["mission", "drive", mission_id, "--output", "json"],
    );
    let delivery_task = delivery["assignment"]["task"]["id"].as_str().unwrap();
    let delivery_agent = delivery["assignment"]["agent"]["instance_id"]
        .as_str()
        .unwrap();
    let delivery_receipt = execute_receipt(
        &fixture,
        mission_id,
        delivery_task,
        delivery_agent,
        "delivery-execution-v1",
        "test",
        &[],
    );
    submit_receipt(
        &fixture,
        mission_id,
        delivery_task,
        delivery_agent,
        "delivery-v1",
        &delivery_receipt,
        "Bounded implementation delivery is ready for independent review",
        None,
    );
    let delivery_consumed = foundry_json(
        &fixture.store,
        &["mission", "resume", mission_id, "--output", "json"],
    );
    assert_eq!(delivery_consumed["action"], "handoff_consumed");

    let review = foundry_json(
        &fixture.store,
        &["mission", "drive", mission_id, "--output", "json"],
    );
    assert_eq!(review["mission"]["status"], "reviewing");
    let review_task = review["assignment"]["task"]["id"].as_str().unwrap();
    let review_agent = review["assignment"]["agent"]["instance_id"]
        .as_str()
        .unwrap();
    let failed_review_receipt = execute_receipt(
        &fixture,
        mission_id,
        review_task,
        review_agent,
        "review-with-risk-execution-v1",
        "test",
        &["review_passed", "structured_delivery"],
    );
    submit_receipt(
        &fixture,
        mission_id,
        review_task,
        review_agent,
        "review-with-risk-v1",
        &failed_review_receipt,
        "Independent review found one unresolved promotion risk",
        Some("promotion risk requires a bounded repair"),
    );
    let failed = foundry_json(
        &fixture.store,
        &["mission", "resume", mission_id, "--output", "json"],
    );
    assert_eq!(failed["action"], "repair_created");
    assert_eq!(failed["mission"]["status"], "repairing");
    let reopened_review = failed["mission"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["id"] == review_task)
        .unwrap();
    assert_eq!(reopened_review["status"], "repairing");
    let failed_outcome_gate = failed["mission"]["gates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|gate| gate["gate_id"] == "mission_outcome_ready" && gate["status"] == "failed")
        .unwrap();
    assert_eq!(failed_outcome_gate["attempt"], 1);

    let repair = foundry_json(
        &fixture.store,
        &["mission", "drive", mission_id, "--output", "json"],
    );
    let repair_agent = repair["assignment"]["agent"]["instance_id"]
        .as_str()
        .unwrap();
    assert_eq!(repair["assignment"]["task"]["id"], review_task);
    assert_eq!(repair["assignment"]["task"]["attempt"], 2);
    let repaired_receipt = execute_receipt(
        &fixture,
        mission_id,
        review_task,
        repair_agent,
        "repaired-review-execution-v1",
        "test",
        &[
            "review_passed",
            "structured_delivery",
            "no_unresolved_risks",
        ],
    );
    submit_receipt(
        &fixture,
        mission_id,
        review_task,
        repair_agent,
        "repaired-review-v1",
        &repaired_receipt,
        "Independent review passed every promotion gate after repair",
        None,
    );
    let completed = foundry_json(
        &fixture.store,
        &["mission", "resume", mission_id, "--output", "json"],
    );
    assert_eq!(completed["action"], "mission_completed");
    assert_eq!(completed["mission"]["status"], "completed");
    assert_eq!(completed["mission"]["rework_cycles"], 1);

    let outcome_gates = completed["mission"]["gates"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|gate| gate["gate_id"] == "mission_outcome_ready")
        .collect::<Vec<_>>();
    assert_eq!(outcome_gates.len(), 2);
    assert_eq!(outcome_gates[1]["status"], "passed");
    assert_eq!(outcome_gates[1]["attempt"], 2);
    assert_eq!(outcome_gates[1]["supersedes_attempt"], 1);
    assert!(completed["mission"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .all(|task| task["status"] == "completed"));
}

#[test]
fn custom_squad_with_five_gates_completes_without_a_repair_loop() {
    let fixture = mission_fixture();
    let squad_id = install_extended_gate_squad(&fixture.store);
    let started = foundry_json(
        &fixture.store,
        &[
            "mission",
            "start",
            "--goal",
            "Complete an extended gate mission without synthetic repair",
            "--squad",
            &squad_id,
            "--squad-version",
            "1.0.0-test",
            "--worktree",
            fixture.repository.to_str().unwrap(),
            "--output",
            "json",
        ],
    );
    let mission_id = started["mission"]["id"].as_str().unwrap();

    let intake = foundry_json(
        &fixture.store,
        &["mission", "drive", mission_id, "--output", "json"],
    );
    let intake_task = intake["assignment"]["task"]["id"].as_str().unwrap();
    let intake_agent = intake["assignment"]["agent"]["instance_id"]
        .as_str()
        .unwrap();
    let intake_receipt = execute_receipt(
        &fixture,
        mission_id,
        intake_task,
        intake_agent,
        "extended-intake-execution",
        "--version",
        &["requirements_summary", "acceptance_criteria"],
    );
    submit_receipt(
        &fixture,
        mission_id,
        intake_task,
        intake_agent,
        "extended-intake-submit",
        &intake_receipt,
        "Extended intake is complete",
        None,
    );
    let intake_resumed = foundry_json(
        &fixture.store,
        &["mission", "resume", mission_id, "--output", "json"],
    );
    assert_eq!(intake_resumed["action"], "handoff_consumed");

    let delivery = foundry_json(
        &fixture.store,
        &["mission", "drive", mission_id, "--output", "json"],
    );
    let delivery_task = delivery["assignment"]["task"]["id"].as_str().unwrap();
    let delivery_agent = delivery["assignment"]["agent"]["instance_id"]
        .as_str()
        .unwrap();
    let delivery_receipt = execute_receipt(
        &fixture,
        mission_id,
        delivery_task,
        delivery_agent,
        "extended-delivery-execution",
        "test",
        &[],
    );
    submit_receipt(
        &fixture,
        mission_id,
        delivery_task,
        delivery_agent,
        "extended-delivery-submit",
        &delivery_receipt,
        "Extended delivery is ready for review",
        None,
    );
    let delivery_resumed = foundry_json(
        &fixture.store,
        &["mission", "resume", mission_id, "--output", "json"],
    );
    assert_eq!(delivery_resumed["action"], "handoff_consumed");

    let review = foundry_json(
        &fixture.store,
        &["mission", "drive", mission_id, "--output", "json"],
    );
    let review_task = review["assignment"]["task"]["id"].as_str().unwrap();
    let review_agent = review["assignment"]["agent"]["instance_id"]
        .as_str()
        .unwrap();
    let review_receipt = execute_receipt(
        &fixture,
        mission_id,
        review_task,
        review_agent,
        "extended-review-execution",
        "test",
        &[
            "review_passed",
            "structured_delivery",
            "no_unresolved_risks",
            "security_attestation",
            "release_manifest",
        ],
    );
    submit_receipt(
        &fixture,
        mission_id,
        review_task,
        review_agent,
        "extended-review-submit",
        &review_receipt,
        "Every extended final gate has authoritative evidence",
        None,
    );
    let completed = foundry_json(
        &fixture.store,
        &["mission", "resume", mission_id, "--output", "json"],
    );
    assert_eq!(completed["action"], "mission_completed");
    assert_eq!(completed["mission"]["status"], "completed");
    assert_eq!(completed["mission"]["rework_cycles"], 0);
    for gate_id in [
        "requirements_ready",
        "implementation_validated",
        "mission_outcome_ready",
        "security_ready",
        "release_ready",
    ] {
        assert!(completed["mission"]["gates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|gate| gate["gate_id"] == gate_id && gate["status"] == "passed"));
    }
}

#[test]
fn operational_start_requires_an_explicit_real_worktree() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("foundry.sqlite");
    foundry()
        .arg("--store")
        .arg(&store)
        .args([
            "mission",
            "start",
            "--goal",
            "This must not silently invent a worktree",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--worktree"));
}
