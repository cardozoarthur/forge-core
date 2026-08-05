#![recursion_limit = "256"]

use foundry_core::mcp::{call_mcp_tool, mcp_tools_manifest};
use foundry_core::storage::FoundryStore;
use serde_json::{json, Value};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::tempdir;

#[test]
fn production_readiness_mcp_tools_are_explicitly_read_only() {
    let manifest = mcp_tools_manifest();

    let plan = manifest
        .tools
        .iter()
        .find(|tool| tool.name == "foundry.milestone.production_plan")
        .expect("production plan tool should be registered");
    assert_eq!(
        plan.output_schema,
        "foundry.milestone.production_readiness_plan.v1"
    );
    assert!(plan.async_safe);
    assert!(!plan.mutates_workflow);

    let readiness = manifest
        .tools
        .iter()
        .find(|tool| tool.name == "foundry.milestone.production_readiness")
        .expect("production readiness tool should be registered");
    assert_eq!(
        readiness.output_schema,
        "foundry.milestone.production_readiness.v1"
    );
    assert!(readiness.async_safe);
    assert!(!readiness.mutates_workflow);
    assert_eq!(
        readiness.input_schema["required"],
        json!(["manifest", "evidence_root"])
    );
}

#[test]
fn production_plan_mcp_call_never_claims_or_mutates_readiness() {
    let temp = tempdir().unwrap();
    let store = FoundryStore::open(temp.path().join("foundry.sqlite")).unwrap();

    let call = call_mcp_tool(
        &store,
        "foundry.milestone.production_plan",
        json!({"version": "0.5"}),
    )
    .unwrap();

    assert_eq!(
        call.result["schema_version"],
        "foundry.milestone.production_readiness_plan.v1"
    );
    assert_eq!(call.result["evaluation_mode"], "plan_only");
    assert_eq!(call.result["production_ready"], false);
    assert_eq!(call.result["commands_executed"], 0);
    assert_eq!(call.result["mutations_performed"], false);
}

#[test]
fn production_readiness_mcp_requires_inputs_and_returns_fail_closed_report() {
    let temp = tempdir().unwrap();
    let store = FoundryStore::open(temp.path().join("foundry.sqlite")).unwrap();
    let manifest_path = temp.path().join("production-readiness.json");
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&incomplete_manifest()).unwrap(),
    )
    .unwrap();

    let missing_root = call_mcp_tool(
        &store,
        "foundry.milestone.production_readiness",
        json!({"manifest": "production-readiness.json"}),
    )
    .unwrap_err();
    assert!(format!("{missing_root:#}").contains("missing field `evidence_root`"));

    let call = call_mcp_tool(
        &store,
        "foundry.milestone.production_readiness",
        json!({
            "version": "0.5",
            "manifest": "production-readiness.json",
            "evidence_root": temp.path(),
        }),
    )
    .unwrap();

    assert_eq!(
        call.result["schema_version"],
        "foundry.milestone.production_readiness.v1"
    );
    assert_eq!(call.result["evaluation_mode"], "read_only");
    assert_eq!(call.result["production_ready"], false);
    assert_eq!(call.result["decision"], "fail_closed");
    assert_eq!(call.result["commands_executed"], 0);
    assert_eq!(call.result["mutations_performed"], false);
    assert!(call.result["blocked_by"]
        .as_array()
        .unwrap()
        .iter()
        .any(|gate| gate == "release_integrity"));
}

fn incomplete_manifest() -> Value {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    json!({
        "schema_version": "foundry.milestone.production_readiness_manifest.v1",
        "milestone": "0.5",
        "profile": "single_host_linux_v0.5",
        "release_version": env!("CARGO_PKG_VERSION"),
        "generated_at_epoch": now,
        "release": {
            "matrix": evidence("release-matrix.json", now),
            "successful_targets": [],
            "artifacts": evidence("release-artifacts.json", now),
            "binary_sha256_by_target": {},
            "sbom": evidence("release-sbom.json", now),
            "sbom_format": "",
            "sbom_component_count": 0,
            "checksums": evidence("release-checksums.json", now),
            "checksum_entry_count": 0,
            "checksums_verified": false,
            "sigstore": evidence("release-sigstore.json", now),
            "sigstore_verified": false,
            "provenance": evidence("release-provenance.json", now),
            "provenance_verified": false
        },
        "installation": {
            "evidence": evidence("installation.json", now),
            "target": "",
            "installed_version": "",
            "installed_binary_sha256": "",
            "ops_service_active": false,
            "runtime_service_active": false,
            "request_supervisor_service_active": false,
            "store_check_passed": false,
            "ops_authenticated_probe_passed": false,
            "ops_http_status": 0,
            "ops_loopback_only": false
        },
        "off_host_backup": {
            "evidence": evidence("off-host-recovery.json", now),
            "recovery_challenge_epoch": 0,
            "immutable_upload_passed": false,
            "remote_digest_verified": false,
            "download_digest_verified": false,
            "downloaded_store_check_passed": false,
            "disposable_restore_passed": false,
            "restored_store_check_passed": false,
            "off_host_retention_enabled": false,
            "foundry_key_isolated_from_uploader": false,
            "uploader_credentials_isolated_from_foundry": false
        },
        "key_escrow": {
            "evidence": evidence("key-escrow.json", now),
            "encrypted": false,
            "separate_access_control": false,
            "recovery_key_available": false,
            "restore_with_escrowed_key_tested": false,
            "excluded_from_database_backup": false
        },
        "alerts": {
            "evidence": evidence("alerting.json", now),
            "ops_service_failure_alert": false,
            "runtime_service_failure_alert": false,
            "request_supervisor_service_failure_alert": false,
            "store_check_alert": false,
            "backup_timer_alert": false,
            "off_host_failure_alert": false,
            "disk_space_alert": false,
            "backup_age_alert": false,
            "delivery_route_verified": false
        },
        "restore_drill": {
            "evidence": evidence("restore-drill.json", now),
            "drill_epoch": 0,
            "disposable_recovery_host": false,
            "downloaded_store_check_passed": false,
            "restored_store_check_passed": false,
            "canary_workflow_verified": false,
            "ops_authenticated_probe_passed": false,
            "rpo_seconds": 0,
            "rto_seconds": 0
        },
        "upgrade_rollback": {
            "evidence": evidence("upgrade-rollback.json", now),
            "target_version": "",
            "simulation_completed": false,
            "pre_upgrade_backup_verified": false,
            "upgraded_store_check_passed": false,
            "upgraded_ops_health_passed": false,
            "rollback_completed": false,
            "previous_version_store_check_passed": false,
            "previous_version_ops_health_passed": false,
            "target_reinstalled_and_healthy": false
        },
        "bounded_load": {
            "evidence": evidence("bounded-load.json", now),
            "duration_seconds": 0,
            "concurrency": 0,
            "operation_count": 0,
            "error_count": 0,
            "p95_latency_millis": 0,
            "max_rss_bytes": 0,
            "max_rss_limit_bytes": 0,
            "timeout_enforced": false,
            "resource_limit_enforced": false,
            "store_check_passed": false,
            "crash_restart_verified": false
        },
        "mission_operational_lifecycle": {
            "evidence": evidence("mission-operational-lifecycle.json", now),
            "capability_inventory_schema_version": "foundry.mission_platform.catalog.v1",
            "capability_inventory_sha256": "0".repeat(64),
            "capability_numbers": [],
            "mission_id": "mission-incomplete",
            "workflow_id": "workflow-incomplete",
            "task_id": "task-incomplete",
            "agent_id": "agent-incomplete",
        "execute_receipt_schema_version": "foundry.mission.execution_receipt.v3",
            "execute_receipt_id": "execution-incomplete",
            "execute_receipt_sha256": "0".repeat(64),
            "execute_status": "blocked",
            "execution_attempted": false,
            "executed": false,
            "execute_exit_code": null,
            "submit_receipt_schema_version": "foundry.mission.submit.v1",
            "submit_receipt_sha256": "0".repeat(64),
            "submit_status": "missing",
            "submit_queued": false,
            "submitted_execute_receipt_sha256": "0".repeat(64),
            "handoff_id": "handoff-incomplete",
            "inbox_id": "inbox-incomplete",
            "resume_receipt_schema_version": "foundry.mission.drive.v1",
            "resume_receipt_sha256": "0".repeat(64),
            "resume_status": "idle",
            "resume_action": "no_pending_inbox",
            "resumed_handoff_id": "handoff-detached",
            "resume_consumed": false,
            "execute_observed_at_epoch": 0,
            "submit_observed_at_epoch": 0,
            "resume_observed_at_epoch": 0
        }
    })
}

fn evidence(path: &str, observed_at_epoch: u64) -> Value {
    json!({
        "artifact_path": path,
        "artifact_sha256": "0".repeat(64),
        "observed_at_epoch": observed_at_epoch,
    })
}
