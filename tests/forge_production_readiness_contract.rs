#![cfg(target_os = "linux")]

use forge_core::artifact::hex_sha256;
use forge_core::executor::ExecutorState;
use forge_core::milestone::{
    build_milestone_status, build_production_mission_lifecycle_evidence,
    build_production_readiness_plan, evaluate_production_readiness,
    production_readiness_claims_sha256, ProductionAlertEvidence, ProductionBoundedLoadEvidence,
    ProductionEvidenceRef, ProductionInstallationEvidence, ProductionKeyEscrowEvidence,
    ProductionMissionOperationalEvidence, ProductionOffHostBackupEvidence,
    ProductionReadinessManifest, ProductionReadinessOptions, ProductionReleaseEvidence,
    ProductionRestoreDrillEvidence, ProductionUpgradeRollbackEvidence,
    PRODUCTION_READINESS_MANIFEST_SCHEMA_VERSION, PRODUCTION_READINESS_PLAN_SCHEMA_VERSION,
    PRODUCTION_READINESS_REPORT_SCHEMA_VERSION, PRODUCTION_READINESS_REQUIRED_GATE_COUNT,
    PRODUCTION_READINESS_REQUIRED_RECEIPT_COUNT,
};
use forge_core::mission::{
    drive_mission, resume_mission, start_mission, submit_mission, MissionSubmission,
};
use forge_core::mission_executor::{
    build_mission_execution_approval, execute_mission_command, plan_mission_execution,
    MissionExecutionRequest,
};
use forge_core::mission_platform::{
    mission_platform_catalog, MISSION_PLATFORM_BOUNDED_SIMULATION, MISSION_PLATFORM_CONTRACT_ONLY,
    MISSION_PLATFORM_RUNTIME_REAL,
};
use forge_core::storage::ForgeStore;
use forge_core::worktree::{
    approve_worktree_config, initialize_worktree, register_worktree, WorktreeRegisterOptions,
};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::tempdir;

const TARGETS: [&str; 5] = [
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
];

#[test]
fn milestone_capability_readiness_does_not_claim_production_readiness() {
    let status = build_milestone_status("0.5").unwrap();

    assert!(status.promotion_decision.promotable);
    assert!(status.promotion_decision.capability_ready);
    assert_eq!(status.promotion_decision.readiness_scope, "capability");
    assert!(!status.promotion_decision.production_ready);
    assert!(!status.promotion_decision.production_evidence_evaluated);
    assert!(status
        .promotion_decision
        .reason
        .contains("does not assert operational production readiness"));
}

#[test]
fn production_readiness_plan_is_complete_and_non_mutating() {
    let report = build_production_readiness_plan("0.5").unwrap();

    assert_eq!(
        report.schema_version,
        PRODUCTION_READINESS_PLAN_SCHEMA_VERSION
    );
    assert_eq!(report.evaluation_mode, "plan_only");
    assert!(report.capability_ready);
    assert!(!report.production_ready);
    assert_eq!(report.commands_executed, 0);
    assert!(!report.mutations_performed);
    assert_eq!(report.capability_inventory_count, 40);
    assert_eq!(
        report.required_gate_count,
        PRODUCTION_READINESS_REQUIRED_GATE_COUNT
    );
    assert_eq!(
        report.required_receipt_count,
        PRODUCTION_READINESS_REQUIRED_RECEIPT_COUNT
    );
    assert_eq!(
        report.capability_inventory_sha256,
        mission_platform_catalog().inventory_sha256
    );
    assert_eq!(
        report
            .capability_proof_kind_counts
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec![
            MISSION_PLATFORM_BOUNDED_SIMULATION,
            MISSION_PLATFORM_CONTRACT_ONLY,
            MISSION_PLATFORM_RUNTIME_REAL,
        ]
    );
    let gates = report
        .requirements
        .iter()
        .map(|requirement| requirement.gate_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        gates,
        vec![
            "capability_readiness",
            "manifest_integrity",
            "release_integrity",
            "installed_ops_health",
            "off_host_recovery",
            "key_escrow",
            "alerting",
            "restore_drill",
            "upgrade_rollback",
            "bounded_load",
            "mission_operational_lifecycle",
        ]
    );
    assert!(report
        .requirements
        .iter()
        .all(|requirement| requirement.blocking));
}

#[test]
fn production_readiness_passes_only_complete_fresh_secret_free_evidence() {
    let temp = tempdir().unwrap();
    let now = now_epoch();
    let manifest = complete_manifest(temp.path(), now);
    write_manifest(temp.path(), &manifest);

    let report = evaluate(temp.path()).unwrap();

    assert_eq!(
        report.schema_version,
        PRODUCTION_READINESS_REPORT_SCHEMA_VERSION
    );
    assert_eq!(report.evaluation_mode, "read_only");
    assert!(report.capability_ready);
    assert!(
        report.production_ready,
        "production readiness gates failed: {:#?}",
        report.gates
    );
    assert_eq!(report.decision, "production_ready");
    assert!(report.blocked_by.is_empty());
    assert_eq!(report.commands_executed, 0);
    assert!(!report.mutations_performed);
    assert_eq!(report.gates.len(), 11);
    assert_eq!(
        report.required_gate_count,
        PRODUCTION_READINESS_REQUIRED_GATE_COUNT
    );
    assert_eq!(
        report.required_receipt_count,
        PRODUCTION_READINESS_REQUIRED_RECEIPT_COUNT
    );
    assert_eq!(report.capability_inventory_count, 40);
    assert_eq!(
        report.capability_inventory_sha256,
        mission_platform_catalog().inventory_sha256
    );
    assert!(report.gates.iter().all(|gate| gate.status == "pass"));
}

#[test]
fn production_readiness_fails_closed_for_missing_stale_or_failed_evidence() {
    let temp = tempdir().unwrap();
    let now = now_epoch();
    let mut manifest = complete_manifest(temp.path(), now);
    manifest.alerts.backup_age_alert = false;
    manifest.off_host_backup.recovery_challenge_epoch = now.saturating_sub(86_401);
    let missing = temp
        .path()
        .join(&manifest.key_escrow.evidence.artifact_path);
    fs::remove_file(missing).unwrap();
    write_manifest(temp.path(), &manifest);

    let report = evaluate(temp.path()).unwrap();

    assert!(report.capability_ready);
    assert!(!report.production_ready);
    assert_eq!(report.decision, "fail_closed");
    assert!(report.blocked_by.contains(&"alerting".to_string()));
    assert!(report.blocked_by.contains(&"off_host_recovery".to_string()));
    assert!(report.blocked_by.contains(&"key_escrow".to_string()));
    assert_eq!(report.commands_executed, 0);
    assert!(!report.mutations_performed);
}

#[test]
fn operational_lifecycle_rejects_wrong_inventory_detached_receipts_and_reordered_events() {
    let temp = tempdir().unwrap();
    let now = now_epoch();
    let mut manifest = complete_manifest(temp.path(), now);
    manifest
        .mission_operational_lifecycle
        .capability_numbers
        .pop();
    manifest
        .mission_operational_lifecycle
        .capability_inventory_sha256 = "f".repeat(64);
    manifest
        .mission_operational_lifecycle
        .submitted_execute_receipt_sha256 = "e".repeat(64);
    manifest.mission_operational_lifecycle.resumed_handoff_id = "handoff-detached".to_string();
    manifest
        .mission_operational_lifecycle
        .resume_observed_at_epoch = manifest
        .mission_operational_lifecycle
        .execute_observed_at_epoch
        .saturating_sub(1);
    bind_receipts(temp.path(), &mut manifest, now);
    write_manifest(temp.path(), &manifest);

    let report = evaluate(temp.path()).unwrap();
    let gate = report
        .gates
        .iter()
        .find(|gate| gate.id == "mission_operational_lifecycle")
        .unwrap();

    assert!(!report.production_ready);
    assert_eq!(gate.status, "fail");
    for check_id in [
        "mission_operational_lifecycle.inventory",
        "mission_operational_lifecycle.submit",
        "mission_operational_lifecycle.resume",
        "mission_operational_lifecycle.order",
    ] {
        assert!(gate
            .checks
            .iter()
            .any(|check| check.id == check_id && !check.passed));
    }
}

#[test]
fn bounded_platform_receipt_cannot_be_reused_as_operational_lifecycle_evidence() {
    let temp = tempdir().unwrap();
    let now = now_epoch();
    let mut manifest = complete_manifest(temp.path(), now);
    let bounded_receipt = serde_json::json!({
        "schema_version": "forge.mission_platform.effect_receipt.v1",
        "id": "bounded-platform-receipt",
        "capability_id": "formal_review_repair",
        "adapter": "quality_gate.repair_and_revalidate",
        "input_sha256": "a".repeat(64),
        "result_sha256": "b".repeat(64),
        "result": {"status": "passed"}
    });
    let bytes = serde_json::to_vec_pretty(&bounded_receipt).unwrap();
    let path = temp.path().join(
        &manifest
            .mission_operational_lifecycle
            .evidence
            .artifact_path,
    );
    fs::write(&path, &bytes).unwrap();
    manifest
        .mission_operational_lifecycle
        .evidence
        .artifact_sha256 = hex_sha256(&bytes);
    write_manifest(temp.path(), &manifest);

    let report = evaluate(temp.path()).unwrap();
    let gate = report
        .gates
        .iter()
        .find(|gate| gate.id == "mission_operational_lifecycle")
        .unwrap();

    assert!(!report.production_ready);
    assert_eq!(gate.status, "fail");
    assert!(gate
        .checks
        .iter()
        .any(|check| { check.id == "mission_operational_lifecycle.receipt" && !check.passed }));
}

#[test]
fn operational_lifecycle_bundle_fails_against_a_different_source_store() {
    let temp = tempdir().unwrap();
    let now = now_epoch();
    let manifest = complete_manifest(temp.path(), now);
    write_manifest(temp.path(), &manifest);
    let different_store_path = temp.path().join("different-forge.sqlite");
    drop(ForgeStore::open(&different_store_path).unwrap());

    let report = evaluate_with_store(temp.path(), &different_store_path).unwrap();
    let gate = report
        .gates
        .iter()
        .find(|gate| gate.id == "mission_operational_lifecycle")
        .unwrap();

    assert!(!report.production_ready);
    assert_eq!(gate.status, "fail");
    assert!(gate.checks.iter().any(|check| {
        check.id == "mission_operational_lifecycle.store_execution" && !check.passed
    }));
    assert!(gate.checks.iter().any(|check| {
        check.id == "mission_operational_lifecycle.store_submission" && !check.passed
    }));
    assert!(gate.checks.iter().any(|check| {
        check.id == "mission_operational_lifecycle.store_resume" && !check.passed
    }));
}

#[test]
fn production_readiness_rejects_secret_material_without_echoing_it() {
    let temp = tempdir().unwrap();
    let now = now_epoch();
    let manifest = complete_manifest(temp.path(), now);
    let mut json = serde_json::to_value(manifest).unwrap();
    let secret = "sk-proj-abcdefghijklmnopqrstuvwxyzABCDEF1234567890";
    json.as_object_mut()
        .unwrap()
        .insert("api_token".to_string(), secret.into());
    fs::write(
        temp.path().join("production-readiness.json"),
        serde_json::to_vec_pretty(&json).unwrap(),
    )
    .unwrap();

    let error = evaluate(temp.path()).unwrap_err().to_string();

    assert!(error.contains("must be secret-free"));
    assert!(!error.contains(secret));
}

fn evaluate(root: &Path) -> anyhow::Result<forge_core::milestone::ProductionReadinessReport> {
    let store_path = root.join("forge.sqlite");
    evaluate_with_store(root, &store_path)
}

fn evaluate_with_store(
    root: &Path,
    store_path: &Path,
) -> anyhow::Result<forge_core::milestone::ProductionReadinessReport> {
    evaluate_production_readiness(ProductionReadinessOptions {
        version: "0.5",
        manifest_path: Path::new("production-readiness.json"),
        evidence_root: root,
        store_path,
    })
}

fn complete_manifest(root: &Path, now: u64) -> ProductionReadinessManifest {
    let mission_operational_lifecycle = build_real_mission_lifecycle_evidence(root);
    let binary_sha256_by_target = TARGETS
        .iter()
        .enumerate()
        .map(|(index, target)| ((*target).to_string(), format!("{:064x}", index + 1)))
        .collect::<BTreeMap<_, _>>();
    let installed_binary_sha256 = binary_sha256_by_target[TARGETS[0]].clone();

    let mut manifest = ProductionReadinessManifest {
        schema_version: PRODUCTION_READINESS_MANIFEST_SCHEMA_VERSION.to_string(),
        milestone: "0.5".to_string(),
        profile: "single_host_linux_v0.5".to_string(),
        release_version: env!("CARGO_PKG_VERSION").to_string(),
        generated_at_epoch: now,
        release: ProductionReleaseEvidence {
            matrix: write_evidence(root, "release-matrix", now),
            successful_targets: TARGETS.iter().map(|target| (*target).to_string()).collect(),
            artifacts: write_evidence(root, "release-artifacts", now),
            binary_sha256_by_target,
            sbom: write_evidence(root, "release-sbom", now),
            sbom_format: "cyclonedx-json".to_string(),
            sbom_component_count: 42,
            checksums: write_evidence(root, "release-checksums", now),
            checksum_entry_count: TARGETS.len() as u64,
            checksums_verified: true,
            sigstore: write_evidence(root, "release-sigstore", now),
            sigstore_verified: true,
            provenance: write_evidence(root, "release-provenance", now),
            provenance_verified: true,
        },
        installation: ProductionInstallationEvidence {
            evidence: write_evidence(root, "installation", now),
            target: TARGETS[0].to_string(),
            installed_version: env!("CARGO_PKG_VERSION").to_string(),
            installed_binary_sha256,
            service_active: true,
            store_check_passed: true,
            ops_authenticated_probe_passed: true,
            ops_http_status: 200,
            ops_loopback_only: true,
        },
        off_host_backup: ProductionOffHostBackupEvidence {
            evidence: write_evidence(root, "off-host-recovery", now),
            recovery_challenge_epoch: now.saturating_sub(60),
            immutable_upload_passed: true,
            remote_digest_verified: true,
            download_digest_verified: true,
            downloaded_store_check_passed: true,
            disposable_restore_passed: true,
            restored_store_check_passed: true,
            off_host_retention_enabled: true,
            forge_key_isolated_from_uploader: true,
            uploader_credentials_isolated_from_forge: true,
        },
        key_escrow: ProductionKeyEscrowEvidence {
            evidence: write_evidence(root, "key-escrow", now),
            encrypted: true,
            separate_access_control: true,
            recovery_key_available: true,
            restore_with_escrowed_key_tested: true,
            excluded_from_database_backup: true,
        },
        alerts: ProductionAlertEvidence {
            evidence: write_evidence(root, "alerting", now),
            service_failure_alert: true,
            store_check_alert: true,
            backup_timer_alert: true,
            off_host_failure_alert: true,
            disk_space_alert: true,
            backup_age_alert: true,
            delivery_route_verified: true,
        },
        restore_drill: ProductionRestoreDrillEvidence {
            evidence: write_evidence(root, "restore-drill", now),
            drill_epoch: now.saturating_sub(30),
            disposable_recovery_host: true,
            downloaded_store_check_passed: true,
            restored_store_check_passed: true,
            canary_workflow_verified: true,
            ops_authenticated_probe_passed: true,
            rpo_seconds: 3_600,
            rto_seconds: 120,
        },
        upgrade_rollback: ProductionUpgradeRollbackEvidence {
            evidence: write_evidence(root, "upgrade-rollback", now),
            target_version: env!("CARGO_PKG_VERSION").to_string(),
            simulation_completed: true,
            pre_upgrade_backup_verified: true,
            upgraded_store_check_passed: true,
            upgraded_ops_health_passed: true,
            rollback_completed: true,
            previous_version_store_check_passed: true,
            previous_version_ops_health_passed: true,
            target_reinstalled_and_healthy: true,
        },
        bounded_load: ProductionBoundedLoadEvidence {
            evidence: write_evidence(root, "bounded-load", now),
            duration_seconds: 5,
            concurrency: 4,
            operation_count: 100,
            error_count: 0,
            p95_latency_millis: 50,
            max_rss_bytes: 64 * 1024 * 1024,
            max_rss_limit_bytes: 128 * 1024 * 1024,
            timeout_enforced: true,
            resource_limit_enforced: true,
            store_check_passed: true,
            crash_restart_verified: true,
        },
        mission_operational_lifecycle,
    };
    bind_receipts(root, &mut manifest, now);
    manifest
}

fn build_real_mission_lifecycle_evidence(root: &Path) -> ProductionMissionOperationalEvidence {
    let repository = root.join("mission-worktree");
    let evidence_command = initialize_mission_repository(&repository);
    let store = ForgeStore::open(root.join("forge.sqlite")).unwrap();
    let worktree_id = "wt-production-readiness";

    register_worktree(
        &store,
        WorktreeRegisterOptions {
            path: repository.clone(),
            id: Some(worktree_id.to_string()),
            workflow_id: None,
            task_id: None,
            origin: "production-readiness-contract".to_string(),
            created_by_forge: false,
        },
    )
    .unwrap();
    initialize_worktree(
        &store,
        worktree_id,
        true,
        false,
        "production-readiness-contract",
    )
    .unwrap();
    let config_path = repository.join(".forge/worktree.toml");
    let mut config: toml::Value =
        toml::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config["guardrails"]["allowed_commands"] =
        toml::Value::Array(vec![toml::Value::String(evidence_command.clone())]);
    config["guardrails"]["max_command_seconds"] = toml::Value::Integer(10);
    config["sandbox"]["runtime"] = toml::Value::String("bubblewrap".to_string());
    config["sandbox"]["network"] = toml::Value::String("deny".to_string());
    fs::write(&config_path, toml::to_string_pretty(&config).unwrap()).unwrap();
    approve_worktree_config(
        &store,
        worktree_id,
        true,
        "production-readiness-contract",
        "production-readiness-contract",
    )
    .unwrap();

    let started = start_mission(
        &store,
        "Prove a persisted production mission lifecycle",
        "software-factory",
        None,
        &repository,
    )
    .unwrap();
    let dispatched = drive_mission(&store, &started.mission.id).unwrap();
    assert_eq!(dispatched.action, "assignment_created");
    let assignment = dispatched.assignment.unwrap();
    let executor_id = assignment.harness.runtime.clone();
    let executor = ExecutorState {
        id: "codex".to_string(),
        display_name: "Production readiness contract executor".to_string(),
        command: "codex".to_string(),
        installed: true,
        configured: true,
        command_path: Some("codex".to_string()),
        config_evidence: vec!["production readiness contract fixture".to_string()],
        non_interactive_ready: true,
        probe_evidence: vec!["fixture executes only the bounded evidence observer".to_string()],
        forge_first_ready: true,
        forge_first_entrypoint: None,
        harness_status: None,
        allowed: true,
        decision_source: "production-readiness-contract".to_string(),
        synced_at: "2026-07-24T00:00:00Z".to_string(),
    };
    store
        .save_executor_state(&executor.id, &serde_json::to_value(&executor).unwrap())
        .unwrap();

    let current = dispatched.mission;
    let mut execution_request = MissionExecutionRequest {
        idempotency_key: "production-lifecycle-execution-v1".to_string(),
        mission_id: current.id.clone(),
        workflow_id: current.workflow_id.clone(),
        expected_mission_revision: current.revision,
        task_id: assignment.task.id.clone(),
        agent_id: assignment.agent.instance_id.clone(),
        executor_id,
        worktree: current.worktree.clone(),
        purpose: "preview".to_string(),
        command: vec![evidence_command],
        requested_evidence: vec![
            "requirements_summary".to_string(),
            "acceptance_criteria".to_string(),
        ],
        approval: None,
        dry_run: false,
        allow_trusted_process_runtime: false,
    };
    let plan = plan_mission_execution(&store, &execution_request).unwrap();
    assert!(
        plan.allowed,
        "production lifecycle plan was denied: {:#?}",
        plan.policy_trace
    );
    execution_request.approval = Some(
        build_mission_execution_approval(&plan, "production-readiness-contract", 300).unwrap(),
    );
    let executed = execute_mission_command(&store, execution_request).unwrap();
    assert_eq!(executed.receipt.status, "completed");
    assert_eq!(executed.receipt.exit_code, Some(0));
    assert_eq!(
        executed
            .receipt
            .sandbox
            .as_ref()
            .map(|sandbox| sandbox.status.as_str()),
        Some("sandbox_completed")
    );

    let submitted = submit_mission(
        &store,
        &current.id,
        MissionSubmission {
            idempotency_key: "production-lifecycle-submission-v1".to_string(),
            execution_receipt_id: executed.receipt.receipt_id.clone(),
            task_id: assignment.task.id,
            agent_id: assignment.agent.instance_id,
            status: "completed".to_string(),
            summary: "Bounded intake evidence was observed and persisted".to_string(),
            artifacts: Vec::new(),
            validations: Vec::new(),
            risks: Vec::new(),
            followups: Vec::new(),
            tests_passed: executed.receipt.tests_passed,
            tests_failed: executed.receipt.tests_failed,
        },
    )
    .unwrap();
    assert_eq!(submitted.status, "queued");
    let resumed = resume_mission(&store, &current.id).unwrap();
    assert_eq!(resumed.action, "handoff_consumed");
    assert_eq!(
        resumed.handoff_id.as_deref(),
        Some(submitted.handoff_id.as_str())
    );

    let artifact_path = "mission-operational-lifecycle.json";
    let package = build_production_mission_lifecycle_evidence(
        &store,
        env!("CARGO_PKG_VERSION"),
        &current.id,
        &executed.receipt.receipt_id,
        artifact_path,
    )
    .unwrap();
    let artifact_bytes = serde_json::to_vec(&package.artifact).unwrap();
    fs::write(root.join(artifact_path), &artifact_bytes).unwrap();
    assert_eq!(
        package.manifest_section.evidence.artifact_sha256,
        hex_sha256(&artifact_bytes)
    );
    package.manifest_section
}

fn initialize_mission_repository(repository: &Path) -> String {
    fs::create_dir_all(repository.join("fixture-bin")).unwrap();
    let evidence_command = repository.join("fixture-bin/git");
    fs::write(
        &evidence_command,
        concat!(
            "#!/bin/sh\n",
            "printf '%s\\n' 'FORGE_GATE_EVIDENCE:",
            "{\"schema_version\":\"forge.mission.gate_evidence_observation.v1\",",
            "\"evidence\":{\"requirements_summary\":\"The bounded mission requirements were inspected.\",",
            "\"acceptance_criteria\":[\"Execution, submission and resume must persist atomically.\"]}}'\n"
        ),
    )
    .unwrap();
    fs::set_permissions(&evidence_command, fs::Permissions::from_mode(0o755)).unwrap();

    run_git(repository, &["init", "--initial-branch=main"]);
    run_git(repository, &["add", "fixture-bin/git"]);
    run_git(
        repository,
        &[
            "-c",
            "user.name=Forge Production Readiness",
            "-c",
            "user.email=forge-production-readiness@example.invalid",
            "commit",
            "-m",
            "initial evidence observer",
        ],
    );
    evidence_command.to_string_lossy().into_owned()
}

fn run_git(repository: &Path, arguments: &[&str]) {
    let mut command = Command::new("git");
    if arguments.first() == Some(&"init") {
        command.args(arguments).arg(repository);
    } else {
        command.arg("-C").arg(repository);
        command.args(arguments);
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        arguments,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn bind_receipts(root: &Path, manifest: &mut ProductionReadinessManifest, observed_at_epoch: u64) {
    for kind in [
        "release_matrix",
        "release_artifacts",
        "release_sbom",
        "release_checksums",
        "release_sigstore",
        "release_provenance",
        "installation",
        "off_host_recovery",
        "key_escrow",
        "alerting",
        "restore_drill",
        "upgrade_rollback",
        "bounded_load",
    ] {
        let claims_sha256 = production_readiness_claims_sha256(manifest, kind).unwrap();
        let receipt = serde_json::json!({
            "schema_version": format!("forge.milestone.production_evidence.{kind}.v1"),
            "kind": kind,
            "status": "passed",
            "subject_version": manifest.release_version,
            "claims_sha256": claims_sha256,
        });
        let bytes = serde_json::to_vec_pretty(&receipt).unwrap();
        let artifact_path = PathBuf::from(format!("{kind}.json"));
        fs::write(root.join(&artifact_path), &bytes).unwrap();
        let evidence = ProductionEvidenceRef {
            artifact_path: artifact_path.to_string_lossy().into_owned(),
            artifact_sha256: hex_sha256(&bytes),
            observed_at_epoch,
        };
        match kind {
            "release_matrix" => manifest.release.matrix = evidence,
            "release_artifacts" => manifest.release.artifacts = evidence,
            "release_sbom" => manifest.release.sbom = evidence,
            "release_checksums" => manifest.release.checksums = evidence,
            "release_sigstore" => manifest.release.sigstore = evidence,
            "release_provenance" => manifest.release.provenance = evidence,
            "installation" => manifest.installation.evidence = evidence,
            "off_host_recovery" => manifest.off_host_backup.evidence = evidence,
            "key_escrow" => manifest.key_escrow.evidence = evidence,
            "alerting" => manifest.alerts.evidence = evidence,
            "restore_drill" => manifest.restore_drill.evidence = evidence,
            "upgrade_rollback" => manifest.upgrade_rollback.evidence = evidence,
            "bounded_load" => manifest.bounded_load.evidence = evidence,
            _ => unreachable!(),
        }
    }
}

fn write_evidence(root: &Path, name: &str, observed_at_epoch: u64) -> ProductionEvidenceRef {
    let artifact_path = PathBuf::from(format!("{name}.json"));
    let bytes = format!("{{\"kind\":\"{name}\",\"status\":\"pass\"}}\n").into_bytes();
    fs::write(root.join(&artifact_path), &bytes).unwrap();
    ProductionEvidenceRef {
        artifact_path: artifact_path.to_string_lossy().into_owned(),
        artifact_sha256: hex_sha256(&bytes),
        observed_at_epoch,
    }
}

fn write_manifest(root: &Path, manifest: &ProductionReadinessManifest) {
    fs::write(
        root.join("production-readiness.json"),
        serde_json::to_vec_pretty(manifest).unwrap(),
    )
    .unwrap();
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
