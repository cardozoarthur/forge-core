use assert_cmd::Command;
#[cfg(unix)]
use forge_core::executor::ExecutorState;
#[cfg(unix)]
use forge_core::milestone::ProductionMissionLifecycleReceipt;
#[cfg(unix)]
use forge_core::storage::ForgeStore;
use predicates::prelude::*;
#[cfg(unix)]
use serde_json::Value;
#[cfg(unix)]
use sha2::{Digest, Sha256};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;
#[cfg(unix)]
use std::process::Command as ProcessCommand;
use tempfile::{tempdir, TempDir};

fn forge() -> Command {
    Command::cargo_bin("forge").expect("forge binary should build")
}

#[cfg(unix)]
fn forge_json(store: &Path, args: &[&str]) -> Value {
    let mut command = forge();
    let assertion = command.arg("--store").arg(store).args(args).assert();
    let output = assertion.get_output();
    let json: Value = serde_json::from_slice(&output.stdout).expect("command should return JSON");
    assert!(
        output.status.success(),
        "forge command failed: args={args:?} status={} policy_trace={}",
        output.status,
        json["receipt"]["policy_trace"]
    );
    json
}

#[cfg(unix)]
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
            "user.name=Forge Production Evidence",
            "-c",
            "user.email=forge-production-evidence@example.invalid",
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

#[cfg(unix)]
fn executable(path: &Path, source: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
struct MissionEvidenceFixture {
    temp: TempDir,
    store: PathBuf,
    repository: PathBuf,
    evidence_command: PathBuf,
}

#[cfg(unix)]
fn mission_evidence_fixture() -> MissionEvidenceFixture {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let repository = temp.path().join("repository");
    git_repository(&repository);

    let registered = forge_json(
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
    forge_json(
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

    let evidence_command = repository.join("fixture-bin/git");
    executable(
        &evidence_command,
        concat!(
            "#!/bin/sh\n",
            "printf '%s\\n' 'FORGE_GATE_EVIDENCE:",
            "{\"schema_version\":\"forge.mission.gate_evidence_observation.v1\",",
            "\"evidence\":{\"requirements_summary\":\"Production evidence requirements were inspected.\",",
            "\"acceptance_criteria\":[\"The typed lifecycle must persist execute, submit and resume.\"]}}'\n"
        ),
    );
    let config_path = repository.join(".forge/worktree.toml");
    let mut config: toml::Value =
        toml::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config["guardrails"]["allowed_commands"] = toml::Value::Array(vec![toml::Value::String(
        evidence_command.display().to_string(),
    )]);
    config["guardrails"]["max_command_seconds"] = toml::Value::Integer(5);
    config["sandbox"]["runtime"] = toml::Value::String("bubblewrap".to_string());
    config["sandbox"]["network"] = toml::Value::String("deny".to_string());
    fs::write(&config_path, toml::to_string_pretty(&config).unwrap()).unwrap();
    forge_json(
        &store,
        &[
            "worktree",
            "approve-config",
            "--worktree",
            worktree_id,
            "--allow-guardrail-update",
            "--approved-by",
            "production-evidence-test",
            "--output",
            "json",
        ],
    );

    let forge_store = ForgeStore::open(&store).unwrap();
    let executor = ExecutorState {
        id: "codex".to_string(),
        display_name: "Codex CLI".to_string(),
        command: "codex".to_string(),
        installed: true,
        configured: true,
        command_path: Some("codex".to_string()),
        config_evidence: vec!["production evidence fixture".to_string()],
        non_interactive_ready: true,
        probe_evidence: vec!["fixture does not invoke a model".to_string()],
        forge_first_ready: true,
        forge_first_entrypoint: None,
        harness_status: None,
        allowed: true,
        decision_source: "production-evidence-test".to_string(),
        synced_at: "2026-01-01T00:00:00Z".to_string(),
    };
    forge_store
        .save_executor_state(&executor.id, &serde_json::to_value(&executor).unwrap())
        .unwrap();

    MissionEvidenceFixture {
        temp,
        store,
        repository,
        evidence_command,
    }
}

#[cfg(unix)]
fn consumed_mission_lifecycle() -> (MissionEvidenceFixture, String, String) {
    let fixture = mission_evidence_fixture();
    let started = forge_json(
        &fixture.store,
        &[
            "mission",
            "start",
            "--goal",
            "Produce one typed operational mission lifecycle receipt",
            "--worktree",
            fixture.repository.to_str().unwrap(),
            "--output",
            "json",
        ],
    );
    let mission_id = started["mission"]["id"].as_str().unwrap().to_string();
    let workflow_id = started["mission"]["workflow_id"]
        .as_str()
        .unwrap()
        .to_string();
    let task_id = started["mission"]["tasks"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let context = forge_json(
        &fixture.store,
        &[
            "context",
            "--workflow",
            &workflow_id,
            "--task",
            &task_id,
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
    );
    assert_eq!(context["handoff_ready"], true);
    assert_eq!(context["guardrail"]["status"], "ready");
    let driven = forge_json(
        &fixture.store,
        &["mission", "drive", &mission_id, "--output", "json"],
    );
    assert_eq!(driven["assignment"]["task"]["id"], task_id);
    let agent_id = driven["assignment"]["agent"]["instance_id"]
        .as_str()
        .unwrap()
        .to_string();

    let executed = forge_json(
        &fixture.store,
        &[
            "mission",
            "execute",
            &mission_id,
            "--task",
            &task_id,
            "--agent",
            &agent_id,
            "--idempotency-key",
            "production-evidence-execution",
            "--purpose",
            "preview",
            "--approved-by",
            "production-evidence-test",
            "--evidence",
            "requirements_summary",
            "--evidence",
            "acceptance_criteria",
            "--command",
            fixture.evidence_command.to_str().unwrap(),
            "--output",
            "json",
        ],
    );
    assert_eq!(executed["receipt"]["status"], "completed");
    let receipt_id = executed["receipt"]["receipt_id"]
        .as_str()
        .unwrap()
        .to_string();
    let submitted = forge_json(
        &fixture.store,
        &[
            "mission",
            "submit",
            &mission_id,
            "--task",
            &task_id,
            "--agent",
            &agent_id,
            "--idempotency-key",
            "production-evidence-submission",
            "--receipt-id",
            &receipt_id,
            "--summary",
            "Typed operational lifecycle evidence",
            "--output",
            "json",
        ],
    );
    assert_eq!(submitted["status"], "queued");
    let resumed = forge_json(
        &fixture.store,
        &["mission", "resume", &mission_id, "--output", "json"],
    );
    assert_eq!(resumed["action"], "handoff_consumed");

    (fixture, mission_id, receipt_id)
}

#[test]
fn production_plan_is_read_only_and_never_claims_readiness() {
    let output = forge()
        .args([
            "milestone",
            "production-plan",
            "--version",
            "0.5",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        json["schema_version"],
        "forge.milestone.production_readiness_plan.v1"
    );
    assert_eq!(json["evaluation_mode"], "plan_only");
    assert_eq!(json["production_ready"], false);
    assert_eq!(json["commands_executed"], 0);
    assert_eq!(json["mutations_performed"], false);
    assert_eq!(json["capability_inventory_count"], 40);
    assert_eq!(
        json["capability_inventory_sha256"].as_str().unwrap().len(),
        64
    );
    assert_eq!(json["required_gate_count"], 11);
    assert_eq!(json["required_receipt_count"], 14);
    assert_eq!(json["requirements"].as_array().unwrap().len(), 11);
}

#[test]
fn production_evaluator_fails_closed_when_evidence_is_absent() {
    let temp = tempdir().unwrap();
    forge()
        .args(["milestone", "production-readiness", "--version", "0.5"])
        .arg("--manifest")
        .arg("missing.json")
        .arg("--evidence-root")
        .arg(temp.path())
        .args(["--output", "json"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("production_ready").not())
        .stderr(predicate::str::contains(
            "failed to inspect production readiness manifest",
        ));
}

#[cfg(unix)]
#[test]
fn production_mission_evidence_writes_exact_typed_artifact_bytes() {
    let (fixture, mission_id, receipt_id) = consumed_mission_lifecycle();
    let evidence_root = fixture.temp.path().join("evidence");
    fs::create_dir(&evidence_root).unwrap();
    let artifact = Path::new("mission-operational-lifecycle.json");

    let output = forge()
        .arg("--store")
        .arg(&fixture.store)
        .args([
            "milestone",
            "production-mission-evidence",
            "--mission",
            &mission_id,
            "--receipt",
            &receipt_id,
            "--evidence-root",
        ])
        .arg(&evidence_root)
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let package: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        package["schema_version"],
        "forge.milestone.mission_lifecycle_evidence_package.v1"
    );
    assert_eq!(package["status"], "ready");
    assert_eq!(
        package["artifact"]["schema_version"],
        "forge.milestone.mission_lifecycle.v1"
    );
    assert_eq!(
        package["artifact"]["subject_version"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(
        package["manifest_section"]["evidence"]["artifact_path"],
        artifact.to_str().unwrap()
    );

    let typed_artifact: ProductionMissionLifecycleReceipt =
        serde_json::from_value(package["artifact"].clone()).unwrap();
    let expected_bytes = serde_json::to_vec(&typed_artifact).unwrap();
    let persisted_bytes = fs::read(evidence_root.join(artifact)).unwrap();
    assert_eq!(persisted_bytes, expected_bytes);
    assert_eq!(
        package["manifest_section"]["evidence"]["artifact_sha256"],
        format!("{:x}", Sha256::digest(&persisted_bytes))
    );
}

#[test]
fn production_mission_evidence_rejects_artifact_path_escape() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let evidence_root = temp.path().join("evidence");
    fs::create_dir(&evidence_root).unwrap();
    let escaped = temp.path().join("escaped.json");

    forge()
        .arg("--store")
        .arg(&store)
        .args([
            "milestone",
            "production-mission-evidence",
            "--mission",
            "mission-missing",
            "--receipt",
            "receipt-missing",
            "--evidence-root",
        ])
        .arg(&evidence_root)
        .args([
            "--artifact",
            "../escaped.json",
            "--release-version",
            "0.5.2",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "mission lifecycle artifact path must be a contained relative path",
        ));
    assert!(!escaped.exists());
}
