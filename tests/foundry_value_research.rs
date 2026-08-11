use assert_cmd::Command;
use foundry_core::graph::{create_workflow, Workflow};
use foundry_core::intent::parse_intent;
use foundry_core::storage::FoundryStore;
use foundry_core::validation::validate_workflow_structure;
use serde_json::{json, Value};
use std::path::Path;
use tempfile::tempdir;

fn foundry() -> Command {
    Command::cargo_bin("foundry").expect("foundry binary should build")
}

fn write_json(path: &Path, value: Value) {
    std::fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

fn run_json(store: &Path, current_dir: &Path, args: &[&str]) -> Value {
    let output = foundry()
        .current_dir(current_dir)
        .arg("--store")
        .arg(store)
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).unwrap()
}

#[test]
fn legacy_workflows_default_research_contracts_to_absent() {
    let workflow = create_workflow(parse_intent("Deserialize legacy research-free workflow"));
    let mut legacy = serde_json::to_value(workflow).unwrap();
    let object = legacy.as_object_mut().unwrap();
    object.remove("value_contract");
    object.remove("experiment");
    object.remove("gate_decisions");
    object.remove("outcomes");
    object.remove("research_revisions");

    let restored: Workflow = serde_json::from_value(legacy).unwrap();

    assert!(restored.value_contract.is_none());
    assert!(restored.experiment.is_none());
    assert!(restored.gate_decisions.is_empty());
    assert!(restored.outcomes.is_empty());
    assert!(restored.research_revisions.is_empty());
}

#[test]
fn public_value_research_schema_is_valid_json() {
    let schema_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/schemas/value-research-v1.schema.json");
    let schema: Value = serde_json::from_slice(&std::fs::read(schema_path).unwrap()).unwrap();

    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(
        schema["$defs"]["gateDecision"]["properties"]["applied"]["const"],
        false
    );
    assert!(schema["$defs"]["outcomeContractInput"]["properties"]
        .get("reported_executed_policy")
        .is_some());
    assert!(schema["$defs"]["outcomeContractInput"]["properties"]
        .get("executed_policy")
        .is_none());
}

#[test]
fn cli_records_observational_shadow_trace_without_changing_execution() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("foundry.sqlite");
    let planned = run_json(
        &store_path,
        temp.path(),
        &[
            "plan",
            "--goal",
            "Evaluate a value-aware delivery policy",
            "--output",
            "json",
        ],
    );
    let workflow_id = planned["workflow_id"].as_str().unwrap();

    let value_path = temp.path().join("value.json");
    write_json(
        &value_path,
        json!({
            "value_class": "time_critical",
            "measurement_mode": "gross_value_with_separate_losses",
            "expected_value": 100.0,
            "currency": "USD",
            "deadline": "2030-01-01T00:00:00Z",
            "delay_cost": {
                "kind": "deadline_linear",
                "version": "delay-v1",
                "rate_per_second": 0.25
            },
            "opportunity_cost": {
                "counterfactual": "serve a higher-value queued case",
                "method_version": "capacity-counterfactual-v1",
                "estimated_loss": 12.0,
                "evidence_refs": ["artifact:capacity-study"]
            },
            "failure_cost": {
                "method_version": "risk-v1",
                "probability_bps": 500,
                "severe_probability_bps": 25,
                "estimated_loss": 8.0
            },
            "severity": "high",
            "reversibility": "partially_reversible",
            "constraints": {
                "min_quality_bps": 9000,
                "max_severe_failure_probability_bps": 100,
                "hard_constraints": ["do not bypass validation"]
            },
            "accounting": {},
            "policy": {
                "id": "candidate-value-policy",
                "version": "1.0.0",
                "source": "addon"
            }
        }),
    );
    let value_report = run_json(
        &store_path,
        temp.path(),
        &[
            "workflow",
            "set-value-contract",
            "--workflow",
            workflow_id,
            "--spec",
            value_path.to_str().unwrap(),
            "--output",
            "json",
        ],
    );
    assert_eq!(value_report["status"], "workflow_value_contract_set");
    let value_retry = run_json(
        &store_path,
        temp.path(),
        &[
            "workflow",
            "set-value-contract",
            "--workflow",
            workflow_id,
            "--spec",
            value_path.to_str().unwrap(),
            "--output",
            "json",
        ],
    );
    assert_eq!(value_retry["status"], "workflow_value_contract_unchanged");

    let task_ids = {
        let store = FoundryStore::open(&store_path).unwrap();
        store
            .load_workflow(workflow_id)
            .unwrap()
            .tasks
            .into_iter()
            .map(|task| task.id)
            .collect::<Vec<_>>()
    };
    for task_id in &task_ids {
        let duration_report = run_json(
            &store_path,
            temp.path(),
            &[
                "workflow",
                "set-task-duration",
                "--workflow",
                workflow_id,
                "--task",
                task_id,
                "--duration-ms",
                "25",
                "--output",
                "json",
            ],
        );
        assert_eq!(
            duration_report["status"],
            "workflow_task_duration_estimate_set"
        );
    }

    let experiment_path = temp.path().join("experiment.json");
    write_json(
        &experiment_path,
        json!({
            "experiment_id": "exp-value-shadow-v1",
            "arm": "candidate_v1",
            "cohort_id": "cohort-workflow-001",
            "policy": {
                "id": "candidate-value-policy",
                "version": "1.0.0",
                "source": "addon"
            },
            "assignment_method": "randomized",
            "assignment_evidence_refs": ["artifact:assignment-plan-v1"],
            "seed": 42,
            "shadow_mode": true,
            "holdout": false,
            "primary_endpoint": "realized_value",
            "secondary_endpoints": ["direct_cost", "service_time_ms"],
            "kill_conditions": ["quality below registered floor"]
        }),
    );
    let experiment_report = run_json(
        &store_path,
        temp.path(),
        &[
            "workflow",
            "set-experiment",
            "--workflow",
            workflow_id,
            "--spec",
            experiment_path.to_str().unwrap(),
            "--output",
            "json",
        ],
    );
    assert!(experiment_report["experiment"]["shadow_mode"]
        .as_bool()
        .unwrap());
    let experiment_retry = run_json(
        &store_path,
        temp.path(),
        &[
            "workflow",
            "set-experiment",
            "--workflow",
            workflow_id,
            "--spec",
            experiment_path.to_str().unwrap(),
            "--output",
            "json",
        ],
    );
    assert_eq!(
        experiment_retry["status"],
        "workflow_experiment_assignment_unchanged"
    );

    foundry()
        .current_dir(temp.path())
        .arg("--store")
        .arg(&store_path)
        .args([
            "workflow",
            "set-task-duration",
            "--workflow",
            workflow_id,
            "--task",
            "task-001",
            "--duration-ms",
            "30",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("frozen"));

    let replacement_experiment_path = temp.path().join("replacement-experiment.json");
    write_json(
        &replacement_experiment_path,
        json!({
            "experiment_id": "exp-value-shadow-v2",
            "arm": "candidate_v2",
            "cohort_id": "cohort-workflow-002",
            "policy": {
                "id": "candidate-value-policy",
                "version": "2.0.0",
                "source": "addon"
            },
            "assignment_method": "randomized",
            "assignment_evidence_refs": ["artifact:assignment-plan-v2"],
            "seed": 43,
            "shadow_mode": true,
            "holdout": false,
            "primary_endpoint": "realized_value"
        }),
    );
    foundry()
        .current_dir(temp.path())
        .arg("--store")
        .arg(&store_path)
        .args([
            "workflow",
            "set-experiment",
            "--workflow",
            workflow_id,
            "--spec",
            replacement_experiment_path.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("frozen"));

    let policy = json!({
        "id": "candidate-value-policy",
        "version": "1.0.0",
        "source": "addon"
    });
    let common_experiment = json!({
        "experiment_id": "exp-value-shadow-v1",
        "experiment_arm": "candidate_v1",
        "cohort_id": "cohort-workflow-001",
        "seed": 42,
        "applied": false
    });
    let decisions = [
        json!({
            "decision_point": "workflow-admission",
            "gate": "gate0_value_admission",
            "decision": "admit",
            "confidence_bps": 8000,
            "rationale": "positive declared value under hard constraints"
        }),
        json!({
            "decision_point": "task-001-inference-need",
            "task_id": "task-001",
            "gate": "gate1_inference_need",
            "decision": "deterministic",
            "confidence_bps": 7500,
            "rationale": "intent parsing has a deterministic implementation"
        }),
        json!({
            "decision_point": "task-001-resource-selection",
            "task_id": "task-001",
            "gate": "gate2_resource_selection",
            "decision": "select",
            "candidates": [{
                "candidate_id": "foundry-command",
                "resource_type": "command",
                "predicted_success_bps": 9800,
                "predicted_duration_ms": 25,
                "predicted_direct_cost": 0.0001,
                "currency": "USD",
                "predicted_risk_bps": 10
            }],
            "selected_candidate_id": "foundry-command",
            "confidence_bps": 9000,
            "rationale": "authorized deterministic candidate"
        }),
        json!({
            "decision_point": "task-001-assurance",
            "task_id": "task-001",
            "gate": "gate3_assurance",
            "decision": "a2",
            "confidence_bps": 7000,
            "rationale": "high severity warrants independent checks"
        }),
        json!({
            "decision_point": "task-001-terminal-stop",
            "task_id": "task-001",
            "gate": "gate4_stopping",
            "decision": "stop",
            "confidence_bps": 8500,
            "rationale": "best valid result is ready and marginal value is non-positive"
        }),
    ];
    let mut decision_ids = Vec::new();
    for (index, mut decision) in decisions.into_iter().enumerate() {
        decision["idempotency_key"] = json!(format!("exp-value-shadow-v1:gate:{index}"));
        decision["policy"] = policy.clone();
        for (key, value) in common_experiment.as_object().unwrap() {
            decision[key] = value.clone();
        }
        if index > 0 {
            decision["run_id"] = json!("run-research-001");
            decision["lease_id"] = json!("lease-research-001");
            decision["input_hash"] =
                json!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        }
        let path = temp.path().join(format!("gate-{index}.json"));
        write_json(&path, decision);
        let report = run_json(
            &store_path,
            temp.path(),
            &[
                "workflow",
                "record-gate-decision",
                "--workflow",
                workflow_id,
                "--spec",
                path.to_str().unwrap(),
                "--output",
                "json",
            ],
        );
        assert!(!report["receipt"]["applied"].as_bool().unwrap());
        let decision_id = report["receipt"]["decision_id"]
            .as_str()
            .unwrap()
            .to_string();
        if index == 0 {
            let retry = run_json(
                &store_path,
                temp.path(),
                &[
                    "workflow",
                    "record-gate-decision",
                    "--workflow",
                    workflow_id,
                    "--spec",
                    path.to_str().unwrap(),
                    "--output",
                    "json",
                ],
            );
            assert_eq!(retry["status"], "workflow_value_gate_decision_unchanged");
            assert_eq!(retry["receipt"]["decision_id"], decision_id);
        }
        decision_ids.push(decision_id);
    }

    let applied_shadow_path = temp.path().join("applied-shadow.json");
    write_json(
        &applied_shadow_path,
        json!({
            "idempotency_key": "exp-value-shadow-v1:applied-shadow",
            "decision_point": "task-001-forbidden-application",
            "task_id": "task-001",
            "gate": "gate4_stopping",
            "decision": "continue",
            "rationale": "this must remain observational",
            "policy": policy,
            "experiment_id": "exp-value-shadow-v1",
            "experiment_arm": "candidate_v1",
            "cohort_id": "cohort-workflow-001",
            "run_id": "run-research-001",
            "lease_id": "lease-research-001",
            "input_hash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "seed": 42,
            "applied": true
        }),
    );
    foundry()
        .current_dir(temp.path())
        .arg("--store")
        .arg(&store_path)
        .args([
            "workflow",
            "record-gate-decision",
            "--workflow",
            workflow_id,
            "--spec",
            applied_shadow_path.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("observational"));

    let outcome_path = temp.path().join("outcome.json");
    write_json(
        &outcome_path,
        json!({
            "idempotency_key": "exp-value-shadow-v1:outcome:task-001",
            "task_id": "task-001",
            "run_id": "run-research-001",
            "lease_id": "lease-research-001",
            "input_hash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "output_hash": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "measurement_status": "simulated",
            "status": "modeled",
            "process_quality_bps": 9700,
            "artifact_quality_bps": 9500,
            "quality_in_use_bps": 9300,
            "direct_cost": 0.0001,
            "process_cost": 0.0002,
            "assurance_cost": 0.0001,
            "internal_failure_cost": 0.0,
            "external_failure_cost": 0.0,
            "delay_cost": 0.0,
            "opportunity_cost": 0.0,
            "opportunity_cost_counterfactual": "serve a higher-value queued case",
            "opportunity_cost_method_version": "capacity-counterfactual-v1",
            "opportunity_cost_evidence_refs": ["artifact:capacity-study"],
            "realized_value": 80.0,
            "realized_value_method_version": "realized-value-v1",
            "currency": "USD",
            "service_time_ms": 20,
            "queue_time_ms": 2,
            "wait_time_ms": 0,
            "human_time_ms": 0,
            "accepted": true,
            "evaluator": "independent-validator",
            "oracle": "schema-and-fixture",
            "gate_decision_ids": decision_ids,
            "evidence_refs": ["artifact:validated-output"],
            "metric_provenance": {
                "realized_value": "registered-simulation-model-v1",
                "direct_cost": "registered-simulation-model-v1",
                "service_time_ms": "registered-simulation-model-v1",
                "artifact_quality_bps": "registered-simulation-model-v1"
            },
            "evaluated_policy": {
                "id": "candidate-value-policy",
                "version": "1.0.0",
                "source": "addon"
            },
            "experiment_id": "exp-value-shadow-v1",
            "experiment_arm": "candidate_v1",
            "cohort_id": "cohort-workflow-001",
            "seed": 42
        }),
    );
    let outcome_report = run_json(
        &store_path,
        temp.path(),
        &[
            "workflow",
            "record-outcome",
            "--workflow",
            workflow_id,
            "--spec",
            outcome_path.to_str().unwrap(),
            "--output",
            "json",
        ],
    );
    assert_eq!(
        outcome_report["status"],
        "workflow_outcome_contract_recorded"
    );

    let export = run_json(
        &store_path,
        temp.path(),
        &[
            "workflow",
            "export-research",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ],
    );
    assert_eq!(export["schema_version"], "foundry.research_export.v1");
    assert_eq!(export["gate_decisions"].as_array().unwrap().len(), 5);
    assert_eq!(export["outcomes"].as_array().unwrap().len(), 1);
    assert_eq!(export["readiness"]["status"], "declared_trace_complete");
    assert!(export["readiness"]["declared_trace_complete"]
        .as_bool()
        .unwrap());
    assert!(!export["readiness"]["runtime_evidence_verified"]
        .as_bool()
        .unwrap());
    assert!(!export["readiness"]["prospective_gate_capture_ready"]
        .as_bool()
        .unwrap());
    assert!(!export["readiness"]["executed_policy_verified"]
        .as_bool()
        .unwrap());
    assert!(!export["readiness"]["causal_claim_ready"].as_bool().unwrap());
    assert_eq!(export["readiness"]["missing"], json!([]));

    let fabricated_observed_path = temp.path().join("fabricated-observed.json");
    write_json(
        &fabricated_observed_path,
        json!({
            "idempotency_key": "exp-value-shadow-v1:fabricated-observed:task-001",
            "task_id": "task-001",
            "run_id": "run-research-001",
            "lease_id": "lease-research-001",
            "input_hash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "output_hash": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "execution_receipt_sha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            "measurement_status": "observed",
            "status": "accepted",
            "realized_value": 80.0,
            "realized_value_method_version": "realized-value-v1",
            "direct_cost": 0.0001,
            "currency": "USD",
            "service_time_ms": 20,
            "accepted": true,
            "gate_decision_ids": decision_ids,
            "evidence_refs": ["artifact:claimed-runtime-receipt"],
            "metric_provenance": {
                "realized_value": "claimed-runtime-receipt",
                "direct_cost": "claimed-runtime-receipt",
                "service_time_ms": "claimed-runtime-receipt"
            },
            "evaluated_policy": {
                "id": "candidate-value-policy",
                "version": "1.0.0",
                "source": "addon"
            },
            "reported_executed_policy": {
                "id": "foundry-core-baseline",
                "version": "0.6.0",
                "source": "core_baseline"
            },
            "experiment_id": "exp-value-shadow-v1",
            "experiment_arm": "candidate_v1",
            "cohort_id": "cohort-workflow-001",
            "seed": 42
        }),
    );
    foundry()
        .current_dir(temp.path())
        .arg("--store")
        .arg(&store_path)
        .args([
            "workflow",
            "record-outcome",
            "--workflow",
            workflow_id,
            "--spec",
            fabricated_observed_path.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("not persisted"));

    let priority_report = run_json(
        &store_path,
        temp.path(),
        &[
            "workflow",
            "set-priority",
            "--workflow",
            workflow_id,
            "--task",
            "task-001",
            "--priority",
            "low",
            "--output",
            "json",
        ],
    );
    assert_eq!(priority_report["status"], "workflow_task_priority_updated");
    let drifted_export = run_json(
        &store_path,
        temp.path(),
        &[
            "workflow",
            "export-research",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ],
    );
    assert!(!drifted_export["readiness"]["declared_trace_complete"]
        .as_bool()
        .unwrap());
    assert!(drifted_export["readiness"]["missing"]
        .as_array()
        .unwrap()
        .contains(&json!("frozen_task_definition")));

    let store = FoundryStore::open(&store_path).unwrap();
    let workflow = store.load_workflow(workflow_id).unwrap();
    assert!(validate_workflow_structure(&workflow).is_empty());
    assert!(workflow.gate_decisions.is_empty());
    assert!(workflow.outcomes.is_empty());
    assert!(workflow.research_revisions.is_empty());

    let workflow_with_research = store.load_workflow_with_research(workflow_id).unwrap();
    assert!(validate_workflow_structure(&workflow_with_research).is_empty());
    assert_eq!(workflow_with_research.gate_decisions.len(), 5);
    assert_eq!(workflow_with_research.outcomes.len(), 1);
    assert_eq!(workflow_with_research.research_revisions.len(), 6);
    assert!(workflow_with_research
        .gate_decisions
        .iter()
        .all(|decision| !decision.applied));
}
