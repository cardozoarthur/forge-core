use assert_cmd::Command;
use forge_core::addon::{
    apply_addon_validator_outcome, AddonValidatorOutcomeApplyInput,
    ADDON_VALIDATOR_OUTCOME_APPLICATION_SCHEMA_VERSION,
};
use forge_core::graph::{create_workflow, TaskStatus};
use forge_core::intent::parse_intent;
use forge_core::storage::{ForgeStore, RuntimeContractDispatchWrite};
use serde_json::{json, Value};
use std::path::Path;
use tempfile::tempdir;

fn forge() -> Command {
    Command::cargo_bin("forge").expect("forge binary should build")
}

fn setup_workflow(store: &ForgeStore) -> (String, String) {
    let workflow = create_workflow(parse_intent(
        "Build and validate a bounded delivery with explicit rework",
    ));
    let workflow_id = workflow.id.clone();
    let task_id = workflow.tasks.first().unwrap().id.clone();
    store.save_workflow(&workflow).unwrap();
    (workflow_id, task_id)
}

fn seed_validator_dispatch(
    store: &ForgeStore,
    dispatch_id: &str,
    workflow_id: &str,
    task_id: &str,
    decision: &str,
    issues: &[&str],
) {
    let input = json!({
        "schema_version": "forge.addon_validator_dispatch_input.v1",
        "subject": format!("artifact for {task_id}"),
        "workflow_binding": {
            "workflow_id": workflow_id,
            "task_id": task_id,
        },
        "input": {},
        "context": {},
    });
    let policy = json!({"status": "allowed"});
    let data = json!({
        "runtime_processing": {
            "outcome": {
                "result": {
                    "schema_version": "forge.addon_validator_result.v1",
                    "decision": decision,
                    "checks": [{"id": "quality", "status": decision}],
                    "issues": issues,
                }
            }
        }
    });
    store
        .save_runtime_contract_dispatch(RuntimeContractDispatchWrite {
            id: dispatch_id,
            addon_id: "forge.addon.validator-test",
            contract_id: "validator.quality",
            contract_type: "validator",
            capability_id: "quality_validation",
            runtime: "local_process",
            entrypoint: "validator-test",
            status: "completed",
            source: "test",
            input: &input,
            policy: &policy,
            data: &data,
        })
        .unwrap();
}

fn apply(
    store: &ForgeStore,
    dispatch_id: &str,
    workflow_id: &str,
    task_id: &str,
    expected_revision: u64,
) -> forge_core::addon::AddonValidatorOutcomeApplicationReport {
    apply_addon_validator_outcome(
        store,
        AddonValidatorOutcomeApplyInput {
            dispatch_id,
            workflow_id,
            task_id,
            expected_revision,
            origin: "test",
        },
    )
    .unwrap()
}

#[test]
fn passed_outcome_records_revisioned_evidence_without_promoting() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let store = ForgeStore::open(&store_path).unwrap();
    let (workflow_id, task_id) = setup_workflow(&store);
    seed_validator_dispatch(
        &store,
        "dispatch-passed",
        &workflow_id,
        &task_id,
        "passed",
        &[],
    );

    let report = apply(&store, "dispatch-passed", &workflow_id, &task_id, 0);
    assert_eq!(
        report.schema_version,
        ADDON_VALIDATOR_OUTCOME_APPLICATION_SCHEMA_VERSION
    );
    assert_eq!(report.status, "addon_validator_outcome_evidence_recorded");
    assert_eq!(report.action, "record_evidence");
    assert!(report.application_changed);
    assert!(!report.workflow_changed);
    assert_eq!(report.previous_revision, 0);
    assert_eq!(report.workflow_revision, 1);

    let workflow = store.load_workflow(&workflow_id).unwrap();
    let task = workflow
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .unwrap();
    assert_eq!(task.status, TaskStatus::Pending);
    assert!(!task.work_item.goal_validation.definitively_ready);
    assert_eq!(workflow.revisions.len(), 1);
    assert_eq!(
        workflow.revisions[0].change_type,
        "addon_validator_evidence_recorded"
    );
}

#[test]
fn failed_outcome_creates_correlated_rework_impediment() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let store = ForgeStore::open(&store_path).unwrap();
    let (workflow_id, task_id) = setup_workflow(&store);
    seed_validator_dispatch(
        &store,
        "dispatch-failed",
        &workflow_id,
        &task_id,
        "failed",
        &["contract test failed"],
    );

    let report = apply(&store, "dispatch-failed", &workflow_id, &task_id, 0);
    assert_eq!(report.status, "addon_validator_outcome_rework_required");
    assert_eq!(report.action, "require_rework");
    assert!(report.workflow_changed);
    assert!(report.impediment_id.is_some());
    assert_eq!(report.feedback, vec!["contract test failed"]);

    let workflow = store.load_workflow(&workflow_id).unwrap();
    let task = workflow
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .unwrap();
    assert_eq!(task.status, TaskStatus::Blocked);
    assert!(!task.work_item.goal_validation.definitively_ready);
    assert_eq!(task.active_impediments.len(), 1);
    assert!(task.active_impediments[0]
        .reason
        .contains("contract test failed"));
    assert!(task.active_impediments[0]
        .origin
        .contains("dispatch-failed"));
}

#[test]
fn review_required_outcome_creates_revisioned_human_gate() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let store = ForgeStore::open(&store_path).unwrap();
    let (workflow_id, task_id) = setup_workflow(&store);
    seed_validator_dispatch(
        &store,
        "dispatch-review",
        &workflow_id,
        &task_id,
        "review_required",
        &["confidence below policy threshold"],
    );

    let report = apply(&store, "dispatch-review", &workflow_id, &task_id, 0);
    assert_eq!(
        report.status,
        "addon_validator_outcome_human_review_required"
    );
    assert_eq!(report.action, "request_human_review");
    assert!(report.interaction_id.is_some());
    assert_eq!(report.workflow_revision, 1);

    let workflow = store.load_workflow(&workflow_id).unwrap();
    let task = workflow
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .unwrap();
    assert_eq!(task.status, TaskStatus::Blocked);
    assert!(task.human_required);
    let interaction = task.human_interaction.as_ref().unwrap();
    assert_eq!(interaction.state, "pending");
    assert_eq!(interaction.interaction_id, report.interaction_id.unwrap());
    assert!(interaction.origin.contains("dispatch-review"));
}

#[test]
fn applying_the_same_dispatch_is_idempotent_even_with_the_original_revision() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let store = ForgeStore::open(&store_path).unwrap();
    let (workflow_id, task_id) = setup_workflow(&store);
    seed_validator_dispatch(
        &store,
        "dispatch-idempotent",
        &workflow_id,
        &task_id,
        "failed",
        &["retry the bounded check"],
    );

    let first = apply(&store, "dispatch-idempotent", &workflow_id, &task_id, 0);
    let replay = apply(&store, "dispatch-idempotent", &workflow_id, &task_id, 0);
    assert!(first.application_changed);
    assert!(!replay.application_changed);
    assert_eq!(replay.status, "addon_validator_outcome_already_applied");
    assert_eq!(replay.workflow_revision, first.workflow_revision);

    let workflow = store.load_workflow(&workflow_id).unwrap();
    let task = workflow
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .unwrap();
    assert_eq!(task.active_impediments.len(), 1);
    assert_eq!(workflow.revisions.len(), 1);
    let applied_events = store
        .load_workflow_events(&workflow_id)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == "addon_validator_outcome_applied")
        .count();
    assert_eq!(applied_events, 1);
}

#[test]
fn stale_revision_and_binding_mismatch_fail_closed() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let store = ForgeStore::open(&store_path).unwrap();
    let (workflow_id, task_id) = setup_workflow(&store);
    seed_validator_dispatch(
        &store,
        "dispatch-first",
        &workflow_id,
        &task_id,
        "passed",
        &[],
    );
    seed_validator_dispatch(
        &store,
        "dispatch-stale",
        &workflow_id,
        &task_id,
        "passed",
        &[],
    );
    apply(&store, "dispatch-first", &workflow_id, &task_id, 0);

    let stale = apply_addon_validator_outcome(
        &store,
        AddonValidatorOutcomeApplyInput {
            dispatch_id: "dispatch-stale",
            workflow_id: &workflow_id,
            task_id: &task_id,
            expected_revision: 0,
            origin: "test",
        },
    )
    .unwrap_err();
    assert!(stale.to_string().contains("stale workflow revision"));

    let mismatch = apply_addon_validator_outcome(
        &store,
        AddonValidatorOutcomeApplyInput {
            dispatch_id: "dispatch-stale",
            workflow_id: &workflow_id,
            task_id: "task-not-bound",
            expected_revision: 1,
            origin: "test",
        },
    )
    .unwrap_err();
    assert!(mismatch.to_string().contains("is bound to workflow"));
}

#[test]
fn tenant_policy_is_enforced_before_outcome_application() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let store = ForgeStore::open(&store_path).unwrap();
    let (workflow_id, task_id) = setup_workflow(&store);
    let mut workflow = store.load_workflow(&workflow_id).unwrap();
    workflow.intent.operating_context.tenant_policy_mode = "enforce".to_string();
    workflow.intent.operating_context.organization.id = "validator-org".to_string();
    workflow.intent.operating_context.brand.id = "validator-brand".to_string();
    workflow.intent.operating_context.product.id = "validator-product".to_string();
    workflow.intent.operating_context.user.id = "validator-user".to_string();
    store.save_workflow(&workflow).unwrap();
    seed_validator_dispatch(
        &store,
        "dispatch-policy-denied",
        &workflow_id,
        &task_id,
        "passed",
        &[],
    );

    let error = apply_addon_validator_outcome(
        &store,
        AddonValidatorOutcomeApplyInput {
            dispatch_id: "dispatch-policy-denied",
            workflow_id: &workflow_id,
            task_id: &task_id,
            expected_revision: 0,
            origin: "test",
        },
    )
    .unwrap_err();
    assert!(error
        .to_string()
        .contains("multi-tenant enforcement blocked apply Addon validator outcome"));
}

fn setup_cli_fixture(store_path: &Path, dispatch_id: &str) -> (String, String) {
    let store = ForgeStore::open(store_path).unwrap();
    let (workflow_id, task_id) = setup_workflow(&store);
    seed_validator_dispatch(&store, dispatch_id, &workflow_id, &task_id, "passed", &[]);
    (workflow_id, task_id)
}

#[test]
fn cli_and_mcp_expose_the_same_validator_outcome_contract() {
    let cli_temp = tempdir().unwrap();
    let cli_store = cli_temp.path().join("forge.sqlite");
    let (cli_workflow, cli_task) = setup_cli_fixture(&cli_store, "dispatch-cli");
    let cli_output = forge()
        .args([
            "--store",
            cli_store.to_str().unwrap(),
            "addons",
            "apply-validator-outcome",
            "--dispatch",
            "dispatch-cli",
            "--workflow",
            &cli_workflow,
            "--task",
            &cli_task,
            "--expected-revision",
            "0",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let cli_json: Value = serde_json::from_slice(&cli_output).unwrap();
    assert_eq!(
        cli_json["schema_version"],
        ADDON_VALIDATOR_OUTCOME_APPLICATION_SCHEMA_VERSION
    );
    assert_eq!(cli_json["action"], "record_evidence");

    let mcp_temp = tempdir().unwrap();
    let mcp_store = mcp_temp.path().join("forge.sqlite");
    let (mcp_workflow, mcp_task) = setup_cli_fixture(&mcp_store, "dispatch-mcp");
    let input = json!({
        "dispatch_id": "dispatch-mcp",
        "workflow_id": mcp_workflow,
        "task_id": mcp_task,
        "expected_revision": 0,
    });
    let mcp_output = forge()
        .args([
            "--store",
            mcp_store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.addons.apply_validator_outcome",
            "--input",
            &input.to_string(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_json: Value = serde_json::from_slice(&mcp_output).unwrap();
    assert_eq!(
        mcp_json["result"]["schema_version"],
        ADDON_VALIDATOR_OUTCOME_APPLICATION_SCHEMA_VERSION
    );
    assert_eq!(mcp_json["result"]["action"], "record_evidence");

    let manifest_output = forge()
        .args(["mcp", "tools", "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let manifest: Value = serde_json::from_slice(&manifest_output).unwrap();
    let tool = manifest["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "forge.addons.apply_validator_outcome")
        .unwrap();
    assert_eq!(
        tool["output_schema"],
        ADDON_VALIDATOR_OUTCOME_APPLICATION_SCHEMA_VERSION
    );
    assert_eq!(tool["mutates_workflow"], true);
}
