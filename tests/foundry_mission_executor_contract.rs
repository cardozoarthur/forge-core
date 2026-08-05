use foundry_core::executor::ExecutorState;
use foundry_core::mission::{
    load_mission, simulate_mission, submit_mission, MissionMode, MissionRecord, MissionStatus,
    MissionSubmission,
};
use foundry_core::mission_executor::{
    build_mission_execution_approval, claim_mission_execution_receipt_for_submission,
    execute_mission_command, inspect_mission_execution_receipt, plan_mission_execution,
    reconcile_mission_execution, release_mission_execution_receipt_submission_claim,
    resolved_mission_execution_metrics, verified_mission_execution_claims,
    MissionExecutionClaimKind, MissionExecutionClaimScope, MissionExecutionMetricStatus,
    MissionExecutionReceiptClaimKind, MissionExecutionReconcileRequest, MissionExecutionRequest,
};
use foundry_core::storage::FoundryStore;
use foundry_core::worktree::{
    approve_worktree_config, bind_worktree, initialize_worktree, register_worktree,
    WorktreeRegisterOptions,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;
use tempfile::{tempdir, TempDir};

struct Fixture {
    _temp: TempDir,
    store: FoundryStore,
    repository: PathBuf,
    mission_id: String,
    workflow_id: String,
    task_id: String,
    agent_id: String,
    worktree_id: String,
    cargo_command: String,
    evidence_command: String,
}

fn git_repository(path: &Path) -> (String, String) {
    fs::create_dir_all(path).unwrap();
    let cargo = path.join("fixture-bin/cargo");
    let evidence = path.join("fixture-bin/git");
    fs::create_dir_all(cargo.parent().unwrap()).unwrap();
    fs::write(
        &cargo,
        "#!/bin/sh\ncase \"${1:-}\" in\n  slow) sleep 2 ;;\n  fail) exit 7 ;;\n  test|--version) printf '%s\\n' 'foundry mission executor fixture' ;;\n  *) exit 2 ;;\nesac\n",
    )
    .unwrap();
    fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(
        &evidence,
        "#!/bin/sh\nif [ \"${1:-}\" = '--fail' ]; then exit 7; fi\nprintf '%s%s\\n' 'FOUNDRY_GATE_EVIDENCE:' \"${1:?missing evidence envelope}\"\n",
    )
    .unwrap();
    fs::set_permissions(&evidence, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(Command::new("git")
        .args(["init", "--initial-branch=main"])
        .arg(path)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args(["-C", path.to_str().unwrap(), "add", "fixture-bin"])
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args([
            "-C",
            path.to_str().unwrap(),
            "-c",
            "user.name=Foundry Mission Test",
            "-c",
            "user.email=foundry-mission@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "initial",
        ])
        .status()
        .unwrap()
        .success());
    (
        cargo.to_str().unwrap().to_string(),
        evidence.to_str().unwrap().to_string(),
    )
}

fn fixture_with_isolation(configured: bool, isolated: bool) -> Fixture {
    let temp = tempdir().unwrap();
    let repository = temp.path().join("repository");
    let (cargo_command, evidence_command) = git_repository(&repository);
    let store = FoundryStore::open(temp.path().join("foundry.sqlite")).unwrap();
    let report = simulate_mission(
        &store,
        "Execute one bounded mission command",
        "software-factory",
        None,
        false,
    )
    .unwrap();
    let mut mission = report.mission;
    let worktree_id = "wt-mission-executor".to_string();
    register_worktree(
        &store,
        WorktreeRegisterOptions {
            path: repository.clone(),
            id: Some(worktree_id.clone()),
            workflow_id: None,
            task_id: None,
            origin: "mission-executor-test".to_string(),
            created_by_foundry: false,
        },
    )
    .unwrap();
    if configured {
        initialize_worktree(&store, &worktree_id, true, false, "mission-executor-test").unwrap();
        let config_path = repository.join(".foundry/worktree.toml");
        let mut config: toml::Value =
            toml::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
        config["guardrails"]["allowed_commands"] = toml::Value::Array(vec![
            toml::Value::String(cargo_command.clone()),
            toml::Value::String(evidence_command.clone()),
        ]);
        config["guardrails"]["max_command_seconds"] = toml::Value::Integer(5);
        if isolated {
            config["sandbox"]["runtime"] = toml::Value::String("bubblewrap".to_string());
            config["sandbox"]["network"] = toml::Value::String("deny".to_string());
        }
        fs::write(&config_path, toml::to_string_pretty(&config).unwrap()).unwrap();
        approve_worktree_config(
            &store,
            &worktree_id,
            true,
            "mission-executor-test",
            "mission-executor-test",
        )
        .unwrap();
    }
    bind_worktree(
        &store,
        &worktree_id,
        &mission.workflow_id,
        None,
        "mission-executor-test",
    )
    .unwrap();
    let executor = ExecutorState {
        id: "codex".to_string(),
        display_name: "Codex CLI".to_string(),
        command: "codex".to_string(),
        installed: true,
        configured: true,
        command_path: Some("/usr/bin/true".to_string()),
        config_evidence: vec!["test fixture".to_string()],
        non_interactive_ready: true,
        probe_evidence: vec!["test fixture".to_string()],
        foundry_first_ready: false,
        foundry_first_entrypoint: None,
        harness_status: None,
        allowed: true,
        decision_source: "human_allow".to_string(),
        synced_at: "2026-07-24T00:00:00Z".to_string(),
    };
    store
        .save_executor_state("codex", &serde_json::to_value(executor).unwrap())
        .unwrap();

    mission.status = MissionStatus::Running;
    mission.worktree = Some(worktree_id.clone());
    let orchestrator_id = mission.orchestrator_instance_id.clone();
    let orchestrator = mission
        .agents
        .iter_mut()
        .find(|agent| agent.instance_id == orchestrator_id)
        .unwrap();
    orchestrator.status = "running".to_string();
    let task = mission
        .tasks
        .iter_mut()
        .find(|task| task.id == "mission-task-002")
        .unwrap();
    task.status = "running".to_string();
    let agent = mission
        .agents
        .iter_mut()
        .find(|agent| agent.definition_id == "rust-builder")
        .unwrap();
    agent.status = "running".to_string();
    let agent_id = agent.instance_id.clone();
    task.assigned_agent_id = Some(agent_id.clone());
    Connection::open(store.path())
        .unwrap()
        .execute(
            // foundry-brand-allow: legacy-compat
            "UPDATE forge_missions SET status = 'running', data_json = ?1 WHERE id = ?2",
            params![serde_json::to_string(&mission).unwrap(), mission.id],
        )
        .unwrap();
    Fixture {
        _temp: temp,
        store,
        repository,
        mission_id: mission.id,
        workflow_id: mission.workflow_id,
        task_id: "mission-task-002".to_string(),
        agent_id,
        worktree_id,
        cargo_command,
        evidence_command,
    }
}

fn fixture(configured: bool) -> Fixture {
    fixture_with_isolation(configured, false)
}

fn request(fixture: &Fixture, key: &str) -> MissionExecutionRequest {
    let mission = load_mission(&fixture.store, &fixture.mission_id).unwrap();
    MissionExecutionRequest {
        idempotency_key: key.to_string(),
        mission_id: fixture.mission_id.clone(),
        workflow_id: fixture.workflow_id.clone(),
        expected_mission_revision: mission.revision,
        task_id: fixture.task_id.clone(),
        agent_id: fixture.agent_id.clone(),
        executor_id: "codex".to_string(),
        worktree: Some(fixture.worktree_id.clone()),
        purpose: "test".to_string(),
        command: vec![fixture.cargo_command.clone(), "--version".to_string()],
        requested_evidence: Vec::new(),
        approval: None,
        dry_run: false,
        allow_trusted_process_runtime: true,
    }
}

fn approve_request(fixture: &Fixture, request: &mut MissionExecutionRequest) {
    let plan = plan_mission_execution(&fixture.store, request).unwrap();
    request.approval =
        Some(build_mission_execution_approval(&plan, "mission-executor-test", 300).unwrap());
}

fn approved_request(fixture: &Fixture, key: &str) -> MissionExecutionRequest {
    let mut request = request(fixture, key);
    approve_request(fixture, &mut request);
    request
}

fn gate_evidence_envelope(evidence: serde_json::Value) -> String {
    serde_json::json!({
        "schema_version": "foundry.mission.gate_evidence_observation.v1",
        "evidence": evidence,
    })
    .to_string()
}

fn sha256_json(value: &impl Serialize) -> String {
    let bytes = serde_json::to_vec(value).unwrap();
    format!("{:x}", Sha256::digest(bytes))
}

fn rehash_receipt(receipt: &mut foundry_core::mission_executor::MissionExecutionReceipt) {
    receipt.receipt_sha256.clear();
    receipt.receipt_sha256 = sha256_json(receipt);
}

fn hard_link_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type().unwrap();
        if file_type.is_dir() {
            hard_link_tree(&source_path, &destination_path);
        } else if file_type.is_symlink() {
            symlink(fs::read_link(&source_path).unwrap(), &destination_path).unwrap();
        } else {
            fs::hard_link(&source_path, &destination_path).unwrap();
        }
    }
}

fn install_real_host_toolchain(fixture: &Fixture) -> String {
    let cargo = Command::new("rustup")
        .args(["which", "cargo"])
        .output()
        .unwrap();
    assert!(cargo.status.success());
    let cargo = PathBuf::from(String::from_utf8(cargo.stdout).unwrap().trim());
    let rustc = Command::new("rustup")
        .args(["which", "rustc"])
        .output()
        .unwrap();
    assert!(rustc.status.success());
    let rustc = PathBuf::from(String::from_utf8(rustc.stdout).unwrap().trim());
    let toolchain_root = cargo.parent().unwrap().parent().unwrap();
    assert_eq!(rustc.parent().unwrap().parent().unwrap(), toolchain_root);

    let guest_toolchain = fixture.repository.join("host-toolchain");
    let guest_bin = guest_toolchain.join("bin");
    fs::create_dir_all(&guest_bin).unwrap();
    fs::hard_link(&cargo, guest_bin.join("cargo")).unwrap();
    fs::hard_link(&rustc, guest_bin.join("rustc")).unwrap();
    hard_link_tree(&toolchain_root.join("lib"), &guest_toolchain.join("lib"));

    fs::create_dir_all(fixture.repository.join("src")).unwrap();
    fs::create_dir_all(fixture.repository.join(".cargo")).unwrap();
    fs::write(
        fixture.repository.join("Cargo.toml"),
        "[package]\nname = \"foundry-toolchain-smoke\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\ndoctest = false\n",
    )
    .unwrap();
    fs::write(
        fixture.repository.join("Cargo.lock"),
        "# This file is automatically @generated by Cargo.\n# It is not intended for manual editing.\nversion = 4\n\n[[package]]\nname = \"foundry-toolchain-smoke\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::write(
        fixture.repository.join("src/lib.rs"),
        "#[cfg(test)]\nmod tests {\n    #[test]\n    fn authorized_toolchain_executes_tests() {\n        assert_eq!(2 + 2, 4);\n    }\n}\n",
    )
    .unwrap();
    fs::write(
        fixture.repository.join(".cargo/config.toml"),
        "[build]\nrustc = \"/workspace/host-toolchain/bin/rustc\"\ntarget-dir = \"/tmp/foundry-target\"\nrustflags = [\"-C\", \"linker=/usr/bin/cc\"]\n",
    )
    .unwrap();

    let cargo_command = guest_bin.join("cargo").to_str().unwrap().to_string();
    let config_path = fixture.repository.join(".foundry/worktree.toml");
    let mut config: toml::Value =
        toml::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config["guardrails"]["allowed_commands"]
        .as_array_mut()
        .unwrap()
        .push(toml::Value::String(cargo_command.clone()));
    config["guardrails"]["max_command_seconds"] = toml::Value::Integer(30);
    fs::write(&config_path, toml::to_string_pretty(&config).unwrap()).unwrap();
    approve_worktree_config(
        &fixture.store,
        &fixture.worktree_id,
        true,
        "mission-executor-test",
        "authorize real host Rust toolchain for isolated smoke",
    )
    .unwrap();
    bind_worktree(
        &fixture.store,
        &fixture.worktree_id,
        &fixture.workflow_id,
        None,
        "mission-executor-test",
    )
    .unwrap();
    cargo_command
}

fn persist_mission(store: &FoundryStore, mission: &MissionRecord) {
    Connection::open(store.path())
        .unwrap()
        .execute(
            // foundry-brand-allow: legacy-compat
            "UPDATE forge_missions SET data_json = ?1 WHERE id = ?2",
            params![serde_json::to_string(mission).unwrap(), mission.id],
        )
        .unwrap();
}

fn select_operational_task(fixture: &mut Fixture, task_id: &str, definition_id: &str) {
    let mut mission = load_mission(&fixture.store, &fixture.mission_id).unwrap();
    mission.mode = MissionMode::Workflow;
    let agent = mission
        .agents
        .iter_mut()
        .find(|agent| agent.definition_id == definition_id)
        .unwrap();
    agent.status = "running".to_string();
    let agent_id = agent.instance_id.clone();
    let task = mission
        .tasks
        .iter_mut()
        .find(|task| task.id == task_id)
        .unwrap();
    task.status = "running".to_string();
    task.assigned_agent_id = Some(agent_id.clone());
    persist_mission(&fixture.store, &mission);
    fixture.task_id = task_id.to_string();
    fixture.agent_id = agent_id;
}

#[test]
fn dry_run_and_missing_worktree_fail_closed() {
    let fixture = fixture(true);
    let mut dry_run = request(&fixture, "dry-run");
    dry_run.dry_run = true;
    let report = execute_mission_command(&fixture.store, dry_run).unwrap();
    assert_eq!(report.status, "planned");
    assert!(!report.persisted);
    assert_eq!(report.receipt.status, "planned");
    assert!(!report.receipt.execution_attempted);
    let dry_run_count: i64 = Connection::open(fixture.store.path())
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM mission_execution_receipts",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(dry_run_count, 0);

    let mut blocked_dry_run = request(&fixture, "blocked-dry-run");
    blocked_dry_run.worktree = None;
    blocked_dry_run.dry_run = true;
    let blocked = execute_mission_command(&fixture.store, blocked_dry_run).unwrap();
    assert_eq!(blocked.status, "blocked");
    assert_eq!(blocked.receipt.status, "blocked");
    assert!(!blocked.persisted);

    let unapproved = request(&fixture, "unapproved");
    let error = execute_mission_command(&fixture.store, unapproved).unwrap_err();
    assert!(format!("{error:#}").contains("requires explicit approval"));

    let mut missing = request(&fixture, "missing-worktree");
    missing.worktree = None;
    approve_request(&fixture, &mut missing);
    let receipt = execute_mission_command(&fixture.store, missing)
        .unwrap()
        .receipt;
    assert_eq!(receipt.status, "blocked_by_policy");
    assert!(!receipt.executed);
}

#[test]
fn absent_config_blocks_even_with_approval() {
    let fixture = fixture(false);
    let receipt = execute_mission_command(&fixture.store, approved_request(&fixture, "no-config"))
        .unwrap()
        .receipt;
    assert_eq!(receipt.status, "blocked_by_policy");
    assert!(!receipt.execution_attempted);
    assert!(receipt
        .policy_trace
        .iter()
        .any(|decision| decision.check == "worktree_guardrails" && !decision.allowed));
}

#[test]
fn wrong_agent_and_missing_harness_never_execute() {
    let fixture = fixture(true);
    let mut mission = load_mission(&fixture.store, &fixture.mission_id).unwrap();
    let other = mission
        .agents
        .iter_mut()
        .find(|agent| agent.instance_id != fixture.agent_id)
        .unwrap();
    other.status = "running".to_string();
    let other_id = other.instance_id.clone();
    persist_mission(&fixture.store, &mission);

    let mut wrong_agent = request(&fixture, "wrong-agent");
    wrong_agent.agent_id = other_id;
    approve_request(&fixture, &mut wrong_agent);
    let receipt = execute_mission_command(&fixture.store, wrong_agent)
        .unwrap()
        .receipt;
    assert!(!receipt.execution_attempted);
    assert!(receipt
        .policy_trace
        .iter()
        .any(|decision| decision.check == "task_assignment" && !decision.allowed));

    let mut mission = load_mission(&fixture.store, &fixture.mission_id).unwrap();
    mission
        .harnesses
        .retain(|harness| harness.task_id != fixture.task_id || harness.agent_id != "rust-builder");
    persist_mission(&fixture.store, &mission);
    let receipt = execute_mission_command(
        &fixture.store,
        approved_request(&fixture, "missing-harness"),
    )
    .unwrap()
    .receipt;
    assert!(!receipt.execution_attempted);
    assert!(receipt
        .policy_trace
        .iter()
        .any(|decision| decision.check == "harness_resolution" && !decision.allowed));
}

#[test]
fn approved_short_execution_is_bounded_and_idempotent() {
    let fixture = fixture(true);
    let first =
        execute_mission_command(&fixture.store, approved_request(&fixture, "approved")).unwrap();
    assert!(!first.replayed);
    assert_eq!(first.receipt.status, "completed");
    assert_eq!(first.receipt.exit_code, Some(0));
    assert_eq!(first.receipt.tests_passed, 0);
    assert!(first.receipt.validations.is_empty());
    assert!(first
        .receipt
        .claims
        .iter()
        .any(|claim| claim.kind == MissionExecutionClaimKind::ExecutionCompleted));
    assert!(!first
        .receipt
        .claims
        .iter()
        .any(|claim| claim.kind == MissionExecutionClaimKind::TestsPassed));
    assert!(first
        .receipt
        .claims
        .iter()
        .all(|claim| claim.kind != MissionExecutionReceiptClaimKind::GateEvidence));
    assert!(first.receipt.requested_evidence.is_empty());
    let verified = verified_mission_execution_claims(
        &first.receipt,
        &fixture.mission_id,
        &fixture.workflow_id,
        load_mission(&fixture.store, &fixture.mission_id)
            .unwrap()
            .revision,
        &fixture.task_id,
        &fixture.agent_id,
    )
    .unwrap();
    assert_eq!(
        verified.claims,
        vec![MissionExecutionClaimKind::ExecutionCompleted]
    );
    let sandbox = first.receipt.sandbox.as_ref().unwrap();
    assert!(sandbox.stdout_bytes > 0);
    assert!(!sandbox.filesystem_isolation_enforced);

    let second =
        execute_mission_command(&fixture.store, approved_request(&fixture, "approved")).unwrap();
    assert!(second.replayed);
    assert_eq!(second.receipt.receipt_id, first.receipt.receipt_id);
    let event_count: i64 = Connection::open(fixture.store.path())
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM events WHERE workflow_id = ?1 AND kind = 'mission.execution.receipt'",
            [&fixture.workflow_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(event_count, 1);
}

fn submission(fixture: &Fixture, key: &str, receipt_id: &str) -> MissionSubmission {
    MissionSubmission {
        idempotency_key: key.to_string(),
        execution_receipt_id: receipt_id.to_string(),
        task_id: fixture.task_id.clone(),
        agent_id: fixture.agent_id.clone(),
        status: "completed".to_string(),
        summary: "receipt metrics are ready for accounting".to_string(),
        artifacts: Vec::new(),
        validations: Vec::new(),
        risks: Vec::new(),
        followups: Vec::new(),
        tests_passed: 0,
        tests_failed: 0,
    }
}

#[test]
fn trusted_process_unknown_metrics_block_finite_budget_submission() {
    let fixture = fixture(true);
    let executed = execute_mission_command(
        &fixture.store,
        approved_request(&fixture, "unknown-metrics"),
    )
    .unwrap();
    let metrics = resolved_mission_execution_metrics(&executed.receipt).unwrap();
    assert_eq!(
        metrics.cost_usd.status,
        MissionExecutionMetricStatus::Unknown
    );
    assert_eq!(
        metrics.files_changed.status,
        MissionExecutionMetricStatus::Unknown
    );
    assert_eq!(
        metrics.external_calls.status,
        MissionExecutionMetricStatus::Unknown
    );

    let error = submit_mission(
        &fixture.store,
        &fixture.mission_id,
        submission(
            &fixture,
            "unknown-metrics-submit",
            &executed.receipt.receipt_id,
        ),
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("execution cost is unknown"));
    let consumed_by: Option<String> = Connection::open(fixture.store.path())
        .unwrap()
        .query_row(
            "SELECT consumed_by_submission FROM mission_execution_receipts WHERE receipt_id=?1",
            [&executed.receipt.receipt_id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(consumed_by.is_none());
}

#[test]
fn isolated_deterministic_execution_has_observed_zero_metrics_and_submits() {
    let fixture = fixture_with_isolation(true, true);
    let executed = execute_mission_command(
        &fixture.store,
        approved_request(&fixture, "observed-metrics"),
    )
    .unwrap();
    assert_eq!(
        executed.receipt.status, "completed",
        "{:#?}",
        executed.receipt.sandbox
    );
    let sandbox = executed.receipt.sandbox.as_ref().unwrap();
    assert!(sandbox.worktree_read_only);
    assert!(sandbox.writes_restricted_to_sandbox);
    assert!(sandbox.network_isolation_enforced);
    let metrics = resolved_mission_execution_metrics(&executed.receipt).unwrap();
    assert_eq!(metrics.cost_usd.value, Some(0.0));
    assert_eq!(metrics.files_changed.value, Some(0));
    assert_eq!(metrics.external_calls.value, Some(0));

    let report = submit_mission(
        &fixture.store,
        &fixture.mission_id,
        submission(
            &fixture,
            "observed-metrics-submit",
            &executed.receipt.receipt_id,
        ),
    )
    .unwrap();
    assert_eq!(report.status, "queued");
}

#[test]
fn explicit_gate_evidence_is_allowlisted_scoped_and_approval_bound() {
    let mut fixture = fixture(true);
    select_operational_task(&mut fixture, "mission-task-001", "repository-scout");

    let mut generic = request(&fixture, "evidence-scope");
    generic.command = vec![
        fixture.evidence_command.clone(),
        gate_evidence_envelope(serde_json::json!({
            "requirements_summary": "Repository requirements were inspected and summarized."
        })),
    ];
    let generic_plan = plan_mission_execution(&fixture.store, &generic).unwrap();
    let mut explicit = generic.clone();
    explicit.requested_evidence = vec!["requirements_summary".to_string()];
    let explicit_plan = plan_mission_execution(&fixture.store, &explicit).unwrap();
    assert!(
        explicit_plan.allowed,
        "unexpected policy denial: {:#?}",
        explicit_plan.policy_trace
    );
    assert_ne!(generic_plan.request_sha256, explicit_plan.request_sha256);
    assert_ne!(
        generic_plan.approval_scope_sha256,
        explicit_plan.approval_scope_sha256
    );
    assert_eq!(
        explicit_plan.gate_evidence_contract[0].gate_ids,
        vec!["requirements_ready"]
    );

    explicit.approval = Some(
        build_mission_execution_approval(&explicit_plan, "mission-executor-test", 300).unwrap(),
    );
    let result = execute_mission_command(&fixture.store, explicit).unwrap();
    let gate_claim = result
        .receipt
        .claims
        .iter()
        .find(|claim| claim.kind == MissionExecutionReceiptClaimKind::GateEvidence)
        .unwrap();
    assert_eq!(
        gate_claim.evidence_kind.as_deref(),
        Some("requirements_summary")
    );
    assert_eq!(gate_claim.gate_ids, vec!["requirements_ready"]);
    assert_eq!(gate_claim.scope, MissionExecutionClaimScope::Operational);
    assert_eq!(gate_claim.command_sha256, result.receipt.command_sha256);
    let gate_evidence = result
        .receipt
        .evidence
        .iter()
        .find(|evidence| evidence.kind == "gate_evidence:requirements_summary")
        .unwrap();
    assert_eq!(gate_claim.locator, gate_evidence.locator);
    assert_eq!(gate_claim.sha256, gate_evidence.sha256);
    assert!(gate_evidence.observation.is_some());
    assert_ne!(
        gate_claim.locator,
        format!("mission-execution://{}/sandbox", result.receipt.receipt_id)
    );

    let verified = verified_mission_execution_claims(
        &result.receipt,
        &fixture.mission_id,
        &fixture.workflow_id,
        result.receipt.mission_revision,
        &fixture.task_id,
        &fixture.agent_id,
    )
    .unwrap();
    assert_eq!(verified.gate_evidence.len(), 1);
    assert_eq!(
        verified.gate_evidence[0].evidence_kind,
        "requirements_summary"
    );

    let mut tampered = result.receipt;
    let evidence = tampered
        .evidence
        .iter_mut()
        .find(|evidence| evidence.kind == "gate_evidence:requirements_summary")
        .unwrap();
    evidence.observation.as_mut().unwrap()["value"] = serde_json::json!("");
    evidence.bytes = serde_json::to_vec(evidence.observation.as_ref().unwrap())
        .unwrap()
        .len();
    evidence.sha256 = sha256_json(evidence.observation.as_ref().unwrap());
    let claim = tampered
        .claims
        .iter_mut()
        .find(|claim| claim.kind == MissionExecutionReceiptClaimKind::GateEvidence)
        .unwrap();
    claim.sha256 = evidence.sha256.clone();
    rehash_receipt(&mut tampered);
    let error = verified_mission_execution_claims(
        &tampered,
        &fixture.mission_id,
        &fixture.workflow_id,
        tampered.mission_revision,
        &fixture.task_id,
        &fixture.agent_id,
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("valid semantic observation"));
}

#[test]
fn cargo_metadata_and_fake_cargo_cannot_prove_gate_evidence() {
    let mut fixture = fixture(true);
    select_operational_task(&mut fixture, "mission-task-001", "repository-scout");
    let mut request = request(&fixture, "fake-cargo-metadata");
    request.requested_evidence = vec!["requirements_summary".to_string()];
    let plan = plan_mission_execution(&fixture.store, &request).unwrap();
    assert!(!plan.allowed);
    assert!(plan
        .policy_trace
        .iter()
        .any(|decision| { decision.check == "semantic_evidence_command" && !decision.allowed }));
    request.approval =
        Some(build_mission_execution_approval(&plan, "mission-executor-test", 300).unwrap());
    let result = execute_mission_command(&fixture.store, request).unwrap();
    assert_eq!(result.receipt.status, "blocked_by_policy");
    assert!(result
        .receipt
        .claims
        .iter()
        .all(|claim| claim.kind != MissionExecutionReceiptClaimKind::GateEvidence));
    assert!(result
        .receipt
        .evidence
        .iter()
        .all(|evidence| !evidence.kind.starts_with("gate_evidence:")));
}

#[test]
fn invalid_or_simulated_gate_evidence_is_rejected_before_execution() {
    let mut operational_fixture = fixture(true);
    select_operational_task(&mut operational_fixture, "mission-task-002", "rust-builder");
    for (requested, expected) in [
        (vec![""], "cannot be empty"),
        (vec!["review_passed", "review_passed"], "is duplicated"),
        (vec!["tests_passed"], "reserved"),
        (vec!["unknown_evidence"], "is not allowed"),
    ] {
        let mut invalid = request(&operational_fixture, "invalid-evidence");
        invalid.requested_evidence = requested.into_iter().map(str::to_string).collect();
        let error = execute_mission_command(&operational_fixture.store, invalid).unwrap_err();
        assert!(
            format!("{error:#}").contains(expected),
            "unexpected error: {error:#}"
        );
    }
    let receipt_count: i64 = Connection::open(operational_fixture.store.path())
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM mission_execution_receipts",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(receipt_count, 0);

    let simulated_fixture = fixture(true);
    let mut simulated = request(&simulated_fixture, "simulated-evidence");
    simulated.requested_evidence = vec!["review_passed".to_string()];
    let error = plan_mission_execution(&simulated_fixture.store, &simulated).unwrap_err();
    assert!(format!("{error:#}").contains("bounded mission simulation"));
}

#[test]
fn failed_execution_never_emits_requested_gate_evidence() {
    let mut fixture = fixture(true);
    select_operational_task(&mut fixture, "mission-task-001", "repository-scout");
    let mut failed = request(&fixture, "failed-gate-evidence");
    failed.requested_evidence = vec!["requirements_summary".to_string()];
    failed.command = vec![fixture.evidence_command.clone(), "--fail".to_string()];
    let plan = plan_mission_execution(&fixture.store, &failed).unwrap();
    assert!(
        plan.allowed,
        "unexpected policy denial: {:#?}",
        plan.policy_trace
    );
    approve_request(&fixture, &mut failed);
    let result = execute_mission_command(&fixture.store, failed).unwrap();
    assert_eq!(result.receipt.status, "failed");
    assert_eq!(
        result.receipt.requested_evidence,
        vec!["requirements_summary"]
    );
    assert!(result
        .receipt
        .claims
        .iter()
        .all(|claim| claim.kind != MissionExecutionReceiptClaimKind::GateEvidence));
}

#[test]
fn final_task_independent_evidence_enforces_reviewer_anti_affinity() {
    let mut fixture = fixture(true);
    select_operational_task(&mut fixture, "mission-task-002", "rust-builder");
    let mut builder_request = request(&fixture, "builder-review-evidence");
    builder_request.requested_evidence = vec!["review_passed".to_string()];
    let builder_blocked = plan_mission_execution(&fixture.store, &builder_request).unwrap();
    assert!(!builder_blocked.allowed);
    assert!(builder_blocked
        .policy_trace
        .iter()
        .any(|decision| { decision.check == "reviewer_anti_affinity" && !decision.allowed }));

    select_operational_task(&mut fixture, "mission-task-003", "independent-reviewer");
    let mut resumed = load_mission(&fixture.store, &fixture.mission_id).unwrap();
    resumed.tasks[1].assigned_agent_id = None;
    assert!(resumed
        .handoffs
        .iter()
        .any(|handoff| { handoff.task_id == "mission-task-002" && handoff.status == "accepted" }));
    persist_mission(&fixture.store, &resumed);
    let mut request = request(&fixture, "independent-evidence");
    request.requested_evidence = vec!["review_passed".to_string()];
    let allowed = plan_mission_execution(&fixture.store, &request).unwrap();
    assert!(allowed
        .policy_trace
        .iter()
        .any(|decision| { decision.check == "reviewer_anti_affinity" && decision.allowed }));
    assert_eq!(
        allowed.gate_evidence_contract[0].gate_ids,
        vec!["implementation_validated"]
    );

    let mut mission = load_mission(&fixture.store, &fixture.mission_id).unwrap();
    mission
        .handoffs
        .iter_mut()
        .rev()
        .find(|handoff| handoff.task_id == "mission-task-002" && handoff.status == "accepted")
        .unwrap()
        .from_agent = fixture.agent_id.clone();
    persist_mission(&fixture.store, &mission);
    let blocked = plan_mission_execution(&fixture.store, &request).unwrap();
    assert!(!blocked.allowed);
    assert!(blocked
        .policy_trace
        .iter()
        .any(|decision| { decision.check == "reviewer_anti_affinity" && !decision.allowed }));
}

#[test]
fn compile_only_cargo_test_never_produces_a_tests_passed_claim() {
    let fixture = fixture(true);
    let mut compile_only = request(&fixture, "cargo-test-no-run");
    compile_only.command = vec![
        fixture.cargo_command.clone(),
        "test".to_string(),
        "--no-run".to_string(),
    ];
    approve_request(&fixture, &mut compile_only);
    let receipt = execute_mission_command(&fixture.store, compile_only)
        .unwrap()
        .receipt;
    assert_eq!(receipt.status, "completed");
    assert_eq!(receipt.tests_passed, 0);
    assert!(receipt.validations.is_empty());
    assert!(receipt
        .claims
        .iter()
        .all(|claim| claim.kind != MissionExecutionClaimKind::TestsPassed));

    let mut tampered = receipt.clone();
    tampered.validations.push("tests_passed".to_string());
    let error = verified_mission_execution_claims(
        &tampered,
        &fixture.mission_id,
        &fixture.workflow_id,
        tampered.mission_revision,
        &fixture.task_id,
        &fixture.agent_id,
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("hash mismatch"));
}

#[test]
fn authorized_host_cargo_runs_tests_inside_network_denied_bubblewrap() {
    let fixture = fixture_with_isolation(true, true);
    let cargo = install_real_host_toolchain(&fixture);
    let mut request = request(&fixture, "real-cargo-test");
    request.command = vec![cargo, "test".to_string(), "--offline".to_string()];
    request.allow_trusted_process_runtime = false;
    let plan = plan_mission_execution(&fixture.store, &request).unwrap();
    assert!(
        plan.allowed,
        "unexpected policy denial: {:#?}; sandbox: {:#?}",
        plan.policy_trace, plan.sandbox_plan
    );
    approve_request(&fixture, &mut request);
    let receipt = execute_mission_command(&fixture.store, request)
        .unwrap()
        .receipt;
    assert_eq!(receipt.status, "completed");
    assert_eq!(receipt.tests_passed, 1);
    let sandbox = receipt.sandbox.as_ref().unwrap();
    assert_eq!(sandbox.runtime, "bubblewrap");
    assert!(sandbox.filesystem_isolation_enforced);
    assert!(sandbox.worktree_read_only);
    assert!(sandbox.writes_restricted_to_sandbox);
    assert!(sandbox.network_isolation_enforced);
    let evidence = receipt
        .evidence
        .iter()
        .find(|evidence| evidence.kind == "tests_passed")
        .unwrap();
    assert!(evidence.observation.is_some());
    let claim = receipt
        .claims
        .iter()
        .find(|claim| claim.kind == MissionExecutionReceiptClaimKind::TestsPassed)
        .unwrap();
    assert_eq!(claim.locator, evidence.locator);
    assert_eq!(claim.sha256, evidence.sha256);
    verified_mission_execution_claims(
        &receipt,
        &fixture.mission_id,
        &fixture.workflow_id,
        receipt.mission_revision,
        &fixture.task_id,
        &fixture.agent_id,
    )
    .unwrap();
}

#[test]
fn zero_test_filter_cannot_prove_tests_or_review() {
    let mut fixture = fixture_with_isolation(true, true);
    let cargo = install_real_host_toolchain(&fixture);
    select_operational_task(&mut fixture, "mission-task-003", "independent-reviewer");
    let mut mission = load_mission(&fixture.store, &fixture.mission_id).unwrap();
    mission.tasks[1].assigned_agent_id = None;
    persist_mission(&fixture.store, &mission);

    let mut request = request(&fixture, "real-cargo-zero-tests");
    request.command = vec![
        cargo,
        "test".to_string(),
        "definitely_no_test_matches_this_filter".to_string(),
        "--offline".to_string(),
    ];
    request.requested_evidence = vec!["review_passed".to_string()];
    request.allow_trusted_process_runtime = false;
    let plan = plan_mission_execution(&fixture.store, &request).unwrap();
    assert!(
        plan.allowed,
        "unexpected policy denial: {:#?}",
        plan.policy_trace
    );
    assert_eq!(plan.gate_evidence_contract.len(), 1);
    request.approval =
        Some(build_mission_execution_approval(&plan, "mission-executor-test", 300).unwrap());
    let receipt = execute_mission_command(&fixture.store, request)
        .unwrap()
        .receipt;
    assert_eq!(receipt.status, "completed");
    assert_eq!(receipt.tests_passed, 0);
    assert!(receipt.claims.iter().all(|claim| !matches!(
        claim.kind,
        MissionExecutionReceiptClaimKind::TestsPassed
            | MissionExecutionReceiptClaimKind::GateEvidence
    )));
    assert!(receipt.evidence.iter().all(|evidence| {
        evidence.kind != "tests_passed" && !evidence.kind.starts_with("gate_evidence:")
    }));
    verified_mission_execution_claims(
        &receipt,
        &fixture.mission_id,
        &fixture.workflow_id,
        receipt.mission_revision,
        &fixture.task_id,
        &fixture.agent_id,
    )
    .unwrap();
}

#[test]
fn zero_test_filter_cannot_prove_requirements() {
    let mut fixture = fixture_with_isolation(true, true);
    let cargo = install_real_host_toolchain(&fixture);
    select_operational_task(&mut fixture, "mission-task-001", "repository-scout");
    let mut request = request(&fixture, "real-cargo-zero-tests-requirements");
    request.command = vec![
        cargo,
        "test".to_string(),
        "definitely_no_test_matches_requirements".to_string(),
        "--offline".to_string(),
    ];
    request.requested_evidence = vec!["requirements_summary".to_string()];
    request.allow_trusted_process_runtime = false;
    let plan = plan_mission_execution(&fixture.store, &request).unwrap();
    assert!(
        plan.allowed,
        "unexpected policy denial: {:#?}",
        plan.policy_trace
    );
    request.approval =
        Some(build_mission_execution_approval(&plan, "mission-executor-test", 300).unwrap());
    let receipt = execute_mission_command(&fixture.store, request)
        .unwrap()
        .receipt;
    assert_eq!(receipt.status, "completed");
    assert_eq!(receipt.tests_passed, 0);
    assert!(receipt.claims.iter().all(|claim| !matches!(
        claim.kind,
        MissionExecutionReceiptClaimKind::TestsPassed
            | MissionExecutionReceiptClaimKind::GateEvidence
    )));
    assert!(receipt.evidence.iter().all(|evidence| {
        evidence.kind != "tests_passed" && !evidence.kind.starts_with("gate_evidence:")
    }));
}

#[test]
fn failed_execution_blocks_assignment_until_explicit_no_effect_reconciliation() {
    let fixture = fixture(true);
    let mut failed = request(&fixture, "failed-attempt");
    failed.command = vec![fixture.cargo_command.clone(), "fail".to_string()];
    approve_request(&fixture, &mut failed);
    let failed = execute_mission_command(&fixture.store, failed).unwrap();
    assert_eq!(failed.receipt.status, "failed");
    assert_eq!(failed.receipt.exit_code, Some(7));
    assert!(failed.persisted);

    let protected =
        execute_mission_command(&fixture.store, approved_request(&fixture, "retry-blocked"))
            .unwrap_err();
    assert!(format!("{protected:#}").contains("already has protected execution"));

    let rejected = reconcile_mission_execution(
        &fixture.store,
        MissionExecutionReconcileRequest {
            receipt_id: failed.receipt.receipt_id.clone(),
            outcome: "unknown".to_string(),
            approved_by: "mission-executor-test".to_string(),
            reason: "command exited before making a change".to_string(),
            confirm_no_effect_retry: true,
        },
    )
    .unwrap_err();
    assert!(format!("{rejected:#}").contains("only `no_effect_retry` is safe"));

    let still_protected =
        execute_mission_command(&fixture.store, approved_request(&fixture, "still-blocked"))
            .unwrap_err();
    assert!(format!("{still_protected:#}").contains("already has protected execution"));

    let unconfirmed = reconcile_mission_execution(
        &fixture.store,
        MissionExecutionReconcileRequest {
            receipt_id: failed.receipt.receipt_id.clone(),
            outcome: "no_effect_retry".to_string(),
            approved_by: "mission-executor-test".to_string(),
            reason: "command exited before making a change".to_string(),
            confirm_no_effect_retry: false,
        },
    )
    .unwrap_err();
    assert!(format!("{unconfirmed:#}").contains("explicit no-effect retry confirmation"));

    let reconciled = reconcile_mission_execution(
        &fixture.store,
        MissionExecutionReconcileRequest {
            receipt_id: failed.receipt.receipt_id.clone(),
            outcome: "no_effect_retry".to_string(),
            approved_by: "mission-executor-test".to_string(),
            reason: "command exited before making a change".to_string(),
            confirm_no_effect_retry: true,
        },
    )
    .unwrap();
    assert!(!reconciled.replayed);
    assert_eq!(reconciled.status, "reconciled_no_effect_retry");
    let replayed = reconcile_mission_execution(
        &fixture.store,
        MissionExecutionReconcileRequest {
            receipt_id: failed.receipt.receipt_id.clone(),
            outcome: "no_effect_retry".to_string(),
            approved_by: "mission-executor-test".to_string(),
            reason: "command exited before making a change".to_string(),
            confirm_no_effect_retry: true,
        },
    )
    .unwrap();
    assert!(replayed.replayed);
    assert_eq!(
        replayed.reconciliation.reconciliation_id,
        reconciled.reconciliation.reconciliation_id
    );
    let inspected =
        inspect_mission_execution_receipt(&fixture.store, &failed.receipt.receipt_id).unwrap();
    assert_eq!(inspected.state, "reconciled_no_effect_retry");
    assert_eq!(
        inspected.reconciliation.unwrap().reconciliation_sha256,
        reconciled.reconciliation.reconciliation_sha256
    );

    let completed =
        execute_mission_command(&fixture.store, approved_request(&fixture, "retry-success"))
            .unwrap();
    assert_eq!(completed.receipt.status, "completed");

    let error =
        execute_mission_command(&fixture.store, approved_request(&fixture, "second-success"))
            .unwrap_err();
    assert!(format!("{error:#}").contains("already has protected execution"));
}

#[test]
fn expired_reservation_without_process_start_releases_the_assignment() {
    let fixture = fixture(true);
    let reserved = request(&fixture, "expired-reservation");
    let plan = plan_mission_execution(&fixture.store, &reserved).unwrap();
    Connection::open(fixture.store.path())
        .unwrap()
        .execute(
            r#"
            INSERT INTO mission_execution_receipts (
                receipt_id, idempotency_key, mission_id, workflow_id, mission_revision,
                task_id, agent_id, executor_id, worktree_id, command_sha256,
                request_sha256, approval_scope_sha256, state, owner_token,
                lease_expires_at, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                    'reserved', 'expired-owner', '2000-01-01T00:00:00+00:00',
                    '2000-01-01T00:00:00+00:00', '2000-01-01T00:00:00+00:00')
            "#,
            params![
                plan.receipt_id,
                reserved.idempotency_key,
                plan.mission_id,
                plan.workflow_id,
                i64::try_from(plan.mission_revision).unwrap(),
                plan.task_id,
                plan.agent_id,
                plan.executor_id,
                plan.worktree_id,
                plan.command_sha256,
                plan.request_sha256,
                plan.approval_scope_sha256,
            ],
        )
        .unwrap();

    let completed = execute_mission_command(
        &fixture.store,
        approved_request(&fixture, "after-expired-reservation"),
    )
    .unwrap();
    assert_eq!(completed.receipt.status, "completed");
    let expired_state: String = Connection::open(fixture.store.path())
        .unwrap()
        .query_row(
            "SELECT state FROM mission_execution_receipts WHERE receipt_id=?1",
            [&plan.receipt_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(expired_state, "reservation_expired");
}

#[test]
fn expired_started_claim_becomes_indeterminate_and_remains_protected() {
    let fixture = fixture(true);
    let started = request(&fixture, "expired-started");
    let plan = plan_mission_execution(&fixture.store, &started).unwrap();
    Connection::open(fixture.store.path())
        .unwrap()
        .execute(
            r#"
            INSERT INTO mission_execution_receipts (
                receipt_id, idempotency_key, mission_id, workflow_id, mission_revision,
                task_id, agent_id, executor_id, worktree_id, command_sha256,
                request_sha256, approval_scope_sha256, state, owner_token,
                lease_expires_at, execution_started_at, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                    'running', 'expired-owner', '2000-01-01T00:00:00+00:00',
                    '2000-01-01T00:00:00+00:00', '2000-01-01T00:00:00+00:00',
                    '2000-01-01T00:00:00+00:00')
            "#,
            params![
                plan.receipt_id,
                started.idempotency_key,
                plan.mission_id,
                plan.workflow_id,
                i64::try_from(plan.mission_revision).unwrap(),
                plan.task_id,
                plan.agent_id,
                plan.executor_id,
                plan.worktree_id,
                plan.command_sha256,
                plan.request_sha256,
                plan.approval_scope_sha256,
            ],
        )
        .unwrap();

    let blocked = execute_mission_command(
        &fixture.store,
        approved_request(&fixture, "after-expired-started"),
    )
    .unwrap_err();
    assert!(format!("{blocked:#}").contains("already has protected execution"));
    let state: String = Connection::open(fixture.store.path())
        .unwrap()
        .query_row(
            "SELECT state FROM mission_execution_receipts WHERE receipt_id=?1",
            [&plan.receipt_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(state, "indeterminate");
}

#[test]
fn active_execution_lease_survives_same_key_retry_and_blocks_other_keys() {
    let fixture = fixture(true);
    let mut running = request(&fixture, "running-attempt");
    running.command = vec![fixture.cargo_command.clone(), "slow".to_string()];
    approve_request(&fixture, &mut running);
    let same_key_retry = running.clone();
    let post_completion_replay = running.clone();
    let mut other_key = request(&fixture, "competing-attempt");
    other_key.command = vec![fixture.cargo_command.clone(), "slow".to_string()];
    approve_request(&fixture, &mut other_key);

    let store_path = fixture.store.path().to_path_buf();
    let execution_store = store_path.clone();
    let execution = thread::spawn(move || {
        let store = FoundryStore::open(execution_store).unwrap();
        execute_mission_command(&store, running)
    });

    let mut observed_owner = None;
    for _ in 0..200 {
        let row = Connection::open(&store_path)
            .unwrap()
            .query_row(
                "SELECT state, owner_token FROM mission_execution_receipts WHERE idempotency_key = 'running-attempt'",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()
            .unwrap();
        if let Some((state, owner)) = row {
            if state == "running" {
                observed_owner = owner;
                break;
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
    let observed_owner = observed_owner.expect("execution should enter running with an owner");

    let retry_store = FoundryStore::open(&store_path).unwrap();
    let retry_error = execute_mission_command(&retry_store, same_key_retry).unwrap_err();
    assert!(format!("{retry_error:#}").contains("already running under an active lease"));
    let (state_after_retry, owner_after_retry): (String, Option<String>) =
        Connection::open(&store_path)
            .unwrap()
            .query_row(
                "SELECT state, owner_token FROM mission_execution_receipts WHERE idempotency_key = 'running-attempt'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
    assert_eq!(state_after_retry, "running");
    assert_eq!(owner_after_retry.as_deref(), Some(observed_owner.as_str()));

    let competing_store = FoundryStore::open(&store_path).unwrap();
    let competing_error = execute_mission_command(&competing_store, other_key).unwrap_err();
    assert!(format!("{competing_error:#}").contains("already has protected execution"));

    let completed = execution.join().unwrap().unwrap();
    assert_eq!(completed.receipt.status, "completed");
    let replay = execute_mission_command(&fixture.store, post_completion_replay).unwrap();
    assert!(replay.replayed);
    assert_eq!(completed.receipt.receipt_id, replay.receipt.receipt_id);
}

#[test]
fn stale_receipt_remains_unconsumed_and_fresh_retry_survives_reopen() {
    let fixture = fixture(true);
    let stale_execution = execute_mission_command(
        &fixture.store,
        approved_request(&fixture, "stale-revision-execution"),
    )
    .unwrap();
    let stale_revision = stale_execution.receipt.mission_revision;

    let mut repaired = load_mission(&fixture.store, &fixture.mission_id).unwrap();
    assert_eq!(repaired.revision, stale_revision);
    repaired.revision = repaired.revision.checked_add(1).unwrap();
    persist_mission(&fixture.store, &repaired);

    let current_revision = repaired.revision;
    let stale_error = claim_mission_execution_receipt_for_submission(
        &fixture.store,
        &stale_execution.receipt.receipt_id,
        &fixture.mission_id,
        current_revision,
        &fixture.task_id,
        &fixture.agent_id,
        "stale-revision-submission",
    )
    .unwrap_err();
    assert!(format!("{stale_error:#}").contains("does not match expected mission revision"));

    let consumed_after_failure: Option<String> = Connection::open(fixture.store.path())
        .unwrap()
        .query_row(
            "SELECT consumed_by_submission FROM mission_execution_receipts WHERE receipt_id=?1",
            [&stale_execution.receipt.receipt_id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(consumed_after_failure.is_none());

    let reopened = FoundryStore::open(fixture.store.path()).unwrap();
    let reopened_stale_error = claim_mission_execution_receipt_for_submission(
        &reopened,
        &stale_execution.receipt.receipt_id,
        &fixture.mission_id,
        current_revision,
        &fixture.task_id,
        &fixture.agent_id,
        "stale-revision-submission",
    )
    .unwrap_err();
    assert!(
        format!("{reopened_stale_error:#}").contains("does not match expected mission revision")
    );

    let fresh_execution = execute_mission_command(
        &reopened,
        approved_request(&fixture, "fresh-revision-execution"),
    )
    .unwrap();
    assert_eq!(fresh_execution.receipt.mission_revision, current_revision);
    let claimed = claim_mission_execution_receipt_for_submission(
        &reopened,
        &fresh_execution.receipt.receipt_id,
        &fixture.mission_id,
        current_revision,
        &fixture.task_id,
        &fixture.agent_id,
        "fresh-revision-submission",
    )
    .unwrap();
    assert_eq!(claimed.receipt_id, fresh_execution.receipt.receipt_id);

    let retry_store = FoundryStore::open(fixture.store.path()).unwrap();
    let retried = claim_mission_execution_receipt_for_submission(
        &retry_store,
        &fresh_execution.receipt.receipt_id,
        &fixture.mission_id,
        current_revision,
        &fixture.task_id,
        &fixture.agent_id,
        "fresh-revision-submission",
    )
    .unwrap();
    assert_eq!(retried.receipt_id, fresh_execution.receipt.receipt_id);

    release_mission_execution_receipt_submission_claim(
        &retry_store,
        &fresh_execution.receipt.receipt_id,
        "fresh-revision-submission",
    )
    .unwrap();
}
