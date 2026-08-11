use super::*;
use crate::graph::{create_workflow, TaskStatus};
use crate::intent::parse_intent;
use chrono::Utc;
use std::collections::BTreeMap;

fn policy() -> PolicyRef {
    PolicyRef {
        id: "value-aware-baseline".to_string(),
        version: "1.0.0".to_string(),
        source: PolicySource::Addon,
    }
}

fn contract() -> ValueContract {
    ValueContract {
        schema_version: VALUE_CONTRACT_SCHEMA_VERSION.to_string(),
        value_class: "time_critical".to_string(),
        measurement_mode: ValueMeasurementMode::GrossValueWithSeparateLosses,
        expected_value: Some(100.0),
        currency: Some("USD".to_string()),
        deadline: Some(Utc::now()),
        delay_cost: Some(DelayCostSpec {
            kind: DelayFunctionKind::DeadlineLinear,
            version: "delay-v1".to_string(),
            threshold_at: None,
            rate_per_second: Some(0.5),
            quadratic_rate_per_second_squared: None,
            fixed_penalty: None,
            external_model_ref: None,
        }),
        opportunity_cost: Some(OpportunityCostSpec {
            counterfactual: "serve the critical incident instead".to_string(),
            method_version: "counterfactual-v1".to_string(),
            estimated_loss: Some(20.0),
            evidence_refs: vec!["artifact:capacity-study".to_string()],
        }),
        failure_cost: Some(FailureCostSpec {
            method_version: "risk-v1".to_string(),
            probability_bps: Some(500),
            severe_probability_bps: Some(50),
            estimated_loss: Some(10.0),
            evidence_refs: Vec::new(),
        }),
        severity: "high".to_string(),
        reversibility: "partially_reversible".to_string(),
        constraints: ValueConstraints {
            min_quality_bps: Some(9_000),
            ..ValueConstraints::default()
        },
        accounting: ValueAccountingBoundary::default(),
        policy: policy(),
        evidence_refs: Vec::new(),
    }
}

#[test]
fn valid_contract_keeps_losses_separate() {
    assert!(validate_value_contract(&contract()).is_empty());
}

#[test]
fn embedded_delay_cannot_be_counted_again() {
    let mut contract = contract();
    contract.measurement_mode = ValueMeasurementMode::TerminalValue;
    contract.accounting.terminal_value_includes_delay = true;
    let violations = validate_value_contract(&contract);
    assert!(violations
        .iter()
        .any(|violation| violation.contains("delay_cost must be omitted")));
}

#[test]
fn constrained_contract_with_money_requires_currency() {
    let mut contract = contract();
    contract.measurement_mode = ValueMeasurementMode::ConstrainedMulticriteria;
    contract.expected_value = None;
    contract.currency = None;
    contract.delay_cost = None;
    contract.opportunity_cost = Some(OpportunityCostSpec {
        counterfactual: "use the constrained capacity elsewhere".to_string(),
        method_version: "counterfactual-v1".to_string(),
        estimated_loss: Some(20.0),
        evidence_refs: vec!["artifact:capacity-study".to_string()],
    });

    assert!(validate_value_contract(&contract)
        .iter()
        .any(|violation| violation.contains("currency is required whenever")));
}

#[test]
fn gate_two_selection_requires_declared_candidate() {
    let input = GateDecisionInput {
        idempotency_key: "gate-two-selection".to_string(),
        decision_point: "initial_selection".to_string(),
        task_id: Some("task-001".to_string()),
        run_id: None,
        lease_id: None,
        input_hash: None,
        gate: ValueGate::Gate2ResourceSelection,
        decision: "select".to_string(),
        candidates: Vec::new(),
        selected_candidate_id: Some("codex".to_string()),
        confidence_bps: Some(8_000),
        rationale: "best calibrated candidate".to_string(),
        policy: policy(),
        cohort_id: None,
        experiment_id: None,
        experiment_arm: None,
        seed: None,
        applied: true,
        evidence_refs: Vec::new(),
        hard_constraint_violations: Vec::new(),
    };
    let violations = validate_gate_decision_input(&input);
    assert!(violations
        .iter()
        .any(|violation| violation.contains("not present in candidates")));
}

#[test]
fn observed_outcome_requires_evidence() {
    let input = OutcomeContractInput {
        idempotency_key: "observed-outcome".to_string(),
        task_id: None,
        run_id: None,
        lease_id: None,
        input_hash: None,
        output_hash: None,
        execution_receipt_sha256: None,
        measurement_status: OutcomeMeasurementStatus::Observed,
        status: OutcomeStatus::Accepted,
        process_quality_bps: None,
        artifact_quality_bps: None,
        quality_in_use_bps: None,
        direct_cost: None,
        process_cost: None,
        assurance_cost: None,
        internal_failure_cost: None,
        external_failure_cost: None,
        delay_cost: None,
        opportunity_cost: None,
        opportunity_cost_counterfactual: None,
        opportunity_cost_method_version: None,
        opportunity_cost_evidence_refs: Vec::new(),
        realized_value: None,
        realized_value_method_version: None,
        currency: None,
        service_time_ms: None,
        queue_time_ms: None,
        wait_time_ms: None,
        human_time_ms: None,
        capacity_units: None,
        escaped_defect: None,
        accepted: Some(true),
        evaluator: None,
        oracle: None,
        gate_decision_ids: Vec::new(),
        evidence_refs: vec![" ".to_string()],
        artifact_ids: Vec::new(),
        metric_provenance: BTreeMap::new(),
        evaluated_policy: None,
        reported_executed_policy: Some(policy()),
        cohort_id: None,
        experiment_id: None,
        experiment_arm: None,
        seed: None,
    };
    assert!(validate_outcome_input(&input)
        .iter()
        .any(|violation| violation.contains("evidence_ref")));
}

#[test]
fn monetary_candidate_prediction_requires_currency() {
    let input: GateDecisionInput = serde_json::from_value(serde_json::json!({
        "idempotency_key": "monetary-candidate",
        "decision_point": "initial_selection",
        "task_id": "task-001",
        "gate": "gate2_resource_selection",
        "decision": "select",
        "candidates": [{
            "candidate_id": "candidate-a",
            "resource_type": "command",
            "predicted_direct_cost": 1.0
        }],
        "selected_candidate_id": "candidate-a",
        "rationale": "candidate under test",
        "policy": {"id": "policy-a", "version": "1", "source": "addon"}
    }))
    .unwrap();

    assert!(validate_gate_decision_input(&input)
        .iter()
        .any(|violation| violation.contains("currency")));
}

#[test]
fn custom_gate_decision_requires_named_policy_decision_and_evidence() {
    let input: GateDecisionInput = serde_json::from_value(serde_json::json!({
        "idempotency_key": "custom-gate",
        "decision_point": "initial_assurance",
        "task_id": "task-001",
        "gate": "gate3_assurance",
        "decision": "custom:",
        "rationale": "custom decision under test",
        "policy": {"id": "policy-a", "version": "1", "source": "addon"}
    }))
    .unwrap();
    let violations = validate_gate_decision_input(&input);

    assert!(violations
        .iter()
        .any(|violation| violation.contains("non-empty name")));
    assert!(violations
        .iter()
        .any(|violation| violation.contains("evidence_ref")));
}

#[test]
fn isolated_experiment_seed_is_rejected() {
    let input: GateDecisionInput = serde_json::from_value(serde_json::json!({
        "idempotency_key": "isolated-seed",
        "decision_point": "final_stop",
        "task_id": "task-001",
        "gate": "gate4_stopping",
        "decision": "stop",
        "rationale": "seed linkage under test",
        "policy": {"id": "policy-a", "version": "1", "source": "addon"},
        "seed": 42
    }))
    .unwrap();

    assert!(validate_gate_decision_input(&input)
        .iter()
        .any(|violation| violation.contains("seed requires")));
}

#[test]
fn randomized_assignment_requires_seed_and_evidence() {
    let input: ExperimentAssignmentInput = serde_json::from_value(serde_json::json!({
        "experiment_id": "experiment-a",
        "arm": "treatment",
        "cohort_id": "cohort-a",
        "policy": {"id": "policy-a", "version": "1", "source": "addon"},
        "assignment_method": "randomized",
        "primary_endpoint": "accepted"
    }))
    .unwrap();

    let violations = validate_experiment_input(&input);
    assert!(violations
        .iter()
        .any(|violation| violation.contains("seed")));
    assert!(violations
        .iter()
        .any(|violation| violation.contains("assignment_evidence_refs")));
}

#[test]
fn gate_four_requires_a_terminal_decision_for_a_declared_trace() {
    assert!(!super::export::gate_decision_satisfies_declared_trace(
        ValueGate::Gate4Stopping,
        "continue"
    ));
    assert!(super::export::gate_decision_satisfies_declared_trace(
        ValueGate::Gate4Stopping,
        "stop"
    ));
}

#[test]
fn governance_inputs_reject_unknown_fields() {
    let result = serde_json::from_value::<ExperimentAssignmentInput>(serde_json::json!({
        "experiment_id": "experiment-a",
        "arm": "shadow",
        "policy": {"id": "policy-a", "version": "1", "source": "addon"},
        "assignment_method": "deterministic",
        "shadow_mod": true,
        "primary_endpoint": "accepted"
    }));

    assert!(result
        .unwrap_err()
        .to_string()
        .contains("unknown field `shadow_mod`"));
}

#[test]
fn outcome_cannot_reintroduce_an_embedded_delay_cost() {
    let mut contract = contract();
    contract.delay_cost = None;
    contract.measurement_mode = ValueMeasurementMode::TerminalValue;
    contract.accounting.terminal_value_includes_delay = true;
    let input: OutcomeContractInput = serde_json::from_value(serde_json::json!({
        "idempotency_key": "embedded-delay",
        "measurement_status": "estimated",
        "status": "modeled",
        "delay_cost": 1.0,
        "currency": "USD"
    }))
    .unwrap();

    assert!(validate_outcome_against_value_contract(&input, &contract)
        .iter()
        .any(|violation| violation.contains("already includes delay")));
}

#[test]
fn protocol_fingerprints_ignore_runtime_state_and_track_static_protocol() {
    let mut workflow = create_workflow(parse_intent("Audit a protocol fingerprint"));
    let task = workflow.tasks.first_mut().unwrap();
    task.work_item.subtasks.push(
        serde_json::from_value(serde_json::json!({
            "id": "subtask-001",
            "title": "Inspect evidence",
            "goal": "Inspect the registered evidence",
            "definition_of_done": ["evidence inspected"],
            "status": "pending"
        }))
        .unwrap(),
    );
    task.schedule = Some(
        serde_json::from_value(serde_json::json!({
            "kind": "cron",
            "cron": "0 * * * *",
            "timezone": "UTC",
            "next_run_at": "2030-01-01T00:00:00Z",
            "missed_run_policy": "run_once",
            "run_history": [],
            "scale_to_zero_when_idle": true
        }))
        .unwrap(),
    );
    task.loop_control = Some(
        serde_json::from_value(serde_json::json!({
            "kind": "loop_over_items",
            "items": ["evidence-a"],
            "max_iterations": 1,
            "condition": null,
            "backoff_policy": null,
            "subflow_mode": "finite_per_item",
            "stop_policy": "all_items_complete",
            "state": "pending"
        }))
        .unwrap(),
    );
    task.child_subflows.push(
        serde_json::from_value(serde_json::json!({
            "workflow_id": "wf_child",
            "task_id": "task-child-001",
            "title": "Child validation",
            "binding_status": "proposed",
            "lifecycle_state": "idle",
            "reuse_key": "child-validation",
            "context_lineage_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "validation_gate": "child_ready",
            "reason": "registered validation subflow"
        }))
        .unwrap(),
    );
    task.human_interaction = Some(
        serde_json::from_value(serde_json::json!({
            "interaction_id": "interaction-001",
            "kind": "approve_reject_refine_combine",
            "prompt": "Approve the registered evidence?",
            "required": true,
            "state": "pending",
            "explanation": "Independent review is required",
            "choices": [{
                "id": "approve",
                "label": "Approve",
                "description": "Accept the evidence",
                "effect": "resume"
            }],
            "form": null,
            "timeout_at": "2030-01-01T00:00:00Z",
            "on_timeout": "keep_blocked",
            "created_at": "2029-12-31T00:00:00Z",
            "origin": "experiment",
            "pending_decision_id": "decision-001",
            "decisions": []
        }))
        .unwrap(),
    );

    let baseline = task_protocol_fingerprints(&workflow).unwrap();
    let workflow_id = workflow.id.clone();
    let task_id = workflow.tasks[0].id.clone();
    let task = workflow.tasks.first_mut().unwrap();
    task.work_item.subtasks[0].status = TaskStatus::Completed;
    let schedule = task.schedule.as_mut().unwrap();
    schedule.next_run_at = Some(Utc::now());
    schedule.run_history.push(
        serde_json::from_value(serde_json::json!({
            "run_id": "run-001",
            "scheduled_at": "2030-01-01T00:00:00Z",
            "started_at": "2030-01-01T00:00:01Z",
            "finished_at": "2030-01-01T00:00:02Z",
            "status": "completed",
            "missed": false,
            "missed_run_policy": "run_once",
            "reconciliation_action": "executed"
        }))
        .unwrap(),
    );
    task.loop_control.as_mut().unwrap().state = "completed".to_string();
    task.child_subflows[0].binding_status = "validated".to_string();
    task.child_subflows[0].lifecycle_state = "scaled_to_zero".to_string();
    let interaction = task.human_interaction.as_mut().unwrap();
    interaction.state = "answered".to_string();
    interaction.pending_decision_id.clear();
    interaction.decisions.push(
        serde_json::from_value(serde_json::json!({
            "decision_id": "decision-001",
            "workflow_id": workflow_id,
            "task_id": task_id,
            "interaction_id": "interaction-001",
            "kind": "approve_reject_refine_combine",
            "status": "answered",
            "origin": "human",
            "selected_options": ["approve"],
            "field_values": {},
            "rationale": "evidence accepted",
            "affected_tasks": [task_id],
            "affected_goals": [],
            "affected_artifacts": [],
            "decided_at": "2030-01-01T00:00:03Z",
            "audit_event": "human_interaction_answered"
        }))
        .unwrap(),
    );

    assert_eq!(task_protocol_fingerprints(&workflow).unwrap(), baseline);

    workflow.tasks[0].human_interaction.as_mut().unwrap().prompt =
        "Reject the registered evidence?".to_string();
    assert_ne!(task_protocol_fingerprints(&workflow).unwrap(), baseline);

    workflow.tasks[0].human_interaction.as_mut().unwrap().prompt =
        "Approve the registered evidence?".to_string();
    workflow.tasks[0]
        .human_interaction
        .as_mut()
        .unwrap()
        .timeout_at = Some(Utc::now());
    assert_ne!(task_protocol_fingerprints(&workflow).unwrap(), baseline);

    let workflow_baseline = workflow_protocol_fingerprint(&workflow).unwrap();
    workflow.goal = "Audit a materially changed workflow protocol".to_string();
    assert_ne!(
        workflow_protocol_fingerprint(&workflow).unwrap(),
        workflow_baseline
    );
}
