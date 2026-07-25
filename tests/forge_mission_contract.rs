use assert_cmd::Command;
use rusqlite::Connection;
use tempfile::tempdir;

const FORGE_ORIGINALS: [(&str, &str); 16] = [
    ("software-factory", "Software Factory"),
    ("bug-triage", "Bug Triage"),
    ("security-audit", "Security Audit"),
    ("architecture-review", "Architecture Review"),
    ("migration-squad", "Migration Squad"),
    ("incident-response", "Incident Response"),
    ("research-squad", "Research Squad"),
    ("content-studio", "Content Studio"),
    ("crm-operations", "CRM Operations"),
    ("sales-squad", "Sales Squad"),
    ("customer-support", "Customer Support"),
    ("data-analysis", "Data Analysis"),
    ("infrastructure-operations", "Infrastructure Operations"),
    ("product-discovery", "Product Discovery"),
    ("qa-factory", "QA Factory"),
    ("release-squad", "Release Squad"),
];

fn forge() -> Command {
    Command::cargo_bin("forge").expect("forge binary should build")
}

#[test]
fn squad_catalog_exposes_restricted_versioned_original() {
    let output = forge()
        .args(["squad", "catalog", "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["schema_version"], "forge.squad.catalog.v1");
    let squads = json["squads"].as_array().unwrap();
    assert_eq!(squads.len(), FORGE_ORIGINALS.len());
    for (squad, (expected_id, expected_name)) in squads.iter().zip(FORGE_ORIGINALS) {
        assert_eq!(squad["id"], expected_id);
        assert_eq!(squad["name"], expected_name);
        assert_eq!(squad["version"], "1.0.0");
        assert_eq!(squad["distribution"]["origin"], "forge-original");
        assert_eq!(squad["distribution"]["channel"], "stable");
        assert_eq!(squad["distribution"]["signed"], true);
        assert_eq!(squad["distribution"]["trusted"], true);
        assert_eq!(
            squad["distribution"]["signature"],
            format!("forge-original:{expected_id}:1.0.0")
        );
        assert_eq!(squad["lifecycle_policy"]["spawn"], "on_demand");
        assert_eq!(squad["lifecycle_policy"]["scale_to_zero"], true);

        let denied = squad["orchestrator"]["permissions"]["denied_capabilities"]
            .as_array()
            .unwrap();
        for capability in ["shell", "modify_files", "commit", "deploy"] {
            assert!(denied.iter().any(|value| value == capability));
        }
        assert!(squad["orchestrator"]["permissions"]["filesystem_allow"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(squad["orchestrator"]["permissions"]["shell_allow"]
            .as_array()
            .unwrap()
            .is_empty());

        let roster = squad["roster"].as_array().unwrap();
        assert!(!roster.is_empty());
        assert!(roster.iter().all(|member| {
            member["spawn"] == "on_demand"
                && member["min_instances"] == 0
                && member["max_instances"].as_u64().unwrap() > 0
                && !member["skill_policy"]["allowed"]
                    .as_array()
                    .unwrap()
                    .is_empty()
        }));

        let gates = squad["gates"].as_array().unwrap();
        assert!(gates.len() >= 3);
        let mut gate_ids = std::collections::BTreeSet::new();
        assert!(gates.iter().all(|gate| {
            gate_ids.insert(gate["id"].as_str().unwrap())
                && !gate["trigger"].as_str().unwrap().is_empty()
                && !gate["validator"].as_str().unwrap().is_empty()
                && !gate["required_evidence"].as_array().unwrap().is_empty()
                && gate["timeout_action"] == "block"
        }));
    }
}

#[test]
fn originals_install_list_inspect_and_clone_as_immutable_versions() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let install = forge()
        .arg("--store")
        .arg(&store)
        .args(["squad", "install-originals", "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let install_json: serde_json::Value = serde_json::from_slice(&install).unwrap();
    let install_reports = install_json.as_array().unwrap();
    assert_eq!(install_reports.len(), FORGE_ORIGINALS.len());
    assert!(install_reports.iter().all(|report| {
        report["status"] == "installed"
            && report["validation"]["valid"] == true
            && report["composition_sha256"].as_str().unwrap().len() == 64
    }));

    let output = forge()
        .arg("--store")
        .arg(&store)
        .args(["squad", "list", "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let installed = json["squads"].as_array().unwrap();
    assert_eq!(installed.len(), FORGE_ORIGINALS.len());
    let installed_ids = installed
        .iter()
        .map(|squad| squad["id"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    let expected_ids = FORGE_ORIGINALS
        .iter()
        .map(|(id, _)| *id)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(installed_ids, expected_ids);

    let original_inspect = forge()
        .arg("--store")
        .arg(&store)
        .args([
            "squad",
            "inspect",
            "release-squad",
            "--version",
            "1.0.0",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let original_inspect_json: serde_json::Value =
        serde_json::from_slice(&original_inspect).unwrap();
    assert_eq!(original_inspect_json["id"], "release-squad");
    assert_eq!(
        original_inspect_json["distribution"]["signature"],
        "forge-original:release-squad:1.0.0"
    );
    assert!(original_inspect_json["roster"]
        .as_array()
        .unwrap()
        .iter()
        .all(|member| member["spawn"] == "on_demand"));

    forge()
        .arg("--store")
        .arg(&store)
        .args([
            "squad",
            "clone",
            "--source",
            "software-factory",
            "--new-id",
            "project-factory",
            "--new-name",
            "Project Factory",
            "--new-version",
            "1.0.0",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let inspect = forge()
        .arg("--store")
        .arg(&store)
        .args([
            "squad",
            "inspect",
            "project-factory",
            "--version",
            "1.0.0",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let inspect_json: serde_json::Value = serde_json::from_slice(&inspect).unwrap();
    assert_eq!(inspect_json["distribution"]["origin"], "local-fork");
    assert_eq!(inspect_json["distribution"]["auto_update"], false);
}

#[test]
fn bounded_mission_simulation_persists_composition_handoffs_gates_and_repair() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .arg("--store")
        .arg(&store)
        .args([
            "mission",
            "simulate",
            "--goal",
            "Implement a production-safe Rust API",
            "--squad",
            "software-factory",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["schema_version"], "forge.mission.simulation.v1");
    assert_eq!(json["status"], "passed");
    assert_eq!(json["bounded"], true);
    assert_eq!(json["model_execution_performed"], false);
    assert_eq!(json["external_mutation_performed"], false);
    assert_eq!(json["orchestrator_restricted"], true);
    assert_eq!(json["on_demand_spawn_proven"], true);
    assert_eq!(json["event_driven_handoff_proven"], true);
    assert_eq!(json["validation_before_promotion_proven"], true);
    assert_eq!(json["rework_cycle_proven"], true);
    assert_eq!(json["incremental_persistence_proven"], true);
    assert_eq!(json["hierarchy_limits_enforced"], true);
    assert_eq!(json["cost_limits_enforced"], true);
    assert_eq!(json["inbox_wakeup_proven"], true);
    assert!(json["proof_scope"].as_array().unwrap().len() >= 5);
    assert!(json["not_proven"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item == "real model or provider execution"));
    assert_eq!(json["mission"]["status"], "completed");
    assert_eq!(json["mission"]["rework_cycles"], 1);
    assert_eq!(
        json["mission"]["squad_composition_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );

    let mission_id = json["mission"]["id"].as_str().unwrap();
    let inspect = forge()
        .arg("--store")
        .arg(&store)
        .args(["mission", "inspect", mission_id, "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let inspect_json: serde_json::Value = serde_json::from_slice(&inspect).unwrap();
    assert_eq!(inspect_json["id"], mission_id);
    assert!(inspect_json["handoffs"].as_array().unwrap().len() >= 4);
    assert!(inspect_json["events"].as_array().unwrap().len() >= 12);
    assert!(inspect_json["gates"].as_array().unwrap().len() >= 4);
    assert_eq!(
        inspect_json["revision"].as_u64().unwrap(),
        inspect_json["events"].as_array().unwrap().len() as u64
    );
    assert_eq!(
        inspect_json["inbox"].as_array().unwrap().len(),
        inspect_json["handoffs"].as_array().unwrap().len()
    );
    assert!(inspect_json["inbox"]
        .as_array()
        .unwrap()
        .iter()
        .all(|item| {
            item["status"] == "consumed"
                && item["consumed_at"].is_string()
                && item["wakeup_event_sequence"].as_u64().unwrap() > 0
        }));
    let implementation_gates: Vec<&serde_json::Value> = inspect_json["gates"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|gate| gate["gate_id"] == "implementation_validated")
        .collect();
    assert_eq!(implementation_gates.len(), 2);
    assert_eq!(implementation_gates[0]["attempt"], 1);
    assert_eq!(implementation_gates[0]["status"], "failed");
    assert_eq!(implementation_gates[1]["attempt"], 2);
    assert_eq!(implementation_gates[1]["status"], "passed");
    assert_eq!(implementation_gates[1]["supersedes_attempt"], 1);

    let connection = Connection::open(&store).unwrap();
    let persisted_events: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM events WHERE workflow_id = ?1",
            [json["mission"]["workflow_id"].as_str().unwrap()],
            |row| row.get(0),
        )
        .unwrap();
    assert!(persisted_events >= 12);
    let persisted_agents: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM mission_agent_instances WHERE mission_id = ?1",
            [mission_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        persisted_agents,
        inspect_json["agents"].as_array().unwrap().len() as i64
    );
    let accepted_handoffs: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM mission_handoffs WHERE mission_id = ?1 AND status = 'accepted' AND accepted_at IS NOT NULL",
            [mission_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        accepted_handoffs,
        inspect_json["handoffs"].as_array().unwrap().len() as i64
    );
}
