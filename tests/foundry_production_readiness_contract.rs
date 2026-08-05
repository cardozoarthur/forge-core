#![cfg(target_os = "linux")]

use foundry_core::artifact::hex_sha256;
use foundry_core::executor::ExecutorState;
use foundry_core::milestone::{
    assemble_production_evidence, build_milestone_status,
    build_production_mission_lifecycle_evidence, build_production_readiness_plan,
    evaluate_production_readiness, production_readiness_claims_sha256,
    production_readiness_claims_value, write_production_evidence_template, ProductionAlertEvidence,
    ProductionBoundedLoadEvidence, ProductionEvidenceAssemblyOptions, ProductionEvidenceRef,
    ProductionEvidenceTemplateOptions, ProductionInstallationEvidence, ProductionKeyEscrowEvidence,
    ProductionMissionOperationalEvidence, ProductionOffHostBackupEvidence,
    ProductionReadinessManifest, ProductionReadinessOptions, ProductionReleaseEvidence,
    ProductionRestoreDrillEvidence, ProductionUpgradeRollbackEvidence,
    PRODUCTION_EVIDENCE_DRAFT_SCHEMA_VERSION, PRODUCTION_READINESS_MANIFEST_SCHEMA_VERSION,
    PRODUCTION_READINESS_PLAN_SCHEMA_VERSION, PRODUCTION_READINESS_REPORT_SCHEMA_VERSION,
    PRODUCTION_READINESS_REQUIRED_GATE_COUNT, PRODUCTION_READINESS_REQUIRED_RECEIPT_COUNT,
};
use foundry_core::mission::{
    drive_mission, resume_mission, start_mission, submit_mission, MissionSubmission,
};
use foundry_core::mission_executor::{
    build_mission_execution_approval, execute_mission_command, plan_mission_execution,
    MissionExecutionRequest,
};
use foundry_core::mission_platform::{
    mission_platform_catalog, MISSION_PLATFORM_BOUNDED_SIMULATION, MISSION_PLATFORM_CONTRACT_ONLY,
    MISSION_PLATFORM_RUNTIME_REAL,
};
use foundry_core::storage::FoundryStore;
use foundry_core::worktree::{
    approve_worktree_config, initialize_worktree, register_worktree, WorktreeRegisterOptions,
};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
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
const CURRENT_MILESTONE: &str = "0.6";
const CURRENT_PRODUCTION_PROFILE: &str = "single_host_linux_v0.6";
const LEGACY_MILESTONE_QUERY: &str = "0.5";

#[test]
fn milestone_capability_readiness_does_not_claim_production_readiness() {
    let status = build_milestone_status(LEGACY_MILESTONE_QUERY).unwrap();

    assert_eq!(status.milestone, CURRENT_MILESTONE);
    assert_eq!(status.requested_milestone, LEGACY_MILESTONE_QUERY);
    assert_eq!(status.compatibility_mode, "legacy_query_alias");

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
    let report = build_production_readiness_plan(LEGACY_MILESTONE_QUERY).unwrap();

    assert_eq!(report.milestone, CURRENT_MILESTONE);
    assert_eq!(report.profile, CURRENT_PRODUCTION_PROFILE);

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
    let installed_requirement = report
        .requirements
        .iter()
        .find(|requirement| requirement.gate_id == "installed_ops_health")
        .unwrap();
    assert!(installed_requirement
        .required_evidence
        .iter()
        .any(|evidence| evidence.contains("Ops, runtime and request-supervisor services active")));
    let alerting_requirement = report
        .requirements
        .iter()
        .find(|requirement| requirement.gate_id == "alerting")
        .unwrap();
    assert!(alerting_requirement
        .required_evidence
        .iter()
        .any(|evidence| {
            evidence.contains("Ops, runtime and request-supervisor service-failure alerts")
        }));
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
    let source_bound_receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(temp.path().join("release_matrix.json")).unwrap())
            .unwrap();
    assert_eq!(
        source_bound_receipt["schema_version"],
        "foundry.milestone.production_evidence.release_matrix.v2"
    );
}

#[test]
fn current_release_rejects_legacy_unbound_receipts() {
    let temp = tempdir().unwrap();
    let now = now_epoch();
    let mut manifest = complete_manifest(temp.path(), now);
    let receipt_path = temp.path().join("release_matrix.json");
    let mut receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).unwrap()).unwrap();
    receipt["schema_version"] = "foundry.milestone.production_evidence.release_matrix.v1".into();
    let receipt_object = receipt.as_object_mut().unwrap();
    receipt_object.remove("source_artifact_path");
    receipt_object.remove("source_artifact_sha256");
    receipt_object.remove("source_observed_at_epoch");
    let bytes = serde_json::to_vec(&receipt).unwrap();
    fs::write(&receipt_path, &bytes).unwrap();
    manifest.release.matrix.artifact_sha256 = hex_sha256(&bytes);
    write_manifest(temp.path(), &manifest);

    let report = evaluate(temp.path()).unwrap();
    assert!(!report.production_ready);
    let release_gate = report
        .gates
        .iter()
        .find(|gate| gate.id == "release_integrity")
        .unwrap();
    assert!(release_gate
        .checks
        .iter()
        .any(|check| { check.id == "release_matrix.receipt_schema" && !check.passed }));
}

#[test]
fn production_evaluator_rejects_source_claims_even_when_all_digests_are_rebound() {
    let temp = tempdir().unwrap();
    let now = now_epoch();
    let mut manifest = complete_manifest(temp.path(), now);
    let source_path = temp.path().join("source-bounded_load.json");
    let mut source: serde_json::Value =
        serde_json::from_slice(&fs::read(&source_path).unwrap()).unwrap();
    source["claims"]["error_count"] = 1.into();
    let mut source_bytes = serde_json::to_vec(&source).unwrap();
    source_bytes.push(b'\n');
    fs::write(&source_path, &source_bytes).unwrap();

    let receipt_path = temp.path().join("bounded_load.json");
    let mut receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(&receipt_path).unwrap()).unwrap();
    receipt["source_artifact_sha256"] = hex_sha256(&source_bytes).into();
    let receipt_bytes = serde_json::to_vec_pretty(&receipt).unwrap();
    fs::write(&receipt_path, &receipt_bytes).unwrap();
    manifest.bounded_load.evidence.artifact_sha256 = hex_sha256(&receipt_bytes);
    write_manifest(temp.path(), &manifest);

    let report = evaluate(temp.path()).unwrap();
    assert!(!report.production_ready);
    let bounded_gate = report
        .gates
        .iter()
        .find(|gate| gate.id == "bounded_load")
        .unwrap();
    assert!(bounded_gate
        .checks
        .iter()
        .any(|check| check.id == "bounded_load.source.claims" && !check.passed));
}

#[test]
fn mandatory_service_claims_and_source_attestations_are_exact() {
    let temp = tempdir().unwrap();
    let now = now_epoch();
    let manifest = complete_manifest(temp.path(), now);

    let expected_installation = serde_json::json!({
        "target": &manifest.installation.target,
        "installed_version": &manifest.installation.installed_version,
        "installed_binary_sha256": &manifest.installation.installed_binary_sha256,
        "ops_service_active": true,
        "runtime_service_active": true,
        "request_supervisor_service_active": true,
        "store_check_passed": true,
        "ops_authenticated_probe_passed": true,
        "ops_http_status": 200,
        "ops_loopback_only": true,
    });
    let installation_claims = production_readiness_claims_value(&manifest, "installation").unwrap();
    assert_eq!(installation_claims, expected_installation);
    assert!(installation_claims.get("service_active").is_none());
    let installation_source: serde_json::Value =
        serde_json::from_slice(&fs::read(temp.path().join("source-installation.json")).unwrap())
            .unwrap();
    assert_eq!(installation_source["claims"], expected_installation);

    let expected_alerts = serde_json::json!({
        "ops_service_failure_alert": true,
        "runtime_service_failure_alert": true,
        "request_supervisor_service_failure_alert": true,
        "store_check_alert": true,
        "backup_timer_alert": true,
        "off_host_failure_alert": true,
        "disk_space_alert": true,
        "backup_age_alert": true,
        "delivery_route_verified": true,
    });
    let alert_claims = production_readiness_claims_value(&manifest, "alerting").unwrap();
    assert_eq!(alert_claims, expected_alerts);
    assert!(alert_claims.get("service_failure_alert").is_none());
    let alert_source: serde_json::Value =
        serde_json::from_slice(&fs::read(temp.path().join("source-alerting.json")).unwrap())
            .unwrap();
    assert_eq!(alert_source["claims"], expected_alerts);
}

#[test]
fn legacy_ambiguous_service_fields_fail_closed() {
    let temp = tempdir().unwrap();
    let manifest = complete_manifest(temp.path(), now_epoch());

    let mut legacy_installation = serde_json::to_value(&manifest).unwrap();
    legacy_installation["installation"]["service_active"] = true.into();
    let installation_error =
        serde_json::from_value::<ProductionReadinessManifest>(legacy_installation)
            .unwrap_err()
            .to_string();
    assert!(installation_error.contains("service_active"));

    let mut legacy_alert = serde_json::to_value(&manifest).unwrap();
    legacy_alert["alerts"]["service_failure_alert"] = true.into();
    let alert_error = serde_json::from_value::<ProductionReadinessManifest>(legacy_alert)
        .unwrap_err()
        .to_string();
    assert!(alert_error.contains("service_failure_alert"));
}

#[test]
fn production_readiness_requires_every_service_and_service_failure_alert() {
    let temp = tempdir().unwrap();
    let now = now_epoch();
    let mut manifest = complete_manifest(temp.path(), now);
    manifest.installation.runtime_service_active = false;
    manifest.alerts.request_supervisor_service_failure_alert = false;
    bind_receipts(temp.path(), &mut manifest, now);
    write_manifest(temp.path(), &manifest);

    let report = evaluate(temp.path()).unwrap();
    assert!(!report.production_ready);
    let installation_gate = report
        .gates
        .iter()
        .find(|gate| gate.id == "installed_ops_health")
        .unwrap();
    assert!(installation_gate
        .checks
        .iter()
        .any(|check| check.id == "runtime_service_active" && !check.passed));
    let alert_gate = report
        .gates
        .iter()
        .find(|gate| gate.id == "alerting")
        .unwrap();
    assert!(alert_gate
        .checks
        .iter()
        .any(|check| check.id == "required_alerts" && !check.passed));
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
        "schema_version": "foundry.mission_platform.effect_receipt.v1",
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
    let different_store_path = temp.path().join("different-foundry.sqlite");
    drop(FoundryStore::open(&different_store_path).unwrap());

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

#[test]
fn production_evidence_assembly_is_explicit_source_bound_and_fail_closed() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let release_version = env!("CARGO_PKG_VERSION");

    let template = write_production_evidence_template(ProductionEvidenceTemplateOptions {
        version: CURRENT_MILESTONE,
        release_version,
        evidence_root: root,
        template_path: Path::new("production-evidence-template.json"),
    })
    .unwrap();
    assert_eq!(template.status, "template_written");
    assert!(template.unresolved_field_count > 13);
    let template_json: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("production-evidence-template.json")).unwrap())
            .unwrap();
    assert!(template_json["installation"]["ops_service_active"].is_null());
    assert!(template_json["installation"]["runtime_service_active"].is_null());
    assert!(template_json["installation"]["request_supervisor_service_active"].is_null());
    assert!(template_json["installation"]
        .get("service_active")
        .is_none());
    assert!(template_json["alerts"]["ops_service_failure_alert"].is_null());
    assert!(template_json["alerts"]["runtime_service_failure_alert"].is_null());
    assert!(template_json["alerts"]["request_supervisor_service_failure_alert"].is_null());
    assert!(template_json["alerts"]
        .get("service_failure_alert")
        .is_none());
    assert!(template_json["sources"]["bounded_load"]["artifact_path"].is_null());
    let null_error = assemble_production_evidence(ProductionEvidenceAssemblyOptions {
        version: CURRENT_MILESTONE,
        release_version,
        evidence_root: root,
        draft_path: Path::new("production-evidence-template.json"),
        receipt_directory: Path::new("null-receipts"),
        manifest_path: Path::new("null-manifest.json"),
    })
    .unwrap_err()
    .to_string();
    assert!(null_error.contains("unresolved null"));
    assert!(!root.join("null-receipts").exists());
    assert!(!root.join("null-manifest.json").exists());

    let now = now_epoch();
    let manifest = complete_manifest(root, now);
    let draft = completed_evidence_draft(&manifest, now);
    write_json(root.join("production-evidence-draft.json"), &draft);

    let mut stale = draft.clone();
    stale["sources"]["bounded_load"]["observed_at_epoch"] = now.saturating_sub(86_401).into();
    write_json(root.join("stale-draft.json"), &stale);
    let stale_error = assemble_production_evidence(ProductionEvidenceAssemblyOptions {
        version: CURRENT_MILESTONE,
        release_version,
        evidence_root: root,
        draft_path: Path::new("stale-draft.json"),
        receipt_directory: Path::new("stale-receipts"),
        manifest_path: Path::new("stale-manifest.json"),
    })
    .unwrap_err()
    .to_string();
    assert!(stale_error.contains("future-dated, zero or stale"));

    fs::write(
        root.join("untyped-source.json"),
        b"{\"kind\":\"bounded_load\",\"status\":\"passed\"}\n",
    )
    .unwrap();
    let mut untyped = draft.clone();
    untyped["sources"]["bounded_load"]["artifact_path"] = "untyped-source.json".into();
    write_json(root.join("untyped-draft.json"), &untyped);
    let untyped_error = assemble_production_evidence(ProductionEvidenceAssemblyOptions {
        version: CURRENT_MILESTONE,
        release_version,
        evidence_root: root,
        draft_path: Path::new("untyped-draft.json"),
        receipt_directory: Path::new("untyped-receipts"),
        manifest_path: Path::new("untyped-manifest.json"),
    })
    .unwrap_err()
    .to_string();
    assert!(untyped_error.contains("not a typed attestation"));

    let bounded_source: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("source-bounded_load.json")).unwrap()).unwrap();
    let mut test_source = bounded_source.clone();
    test_source["execution_mode"] = "test".into();
    write_json(root.join("test-mode-source.json"), &test_source);
    let mut test_mode = draft.clone();
    test_mode["sources"]["bounded_load"]["artifact_path"] = "test-mode-source.json".into();
    write_json(root.join("test-mode-draft.json"), &test_mode);
    let test_mode_error = assemble_production_evidence(ProductionEvidenceAssemblyOptions {
        version: CURRENT_MILESTONE,
        release_version,
        evidence_root: root,
        draft_path: Path::new("test-mode-draft.json"),
        receipt_directory: Path::new("test-mode-receipts"),
        manifest_path: Path::new("test-mode-manifest.json"),
    })
    .unwrap_err()
    .to_string();
    assert!(test_mode_error.contains("not collected in production mode"));

    let mut mismatched_source = bounded_source;
    mismatched_source["claims"]["error_count"] = 1.into();
    write_json(root.join("mismatched-source.json"), &mismatched_source);
    let mut mismatched = draft.clone();
    mismatched["sources"]["bounded_load"]["artifact_path"] = "mismatched-source.json".into();
    write_json(root.join("mismatched-draft.json"), &mismatched);
    let mismatched_error = assemble_production_evidence(ProductionEvidenceAssemblyOptions {
        version: CURRENT_MILESTONE,
        release_version,
        evidence_root: root,
        draft_path: Path::new("mismatched-draft.json"),
        receipt_directory: Path::new("mismatched-receipts"),
        manifest_path: Path::new("mismatched-manifest.json"),
    })
    .unwrap_err()
    .to_string();
    assert!(mismatched_error.contains("claims do not match"));

    let mut duplicate = draft.clone();
    duplicate["sources"]["release_artifacts"]["artifact_path"] =
        duplicate["sources"]["release_matrix"]["artifact_path"].clone();
    write_json(root.join("duplicate-source-draft.json"), &duplicate);
    let duplicate_error = assemble_production_evidence(ProductionEvidenceAssemblyOptions {
        version: CURRENT_MILESTONE,
        release_version,
        evidence_root: root,
        draft_path: Path::new("duplicate-source-draft.json"),
        receipt_directory: Path::new("duplicate-source-receipts"),
        manifest_path: Path::new("duplicate-source-manifest.json"),
    })
    .unwrap_err()
    .to_string();
    assert!(duplicate_error.contains("reuses another source artifact"));

    symlink(
        "source-bounded_load.json",
        root.join("bounded-load-link.json"),
    )
    .unwrap();
    let mut symlinked = draft.clone();
    symlinked["sources"]["bounded_load"]["artifact_path"] = "bounded-load-link.json".into();
    write_json(root.join("symlink-draft.json"), &symlinked);
    let symlink_error = format!(
        "{:#}",
        assemble_production_evidence(ProductionEvidenceAssemblyOptions {
            version: CURRENT_MILESTONE,
            release_version,
            evidence_root: root,
            draft_path: Path::new("symlink-draft.json"),
            receipt_directory: Path::new("symlink-receipts"),
            manifest_path: Path::new("symlink-manifest.json"),
        })
        .unwrap_err()
    );
    assert!(symlink_error.contains("regular non-symlink"));

    let secret = "sk-proj-abcdefghijklmnopqrstuvwxyzABCDEF1234567890";
    fs::write(
        root.join("secret-source.json"),
        format!("{{\"api_token\":\"{secret}\"}}\n"),
    )
    .unwrap();
    let mut secret_bearing = draft.clone();
    secret_bearing["sources"]["bounded_load"]["artifact_path"] = "secret-source.json".into();
    write_json(root.join("secret-draft.json"), &secret_bearing);
    let secret_error = assemble_production_evidence(ProductionEvidenceAssemblyOptions {
        version: CURRENT_MILESTONE,
        release_version,
        evidence_root: root,
        draft_path: Path::new("secret-draft.json"),
        receipt_directory: Path::new("secret-receipts"),
        manifest_path: Path::new("secret-manifest.json"),
    })
    .unwrap_err()
    .to_string();
    assert!(secret_error.contains("detected secret material"));
    assert!(!secret_error.contains(secret));

    let assembled = assemble_production_evidence(ProductionEvidenceAssemblyOptions {
        version: CURRENT_MILESTONE,
        release_version,
        evidence_root: root,
        draft_path: Path::new("production-evidence-draft.json"),
        receipt_directory: Path::new("receipts-v2"),
        manifest_path: Path::new("production-readiness.json"),
    })
    .unwrap();
    assert_eq!(assembled.status, "assembled");
    assert_eq!(assembled.receipt_count, 13);
    assert_eq!(assembled.source_artifact_count, 13);
    assert_eq!(assembled.files_written, 14);
    let bounded_receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("receipts-v2/bounded_load.json")).unwrap())
            .unwrap();
    assert_eq!(
        bounded_receipt["schema_version"],
        "foundry.milestone.production_evidence.bounded_load.v2"
    );
    assert_eq!(
        bounded_receipt["source_artifact_path"],
        "source-bounded_load.json"
    );
    assert!(bounded_receipt["source_artifact_sha256"]
        .as_str()
        .is_some_and(|digest| digest.len() == 64));

    let report = evaluate(root).unwrap();
    assert!(
        report.production_ready,
        "source-bound production readiness gates failed: {:#?}",
        report.gates
    );

    let reassembled = assemble_production_evidence(ProductionEvidenceAssemblyOptions {
        version: CURRENT_MILESTONE,
        release_version,
        evidence_root: root,
        draft_path: Path::new("production-evidence-draft.json"),
        receipt_directory: Path::new("receipts-v2"),
        manifest_path: Path::new("production-readiness.json"),
    })
    .unwrap();
    assert_eq!(reassembled.status, "assembled");
    assert_eq!(reassembled.files_written, 0);

    fs::write(
        root.join("source-bounded_load.json"),
        "{\"kind\":\"bounded-load\",\"status\":\"drifted\"}\n",
    )
    .unwrap();
    let drifted = evaluate(root).unwrap();
    assert!(!drifted.production_ready);
    let bounded_gate = drifted
        .gates
        .iter()
        .find(|gate| gate.id == "bounded_load")
        .unwrap();
    assert!(bounded_gate
        .checks
        .iter()
        .any(|check| check.id == "bounded_load.source.digest" && !check.passed));
}

#[test]
fn production_evidence_assembly_recovers_partial_publish_but_not_committed_corruption() {
    let temp = tempdir().unwrap();
    let root = temp.path();
    let now = now_epoch();
    let release_version = env!("CARGO_PKG_VERSION");
    let manifest = complete_manifest(root, now);
    let draft = completed_evidence_draft(&manifest, now);
    write_json(root.join("production-evidence-draft.json"), &draft);
    let assemble = || {
        assemble_production_evidence(ProductionEvidenceAssemblyOptions {
            version: CURRENT_MILESTONE,
            release_version,
            evidence_root: root,
            draft_path: Path::new("production-evidence-draft.json"),
            receipt_directory: Path::new("receipts-v2"),
            manifest_path: Path::new("production-readiness.json"),
        })
    };

    let first = assemble().unwrap();
    assert_eq!(first.files_written, 14);
    let manifest_path = root.join("production-readiness.json");
    let bounded_path = root.join("receipts-v2/bounded_load.json");
    let alerting_path = root.join("receipts-v2/alerting.json");
    let retained_path = root.join("receipts-v2/release_matrix.json");
    let exact_manifest = fs::read(&manifest_path).unwrap();
    let exact_bounded = fs::read(&bounded_path).unwrap();
    let exact_alerting = fs::read(&alerting_path).unwrap();
    let retained_before = fs::read(&retained_path).unwrap();

    // Simulate SIGKILL before the commit marker: some exact receipt targets and
    // an orphaned staging file remain, while later receipts and the manifest do not.
    fs::remove_file(&manifest_path).unwrap();
    fs::remove_file(&bounded_path).unwrap();
    fs::remove_file(&alerting_path).unwrap();
    fs::write(
        root.join("receipts-v2/.bounded_load.interrupted.tmp"),
        b"orphaned staging bytes",
    )
    .unwrap();
    let resumed = assemble().unwrap();
    assert_eq!(resumed.files_written, 3);
    assert_eq!(fs::read(&retained_path).unwrap(), retained_before);
    assert_eq!(fs::read(&bounded_path).unwrap(), exact_bounded);
    assert_eq!(fs::read(&alerting_path).unwrap(), exact_alerting);
    assert_eq!(fs::read(&manifest_path).unwrap(), exact_manifest);
    assert!(root
        .join("receipts-v2/.bounded_load.interrupted.tmp")
        .exists());

    let idempotent = assemble().unwrap();
    assert_eq!(idempotent.files_written, 0);

    // A published manifest is the commit marker. Missing files behind it are
    // corruption, not an interrupted pre-commit assembly, and are never repaired.
    fs::remove_file(&bounded_path).unwrap();
    let committed_incomplete_error = assemble().unwrap_err().to_string();
    assert!(committed_incomplete_error.contains("receipt set is incomplete"));
    assert!(!bounded_path.exists());
    assert_eq!(fs::read(&manifest_path).unwrap(), exact_manifest);
    fs::write(&bounded_path, &exact_bounded).unwrap();

    // Without a commit marker, exact bytes may be reused, but divergent bytes
    // remain untouched and block the rerun.
    fs::remove_file(&manifest_path).unwrap();
    fs::write(&bounded_path, b"divergent receipt bytes\n").unwrap();
    let divergent_error = assemble().unwrap_err().to_string();
    assert!(divergent_error.contains("different content"));
    assert_eq!(
        fs::read(&bounded_path).unwrap(),
        b"divergent receipt bytes\n"
    );
    assert!(!manifest_path.exists());

    fs::remove_file(&bounded_path).unwrap();
    fs::write(root.join("expected-bounded-receipt.json"), &exact_bounded).unwrap();
    symlink("../expected-bounded-receipt.json", &bounded_path).unwrap();
    let symlink_error = assemble().unwrap_err().to_string();
    assert!(symlink_error.contains("regular non-symlink"));
    assert_eq!(
        fs::read_link(&bounded_path).unwrap(),
        PathBuf::from("../expected-bounded-receipt.json")
    );
    assert!(!manifest_path.exists());

    fs::remove_file(&bounded_path).unwrap();
    fs::write(&bounded_path, &exact_bounded).unwrap();
    let recovered = assemble().unwrap();
    assert_eq!(recovered.files_written, 1);
    assert_eq!(fs::read(&manifest_path).unwrap(), exact_manifest);

    fs::write(&manifest_path, b"divergent manifest bytes\n").unwrap();
    let divergent_manifest_error = assemble().unwrap_err().to_string();
    assert!(divergent_manifest_error.contains("different content"));
    assert_eq!(
        fs::read(&manifest_path).unwrap(),
        b"divergent manifest bytes\n"
    );
}

fn evaluate(root: &Path) -> anyhow::Result<foundry_core::milestone::ProductionReadinessReport> {
    let store_path = root.join("foundry.sqlite");
    evaluate_with_store(root, &store_path)
}

fn evaluate_with_store(
    root: &Path,
    store_path: &Path,
) -> anyhow::Result<foundry_core::milestone::ProductionReadinessReport> {
    evaluate_production_readiness(ProductionReadinessOptions {
        version: CURRENT_MILESTONE,
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
        milestone: CURRENT_MILESTONE.to_string(),
        profile: CURRENT_PRODUCTION_PROFILE.to_string(),
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
            ops_service_active: true,
            runtime_service_active: true,
            request_supervisor_service_active: true,
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
            foundry_key_isolated_from_uploader: true,
            uploader_credentials_isolated_from_foundry: true,
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
            ops_service_failure_alert: true,
            runtime_service_failure_alert: true,
            request_supervisor_service_failure_alert: true,
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
    let store = FoundryStore::open(root.join("foundry.sqlite")).unwrap();
    let worktree_id = "wt-production-readiness";

    register_worktree(
        &store,
        WorktreeRegisterOptions {
            path: repository.clone(),
            id: Some(worktree_id.to_string()),
            workflow_id: None,
            task_id: None,
            origin: "production-readiness-contract".to_string(),
            created_by_foundry: false,
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
    let cargo_command = install_real_host_toolchain(&repository);
    let config_path = repository.join(".foundry/worktree.toml");
    let mut config: toml::Value =
        toml::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
    config["guardrails"]["allowed_commands"] = toml::Value::Array(vec![
        toml::Value::String(evidence_command.clone()),
        toml::Value::String(cargo_command.clone()),
    ]);
    config["guardrails"]["max_command_seconds"] = toml::Value::Integer(30);
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
        foundry_first_ready: true,
        foundry_first_entrypoint: None,
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
        command: vec![evidence_command.clone()],
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

    for task_index in 1..3 {
        let dispatched = drive_mission(&store, &current.id).unwrap();
        assert_eq!(dispatched.action, "assignment_created");
        let assignment = dispatched.assignment.unwrap();
        let mission = dispatched.mission;
        let (command, requested_evidence) = if task_index == 1 {
            (vec![evidence_command.clone()], Vec::new())
        } else {
            (
                vec![
                    cargo_command.clone(),
                    "test".to_string(),
                    "--offline".to_string(),
                    "--".to_string(),
                    "--nocapture".to_string(),
                ],
                vec![
                    "review_passed".to_string(),
                    "structured_delivery".to_string(),
                    "no_unresolved_risks".to_string(),
                ],
            )
        };
        let mut execution_request = MissionExecutionRequest {
            idempotency_key: format!("production-lifecycle-execution-v{}", task_index + 1),
            mission_id: mission.id.clone(),
            workflow_id: mission.workflow_id.clone(),
            expected_mission_revision: mission.revision,
            task_id: assignment.task.id.clone(),
            agent_id: assignment.agent.instance_id.clone(),
            executor_id: assignment.harness.runtime.clone(),
            worktree: mission.worktree.clone(),
            purpose: "preview".to_string(),
            command,
            requested_evidence,
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
        let task_execution = execute_mission_command(&store, execution_request).unwrap();
        assert_eq!(task_execution.receipt.status, "completed");
        let task_submission = submit_mission(
            &store,
            &mission.id,
            MissionSubmission {
                idempotency_key: format!("production-lifecycle-submission-v{}", task_index + 1),
                execution_receipt_id: task_execution.receipt.receipt_id.clone(),
                task_id: assignment.task.id,
                agent_id: assignment.agent.instance_id,
                status: "completed".to_string(),
                summary: format!("Completed production lifecycle task {}", task_index + 1),
                artifacts: Vec::new(),
                validations: Vec::new(),
                risks: Vec::new(),
                followups: Vec::new(),
                tests_passed: task_execution.receipt.tests_passed,
                tests_failed: task_execution.receipt.tests_failed,
            },
        )
        .unwrap();
        assert_eq!(task_submission.status, "queued");
        let task_resume = resume_mission(&store, &mission.id).unwrap();
        assert_eq!(
            task_resume.action,
            if task_index == 2 {
                "mission_completed"
            } else {
                "handoff_consumed"
            }
        );
    }

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
    fs::create_dir_all(repository.join("src")).unwrap();
    fs::write(
        repository.join("Cargo.toml"),
        concat!(
            "[package]\n",
            "name = \"foundry-production-lifecycle-fixture\"\n",
            "version = \"0.1.0\"\n",
            "edition = \"2021\"\n",
            "\n",
            "[lib]\n",
            "doctest = false\n",
        ),
    )
    .unwrap();
    fs::write(
        repository.join("Cargo.lock"),
        concat!(
            "# This file is automatically @generated by Cargo.\n",
            "# It is not intended for manual editing.\n",
            "version = 3\n",
            "\n",
            "[[package]]\n",
            "name = \"foundry-production-lifecycle-fixture\"\n",
            "version = \"0.1.0\"\n",
        ),
    )
    .unwrap();
    fs::write(
        repository.join("src/lib.rs"),
        concat!(
            "#[cfg(test)]\n",
            "mod tests {\n",
            "    #[test]\n",
            "    fn emits_terminal_gate_evidence() {\n",
            "        println!(\"{}\", r#\"FOUNDRY_GATE_EVIDENCE:",
            "{\"schema_version\":\"foundry.mission.gate_evidence_observation.v1\",",
            "\"evidence\":{\"structured_delivery\":{\"status\":\"completed\",",
            "\"summary\":\"Validated production lifecycle fixture\"},",
            "\"no_unresolved_risks\":true}}\"#);\n",
            "    }\n",
            "}\n",
        ),
    )
    .unwrap();
    let evidence_command = repository.join("fixture-bin/git");
    fs::write(
        &evidence_command,
        concat!(
            "#!/bin/sh\n",
            "printf '%s\\n' 'FOUNDRY_GATE_EVIDENCE:",
            "{\"schema_version\":\"foundry.mission.gate_evidence_observation.v1\",",
            "\"evidence\":{\"requirements_summary\":\"The bounded mission requirements were inspected.\",",
            "\"acceptance_criteria\":[\"Execution, submission and resume must persist atomically.\"]}}'\n"
        ),
    )
    .unwrap();
    fs::set_permissions(&evidence_command, fs::Permissions::from_mode(0o755)).unwrap();

    run_git(repository, &["init", "--initial-branch=main"]);
    run_git(repository, &["add", "."]);
    run_git(
        repository,
        &[
            "-c",
            "user.name=Foundry Production Readiness",
            "-c",
            "user.email=foundry-production-readiness@example.invalid",
            "commit",
            "-m",
            "initial evidence observer",
        ],
    );
    evidence_command.to_string_lossy().into_owned()
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

fn install_real_host_toolchain(repository: &Path) -> String {
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

    let guest_toolchain = repository.join("host-toolchain");
    let guest_bin = guest_toolchain.join("bin");
    fs::create_dir_all(&guest_bin).unwrap();
    fs::hard_link(&cargo, guest_bin.join("cargo")).unwrap();
    fs::hard_link(&rustc, guest_bin.join("rustc")).unwrap();
    hard_link_tree(&toolchain_root.join("lib"), &guest_toolchain.join("lib"));
    fs::create_dir_all(repository.join(".cargo")).unwrap();
    fs::write(
        repository.join(".cargo/config.toml"),
        concat!(
            "[build]\n",
            "rustc = \"/workspace/host-toolchain/bin/rustc\"\n",
            "target-dir = \"/tmp/foundry-target\"\n",
            "rustflags = [\"-C\", \"linker=/usr/bin/cc\"]\n",
        ),
    )
    .unwrap();
    guest_bin.join("cargo").to_string_lossy().into_owned()
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
        let claims = production_readiness_claims_value(manifest, kind).unwrap();
        let claims_sha256 = production_readiness_claims_sha256(manifest, kind).unwrap();
        let source_artifact_path = format!("source-{kind}.json");
        let mut source_bytes = serde_json::to_vec(&serde_json::json!({
            "schema_version": format!(
                "foundry.milestone.production_source_evidence.{kind}.v1"
            ),
            "kind": kind,
            "status": "passed",
            "subject_version": manifest.release_version,
            "observed_at_epoch": observed_at_epoch,
            "execution_mode": "production",
            "producer": "foundry-production-readiness-contract-fixture",
            "claims": claims,
            "evidence": {
                "fixture": "typed source attestation",
                "independent_observation": true
            }
        }))
        .unwrap();
        source_bytes.push(b'\n');
        fs::write(root.join(&source_artifact_path), &source_bytes).unwrap();
        let receipt = serde_json::json!({
            "schema_version": format!("foundry.milestone.production_evidence.{kind}.v2"),
            "kind": kind,
            "status": "passed",
            "subject_version": manifest.release_version,
            "claims_sha256": claims_sha256,
            "source_artifact_path": source_artifact_path,
            "source_artifact_sha256": hex_sha256(&source_bytes),
            "source_observed_at_epoch": observed_at_epoch,
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

fn completed_evidence_draft(
    manifest: &ProductionReadinessManifest,
    observed_at_epoch: u64,
) -> serde_json::Value {
    let mut draft = serde_json::to_value(manifest).unwrap();
    draft["schema_version"] = PRODUCTION_EVIDENCE_DRAFT_SCHEMA_VERSION.into();
    let release = draft["release"].as_object_mut().unwrap();
    for field in [
        "matrix",
        "artifacts",
        "sbom",
        "checksums",
        "sigstore",
        "provenance",
    ] {
        release.remove(field).unwrap();
    }
    for section in [
        "installation",
        "off_host_backup",
        "key_escrow",
        "alerts",
        "restore_drill",
        "upgrade_rollback",
        "bounded_load",
    ] {
        draft[section]
            .as_object_mut()
            .unwrap()
            .remove("evidence")
            .unwrap();
    }
    let sources = [
        ("release_matrix", "source-release_matrix.json"),
        ("release_artifacts", "source-release_artifacts.json"),
        ("release_sbom", "source-release_sbom.json"),
        ("release_checksums", "source-release_checksums.json"),
        ("release_sigstore", "source-release_sigstore.json"),
        ("release_provenance", "source-release_provenance.json"),
        ("installation", "source-installation.json"),
        ("off_host_recovery", "source-off_host_recovery.json"),
        ("key_escrow", "source-key_escrow.json"),
        ("alerting", "source-alerting.json"),
        ("restore_drill", "source-restore_drill.json"),
        ("upgrade_rollback", "source-upgrade_rollback.json"),
        ("bounded_load", "source-bounded_load.json"),
    ]
    .into_iter()
    .map(|(kind, artifact_path)| {
        (
            kind.to_string(),
            serde_json::json!({
                "artifact_path": artifact_path,
                "observed_at_epoch": observed_at_epoch
            }),
        )
    })
    .collect::<serde_json::Map<_, _>>();
    draft["sources"] = serde_json::Value::Object(sources);
    draft
}

fn write_json(path: PathBuf, value: &serde_json::Value) {
    let mut bytes = serde_json::to_vec_pretty(value).unwrap();
    bytes.push(b'\n');
    fs::write(path, bytes).unwrap();
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
