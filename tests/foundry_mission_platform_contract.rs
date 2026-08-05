use assert_cmd::Command;
use foundry_core::mission::simulate_mission;
use foundry_core::mission_platform::simulate_mission_platform_with_store;
use foundry_core::storage::FoundryStore;
use foundry_core::worktree::{register_worktree, WorktreeRegisterOptions};
use std::path::PathBuf;
use tempfile::tempdir;

fn foundry() -> Command {
    Command::cargo_bin("foundry").expect("foundry binary should build")
}

#[test]
fn capability_catalog_exposes_all_forty_numbered_backend_contracts() {
    let output = foundry()
        .args(["squad", "capabilities", "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        json["schema_version"],
        "foundry.mission_platform.catalog.v1"
    );
    assert_eq!(json["status"], "classified_not_production_ready");
    assert_eq!(json["capability_count"], 40);
    assert_eq!(json["production_ready"], false);
    assert_eq!(json["inventory_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(json["capabilities"][0]["number"], 1);
    assert_eq!(json["capabilities"][39]["number"], 40);
    assert_eq!(
        json["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|capability| capability["number"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        (1..=40).collect::<Vec<_>>()
    );
    assert_eq!(
        json["proof_kind_counts"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["bounded_simulation", "contract_only", "runtime_real"]
    );
    assert_eq!(json["proof_kind_counts"]["runtime_real"], 20);
    assert_eq!(json["proof_kind_counts"]["bounded_simulation"], 14);
    assert_eq!(json["proof_kind_counts"]["contract_only"], 6);
    assert!(json["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .all(|capability| capability["production_ready"] == false
            && capability["production_gap"]
                .as_str()
                .is_some_and(|gap| !gap.is_empty())));
}

#[test]
fn bounded_platform_simulation_fails_closed_without_a_worktree_binding() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("foundry.sqlite");
    let output = foundry()
        .arg("--store")
        .arg(&store)
        .args([
            "mission",
            "simulate-platform",
            "--goal",
            "Validate every mission platform backend capability",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        json["schema_version"],
        "foundry.mission_platform.simulation.v1"
    );
    assert_eq!(json["status"], "failed");
    assert_eq!(json["evidence_scope"], "bounded_simulation");
    assert_eq!(json["bounded"], true);
    assert_eq!(json["model_execution_performed"], false);
    assert_eq!(json["external_mutation_performed"], false);
    assert_eq!(json["production_ready"], false);
    assert_eq!(json["capability_count"], 40);
    assert_eq!(json["passed_count"], 39);
    assert_eq!(json["failed_count"], 1);
    assert_eq!(json["probes"].as_array().unwrap().len(), 40);
    let failed = json["probes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|probe| probe["passed"] == false)
        .collect::<Vec<_>>();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0]["capability_id"], "mission_worktree");
    assert_eq!(
        failed[0]["evidence"]["verification"],
        "effect_dependency_or_receipt_missing"
    );
    assert!(json["probes"].as_array().unwrap().iter().all(|probe| {
        probe["evidence"]["execution_class"].is_string()
            && (probe["capability_id"] == "mission_worktree"
                || (probe["evidence"]["receipt"].is_object()
                    && probe["evidence"]["receipt"]["input_sha256"]
                        .as_str()
                        .is_some_and(|hash| hash.len() == 64)
                    && probe["evidence"]["receipt"]["result_sha256"]
                        .as_str()
                        .is_some_and(|hash| hash.len() == 64)))
    }));
    assert!(json["not_proven"].as_array().unwrap().len() >= 4);
}

#[test]
fn registered_worktree_binding_receipt_allows_the_api_and_cli_probe_to_pass() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("foundry.sqlite");
    let store = FoundryStore::open(&store_path).unwrap();
    let mut mission = simulate_mission(
        &store,
        "Validate every bounded mission platform effect",
        "software-factory",
        None,
        true,
    )
    .unwrap();
    let registration = register_worktree(
        &store,
        WorktreeRegisterOptions {
            path: PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            id: None,
            workflow_id: Some(mission.mission.workflow_id.clone()),
            task_id: None,
            origin: "mission-platform-contract-test".to_string(),
            created_by_foundry: false,
        },
    )
    .unwrap();
    assert!(registration.binding.is_some());
    mission.mission.worktree = Some(registration.worktree.worktree_root);

    let report = simulate_mission_platform_with_store(&store, &mission);
    assert_eq!(report.status, "passed");
    assert_eq!(report.passed_count, 40);
    assert_eq!(report.failed_count, 0);
    let probe = report
        .probes
        .iter()
        .find(|probe| probe.capability_id == "mission_worktree")
        .unwrap();
    assert!(probe.passed);
    assert_eq!(probe.proof_scope, "bounded_simulation");
    assert_eq!(probe.evidence.result["bound"], true);
    assert_eq!(
        probe.evidence.receipt.as_ref().unwrap().schema_version,
        "foundry.mission_platform.effect_receipt.v1"
    );

    let output = foundry()
        .arg("--store")
        .arg(&store_path)
        .args([
            "mission",
            "simulate-platform",
            "--goal",
            "Validate every mission platform backend capability",
            "--worktree",
        ])
        .arg(env!("CARGO_MANIFEST_DIR"))
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["status"], "passed");
    assert_eq!(json["passed_count"], 40);
    assert_eq!(json["failed_count"], 0);
    assert_eq!(
        json["inventory_sha256"],
        "14c6064fa9f2618647e8bfc1664b1af15c38ab5596c9a8ee8b21921610a486ad"
    );
    assert_eq!(json["production_ready"], false);
}
