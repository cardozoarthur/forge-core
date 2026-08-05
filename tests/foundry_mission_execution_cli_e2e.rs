use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use tempfile::{tempdir, TempDir};

#[path = "support/mission_toolchain.rs"]
mod mission_toolchain;
use mission_toolchain::{gate_evidence_envelope, write_gate_evidence_command};

fn foundry() -> Command {
    Command::cargo_bin("foundry").expect("foundry binary should build")
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
            "user.name=Foundry Mission CLI E2E",
            "-c",
            "user.email=foundry-mission-cli@example.invalid",
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

fn executable(path: &Path, source: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, source).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn json_command(store: &Path, args: &[&str], succeeds: bool) -> Value {
    let assertion = foundry().arg("--store").arg(store).args(args).assert();
    let output = if succeeds {
        assertion.success().get_output().stdout.clone()
    } else {
        assertion.failure().get_output().stdout.clone()
    };
    serde_json::from_slice(&output).expect("command should return a JSON report")
}

struct Fixture {
    _temp: TempDir,
    store: std::path::PathBuf,
    repository: std::path::PathBuf,
    cargo_command: PathBuf,
    evidence_command: PathBuf,
}

fn fixture() -> Fixture {
    let temp = tempdir().unwrap();
    let store = temp.path().join("foundry.sqlite");
    let repository = temp.path().join("repository");
    let home = temp.path().join("home");
    let bin = temp.path().join("bin");
    let codex = bin.join("codex");
    executable(
        &codex,
        "#!/bin/sh\nif [ \"${1:-}\" = \"--version\" ]; then\n  echo 'codex-cli test'\n  exit 0\nfi\nexit 2\n",
    );
    fs::create_dir_all(home.join(".codex")).unwrap();
    fs::write(home.join(".codex/config.toml"), "model = \"test\"\n").unwrap();
    let synced = json_command(
        &store,
        &[
            "sync",
            "executors",
            "--home",
            home.to_str().unwrap(),
            "--executor-path",
            bin.to_str().unwrap(),
            "--allow",
            "codex",
            "--no-prompt",
            "--output",
            "json",
        ],
        true,
    );
    assert!(synced["usable"]
        .as_array()
        .unwrap()
        .iter()
        .any(|executor| executor == "codex"));

    git_repository(&repository);
    let cargo_command = repository.join("fixture-bin/cargo");
    executable(
        &cargo_command,
        "#!/bin/sh\ncase \"${1:-}\" in\n  slow) sleep 2 ;;\n  --version) printf '%s\\n' 'cargo mission-cli-e2e 1.0.0' ;;\n  *) exit 2 ;;\nesac\n",
    );
    let evidence_command = PathBuf::from(write_gate_evidence_command(&repository));
    let repository_text = repository.to_str().unwrap();
    let registered = json_command(
        &store,
        &[
            "worktree",
            "register",
            "--path",
            repository_text,
            "--output",
            "json",
        ],
        true,
    );
    let worktree_id = registered["worktree"]["id"].as_str().unwrap();
    json_command(
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
        true,
    );

    let config_path = repository.join(".foundry/worktree.toml");
    let mut config: toml::Value =
        toml::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config["guardrails"]["allowed_commands"] = toml::Value::Array(vec![
        toml::Value::String(cargo_command.display().to_string()),
        toml::Value::String(evidence_command.display().to_string()),
    ]);
    config["guardrails"]["max_command_seconds"] = toml::Value::Integer(1);
    config["sandbox"]["runtime"] = toml::Value::String("bubblewrap".to_string());
    config["sandbox"]["network"] = toml::Value::String("deny".to_string());
    fs::write(&config_path, toml::to_string_pretty(&config).unwrap()).unwrap();
    json_command(
        &store,
        &[
            "worktree",
            "approve-config",
            "--worktree",
            worktree_id,
            "--allow-guardrail-update",
            "--approved-by",
            "mission-cli-e2e",
            "--output",
            "json",
        ],
        true,
    );
    Fixture {
        _temp: temp,
        store,
        repository,
        cargo_command,
        evidence_command,
    }
}

fn execute_args<'a>(
    mission_id: &'a str,
    task_id: &'a str,
    agent_id: &'a str,
    key: &'a str,
    command: &'a str,
    command_args: &[&'a str],
) -> Vec<&'a str> {
    let mut args = vec![
        "mission",
        "execute",
        mission_id,
        "--task",
        task_id,
        "--agent",
        agent_id,
        "--idempotency-key",
        key,
        "--purpose",
        "test",
        "--approved-by",
        "mission-cli-e2e",
        "--command",
        command,
    ];
    for argument in command_args {
        args.push("--command");
        args.push(argument);
    }
    args.extend(["--output", "json"]);
    args
}

#[test]
fn cli_execution_receipts_cover_nonzero_timeout_replay_submit_and_reopen() {
    let fixture = fixture();
    let repository = fixture.repository.to_str().unwrap();
    let started = json_command(
        &fixture.store,
        &[
            "mission",
            "start",
            "--goal",
            "Execute, evidence and resume one real mission assignment",
            "--worktree",
            repository,
            "--output",
            "json",
        ],
        true,
    );
    let mission_id = started["mission"]["id"].as_str().unwrap();
    let driven = json_command(
        &fixture.store,
        &["mission", "drive", mission_id, "--output", "json"],
        true,
    );
    let task_id = driven["assignment"]["task"]["id"].as_str().unwrap();
    let agent_id = driven["assignment"]["agent"]["instance_id"]
        .as_str()
        .unwrap();

    let planned_evidence = gate_evidence_envelope(&["requirements_summary"]);
    let mut planned_args = execute_args(
        mission_id,
        task_id,
        agent_id,
        "exec-dry-run",
        fixture.evidence_command.to_str().unwrap(),
        &[planned_evidence.as_str()],
    );
    let approval = planned_args
        .iter()
        .position(|argument| *argument == "--approved-by")
        .unwrap();
    planned_args.drain(approval..=approval + 1);
    planned_args.push("--dry-run");
    planned_args.extend(["--evidence", "requirements_summary"]);
    let planned = json_command(&fixture.store, &planned_args, true);
    assert_eq!(planned["status"], "planned");
    assert_eq!(planned["persisted"], false);
    assert_eq!(
        planned["plan"]["requested_evidence"],
        json!(["requirements_summary"])
    );
    assert_eq!(
        planned["plan"]["gate_evidence_contract"][0]["gate_ids"],
        json!(["requirements_ready"])
    );

    let mut blocked_args = execute_args(
        mission_id,
        task_id,
        agent_id,
        "exec-dry-run-blocked",
        "/bin/false",
        &[],
    );
    let approval = blocked_args
        .iter()
        .position(|argument| *argument == "--approved-by")
        .unwrap();
    blocked_args.drain(approval..=approval + 1);
    blocked_args.push("--dry-run");
    let blocked = json_command(&fixture.store, &blocked_args, false);
    assert_eq!(blocked["status"], "blocked");
    assert_eq!(blocked["persisted"], false);

    let nonzero = json_command(
        &fixture.store,
        &execute_args(
            mission_id,
            task_id,
            agent_id,
            "exec-nonzero",
            fixture.cargo_command.to_str().unwrap(),
            &["--definitely-invalid-foundry-test"],
        ),
        false,
    );
    assert_eq!(nonzero["receipt"]["status"], "failed");
    assert_ne!(nonzero["receipt"]["exit_code"], 0);
    assert_eq!(nonzero["receipt"]["timed_out"], false);
    let nonzero_receipt = nonzero["receipt"]["receipt_id"].as_str().unwrap();
    let reconciled_nonzero = json_command(
        &fixture.store,
        &[
            "mission",
            "execution",
            "reconcile",
            nonzero_receipt,
            "--outcome",
            "no_effect_retry",
            "--approved-by",
            "mission-cli-e2e",
            "--reason",
            "invalid cargo argument exited before any repository mutation",
            "--confirm-no-effect-retry",
            "--output",
            "json",
        ],
        true,
    );
    assert_eq!(reconciled_nonzero["status"], "reconciled_no_effect_retry");

    let timed_out = json_command(
        &fixture.store,
        &execute_args(
            mission_id,
            task_id,
            agent_id,
            "exec-timeout",
            fixture.cargo_command.to_str().unwrap(),
            &["slow"],
        ),
        false,
    );
    assert_eq!(timed_out["receipt"]["status"], "timed_out");
    assert_eq!(timed_out["receipt"]["timed_out"], true);
    let timeout_receipt = timed_out["receipt"]["receipt_id"].as_str().unwrap();
    let reconciled_timeout = json_command(
        &fixture.store,
        &[
            "mission",
            "execution",
            "reconcile",
            timeout_receipt,
            "--outcome",
            "no_effect_retry",
            "--approved-by",
            "mission-cli-e2e",
            "--reason",
            "fixture only slept and was independently verified to have no effect",
            "--confirm-no-effect-retry",
            "--output",
            "json",
        ],
        true,
    );
    assert_eq!(
        reconciled_timeout["reconciliation"]["previous_state"],
        "timed_out"
    );

    let success_evidence = gate_evidence_envelope(&["requirements_summary", "acceptance_criteria"]);
    let mut success_args = execute_args(
        mission_id,
        task_id,
        agent_id,
        "exec-success",
        fixture.evidence_command.to_str().unwrap(),
        &[success_evidence.as_str()],
    );
    success_args.extend([
        "--evidence",
        "requirements_summary",
        "--evidence",
        "acceptance_criteria",
    ]);
    let completed = json_command(&fixture.store, &success_args, true);
    assert_eq!(completed["replayed"], false);
    assert_eq!(completed["receipt"]["status"], "completed");
    assert_eq!(completed["receipt"]["exit_code"], 0);
    assert_eq!(
        completed["receipt"]["approval"]["approved_by"],
        "mission-cli-e2e"
    );
    assert!(completed["receipt"]["claims"]
        .as_array()
        .unwrap()
        .iter()
        .any(|claim| {
            claim["kind"] == "gate_evidence"
                && claim["evidence_kind"] == "requirements_summary"
                && claim["gate_ids"] == json!(["requirements_ready"])
        }));
    let receipt_id = completed["receipt"]["receipt_id"].as_str().unwrap();

    let replayed = json_command(&fixture.store, &success_args, true);
    assert_eq!(replayed["replayed"], true);
    assert_eq!(replayed["receipt"]["receipt_id"], receipt_id);

    let listed = json_command(
        &fixture.store,
        &[
            "mission",
            "execution",
            "list",
            "--mission",
            mission_id,
            "--task",
            task_id,
            "--output",
            "json",
        ],
        true,
    );
    assert_eq!(listed["records"].as_array().unwrap().len(), 3);
    let inspected = json_command(
        &fixture.store,
        &[
            "mission",
            "execution",
            "inspect",
            receipt_id,
            "--output",
            "json",
        ],
        true,
    );
    assert_eq!(inspected["receipt"]["receipt_id"], receipt_id);

    let submitted = json_command(
        &fixture.store,
        &[
            "mission",
            "submit",
            mission_id,
            "--task",
            task_id,
            "--agent",
            agent_id,
            "--idempotency-key",
            "submit-success",
            "--receipt-id",
            receipt_id,
            "--summary",
            "Successful sandbox evidence is ready for the orchestrator",
            "--output",
            "json",
        ],
        true,
    );
    assert_eq!(submitted["status"], "queued");
    assert_eq!(submitted["accepted"], false);

    // Missing receipt is rejected at the public CLI boundary, before a caller can
    // invent tests or validation evidence.
    foundry()
        .arg("--store")
        .arg(&fixture.store)
        .args([
            "mission",
            "submit",
            mission_id,
            "--task",
            task_id,
            "--agent",
            agent_id,
            "--idempotency-key",
            "missing-receipt",
            "--summary",
            "This must be rejected",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--receipt-id"));

    let reopened = json_command(
        &fixture.store,
        &["mission", "inspect", mission_id, "--output", "json"],
        true,
    );
    assert_eq!(reopened["handoffs"][0]["status"], "queued");
    assert_eq!(reopened["inbox"][0]["status"], "pending");
    let resumed = json_command(
        &fixture.store,
        &["mission", "resume", mission_id, "--output", "json"],
        true,
    );
    assert_eq!(resumed["action"], "handoff_consumed");
    assert_eq!(resumed["mission"]["handoffs"][0]["status"], "accepted");
}
