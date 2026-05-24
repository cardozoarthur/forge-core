use assert_cmd::Command;
use forge_core::artifact::hex_sha256;
use rusqlite::Connection;
use serde_json::Value;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn forge() -> Command {
    Command::cargo_bin("forge").expect("forge binary should build")
}

#[test]
fn plan_from_human_goal_creates_persistent_atomic_graph() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Create a delivery platform",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["status"], "planned");
    assert!(json["workflow_id"].as_str().unwrap().starts_with("wf_"));
    assert!(json["tasks"].as_array().unwrap().len() >= 7);
    assert!(json["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|task| task["title"] == "Extract requirements"));
    assert!(json["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|task| task["title"] == "Validate build"));
    assert!(store.exists());

    let workflow_id = json["workflow_id"].as_str().unwrap();
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "status",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"pending\""));
}

#[test]
fn plan_supports_autonomous_mixed_workflow_with_cron_non_ai_step_and_email_cost_report() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Execute research now, continue every Friday at 09:00, calculate costs without AI, and email the final workflow cost to finance@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    let tasks = json["tasks"].as_array().unwrap();

    assert!(tasks.iter().any(|task| task["executor"] == "ai"));
    assert!(tasks.iter().any(|task| task["executor"] == "wait"));
    assert!(tasks.iter().any(|task| task["executor"] == "command"));
    assert!(tasks.iter().any(|task| task["executor"] == "notification"));
    assert!(tasks.iter().all(|task| task["human_required"] == false));

    let wait_task = tasks
        .iter()
        .find(|task| task["executor"] == "wait")
        .unwrap();
    assert_eq!(wait_task["schedule"]["cron"], "0 9 * * 5");

    let email_task = tasks
        .iter()
        .find(|task| task["executor"] == "notification")
        .unwrap();
    assert_eq!(email_task["notification"]["channel"], "email");
    assert_eq!(email_task["notification"]["to"], "finance@example.com");
    assert_eq!(email_task["notification"]["include_cost_report"], true);
}

#[test]
fn plan_for_n8n_research_requires_catalog_before_forge_primitive_promotion() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Research current n8n node packages, workflow primitives, loop, if, switch, merge, wait, code, execute-subworkflow, triggers and human approval patterns before promoting Forge graph semantics",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert!(json["intent"]["deliverables"]
        .as_array()
        .unwrap()
        .contains(&Value::String("n8n primitive research catalog".to_string())));

    let tasks = json["tasks"].as_array().unwrap();
    let catalog_task = find_task(tasks, "Catalog n8n workflow primitives");
    let evaluation_task = find_task(tasks, "Evaluate Forge primitive candidates");
    let graph_task = find_task(tasks, "Build atomic task graph");

    assert_eq!(catalog_task["executor"], "ai");
    assert!(catalog_task["dependencies"]
        .as_array()
        .unwrap()
        .contains(&Value::String("task-002".to_string())));
    assert_eq!(
        catalog_task["expected_output"],
        "n8n node and pattern catalog artifact"
    );
    assert!(catalog_task["context_requirements"]
        .as_array()
        .unwrap()
        .contains(&Value::String("n8n source documentation".to_string())));
    assert!(catalog_task["validation_rules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|rule| rule["kind"] == "research_catalog"
            && rule["expected"]
                .as_str()
                .unwrap()
                .contains("loop, condition, router, merge, wait, code")));

    assert_eq!(evaluation_task["executor"], "ai");
    assert!(evaluation_task["dependencies"]
        .as_array()
        .unwrap()
        .contains(&catalog_task["id"]));
    assert_eq!(
        evaluation_task["expected_output"],
        "Forge primitive promotion recommendation"
    );
    assert!(evaluation_task["validation_rules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|rule| rule["kind"] == "promotion_guard"
            && rule["expected"]
                .as_str()
                .unwrap()
                .contains("validated DAG execution")));
    assert!(graph_task["dependencies"]
        .as_array()
        .unwrap()
        .contains(&evaluation_task["id"]));
}

#[test]
fn validation_blocks_promotion_until_required_gates_pass() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Create a reliable workflow runtime",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();

    let validation_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "validate",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();

    let validation: Value = serde_json::from_slice(&validation_output).unwrap();
    assert_eq!(validation["status"], "blocked");
    assert_eq!(validation["promotable"], false);
    assert!(validation["failed_rules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|rule| rule["kind"] == "task_status"));
}

#[test]
fn simulated_run_generates_cost_report_and_email_notification_payload() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Do an AI summary now, wait on cron 0 18 * * 1, run a shell-free deterministic cost calculation, and email the workflow costs to ops@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();

    let run_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "run",
            "--workflow",
            workflow_id,
            "--simulate",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let run: Value = serde_json::from_slice(&run_output).unwrap();
    assert_eq!(run["status"], "completed");
    assert!(
        run["cost_report"]["total_estimated_cost_usd"]
            .as_f64()
            .unwrap()
            > 0.0
    );
    assert_eq!(run["notifications"].as_array().unwrap().len(), 1);
    assert_eq!(run["notifications"][0]["channel"], "email");
    assert_eq!(run["notifications"][0]["to"], "ops@example.com");
    assert!(run["notifications"][0]["body"]
        .as_str()
        .unwrap()
        .contains("total_estimated_cost_usd"));
}

#[test]
fn context_controller_returns_minimal_task_local_context() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build an authenticated dashboard with docs",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let ai_task = find_task(json["tasks"].as_array().unwrap(), "Extract requirements");
    let task_id = ai_task["id"].as_str().unwrap();
    assert_eq!(ai_task["executor"], "ai");

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--budget",
            "900",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let context: Value = serde_json::from_slice(&context_output).unwrap();
    assert_eq!(context["workflow_id"], workflow_id);
    assert_eq!(context["task_id"], task_id);
    assert!(context["context_bytes"].as_u64().unwrap() <= 900);
    assert!(context["included_sections"]
        .as_array()
        .unwrap()
        .contains(&Value::String("local_objective".to_string())));
    assert!(!context["included_sections"]
        .as_array()
        .unwrap()
        .contains(&Value::String("entire_history".to_string())));
}

#[test]
fn context_controller_returns_versioned_shard_manifest() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build context routing for deterministic code nodes and AI executors without unrelated history",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let ai_task = find_task(json["tasks"].as_array().unwrap(), "Extract requirements");
    let task_id = ai_task["id"].as_str().unwrap();
    assert_eq!(ai_task["executor"], "ai");

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--budget",
            "360",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let context: Value = serde_json::from_slice(&context_output).unwrap();
    assert_eq!(context["schema_version"], "forge.context.v30");
    assert_eq!(
        context["routing_policy"],
        "task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30"
    );
    assert_eq!(context["workflow_id"], workflow_id);
    assert_eq!(context["task_id"], task_id);
    assert_eq!(context["workflow_revision"], 0);
    assert_eq!(context["artifact_count"], 0);
    assert_eq!(context["lineage"]["workflow_revision"], 0);
    assert_eq!(context["lineage"]["artifact_count"], 0);
    assert!(context["context_bytes"].as_u64().unwrap() <= 360);
    assert_eq!(context["context_sha256"].as_str().unwrap().len(), 64);

    let shards = context["shards"].as_array().unwrap();
    assert!(shards.len() >= 7);
    assert!(shards.iter().any(|shard| {
        shard["section"] == "local_objective"
            && shard["source"] == "task"
            && shard["included"] == true
            && shard["content_sha256"].as_str().unwrap().len() == 64
    }));
    assert!(shards
        .iter()
        .any(|shard| shard["section"] == "context_requirements"));
    assert!(shards
        .iter()
        .any(|shard| shard["section"] == "workflow_goal" && shard["source"] == "workflow"));
    assert!(shards.iter().any(|shard| shard["included"] == false));
    assert!(!context["omitted_sections"].as_array().unwrap().is_empty());
}

#[test]
fn context_package_routes_dependency_readiness_as_structured_context() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build dependency-aware context routing for executor handoff",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let requirements_task = find_task(json["tasks"].as_array().unwrap(), "Extract requirements");

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            requirements_task["id"].as_str().unwrap(),
            "--budget",
            "1100",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let context: Value = serde_json::from_slice(&context_output).unwrap();
    assert_eq!(context["schema_version"], "forge.context.v30");
    assert_eq!(
        context["routing_policy"],
        "task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30"
    );
    assert_eq!(context["dependency_summary"]["total"], 1);
    assert_eq!(context["dependency_summary"]["pending"], 1);
    assert_eq!(context["dependency_summary"]["completed"], 0);
    assert_eq!(context["dependency_summary"]["missing"], 0);
    assert_eq!(context["dependency_summary"]["ready"], false);
    assert_eq!(
        context["dependency_summary"]["blocking_task_ids"],
        serde_json::json!(["task-001"])
    );

    let dependencies = context["dependency_refs"].as_array().unwrap();
    assert_eq!(dependencies.len(), 1);
    assert_eq!(dependencies[0]["task_id"], "task-001");
    assert_eq!(dependencies[0]["title"], "Parse intent");
    assert_eq!(dependencies[0]["status"], "pending");
    assert_eq!(dependencies[0]["blocking"], true);
    assert_eq!(dependencies[0]["missing"], false);

    let dependency_shard = context["shards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|shard| shard["section"] == "dependencies")
        .unwrap();
    assert_eq!(dependency_shard["included"], true);
    assert_eq!(dependency_shard["source"], "graph");
    assert!(context["content"]
        .as_str()
        .unwrap()
        .contains("task-001 Parse intent [pending] blocking"));
}

#[test]
fn strict_context_blocks_executor_handoff_when_dependencies_are_not_ready() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build dependency-gated executor handoff for context routing",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let requirements_task = find_task(json["tasks"].as_array().unwrap(), "Extract requirements");

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            requirements_task["id"].as_str().unwrap(),
            "--budget",
            "1600",
            "--strict",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();

    let context: Value = serde_json::from_slice(&context_output).unwrap();
    assert_eq!(context["schema_version"], "forge.context.v30");
    assert_eq!(
        context["routing_policy"],
        "task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30"
    );
    assert_eq!(context["context_ready"], true);
    assert_eq!(context["dependency_summary"]["ready"], false);
    assert_eq!(context["handoff_ready"], false);
    assert_eq!(context["handoff_status"], "blocked_dependencies");
    assert!(context["handoff_blockers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|blocker| {
            blocker["kind"] == "dependency_not_ready"
                && blocker["refs"] == serde_json::json!(["task-001"])
                && blocker["message"]
                    .as_str()
                    .unwrap()
                    .contains("dependency tasks are not ready")
        }));
}

#[test]
fn context_package_summarizes_routing_decisions_for_executor_cost_audit() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let goal = format!(
        "Build context routing {}",
        "with repeated operational details ".repeat(40)
    );
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            &goal,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let ai_task = find_task(json["tasks"].as_array().unwrap(), "Extract requirements");
    let budget = 420_u64;

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            ai_task["id"].as_str().unwrap(),
            "--budget",
            &budget.to_string(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let context: Value = serde_json::from_slice(&context_output).unwrap();
    assert_eq!(context["schema_version"], "forge.context.v30");
    assert_eq!(
        context["routing_policy"],
        "task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30"
    );

    let shards = context["shards"].as_array().unwrap();
    let included_count = shards
        .iter()
        .filter(|shard| shard["included"] == true)
        .count();
    let omitted_count = shards.len() - included_count;
    let compressed_count = shards
        .iter()
        .filter(|shard| shard["compressed"] == true)
        .count();
    let budget_omitted_count = shards
        .iter()
        .filter(|shard| shard["routing_decision"] == "omitted_budget")
        .count();
    let selected_bytes: u64 = shards
        .iter()
        .map(|shard| shard["bytes"].as_u64().unwrap())
        .sum();
    let original_bytes: u64 = shards
        .iter()
        .map(|shard| shard["original_bytes"].as_u64().unwrap())
        .sum();

    let summary = &context["routing_summary"];
    assert_eq!(summary["total_shards"], shards.len());
    assert_eq!(summary["included_shards"], included_count);
    assert_eq!(summary["omitted_shards"], omitted_count);
    assert_eq!(summary["compressed_shards"], compressed_count);
    assert_eq!(summary["budget_omitted_shards"], budget_omitted_count);
    assert_eq!(summary["selected_bytes"], selected_bytes);
    assert_eq!(summary["original_bytes"], original_bytes);
    assert_eq!(summary["effective_budget"], context["effective_budget"]);
    assert_eq!(summary["remaining_budget"], budget - selected_bytes);
    assert!(summary["budget_utilization_bps"].as_u64().unwrap() <= 10_000);
    assert!(summary["compression_saved_bytes"].as_u64().unwrap() > 0);
    assert!(summary["omitted_bytes"].as_u64().unwrap() > 0);
}

#[test]
fn context_package_exposes_routing_economy_for_cost_audit() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let goal = format!(
        "Build context routing economy {}",
        "with repeated operational cost details ".repeat(40)
    );
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            &goal,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let ai_task = find_task(json["tasks"].as_array().unwrap(), "Extract requirements");

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            ai_task["id"].as_str().unwrap(),
            "--budget",
            "420",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let context: Value = serde_json::from_slice(&context_output).unwrap();
    assert_eq!(context["schema_version"], "forge.context.v30");
    assert_eq!(
        context["routing_policy"],
        "task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30"
    );

    let economy = &context["routing_economy"];
    let baseline_bytes = context["routing_summary"]["original_bytes"]
        .as_u64()
        .unwrap();
    let selected_bytes = context["routing_summary"]["selected_bytes"]
        .as_u64()
        .unwrap();
    assert_eq!(
        economy["schema_version"],
        "forge.context.routing_economy.v1"
    );
    assert_eq!(economy["executor_profile_id"], "ai_reasoning");
    assert_eq!(economy["baseline_bytes"], baseline_bytes);
    assert_eq!(economy["selected_bytes"], selected_bytes);
    assert_eq!(
        economy["compression_saved_bytes"],
        context["routing_summary"]["compression_saved_bytes"]
    );
    assert_eq!(
        economy["budget_omitted_bytes"],
        context["routing_summary"]["omitted_bytes"]
    );
    assert_eq!(economy["profile_filtered_bytes"], 0);
    assert_eq!(
        economy["total_avoided_bytes"],
        baseline_bytes - selected_bytes
    );
    assert!(economy["reduction_bps"].as_u64().unwrap() > 0);
    assert_eq!(economy["model_call_avoided"], false);
    assert_eq!(economy["estimated_model_calls_avoided"], 0);
    assert_eq!(economy["cost_decision"], "model_context_reduced");
    assert!(economy["reason"]
        .as_str()
        .unwrap()
        .contains("selected a bounded AI context"));
    assert!(context["routing_fingerprint"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .any(|component| component["name"] == "routing_economy"
            && component["sha256"].as_str().unwrap().len() == 64));
}

#[test]
fn deterministic_context_economy_marks_model_call_avoided() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run a cron workflow with repeated local Python cost calculations without AI and email ops@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let deterministic_task = find_task(
        json["tasks"].as_array().unwrap(),
        "Run deterministic non-AI step",
    );

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            deterministic_task["id"].as_str().unwrap(),
            "--budget",
            "1600",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let context: Value = serde_json::from_slice(&context_output).unwrap();
    let economy = &context["routing_economy"];
    assert_eq!(
        economy["schema_version"],
        "forge.context.routing_economy.v1"
    );
    assert_eq!(economy["executor_profile_id"], "no_ai_deterministic");
    assert_eq!(economy["deterministic"], true);
    assert_eq!(economy["reasoning_allowed"], false);
    assert_eq!(economy["model_call_avoided"], true);
    assert_eq!(economy["estimated_model_calls_avoided"], 1);
    assert_eq!(economy["cost_decision"], "no_ai_deterministic_route");
    assert!(economy["profile_filtered_bytes"].as_u64().unwrap() > 0);
    assert!(
        economy["total_avoided_bytes"].as_u64().unwrap()
            >= economy["profile_filtered_bytes"].as_u64().unwrap()
    );

    let inspect_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "inspect",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let inspection: Value = serde_json::from_slice(&inspect_output).unwrap();
    assert!(inspection["diagram"]
        .as_str()
        .unwrap()
        .contains("economy no_ai_deterministic_route"));
    let inspected_task = find_task(
        inspection["nodes"].as_array().unwrap(),
        "Run deterministic non-AI step",
    );
    assert_eq!(
        inspected_task["context_route"]["routing_economy"]["schema_version"],
        "forge.context.routing_economy.v1"
    );
    assert_eq!(
        inspected_task["context_route"]["routing_economy"]["model_call_avoided"],
        true
    );
}

#[test]
fn context_package_exposes_versioned_prompt_packet_for_executor_adapters() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build a persona-aware operator report with bounded context prompt packets",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let documentation_task = find_task(json["tasks"].as_array().unwrap(), "Generate documentation");

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            documentation_task["id"].as_str().unwrap(),
            "--budget",
            "1200",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let context: Value = serde_json::from_slice(&context_output).unwrap();
    assert_eq!(context["schema_version"], "forge.context.v30");
    assert_eq!(
        context["routing_policy"],
        "task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30"
    );

    let packet = &context["prompt_packet"];
    assert_eq!(packet["schema_version"], "forge.context.prompt_packet.v2");
    assert_eq!(packet["packet_version"], "forge.executor.prompt_packet.v2");
    assert_eq!(packet["context_schema_version"], context["schema_version"]);
    assert_eq!(packet["routing_policy"], context["routing_policy"]);
    assert_eq!(packet["executor_profile_id"], "ai_reasoning");
    assert_eq!(packet["task_executor"], "ai");
    assert_eq!(packet["reasoning_allowed"], true);
    assert_eq!(packet["deterministic"], false);
    assert_eq!(packet["persona_mode"], "operator_report");
    assert_eq!(
        packet["persona_profile_id"],
        context["persona_profile"]["profile_id"]
    );
    assert_eq!(packet["context_sha256"], context["context_sha256"]);
    assert_eq!(
        packet["lineage_sha256"],
        context["lineage"]["lineage_sha256"]
    );
    assert_eq!(packet["budget_status"], context["budget_plan"]["status"]);
    assert_eq!(
        packet["routing_quality_status"],
        context["routing_quality"]["status"]
    );
    assert_eq!(packet["handoff_status"], context["handoff_status"]);
    assert_eq!(packet["packet_sha256"].as_str().unwrap().len(), 64);
    assert!(packet["instruction_sources"]
        .as_array()
        .unwrap()
        .contains(&Value::String("forge_context_router".to_string())));
    assert!(packet["instruction_sources"]
        .as_array()
        .unwrap()
        .contains(&Value::String("forge_executor_policy".to_string())));
    assert!(packet["instruction_sources"]
        .as_array()
        .unwrap()
        .contains(&Value::String(
            "forge_personality_soul_routing_v1".to_string()
        )));
    assert!(packet["instruction_sources"]
        .as_array()
        .unwrap()
        .contains(&Value::String(
            "codex_developer_personality_instructions".to_string()
        )));
    assert!(packet["instruction_sources"]
        .as_array()
        .unwrap()
        .contains(&Value::String(
            "paperclip_soul_voice_tone_persona".to_string()
        )));
    assert!(packet["validation_gates"]
        .as_array()
        .unwrap()
        .contains(&Value::String("task_validation_rules".to_string())));
    assert!(packet["validation_gates"]
        .as_array()
        .unwrap()
        .contains(&Value::String("persona_routing_required".to_string())));
    assert!(context["routing_fingerprint"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .any(|component| component["name"] == "prompt_packet"
            && component["sha256"].as_str().unwrap().len() == 64));

    let inspect_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "inspect",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let inspection: Value = serde_json::from_slice(&inspect_output).unwrap();
    let inspected_task = find_task(
        inspection["nodes"].as_array().unwrap(),
        "Generate documentation",
    );
    assert_eq!(
        inspected_task["context_route"]["prompt_packet_version"],
        packet["packet_version"]
    );
    assert_eq!(
        inspected_task["context_route"]["prompt_packet_sha256"],
        packet["packet_sha256"]
    );
    assert!(inspection["diagram"].as_str().unwrap().contains("packet "));
}

#[test]
fn context_package_exposes_replay_manifest_for_resumable_executor_context() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build replayable context routing manifests for resumable executor handoff",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let task = find_task(json["tasks"].as_array().unwrap(), "Extract requirements");
    let task_id = task["id"].as_str().unwrap();

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--budget",
            "1200",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let context: Value = serde_json::from_slice(&context_output).unwrap();
    assert_eq!(context["schema_version"], "forge.context.v30");
    assert_eq!(
        context["routing_policy"],
        "task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30"
    );

    let manifest = &context["replay_manifest"];
    assert_eq!(
        manifest["schema_version"],
        "forge.context.replay_manifest.v1"
    );
    assert_eq!(
        manifest["context_schema_version"],
        context["schema_version"]
    );
    assert_eq!(manifest["routing_policy"], context["routing_policy"]);
    assert_eq!(manifest["selector_version"], "forge.context.selector.v1");
    assert_eq!(manifest["workflow_id"], workflow_id);
    assert_eq!(manifest["task_id"], task_id);
    assert_eq!(manifest["workflow_revision"], context["workflow_revision"]);
    assert_eq!(manifest["executor_profile_id"], "ai_reasoning");
    assert_eq!(manifest["requested_budget"], 1200);
    assert_eq!(manifest["effective_budget"], context["effective_budget"]);
    assert_eq!(manifest["context_sha256"], context["context_sha256"]);
    assert_eq!(manifest["content_bytes"], context["context_bytes"]);
    assert_eq!(manifest["included_sections"], context["included_sections"]);
    assert_eq!(
        manifest["missing_required_sections"],
        context["missing_required_sections"]
    );
    assert_eq!(
        manifest["replay_command"]["args"],
        serde_json::json!([
            "context",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--budget",
            "1200",
            "--output",
            "json"
        ])
    );
    assert_eq!(manifest["replay_command"]["requires_store_path"], true);
    assert_eq!(
        manifest["shard_refs"].as_array().unwrap().len(),
        context["shards"].as_array().unwrap().len()
    );
    assert_eq!(
        manifest["shard_refs"][0]["shard_id"],
        context["shards"][0]["shard_id"]
    );
    assert_eq!(
        manifest["shard_refs"][0]["source_sha256"],
        context["shards"][0]["source_sha256"]
    );
    assert_eq!(manifest["manifest_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(
        context["prompt_packet"]["schema_version"],
        "forge.context.prompt_packet.v2"
    );
    assert_eq!(
        context["prompt_packet"]["packet_version"],
        "forge.executor.prompt_packet.v2"
    );
    assert_eq!(
        context["prompt_packet"]["replay_manifest_sha256"],
        manifest["manifest_sha256"]
    );
    assert!(context["routing_fingerprint"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .any(|component| component["name"] == "replay_manifest"
            && component["value"] == manifest["manifest_sha256"]
            && component["sha256"].as_str().unwrap().len() == 64));

    let inspect_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "inspect",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let inspection: Value = serde_json::from_slice(&inspect_output).unwrap();
    let inspected_task = find_task(
        inspection["nodes"].as_array().unwrap(),
        "Extract requirements",
    );
    assert_eq!(
        inspected_task["context_route"]["replay_manifest_sha256"],
        manifest["manifest_sha256"]
    );
    assert!(inspection["diagram"].as_str().unwrap().contains("replay "));
}

#[test]
fn strict_context_blocks_executor_when_required_sections_are_missing() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let goal = format!(
        "Build strict context readiness {}",
        "with detailed workflow constraints ".repeat(36)
    );
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            &goal,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let ai_task = find_task(json["tasks"].as_array().unwrap(), "Extract requirements");

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            ai_task["id"].as_str().unwrap(),
            "--budget",
            "360",
            "--strict",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();

    let context: Value = serde_json::from_slice(&context_output).unwrap();
    assert_eq!(context["schema_version"], "forge.context.v30");
    assert_eq!(
        context["routing_policy"],
        "task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30"
    );
    assert_eq!(context["context_ready"], false);
    assert!(!context["missing_required_sections"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(context["required_sections"]
        .as_array()
        .unwrap()
        .contains(&Value::String("validation_rules".to_string())));
    assert!(
        context["routing_summary"]["required_omitted_shards"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(context["shards"].as_array().unwrap().iter().any(|shard| {
        shard["required"] == true && shard["included"] == false && shard["missing_required"] == true
    }));
}

#[test]
fn context_controller_compresses_oversized_shards_before_omitting() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let goal = format!(
        "Build context routing {}",
        "with repeated operational details ".repeat(40)
    );
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            &goal,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let ai_task = find_task(json["tasks"].as_array().unwrap(), "Extract requirements");
    let task_id = ai_task["id"].as_str().unwrap();
    assert_eq!(ai_task["executor"], "ai");

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--budget",
            "420",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let context: Value = serde_json::from_slice(&context_output).unwrap();
    assert_eq!(context["schema_version"], "forge.context.v30");
    assert_eq!(
        context["routing_policy"],
        "task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30"
    );
    assert_eq!(context["executor_profile"]["id"], "ai_reasoning");
    assert!(context["context_bytes"].as_u64().unwrap() <= 420);
    assert!(context["included_sections"]
        .as_array()
        .unwrap()
        .contains(&Value::String("workflow_goal".to_string())));
    assert!(!context["omitted_sections"]
        .as_array()
        .unwrap()
        .contains(&Value::String("workflow_goal".to_string())));

    let workflow_goal = context["shards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|shard| shard["section"] == "workflow_goal")
        .unwrap();
    assert_eq!(workflow_goal["included"], true);
    assert_eq!(workflow_goal["compressed"], true);
    assert!(
        workflow_goal["original_bytes"].as_u64().unwrap()
            > workflow_goal["bytes"].as_u64().unwrap()
    );
    assert!(workflow_goal["summary"]
        .as_str()
        .unwrap()
        .starts_with("Current workflow goal: Build context routing"));
    assert!(context["content"]
        .as_str()
        .unwrap()
        .contains("[compressed workflow_goal]"));
}

#[test]
fn context_shards_explain_selection_decisions_for_budget_and_profile_routing() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run a cron workflow with repeated local Python cost calculations without AI and email ops@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let deterministic_task = find_task(
        json["tasks"].as_array().unwrap(),
        "Run deterministic non-AI step",
    );

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            deterministic_task["id"].as_str().unwrap(),
            "--budget",
            "1600",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let context: Value = serde_json::from_slice(&context_output).unwrap();
    assert_eq!(context["schema_version"], "forge.context.v30");
    assert_eq!(
        context["routing_policy"],
        "task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30"
    );

    let shards = context["shards"].as_array().unwrap();
    assert!(shards.iter().any(|shard| {
        shard["section"] == "execution_policy"
            && shard["included"] == true
            && shard["routing_decision"] == "included_full"
            && shard["decision_reason"]
                .as_str()
                .unwrap()
                .contains("fits within remaining effective budget")
    }));
    assert!(shards.iter().any(|shard| {
        shard["section"] == "work_item"
            && shard["profile_excluded"] == true
            && shard["included"] == false
            && shard["routing_decision"] == "omitted_profile"
            && shard["decision_reason"]
                .as_str()
                .unwrap()
                .contains("executor profile")
    }));
    assert!(shards.iter().any(|shard| {
        shard["included"] == false
            && shard["profile_excluded"] == false
            && shard["routing_decision"] == "omitted_budget"
            && shard["bytes"] == 0
    }));
    assert!(shards
        .iter()
        .all(|shard| shard["routing_decision"].is_string()));
    assert!(shards
        .iter()
        .all(|shard| shard["decision_reason"].is_string()));
}

#[test]
fn context_package_applies_no_ai_profile_to_deterministic_executor_nodes() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run a cron workflow with a deterministic non-AI cost calculation without AI and email ops@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let deterministic_task = find_task(
        json["tasks"].as_array().unwrap(),
        "Run deterministic non-AI step",
    );
    let task_id = deterministic_task["id"].as_str().unwrap();
    assert_eq!(deterministic_task["executor"], "command");

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--budget",
            "1600",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let context: Value = serde_json::from_slice(&context_output).unwrap();
    assert_eq!(context["schema_version"], "forge.context.v30");
    assert_eq!(
        context["routing_policy"],
        "task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30"
    );
    assert_eq!(context["requested_budget"], 1600);
    assert!(context["effective_budget"].as_u64().unwrap() < 1600);
    assert!(
        context["context_bytes"].as_u64().unwrap() <= context["effective_budget"].as_u64().unwrap()
    );
    assert_eq!(context["executor_profile"]["id"], "no_ai_deterministic");
    assert_eq!(context["executor_profile"]["executor"], "command");
    assert_eq!(context["executor_profile"]["reasoning_allowed"], false);
    assert_eq!(context["executor_profile"]["deterministic"], true);
    assert!(context["included_sections"]
        .as_array()
        .unwrap()
        .contains(&Value::String("local_objective".to_string())));
    assert!(context["included_sections"]
        .as_array()
        .unwrap()
        .contains(&Value::String("validation_rules".to_string())));
    assert!(context["profile_omitted_sections"]
        .as_array()
        .unwrap()
        .contains(&Value::String("work_item".to_string())));
    assert!(context["profile_omitted_sections"]
        .as_array()
        .unwrap()
        .contains(&Value::String("constraints".to_string())));
    assert!(!context["included_sections"]
        .as_array()
        .unwrap()
        .contains(&Value::String("work_item".to_string())));
    assert!(context["shards"].as_array().unwrap().iter().any(|shard| {
        shard["section"] == "work_item"
            && shard["profile_excluded"] == true
            && shard["included"] == false
    }));
}

#[test]
fn deterministic_code_nodes_carry_no_ai_execution_policy_in_context() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run a cron workflow with repeated local Python cost calculations without AI and email ops@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let code_task = find_task(
        json["tasks"].as_array().unwrap(),
        "Run deterministic non-AI step",
    );
    assert_eq!(code_task["executor"], "command");
    assert_eq!(code_task["execution_policy"]["mode"], "local_code_node");
    assert_eq!(code_task["execution_policy"]["ai_allowed"], false);
    assert_eq!(code_task["execution_policy"]["deterministic"], true);
    assert_eq!(
        code_task["execution_policy"]["code_runtime"]["language"],
        "python"
    );
    assert_eq!(
        code_task["execution_policy"]["validation_gate"],
        "deterministic_code_node_validation_required"
    );

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            code_task["id"].as_str().unwrap(),
            "--budget",
            "1600",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let context: Value = serde_json::from_slice(&context_output).unwrap();
    assert_eq!(context["schema_version"], "forge.context.v30");
    assert_eq!(
        context["routing_policy"],
        "task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30"
    );
    assert_eq!(context["execution_policy"]["mode"], "local_code_node");
    assert_eq!(
        context["execution_policy"]["code_runtime"]["entrypoint"],
        "forge_local_python_code_node"
    );
    assert!(context["included_sections"]
        .as_array()
        .unwrap()
        .contains(&Value::String("execution_policy".to_string())));
    assert!(context["shards"].as_array().unwrap().iter().any(|shard| {
        shard["section"] == "execution_policy"
            && shard["source"] == "execution_policy"
            && shard["included"] == true
    }));
    assert!(context["content"]
        .as_str()
        .unwrap()
        .contains("Execution policy mode: local_code_node"));
}

#[test]
fn frequent_local_code_goals_select_deterministic_node_without_schedule_scaffolding() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run frequent local Node.js invoice normalization",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let tasks = json["tasks"].as_array().unwrap();
    let code_task = find_task(tasks, "Run deterministic non-AI step");

    assert_eq!(code_task["id"], "task-009");
    assert_eq!(code_task["executor"], "command");
    assert_eq!(code_task["execution_policy"]["mode"], "local_code_node");
    assert_eq!(code_task["execution_policy"]["ai_allowed"], false);
    assert_eq!(code_task["execution_policy"]["deterministic"], true);
    assert_eq!(
        code_task["execution_policy"]["reuse_hint"],
        "reuse_compatible_code_node"
    );
    assert_eq!(
        code_task["execution_policy"]["code_runtime"]["language"],
        "nodejs"
    );
    assert_eq!(
        code_task["execution_policy"]["code_runtime"]["entrypoint"],
        "forge_local_node_code_node"
    );
    assert!(!tasks
        .iter()
        .any(|task| task["title"] == "Wait for scheduled continuation"));

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            code_task["id"].as_str().unwrap(),
            "--budget",
            "1600",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let context: Value = serde_json::from_slice(&context_output).unwrap();
    assert_eq!(context["executor_profile"]["id"], "no_ai_deterministic");
    assert_eq!(context["routing_economy"]["model_call_avoided"], true);
    assert_eq!(
        context["routing_economy"]["cost_decision"],
        "no_ai_deterministic_route"
    );
}

#[test]
fn context_package_exposes_execution_policy_decision_for_adapter_routing() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run frequent local Node.js invoice normalization",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let code_task = find_task(
        json["tasks"].as_array().unwrap(),
        "Run deterministic non-AI step",
    );
    let task_id = code_task["id"].as_str().unwrap();

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--budget",
            "1600",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let context: Value = serde_json::from_slice(&context_output).unwrap();
    let decision = &context["execution_policy_decision"];
    assert_eq!(
        decision["schema_version"],
        "forge.context.execution_policy_decision.v1"
    );
    assert_eq!(decision["workflow_id"], workflow_id);
    assert_eq!(decision["task_id"], task_id);
    assert_eq!(decision["workflow_revision"], 0);
    assert_eq!(decision["task_executor"], "command");
    assert_eq!(decision["executor_profile_id"], "no_ai_deterministic");
    assert_eq!(decision["policy_mode"], "local_code_node");
    assert_eq!(decision["route_class"], "local_code_node");
    assert_eq!(decision["ai_allowed"], false);
    assert_eq!(decision["deterministic"], true);
    assert_eq!(decision["model_call_required"], false);
    assert_eq!(decision["model_call_avoided"], true);
    assert_eq!(decision["reusable_as_child_subflow"], true);
    assert_eq!(decision["reuse_hint"], "reuse_compatible_code_node");
    assert_eq!(
        decision["reuse_key"],
        "local_code_node:nodejs:forge_local_node_code_node:deterministic_code_node_validation_required"
    );
    assert_eq!(decision["code_runtime_language"], "nodejs");
    assert_eq!(
        decision["code_runtime_entrypoint"],
        "forge_local_node_code_node"
    );
    assert_eq!(decision["code_runtime_sandbox"], "local_process_no_network");
    assert_eq!(
        decision["validation_gate"],
        "deterministic_code_node_validation_required"
    );
    assert!(decision["selection_reason"]
        .as_str()
        .unwrap()
        .contains("without routing the repeated step through a model"));
    assert_eq!(decision["decision_sha256"].as_str().unwrap().len(), 64);
    assert!(context["routing_fingerprint"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .any(|component| component["name"] == "execution_policy_decision"
            && component["value"] == decision["decision_sha256"]));

    let inspect_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "inspect",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let inspection: Value = serde_json::from_slice(&inspect_output).unwrap();
    let inspected_task = find_task(
        inspection["nodes"].as_array().unwrap(),
        "Run deterministic non-AI step",
    );
    assert_eq!(
        inspected_task["context_route"]["execution_policy_decision"]["decision_sha256"],
        decision["decision_sha256"]
    );
    assert_eq!(
        inspected_task["context_route"]["execution_policy_decision"]["route_class"],
        "local_code_node"
    );
    assert!(inspection["diagram"]
        .as_str()
        .unwrap()
        .contains("decision local_code_node"));
}

#[test]
fn context_package_tracks_runtime_mutation_lineage_and_current_goal() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let artifact = temp.path().join("operator-note.md");
    fs::write(
        &artifact,
        "# Operator note\n\nRuntime artifact attached while routing context.\n",
    )
    .unwrap();

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build context that survives runtime mutations",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let task_id = json["tasks"][0]["id"].as_str().unwrap();

    let updated_goal = "Build revision-aware context that survives runtime mutations";
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "update-goal",
            "--workflow",
            workflow_id,
            "--goal",
            updated_goal,
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "attach-artifact",
            "--workflow",
            workflow_id,
            "--path",
            artifact.to_str().unwrap(),
            "--kind",
            "operator_note",
            "--origin",
            "opencode",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--budget",
            "900",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let context: Value = serde_json::from_slice(&context_output).unwrap();
    assert_eq!(context["schema_version"], "forge.context.v30");
    assert_eq!(
        context["routing_policy"],
        "task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30"
    );
    assert_eq!(context["workflow_revision"], 2);
    assert_eq!(context["artifact_count"], 1);
    assert_eq!(context["lineage"]["workflow_revision"], 2);
    assert_eq!(
        context["lineage"]["workflow_goal_sha256"],
        hex_sha256(updated_goal.as_bytes())
    );
    assert_eq!(context["lineage"]["artifact_count"], 1);
    assert_eq!(
        context["lineage"]["revision_sources"],
        serde_json::json!(["codex", "opencode"])
    );
    assert_eq!(
        context["lineage"]["lineage_sha256"].as_str().unwrap().len(),
        64
    );
    assert!(context["content"].as_str().unwrap().contains(updated_goal));
    assert!(context["included_sections"]
        .as_array()
        .unwrap()
        .contains(&Value::String("workflow_goal".to_string())));
}

#[test]
fn context_package_exposes_stable_routing_fingerprint_for_executor_cache_keys() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build fingerprinted context routing for repeated executor handoffs",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let task = find_task(json["tasks"].as_array().unwrap(), "Extract requirements");
    let task_id = task["id"].as_str().unwrap();

    let first_context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--budget",
            "1100",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let first_context: Value = serde_json::from_slice(&first_context_output).unwrap();
    let fingerprint = &first_context["routing_fingerprint"];
    assert_eq!(
        fingerprint["schema_version"],
        "forge.context.routing_fingerprint.v1"
    );
    assert_eq!(fingerprint["executor_profile_id"], "ai_reasoning");
    assert_eq!(fingerprint["workflow_revision"], 0);
    assert_eq!(fingerprint["cache_key"].as_str().unwrap().len(), 64);
    assert_eq!(
        fingerprint["context_sha256"],
        first_context["context_sha256"]
    );

    let components = fingerprint["components"].as_array().unwrap();
    for component_name in [
        "routing_policy",
        "executor_profile",
        "lineage",
        "budget",
        "selected_sections",
        "dependency_state",
        "context_payload",
    ] {
        assert!(
            components
                .iter()
                .any(|component| component["name"] == component_name
                    && component["sha256"].as_str().unwrap().len() == 64),
            "missing routing fingerprint component {component_name}"
        );
    }

    let repeated_context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--budget",
            "1100",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let repeated_context: Value = serde_json::from_slice(&repeated_context_output).unwrap();
    assert_eq!(
        repeated_context["routing_fingerprint"]["cache_key"],
        fingerprint["cache_key"]
    );

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "update-goal",
            "--workflow",
            workflow_id,
            "--goal",
            "Build fingerprinted context routing after a goal mutation",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let mutated_context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--budget",
            "1100",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mutated_context: Value = serde_json::from_slice(&mutated_context_output).unwrap();
    assert_eq!(
        mutated_context["routing_fingerprint"]["workflow_revision"],
        1
    );
    assert_ne!(
        mutated_context["routing_fingerprint"]["cache_key"],
        fingerprint["cache_key"]
    );
}

#[test]
fn context_package_addresses_each_shard_by_source_content_for_routing_reuse() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let goal = format!(
        "Build content-addressed context shards {}",
        "with repeated operational context ".repeat(36)
    );
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            &goal,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let task = find_task(json["tasks"].as_array().unwrap(), "Extract requirements");
    let task_id = task["id"].as_str().unwrap();

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--budget",
            "420",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let context: Value = serde_json::from_slice(&context_output).unwrap();
    assert_eq!(context["schema_version"], "forge.context.v30");
    assert_eq!(
        context["routing_policy"],
        "task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30"
    );

    let shards = context["shards"].as_array().unwrap();
    assert!(shards.iter().any(|shard| {
        shard["routing_decision"] == "omitted_budget"
            && shard["source_sha256"].as_str().unwrap().len() == 64
            && shard["source_sha256"] != shard["content_sha256"]
    }));

    for (index, shard) in shards.iter().enumerate() {
        assert_eq!(shard["sequence"], index as u64);
        assert_eq!(shard["shard_id"].as_str().unwrap().len(), 64);
        assert_eq!(shard["source_sha256"].as_str().unwrap().len(), 64);
    }

    let source_shards = context["routing_fingerprint"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|component| component["name"] == "source_shards")
        .expect("source_shards fingerprint component should be present");
    assert_eq!(source_shards["sha256"].as_str().unwrap().len(), 64);
    assert!(source_shards["value"]
        .as_str()
        .unwrap()
        .contains("workflow_goal:"));

    let repeated_context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--budget",
            "420",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let repeated_context: Value = serde_json::from_slice(&repeated_context_output).unwrap();
    assert_eq!(repeated_context["shards"], context["shards"]);
    assert_eq!(
        repeated_context["routing_fingerprint"]["cache_key"],
        context["routing_fingerprint"]["cache_key"]
    );
}

#[test]
fn context_shards_include_remaining_budget_ledger_for_replayable_selection() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let goal = format!(
        "Build replayable context routing budget decisions {}",
        "with repeated operational context ".repeat(36)
    );
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            &goal,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let task = find_task(json["tasks"].as_array().unwrap(), "Extract requirements");

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            task["id"].as_str().unwrap(),
            "--budget",
            "420",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let context: Value = serde_json::from_slice(&context_output).unwrap();
    assert_eq!(context["schema_version"], "forge.context.v30");
    assert_eq!(
        context["routing_policy"],
        "task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30"
    );

    let effective_budget = context["effective_budget"].as_u64().unwrap();
    let shards = context["shards"].as_array().unwrap();
    assert!(!shards.is_empty());
    assert_eq!(
        shards[0]["remaining_budget_before"].as_u64().unwrap(),
        effective_budget
    );

    for shard in shards {
        let before = shard["remaining_budget_before"].as_u64().unwrap();
        let after = shard["remaining_budget_after"].as_u64().unwrap();
        let bytes = shard["bytes"].as_u64().unwrap();
        assert!(before <= effective_budget);
        assert!(after <= before);
        assert_eq!(before - after, bytes);
    }

    assert!(shards.iter().any(|shard| {
        shard["routing_decision"] == "omitted_budget"
            && shard["remaining_budget_before"] == shard["remaining_budget_after"]
            && shard["bytes"] == 0
    }));
    assert_eq!(
        shards.last().unwrap()["remaining_budget_after"],
        context["routing_summary"]["remaining_budget"]
    );
    assert!(context["routing_fingerprint"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .any(|component| component["name"] == "budget_ledger"
            && component["sha256"].as_str().unwrap().len() == 64));
}

#[test]
fn context_shards_expose_selection_cost_audit_for_compression_and_omission() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let goal = format!(
        "Build auditable shard selection cost routing {}",
        "with repeated operational context ".repeat(36)
    );
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            &goal,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let task = find_task(json["tasks"].as_array().unwrap(), "Extract requirements");

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            task["id"].as_str().unwrap(),
            "--budget",
            "420",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let context: Value = serde_json::from_slice(&context_output).unwrap();

    assert_eq!(context["schema_version"], "forge.context.v30");
    assert!(context["routing_policy"]
        .as_str()
        .unwrap()
        .contains("shard_selection_audit_v30"));

    let shards = context["shards"].as_array().unwrap();
    let compressed = shards
        .iter()
        .find(|shard| shard["section"] == "workflow_goal")
        .expect("workflow goal shard should exist");
    assert_eq!(compressed["included"], true);
    assert_eq!(compressed["compressed"], true);
    assert!(
        compressed["minimum_routable_bytes"].as_u64().unwrap()
            <= compressed["original_bytes"].as_u64().unwrap()
    );
    assert!(compressed["selection_saved_bytes"].as_u64().unwrap() > 0);
    assert!(compressed["selection_cost_bps"].as_u64().unwrap() > 0);
    assert!(compressed["selection_cost_bps"].as_u64().unwrap() < 10_000);

    let omitted = shards
        .iter()
        .find(|shard| shard["routing_decision"] == "omitted_budget")
        .expect("at least one shard should be omitted by budget");
    assert_eq!(omitted["included"], false);
    assert_eq!(omitted["bytes"], 0);
    assert!(omitted["minimum_routable_bytes"].as_u64().unwrap() > 0);
    assert_eq!(omitted["selection_saved_bytes"], omitted["original_bytes"]);
    assert_eq!(omitted["selection_cost_bps"], 0);

    let omitted_sequence = omitted["sequence"].as_u64().unwrap();
    let replay_ref = context["replay_manifest"]["shard_refs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|shard| shard["sequence"].as_u64().unwrap() == omitted_sequence)
        .expect("replay manifest should carry the same omitted shard");
    assert_eq!(
        replay_ref["minimum_routable_bytes"],
        omitted["minimum_routable_bytes"]
    );
    assert_eq!(
        replay_ref["selection_saved_bytes"],
        omitted["selection_saved_bytes"]
    );
    assert_eq!(
        replay_ref["selection_cost_bps"],
        omitted["selection_cost_bps"]
    );

    assert!(context["routing_fingerprint"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .any(|component| component["name"] == "shard_selection_audit"
            && component["sha256"].as_str().unwrap().len() == 64));
}

#[test]
fn context_package_exposes_versioned_routing_contract_for_executor_adapters() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Route minimum correct context to deterministic Python code nodes without AI",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let task = find_task(
        json["tasks"].as_array().unwrap(),
        "Run deterministic non-AI step",
    );

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            task["id"].as_str().unwrap(),
            "--budget",
            "1600",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let context: Value = serde_json::from_slice(&context_output).unwrap();

    assert_eq!(context["schema_version"], "forge.context.v30");
    assert_eq!(
        context["routing_policy"],
        "task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30"
    );

    let contract = &context["routing_contract"];
    assert_eq!(
        contract["schema_version"],
        "forge.context.routing_contract.v1"
    );
    assert_eq!(contract["selector_version"], "forge.context.selector.v1");
    assert_eq!(
        contract["profile_version"],
        "forge.context.executor_profile.v1"
    );
    assert_eq!(contract["profile_id"], "no_ai_deterministic");
    assert_eq!(
        contract["selection_strategy"],
        "required_first_priority_budgeted_compression"
    );
    assert_eq!(contract["requested_budget"], 1600);
    assert_eq!(contract["effective_budget"], context["effective_budget"]);
    assert_eq!(contract["minimum_budget_bytes"], 128);
    assert_eq!(contract["compression_allowed"], true);
    assert_eq!(contract["required_sections"], context["required_sections"]);
    assert_eq!(contract["profile_sha256"].as_str().unwrap().len(), 64);
    assert!(contract["allowed_sections"]
        .as_array()
        .unwrap()
        .contains(&Value::String("execution_policy".to_string())));
    assert!(contract["optional_sections"]
        .as_array()
        .unwrap()
        .contains(&Value::String("workflow_goal".to_string())));
    assert!(!contract["optional_sections"]
        .as_array()
        .unwrap()
        .contains(&Value::String("validation_rules".to_string())));

    assert!(context["routing_fingerprint"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .any(|component| component["name"] == "routing_contract"
            && component["sha256"].as_str().unwrap().len() == 64));
}

#[test]
fn context_package_exposes_selection_receipt_for_auditable_context_routing() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let goal = format!(
        "Build auditable context selection receipts {}",
        "with repeated operational routing inputs ".repeat(40)
    );
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            &goal,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let task = find_task(json["tasks"].as_array().unwrap(), "Extract requirements");

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            task["id"].as_str().unwrap(),
            "--budget",
            "420",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let context: Value = serde_json::from_slice(&context_output).unwrap();

    let receipt = &context["selection_receipt"];
    assert_eq!(
        receipt["schema_version"],
        "forge.context.selection_receipt.v1"
    );
    assert_eq!(receipt["selector_version"], "forge.context.selector.v1");
    assert_eq!(receipt["workflow_id"], workflow_id);
    assert_eq!(receipt["task_id"], task["id"]);
    assert_eq!(receipt["workflow_revision"], context["workflow_revision"]);
    assert_eq!(receipt["executor_profile_id"], "ai_reasoning");
    assert_eq!(receipt["requested_budget"], 420);
    assert_eq!(receipt["effective_budget"], context["effective_budget"]);
    assert_eq!(
        receipt["minimum_correct_budget_bytes"],
        context["budget_plan"]["minimum_correct_budget_bytes"]
    );
    assert_eq!(receipt["selected_sections"], context["included_sections"]);
    assert_eq!(receipt["required_sections"], context["required_sections"]);
    assert_eq!(
        receipt["missing_required_sections"],
        context["missing_required_sections"]
    );
    assert_eq!(receipt["required_complete"], context["context_ready"]);
    assert_eq!(receipt["route_status"], context["budget_plan"]["status"]);
    assert_eq!(receipt["handoff_status"], context["handoff_status"]);
    assert!(receipt["compressed_sections"]
        .as_array()
        .unwrap()
        .contains(&Value::String("workflow_goal".to_string())));
    assert!(!receipt["budget_omitted_sections"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(receipt["receipt_sha256"].as_str().unwrap().len(), 64);
    assert!(context["routing_fingerprint"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .any(|component| component["name"] == "selection_receipt"
            && component["value"] == receipt["receipt_sha256"]
            && component["sha256"].as_str().unwrap().len() == 64));

    let inspect_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "inspect",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let inspection: Value = serde_json::from_slice(&inspect_output).unwrap();
    let inspected_task = find_task(
        inspection["nodes"].as_array().unwrap(),
        "Extract requirements",
    );
    assert_eq!(
        inspected_task["context_route"]["selection_receipt_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert!(inspected_task["context_route"]["selection_route_status"].is_string());
    assert!(inspected_task["context_route"]["selection_required_complete"].is_boolean());
    assert!(inspection["diagram"].as_str().unwrap().contains("receipt "));
}

#[test]
fn context_package_scores_routing_quality_for_budget_pressure() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let goal = format!(
        "Build context routing quality gates {}",
        "with dense executor instructions ".repeat(40)
    );
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            &goal,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let task = find_task(json["tasks"].as_array().unwrap(), "Extract requirements");

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            task["id"].as_str().unwrap(),
            "--budget",
            "360",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let context: Value = serde_json::from_slice(&context_output).unwrap();

    let quality = &context["routing_quality"];
    assert_eq!(
        quality["schema_version"],
        "forge.context_routing_quality.v1"
    );
    assert_eq!(quality["status"], "blocked");
    assert!(quality["score_bps"].as_u64().unwrap() < 10_000);
    assert!(quality["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| {
            warning["code"] == "required_context_missing"
                && warning["severity"] == "blocking"
                && warning["recommendation"] == "increase_context_budget"
        }));
    assert!(quality["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| {
            warning["code"] == "budget_pressure"
                && warning["severity"] == "warning"
                && warning["refs"].as_array().unwrap().iter().any(|section| {
                    section == "context_requirements" || section == "validation_rules"
                })
        }));
    assert!(context["routing_fingerprint"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .any(|component| component["name"] == "routing_quality"
            && component["sha256"].as_str().unwrap().len() == 64));
}

#[test]
fn context_package_recommends_budget_repair_for_missing_required_sections() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let goal = format!(
        "Build context repair routing {}",
        "with dense executor instructions ".repeat(40)
    );
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            &goal,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let task = find_task(json["tasks"].as_array().unwrap(), "Extract requirements");

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            task["id"].as_str().unwrap(),
            "--budget",
            "360",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let context: Value = serde_json::from_slice(&context_output).unwrap();

    assert_eq!(context["schema_version"], "forge.context.v30");
    assert_eq!(
        context["routing_policy"],
        "task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30"
    );
    assert_eq!(
        context["routing_repair"]["schema_version"],
        "forge.context.routing_repair.v1"
    );
    assert_eq!(context["routing_repair"]["status"], "repair_required");
    assert_eq!(
        context["routing_repair"]["action"],
        "increase_context_budget"
    );
    assert_eq!(context["routing_repair"]["current_effective_budget"], 360);
    assert!(
        context["routing_repair"]["recommended_budget_bytes"]
            .as_u64()
            .unwrap()
            > 360
    );
    assert_eq!(
        context["routing_repair"]["required_budget_deficit_bytes"],
        context["routing_repair"]["recommended_budget_bytes"]
            .as_u64()
            .unwrap()
            - 360
    );
    assert_eq!(
        context["routing_repair"]["missing_required_sections"],
        context["missing_required_sections"]
    );
    assert!(context["routing_repair"]["missing_required_sections"]
        .as_array()
        .unwrap()
        .iter()
        .any(|section| section == "validation_rules" || section == "context_requirements"));
    assert!(context["routing_repair"]["reason"]
        .as_str()
        .unwrap()
        .contains("required context sections were omitted"));
    assert!(context["routing_fingerprint"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .any(|component| component["name"] == "routing_repair"
            && component["sha256"].as_str().unwrap().len() == 64));
}

#[test]
fn context_package_exposes_versioned_budget_plan_for_minimum_correct_context() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let goal = format!(
        "Build minimum correct context budget planning {}",
        "with dense executor instructions ".repeat(40)
    );
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            &goal,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let task = find_task(json["tasks"].as_array().unwrap(), "Extract requirements");

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            task["id"].as_str().unwrap(),
            "--budget",
            "360",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let context: Value = serde_json::from_slice(&context_output).unwrap();
    let budget_plan = &context["budget_plan"];

    assert_eq!(
        budget_plan["schema_version"],
        "forge.context.budget_plan.v1"
    );
    assert_eq!(budget_plan["requested_budget"], 360);
    assert_eq!(budget_plan["effective_budget"], context["effective_budget"]);
    assert_eq!(
        budget_plan["selected_bytes"],
        context["routing_summary"]["selected_bytes"]
    );
    assert_eq!(
        budget_plan["compression_saved_bytes"],
        context["routing_summary"]["compression_saved_bytes"]
    );
    assert_eq!(
        budget_plan["missing_required_sections"],
        context["missing_required_sections"]
    );
    assert!(budget_plan["required_minimum_bytes"].as_u64().unwrap() > 0);
    assert!(
        budget_plan["minimum_correct_budget_bytes"]
            .as_u64()
            .unwrap()
            <= budget_plan["recommended_budget_bytes"].as_u64().unwrap()
    );
    assert!(budget_plan["recommended_budget_bytes"].as_u64().unwrap() > 360);
    assert_eq!(budget_plan["status"], "repair_required");
    assert!(budget_plan["reason"]
        .as_str()
        .unwrap()
        .contains("minimum correct context"));
    assert!(context["routing_fingerprint"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .any(|component| component["name"] == "budget_plan"
            && component["sha256"].as_str().unwrap().len() == 64));
}

#[test]
fn context_package_exposes_minimum_correct_set_for_required_sections() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let goal = format!(
        "Build minimum correct context section receipts {}",
        "with dense executor instructions ".repeat(40)
    );
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            &goal,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let task = find_task(json["tasks"].as_array().unwrap(), "Extract requirements");

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            task["id"].as_str().unwrap(),
            "--budget",
            "360",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let context: Value = serde_json::from_slice(&context_output).unwrap();

    let minimum = &context["minimum_correct_set"];
    assert_eq!(
        minimum["schema_version"],
        "forge.context.minimum_correct_set.v1"
    );
    assert_eq!(minimum["selector_version"], "forge.context.selector.v1");
    assert_eq!(minimum["workflow_id"], workflow_id);
    assert_eq!(minimum["task_id"], task["id"]);
    assert_eq!(minimum["workflow_revision"], context["workflow_revision"]);
    assert_eq!(minimum["executor_profile_id"], "ai_reasoning");
    assert_eq!(minimum["required_complete"], context["context_ready"]);
    assert_eq!(
        minimum["missing_required_sections"],
        context["missing_required_sections"]
    );
    assert_eq!(
        minimum["minimum_correct_budget_bytes"],
        context["budget_plan"]["minimum_correct_budget_bytes"]
    );
    assert!(
        minimum["required_original_bytes"].as_u64().unwrap()
            >= minimum["required_selected_bytes"].as_u64().unwrap()
    );
    assert_eq!(minimum["set_sha256"].as_str().unwrap().len(), 64);

    let sections = minimum["sections"].as_array().unwrap();
    assert!(sections.len() >= 3);
    assert!(sections.iter().any(|section| {
        section["section"] == "validation_rules"
            && section["missing"] == true
            && section["included"] == false
            && section["repair_action"] == "increase_context_budget"
    }));
    assert!(sections.iter().all(|section| {
        section["section"].is_string()
            && section["routing_decision"].is_string()
            && section["source_sha256"].as_str().unwrap().len() == 64
            && section["content_sha256"].as_str().unwrap().len() == 64
    }));
    assert!(context["routing_fingerprint"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .any(|component| component["name"] == "minimum_correct_set"
            && component["value"] == minimum["set_sha256"]
            && component["sha256"].as_str().unwrap().len() == 64));
}

#[test]
fn inspect_projects_budget_plan_for_terminal_context_routes() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build inspectable minimum context budget plans with dense executor requirements",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();

    let inspect_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "inspect",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let inspection: Value = serde_json::from_slice(&inspect_output).unwrap();

    assert!(inspection["diagram"]
        .as_str()
        .unwrap()
        .contains("budget_plan"));
    assert!(inspection["diagram"].as_str().unwrap().contains("delta"));

    let requirements = find_task(
        inspection["nodes"].as_array().unwrap(),
        "Extract requirements",
    );
    let budget_plan = &requirements["context_route"]["budget_plan"];
    assert_eq!(
        budget_plan["schema_version"],
        "forge.context.budget_plan.v1"
    );
    assert!(
        budget_plan["recommended_budget_bytes"].as_u64().unwrap()
            >= budget_plan["minimum_correct_budget_bytes"]
                .as_u64()
                .unwrap()
    );
    assert!(budget_plan["missing_required_sections"].is_array());
    assert_eq!(
        requirements["context_route"]["context_delta"]["schema_version"],
        "forge.context.delta.v1"
    );
    assert_eq!(
        requirements["context_route"]["context_delta"]["status"],
        "no_checkpoint"
    );
    assert_eq!(
        requirements["context_route"]["minimum_correct_set"]["schema_version"],
        "forge.context.minimum_correct_set.v1"
    );
    assert_eq!(
        requirements["context_route"]["minimum_correct_set"]["required_complete"],
        requirements["context_route"]["context_ready"]
    );
}

#[test]
fn inspect_and_request_status_surface_context_routing_quality() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let started = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "start",
            "--goal",
            "Improve context routing quality visibility for async operators",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let started_json: Value = serde_json::from_slice(&started).unwrap();
    let run_id = started_json["run_id"].as_str().unwrap();
    let workflow_id = started_json["workflow_id"].as_str().unwrap();

    let inspect_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "inspect",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let inspection: Value = serde_json::from_slice(&inspect_output).unwrap();
    let quality_summary = &inspection["handoff_summary"]["routing_quality"];
    assert_eq!(
        quality_summary["schema_version"],
        "forge.context_routing_quality_summary.v1"
    );
    assert_eq!(quality_summary["tasks"], inspection["task_count"]);
    assert!(quality_summary["min_score_bps"].as_u64().unwrap() <= 10_000);

    let requirements = find_task(
        inspection["nodes"].as_array().unwrap(),
        "Extract requirements",
    );
    assert_eq!(
        requirements["context_route"]["routing_quality"]["schema_version"],
        "forge.context_routing_quality.v1"
    );
    assert!(
        requirements["context_route"]["routing_quality"]["score_bps"]
            .as_u64()
            .unwrap()
            <= 10_000
    );

    let status = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "status",
            "--run",
            run_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let status_json: Value = serde_json::from_slice(&status).unwrap();
    assert_eq!(
        status_json["handoff_summary"]["routing_quality"]["schema_version"],
        "forge.context_routing_quality_summary.v1"
    );
    let handoff_tasks = status_json["handoff_summary"]["tasks"].as_array().unwrap();
    let requirements_handoff = find_task(handoff_tasks, "Extract requirements");
    assert_eq!(
        requirements_handoff["routing_quality"]["schema_version"],
        "forge.context_routing_quality.v1"
    );
}

#[test]
fn planned_human_facing_tasks_include_node_scoped_persona_routing() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Create an operator report and email a stakeholder summary to ops@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    let tasks = json["tasks"].as_array().unwrap();
    let documentation_task = find_task(tasks, "Generate documentation");
    assert_eq!(documentation_task["persona"]["mode"], "operator_report");
    assert_eq!(documentation_task["persona"]["scope"], "node");
    assert_eq!(
        documentation_task["persona"]["instruction_source"],
        "forge_personality_soul_routing_v1"
    );
    assert_eq!(
        documentation_task["persona"]["validation_gate"],
        "persona_routing_required"
    );
    assert_eq!(documentation_task["persona"]["auditable"], true);
    assert!(documentation_task["persona"]["source_models"]
        .as_array()
        .unwrap()
        .contains(&Value::String(
            "codex_developer_personality_instructions".to_string()
        )));
    assert!(documentation_task["persona"]["source_models"]
        .as_array()
        .unwrap()
        .contains(&Value::String(
            "paperclip_soul_voice_tone_persona".to_string()
        )));

    let notification_task = find_task(tasks, "Send workflow cost email");
    assert_eq!(notification_task["persona"]["mode"], "stakeholder_notice");
    assert_eq!(notification_task["persona"]["scope"], "node");
}

#[test]
fn context_package_includes_persona_routing_lineage_for_human_facing_task() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Generate a human-facing operational report with auditable persona routing",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let documentation_task = find_task(json["tasks"].as_array().unwrap(), "Generate documentation");
    let task_id = documentation_task["id"].as_str().unwrap();

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--budget",
            "1600",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let context: Value = serde_json::from_slice(&context_output).unwrap();
    assert_eq!(context["schema_version"], "forge.context.v30");
    assert_eq!(
        context["routing_policy"],
        "task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30"
    );
    assert_eq!(context["persona"]["mode"], "operator_report");
    assert_eq!(context["persona"]["scope"], "node");
    assert_eq!(
        context["lineage"]["persona_mode_sha256"],
        hex_sha256("operator_report".as_bytes())
    );
    assert_eq!(context["lineage"]["persona_scope"], "node");
    assert!(context["included_sections"]
        .as_array()
        .unwrap()
        .contains(&Value::String("persona_routing".to_string())));
    assert!(context["shards"].as_array().unwrap().iter().any(|shard| {
        shard["section"] == "persona_routing"
            && shard["source"] == "persona"
            && shard["included"] == true
    }));
    assert!(context["content"]
        .as_str()
        .unwrap()
        .contains("Persona mode: operator_report"));
}

#[test]
fn context_package_exposes_versioned_persona_contract_for_human_facing_nodes() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Generate a human-facing operational report with auditable persona routing",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let documentation_task = find_task(json["tasks"].as_array().unwrap(), "Generate documentation");

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            documentation_task["id"].as_str().unwrap(),
            "--budget",
            "1600",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let context: Value = serde_json::from_slice(&context_output).unwrap();
    assert_eq!(context["schema_version"], "forge.context.v30");
    assert_eq!(
        context["routing_policy"],
        "task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30"
    );

    let contract = &context["persona_contract"];
    assert_eq!(
        contract["schema_version"],
        "forge.context.persona_contract.v2"
    );
    assert_eq!(contract["profile_id"], "persona.operator_report.v1");
    assert_eq!(contract["mode"], "operator_report");
    assert_eq!(contract["scope"], "node");
    assert_eq!(
        contract["instruction_source"],
        "forge_personality_soul_routing_v1"
    );
    assert_eq!(contract["validation_gate"], "persona_routing_required");
    assert_eq!(contract["auditable"], true);
    assert_eq!(
        contract["lineage_sha256"],
        context["lineage"]["lineage_sha256"]
    );
    assert_eq!(
        contract["persona_mode_sha256"],
        hex_sha256("operator_report".as_bytes())
    );
    assert_eq!(
        contract["profile_sha256"],
        context["lineage"]["persona_profile_sha256"]
    );
    assert!(contract["routing_rationale"]
        .as_str()
        .unwrap()
        .contains("node-scoped human-facing artifact"));
    assert_eq!(
        contract["persona_scope"],
        context["lineage"]["persona_scope"]
    );
    assert!(contract["source_models"]
        .as_array()
        .unwrap()
        .contains(&Value::String(
            "codex_developer_personality_instructions".to_string()
        )));
    assert!(contract["source_models"]
        .as_array()
        .unwrap()
        .contains(&Value::String(
            "paperclip_soul_voice_tone_persona".to_string()
        )));
    assert!(context["routing_fingerprint"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .any(|component| component["name"] == "persona_contract"
            && component["sha256"].as_str().unwrap().len() == 64));
}

#[test]
fn context_package_derives_persona_profile_for_human_facing_nodes() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Generate a human-facing operational report with auditable persona routing",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let documentation_task = find_task(json["tasks"].as_array().unwrap(), "Generate documentation");

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            documentation_task["id"].as_str().unwrap(),
            "--budget",
            "2200",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let context: Value = serde_json::from_slice(&context_output).unwrap();
    let profile = &context["persona_profile"];
    assert_eq!(
        profile["schema_version"],
        "forge.context.persona_profile.v1"
    );
    assert_eq!(profile["profile_id"], "persona.operator_report.v1");
    assert_eq!(profile["mode"], "operator_report");
    assert_eq!(profile["scope"], "node");
    assert_eq!(profile["validation_gate"], "persona_routing_required");
    assert_eq!(profile["auditable"], true);
    assert!(profile["routing_rationale"]
        .as_str()
        .unwrap()
        .contains("node-scoped human-facing artifact"));
    assert_eq!(
        profile["profile_sha256"],
        context["lineage"]["persona_profile_sha256"]
    );
    assert_eq!(profile["profile_sha256"].as_str().unwrap().len(), 64);
    assert!(profile["source_model_summaries"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |source| source["model_id"] == "codex_developer_personality_instructions"
                && source["summary"]
                    .as_str()
                    .unwrap()
                    .contains("developer/personality instructions")
        ));
    assert!(profile["source_model_summaries"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |source| source["model_id"] == "paperclip_soul_voice_tone_persona"
                && source["summary"]
                    .as_str()
                    .unwrap()
                    .contains("soul, voice, tone and persona")
        ));
    assert!(context["content"]
        .as_str()
        .unwrap()
        .contains("Persona profile: persona.operator_report.v1"));
    assert!(context["routing_fingerprint"]["components"]
        .as_array()
        .unwrap()
        .iter()
        .any(|component| component["name"] == "persona_profile"
            && component["sha256"].as_str().unwrap().len() == 64));
}

#[test]
fn validation_blocks_promotion_when_persona_routing_is_not_auditable() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Create an operator report with auditable persona routing",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "run",
            "--workflow",
            workflow_id,
            "--simulate",
            "--output",
            "json",
        ])
        .assert()
        .success();

    make_persona_routing_non_auditable(&store, workflow_id, "Generate documentation");

    let validation_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "validate",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();

    let validation: Value = serde_json::from_slice(&validation_output).unwrap();
    assert_eq!(validation["status"], "blocked");
    assert_eq!(validation["promotable"], false);
    assert!(validation["failed_rules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|rule| {
            rule["task_id"] == "task-008"
                && rule["kind"] == "persona_routing"
                && rule["message"].as_str().unwrap().contains("node-scoped")
        }));
    assert!(validation["rework_tasks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|task| {
            task["task_id"] == "task-008"
                && task["reason"]
                    .as_str()
                    .unwrap()
                    .contains("persona routing contract")
        }));
}

#[test]
fn inspect_renders_terminal_dag_with_lifecycle_persona_and_verbose_subtasks() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Create an operator report with auditable persona routing",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();

    let inspect_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "inspect",
            workflow_id,
            "--verbose",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let inspection: Value = serde_json::from_slice(&inspect_output).unwrap();
    assert_eq!(inspection["status"], "inspected");
    assert_eq!(inspection["workflow_id"], workflow_id);
    assert_eq!(inspection["lifecycle_state"], "idle");
    assert_eq!(inspection["verbose"], true);
    assert_eq!(inspection["subflow_count"], 0);
    assert!(inspection["diagram"]
        .as_str()
        .unwrap()
        .contains(&format!("Workflow {workflow_id} [idle]")));
    assert!(inspection["diagram"]
        .as_str()
        .unwrap()
        .contains("task-002 Extract requirements [pending] depends_on task-001"));
    assert!(inspection["diagram"].as_str().unwrap().contains(
        "task-008 Generate documentation [pending] depends_on task-007 persona operator_report"
    ));

    let nodes = inspection["nodes"].as_array().unwrap();
    assert!(nodes.len() >= 8);
    let documentation = find_task(nodes, "Generate documentation");
    assert_eq!(documentation["persona_mode"], "operator_report");
    assert!(documentation["subtasks"].as_array().unwrap().len() >= 3);
    assert!(documentation["validation_rules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|rule| {
            rule["kind"] == "documentation"
                && rule["expected"]
                    .as_str()
                    .unwrap()
                    .contains("operator can replay")
        }));
}

#[test]
fn inspect_can_focus_terminal_view_on_one_task() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Create an operator report with auditable persona routing",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();

    let inspect_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "inspect",
            workflow_id,
            "--task",
            "task-008",
            "--verbose",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let inspection: Value = serde_json::from_slice(&inspect_output).unwrap();
    assert_eq!(inspection["status"], "inspected");
    assert_eq!(inspection["workflow_id"], workflow_id);
    assert_eq!(inspection["focus"]["task_id"], "task-008");
    assert_eq!(inspection["focus"]["node_count"], 1);
    assert!(inspection["workflow_task_count"].as_u64().unwrap() >= 8);
    assert_eq!(inspection["task_count"], 1);
    assert_eq!(inspection["handoff_summary"]["total"], 1);
    assert!(inspection["diagram"]
        .as_str()
        .unwrap()
        .contains("focus task: task-008"));
    assert!(inspection["diagram"].as_str().unwrap().contains(
        "task-008 Generate documentation [pending] depends_on task-007 persona operator_report"
    ));
    assert!(!inspection["diagram"]
        .as_str()
        .unwrap()
        .contains("task-002 Extract requirements"));

    let nodes = inspection["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["id"], "task-008");
    assert_eq!(nodes[0]["persona_mode"], "operator_report");
    assert!(nodes[0]["subtasks"].as_array().unwrap().len() >= 3);
}

#[test]
fn inspect_exposes_context_route_summary_for_each_terminal_node() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run a cron workflow with repeated local Python cost calculations without AI and email ops@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();

    let inspect_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "inspect",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let inspection: Value = serde_json::from_slice(&inspect_output).unwrap();
    assert!(inspection["diagram"]
        .as_str()
        .unwrap()
        .contains("context no_ai_deterministic blocked_missing_context_and_dependencies"));

    let nodes = inspection["nodes"].as_array().unwrap();
    let deterministic = find_task(nodes, "Run deterministic non-AI step");
    let route = &deterministic["context_route"];
    assert_eq!(route["schema_version"], "forge.context.v30");
    assert_eq!(route["routing_policy"], "task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30");
    assert_eq!(
        route["routing_fingerprint_schema_version"],
        "forge.context.routing_fingerprint.v1"
    );
    assert_eq!(route["profile_id"], "no_ai_deterministic");
    assert_eq!(route["reasoning_allowed"], false);
    assert_eq!(route["deterministic"], true);
    assert_eq!(route["requested_budget"], 1200);
    assert!(route["effective_budget"].as_u64().unwrap() < 1200);
    assert_eq!(route["context_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(route["routing_cache_key"].as_str().unwrap().len(), 64);
    assert_eq!(route["routing_lineage_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(
        route["handoff_status"],
        "blocked_missing_context_and_dependencies"
    );
    assert_eq!(route["resume_context_status"], "no_checkpoint");
    assert!(route["routing_summary"]["total_shards"].as_u64().unwrap() >= 7);
    assert!(route["included_sections"]
        .as_array()
        .unwrap()
        .contains(&Value::String("execution_policy".to_string())));
    assert!(!route["missing_required_sections"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn inspect_projects_execution_policy_for_deterministic_code_nodes() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run a cron workflow with repeated local Python cost calculations without AI and email ops@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();

    let inspect_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "inspect",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let inspection: Value = serde_json::from_slice(&inspect_output).unwrap();
    assert!(inspection["diagram"]
        .as_str()
        .unwrap()
        .contains("policy local_code_node no_ai deterministic python reuse_compatible_code_node"));

    let deterministic = find_task(
        inspection["nodes"].as_array().unwrap(),
        "Run deterministic non-AI step",
    );
    let policy = &deterministic["execution_policy"];
    assert_eq!(
        policy["schema_version"],
        "forge.inspect_execution_policy.v1"
    );
    assert_eq!(policy["mode"], "local_code_node");
    assert_eq!(policy["ai_allowed"], false);
    assert_eq!(policy["deterministic"], true);
    assert_eq!(policy["reuse_hint"], "reuse_compatible_code_node");
    assert_eq!(
        policy["validation_gate"],
        "deterministic_code_node_validation_required"
    );
    assert_eq!(policy["code_runtime_language"], "python");
    assert_eq!(
        policy["code_runtime_entrypoint"],
        "forge_local_python_code_node"
    );
    assert_eq!(policy["code_runtime_sandbox"], "local_process_no_network");
    assert!(policy["selection_reason"]
        .as_str()
        .unwrap()
        .contains("without routing the repeated step through a model"));
}

#[test]
fn inspect_projects_next_context_action_for_handoff_and_resume() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build inspectable context continuation actions",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();

    let initial_inspect = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "inspect",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let initial_inspection: Value = serde_json::from_slice(&initial_inspect).unwrap();
    assert!(initial_inspection["diagram"]
        .as_str()
        .unwrap()
        .contains("next start_executor_handoff"));

    let nodes = initial_inspection["nodes"].as_array().unwrap();
    let parse = find_task(nodes, "Parse intent");
    assert_eq!(
        parse["context_route"]["next_action"]["schema_version"],
        "forge.inspect_context_action.v1"
    );
    assert_eq!(
        parse["context_route"]["next_action"]["action"],
        "start_executor_handoff"
    );
    assert_eq!(
        parse["context_route"]["next_action"]["ready_for_handoff"],
        true
    );
    assert_eq!(
        parse["context_route"]["next_action"]["partial_retry_recommended"],
        false
    );

    let requirements = find_task(nodes, "Extract requirements");
    assert_eq!(
        requirements["context_route"]["next_action"]["action"],
        "wait_for_dependencies"
    );
    assert_eq!(
        requirements["context_route"]["next_action"]["blocking_refs"],
        serde_json::json!(["task-001"])
    );

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            "task-001",
            "--budget",
            "1200",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let context: Value = serde_json::from_slice(&context_output).unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "task",
            "checkpoint",
            "--workflow",
            workflow_id,
            "--task",
            "task-001",
            "--executor",
            "codex",
            "--state",
            "paused",
            "--summary",
            "Paused after initial context route",
            "--context-sha256",
            context["context_sha256"].as_str().unwrap(),
            "--context-routing-cache-key",
            context["routing_fingerprint"]["cache_key"]
                .as_str()
                .unwrap(),
            "--workflow-revision",
            "0",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let resumed_inspect = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "inspect",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let resumed_inspection: Value = serde_json::from_slice(&resumed_inspect).unwrap();
    assert!(resumed_inspection["diagram"]
        .as_str()
        .unwrap()
        .contains("next partial_retry_with_fresh_context"));

    let resumed_parse = find_task(
        resumed_inspection["nodes"].as_array().unwrap(),
        "Parse intent",
    );
    let next_action = &resumed_parse["context_route"]["next_action"];
    assert_eq!(next_action["action"], "partial_retry_with_fresh_context");
    assert_eq!(next_action["partial_retry_recommended"], true);
    assert_eq!(
        next_action["checkpoint_context_routing_cache_key"],
        context["routing_fingerprint"]["cache_key"]
    );
    assert_eq!(
        next_action["current_context_routing_cache_key"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
}

#[test]
fn context_package_exposes_next_action_for_executor_resume_decisions() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build context packets with resumable executor guidance",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();

    let initial_context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            "task-001",
            "--budget",
            "1200",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let initial_context: Value = serde_json::from_slice(&initial_context_output).unwrap();
    assert_eq!(initial_context["schema_version"], "forge.context.v30");
    assert_eq!(
        initial_context["next_action"]["schema_version"],
        "forge.inspect_context_action.v1"
    );
    assert_eq!(
        initial_context["next_action"]["action"],
        "start_executor_handoff"
    );
    assert_eq!(initial_context["next_action"]["ready_for_handoff"], true);
    assert_eq!(
        initial_context["next_action"]["partial_retry_recommended"],
        false
    );
    assert_eq!(
        initial_context["next_action"]["current_context_routing_cache_key"],
        initial_context["routing_fingerprint"]["cache_key"]
    );
    assert_eq!(initial_context["next_action"]["checkpoint_id"], Value::Null);

    let workflow_revision = initial_context["workflow_revision"]
        .as_u64()
        .unwrap()
        .to_string();
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "task",
            "checkpoint",
            "--workflow",
            workflow_id,
            "--task",
            "task-001",
            "--executor",
            "codex",
            "--state",
            "paused",
            "--summary",
            "Paused after context packet route",
            "--context-sha256",
            initial_context["context_sha256"].as_str().unwrap(),
            "--context-routing-cache-key",
            initial_context["routing_fingerprint"]["cache_key"]
                .as_str()
                .unwrap(),
            "--workflow-revision",
            &workflow_revision,
            "--output",
            "json",
        ])
        .assert()
        .success();

    let resumed_context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            "task-001",
            "--budget",
            "1200",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let resumed_context: Value = serde_json::from_slice(&resumed_context_output).unwrap();
    assert_eq!(
        resumed_context["next_action"]["action"],
        "partial_retry_with_fresh_context"
    );
    assert_eq!(
        resumed_context["next_action"]["partial_retry_recommended"],
        true
    );
    assert_eq!(
        resumed_context["next_action"]["checkpoint_context_routing_cache_key"],
        initial_context["routing_fingerprint"]["cache_key"]
    );
    assert_eq!(
        resumed_context["next_action"]["current_context_routing_cache_key"],
        resumed_context["routing_fingerprint"]["cache_key"]
    );
}

#[test]
fn context_package_exposes_versioned_context_delta_for_resumable_reuse() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build context delta routing for resumable executor handoff",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();

    let initial_context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            "task-001",
            "--budget",
            "1200",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let initial_context: Value = serde_json::from_slice(&initial_context_output).unwrap();
    assert_eq!(initial_context["schema_version"], "forge.context.v30");
    assert_eq!(
        initial_context["context_delta"]["schema_version"],
        "forge.context.delta.v1"
    );
    assert_eq!(initial_context["context_delta"]["status"], "no_checkpoint");
    assert_eq!(
        initial_context["context_delta"]["can_reuse_checkpoint_context"],
        false
    );
    assert_eq!(
        initial_context["context_delta"]["partial_retry_recommended"],
        false
    );
    assert_eq!(
        initial_context["context_delta"]["current_context_routing_cache_key"],
        initial_context["routing_fingerprint"]["cache_key"]
    );
    assert!(initial_context["context_delta"]["changed_components"]
        .as_array()
        .unwrap()
        .is_empty());

    let workflow_revision = initial_context["workflow_revision"]
        .as_u64()
        .unwrap()
        .to_string();
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "task",
            "checkpoint",
            "--workflow",
            workflow_id,
            "--task",
            "task-001",
            "--executor",
            "codex",
            "--state",
            "paused",
            "--summary",
            "Paused after initial context route",
            "--context-sha256",
            initial_context["context_sha256"].as_str().unwrap(),
            "--context-routing-cache-key",
            initial_context["routing_fingerprint"]["cache_key"]
                .as_str()
                .unwrap(),
            "--workflow-revision",
            &workflow_revision,
            "--output",
            "json",
        ])
        .assert()
        .success();

    let resumed_context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            "task-001",
            "--budget",
            "1200",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let resumed_context: Value = serde_json::from_slice(&resumed_context_output).unwrap();
    let delta = &resumed_context["context_delta"];
    assert_eq!(delta["schema_version"], "forge.context.delta.v1");
    assert_eq!(delta["status"], "route_changed");
    assert_eq!(
        delta["checkpoint_id"],
        resumed_context["latest_checkpoint"]["checkpoint_id"]
    );
    assert_eq!(
        delta["checkpoint_context_sha256"],
        initial_context["context_sha256"]
    );
    assert_eq!(
        delta["checkpoint_context_routing_cache_key"],
        initial_context["routing_fingerprint"]["cache_key"]
    );
    assert_eq!(
        delta["current_context_routing_cache_key"],
        resumed_context["routing_fingerprint"]["cache_key"]
    );
    assert_eq!(delta["can_reuse_checkpoint_context"], false);
    assert_eq!(delta["partial_retry_recommended"], true);
    assert!(delta["changed_components"]
        .as_array()
        .unwrap()
        .contains(&Value::String("context_payload".to_string())));
    assert!(delta["changed_components"]
        .as_array()
        .unwrap()
        .contains(&Value::String("routing_cache_key".to_string())));
}

#[test]
fn context_package_exposes_versioned_continuation_plan_for_executor_adapters() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build versioned continuation plans for resumable executor adapters",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();

    let initial_context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            "task-001",
            "--budget",
            "1200",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let initial_context: Value = serde_json::from_slice(&initial_context_output).unwrap();
    assert_eq!(initial_context["schema_version"], "forge.context.v30");
    assert_eq!(
        initial_context["routing_policy"],
        "task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30"
    );

    let initial_plan = &initial_context["continuation_plan"];
    assert_eq!(
        initial_plan["schema_version"],
        "forge.context.continuation_plan.v1"
    );
    assert_eq!(initial_plan["status"], "no_checkpoint");
    assert_eq!(initial_plan["action"], "start_fresh");
    assert_eq!(initial_plan["checkpoint_reusable"], false);
    assert_eq!(initial_plan["requires_fresh_context"], true);
    assert_eq!(initial_plan["partial_retry_recommended"], false);
    assert_eq!(
        initial_plan["current_context_routing_cache_key"],
        initial_context["routing_fingerprint"]["cache_key"]
    );
    assert_eq!(
        initial_plan["current_context_sha256"],
        initial_context["context_sha256"]
    );
    assert_eq!(
        initial_plan["validation_gate"],
        "fresh_executor_handoff_required"
    );
    assert_eq!(initial_plan["checkpoint_id"], Value::Null);

    let workflow_revision = initial_context["workflow_revision"]
        .as_u64()
        .unwrap()
        .to_string();
    let checkpoint_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "task",
            "checkpoint",
            "--workflow",
            workflow_id,
            "--task",
            "task-001",
            "--executor",
            "codex",
            "--state",
            "paused",
            "--summary",
            "Paused after first continuation route",
            "--context-sha256",
            initial_context["context_sha256"].as_str().unwrap(),
            "--context-routing-cache-key",
            initial_context["routing_fingerprint"]["cache_key"]
                .as_str()
                .unwrap(),
            "--workflow-revision",
            &workflow_revision,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let checkpoint_json: Value = serde_json::from_slice(&checkpoint_output).unwrap();

    let resumed_context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            "task-001",
            "--budget",
            "1200",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let resumed_context: Value = serde_json::from_slice(&resumed_context_output).unwrap();
    let resumed_plan = &resumed_context["continuation_plan"];
    assert_eq!(
        resumed_plan["checkpoint_id"],
        checkpoint_json["checkpoint"]["checkpoint_id"]
    );
    assert_eq!(resumed_plan["status"], "checkpoint_route_changed");
    assert_eq!(resumed_plan["action"], "partial_retry_with_fresh_context");
    assert_eq!(resumed_plan["checkpoint_reusable"], false);
    assert_eq!(resumed_plan["requires_fresh_context"], true);
    assert_eq!(resumed_plan["partial_retry_recommended"], true);
    assert_eq!(
        resumed_plan["checkpoint_context_routing_cache_key"],
        initial_context["routing_fingerprint"]["cache_key"]
    );
    assert_eq!(
        resumed_plan["current_context_routing_cache_key"],
        resumed_context["routing_fingerprint"]["cache_key"]
    );
    assert_eq!(
        resumed_plan["validation_gate"],
        "partial_retry_requires_fresh_context_validation"
    );

    let inspect_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "inspect",
            workflow_id,
            "--task",
            "task-001",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let inspection: Value = serde_json::from_slice(&inspect_output).unwrap();
    assert!(inspection["diagram"]
        .as_str()
        .unwrap()
        .contains("continue partial_retry_with_fresh_context checkpoint_route_changed"));
    assert_eq!(
        inspection["nodes"][0]["context_route"]["continuation_plan"],
        *resumed_plan
    );

    let handoff = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "task",
            "handoff",
            "--workflow",
            workflow_id,
            "--task",
            "task-001",
            "--executor",
            "codex",
            "--ttl-seconds",
            "600",
            "--budget",
            "1200",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let handoff_json: Value = serde_json::from_slice(&handoff).unwrap();
    assert_eq!(
        handoff_json["packet"]["schema_version"],
        "forge.executor_handoff.v8"
    );
    assert_eq!(
        handoff_json["packet"]["resume_plan"],
        resumed_context["continuation_plan"]
    );
}

#[test]
fn improve_creates_controlled_experiment_and_never_promotes_without_validation() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Optimize agent execution reliability",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();

    let improve_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "improve",
            "--workflow",
            workflow_id,
            "--target-version",
            "0.2.0",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let improvement: Value = serde_json::from_slice(&improve_output).unwrap();
    assert_eq!(improvement["status"], "experiment_generated");
    assert_eq!(improvement["auto_promoted"], false);
    assert_eq!(
        improvement["promotion_gate"],
        "benchmark_and_validation_required"
    );
    assert_eq!(improvement["target_version"], "0.2.0");
    assert!(improvement["artifact_path"]
        .as_str()
        .unwrap()
        .ends_with(".json"));
    assert!(improvement["changelog_path"]
        .as_str()
        .unwrap()
        .ends_with(".md"));
    assert!(improvement["evolution_domains"]
        .as_array()
        .unwrap()
        .contains(&Value::String("task_structure".to_string())));
    assert!(improvement["candidate_changes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|change| change.as_str().unwrap().contains("impediments")));
    assert!(temp
        .path()
        .join(improvement["artifact_path"].as_str().unwrap())
        .exists());
    let changelog = fs::read_to_string(
        temp.path()
            .join(improvement["changelog_path"].as_str().unwrap()),
    )
    .unwrap();
    assert!(changelog.contains("# Forge Core 0.2.0 Changelog"));
    assert!(changelog.contains("Task Structure"));
    assert!(changelog.contains("Prompt System"));
}

#[test]
fn planned_tasks_include_scrum_safe_style_operational_metadata() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Evolve Forge task structure with backlog, impediments, subtasks and process governance",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    let first_task = &json["tasks"].as_array().unwrap()[0];
    assert!(first_task["goal"]
        .as_str()
        .unwrap()
        .contains("Parse intent"));
    assert_eq!(first_task["work_item"]["backlog_state"], "ready");
    assert_eq!(first_task["work_item"]["priority"], "p1");
    let subtasks = first_task["work_item"]["subtasks"].as_array().unwrap();
    assert!(!subtasks.is_empty());
    assert!(subtasks.iter().all(|subtask| subtask["goal"].is_string()));
    assert!(subtasks
        .iter()
        .all(|subtask| subtask["definition_of_done"].is_array()));
    assert!(first_task["work_item"]["impediments"].is_array());
    assert!(first_task["work_item"]["goal_validation"]["definitively_ready"].is_boolean());
    assert!(first_task["work_item"]["acceptance_criteria"]
        .as_array()
        .unwrap()
        .iter()
        .any(|criterion| criterion.as_str().unwrap().contains("Validation rules")));
}

#[test]
fn validation_blocks_goal_oriented_tasks_until_goals_are_definitively_done() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build a goal-oriented runtime that reworks unfinished tasks",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();

    let validation_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "validate",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let validation: Value = serde_json::from_slice(&validation_output).unwrap();
    assert!(validation["failed_rules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|rule| rule["kind"] == "goal_readiness"));
    assert!(validation["rework_tasks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|task| task["reason"]
            .as_str()
            .unwrap()
            .contains("not definitively ready")));

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "run",
            "--workflow",
            workflow_id,
            "--simulate",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let passed = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "validate",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let passed_json: Value = serde_json::from_slice(&passed).unwrap();
    assert!(passed_json["rework_tasks"].as_array().unwrap().is_empty());
}

#[test]
fn artifacts_command_lists_persistent_outputs_for_workflow() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Create reusable operational memory artifacts",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "improve",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success();

    let artifacts_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "artifacts",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let artifacts: Value = serde_json::from_slice(&artifacts_output).unwrap();
    assert_eq!(artifacts["workflow_id"], workflow_id);
    assert_eq!(artifacts["artifacts"].as_array().unwrap().len(), 2);
    assert!(
        artifacts["artifacts"][0]["path"]
            .as_str()
            .unwrap()
            .contains("changelog-")
            || artifacts["artifacts"][0]["path"]
                .as_str()
                .unwrap()
                .contains("improvement-")
    );
    assert!(artifacts["artifacts"][0]["sha256"].as_str().unwrap().len() >= 64);
    assert!(artifacts["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|artifact| artifact["path"].as_str().unwrap().contains("changelog-")));
    assert!(artifacts["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|artifact| artifact["path"].as_str().unwrap().contains("improvement-")));
}

#[test]
fn simulated_run_completes_graph_then_validation_allows_improvement_cycle() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Create a validated CLI and skill",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "run",
            "--workflow",
            workflow_id,
            "--simulate",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"completed\""));

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "validate",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"promotable\": true"));

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "improve",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "benchmark_and_validation_required",
        ));
}

#[test]
fn skill_install_creates_codex_and_opencode_compatible_skill_files() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "skill",
            "install",
            "--home",
            temp.path().to_str().unwrap(),
            "--target",
            "codex",
            "--target",
            "opencode",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"forge-core\""));

    let codex_skill = temp.path().join(".codex/skills/forge-core/SKILL.md");
    let opencode_skill = temp
        .path()
        .join(".config/opencode/skills/forge-core/SKILL.md");
    let shared_skill = temp.path().join(".agents/skills/forge-core/SKILL.md");
    assert!(codex_skill.exists());
    assert!(opencode_skill.exists());
    assert!(shared_skill.exists());

    let skill = fs::read_to_string(opencode_skill).unwrap();
    assert!(skill.starts_with("---\nname: forge-core\n"));
    assert!(skill.contains("description:"));
    assert!(skill.contains("forge plan"));
    assert!(skill.contains("forge validate"));
}

#[test]
fn sync_detects_configured_clis_and_requires_human_authorization_before_use() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let bin = temp.path().join("bin");
    fs::create_dir_all(temp.path().join(".codex")).unwrap();
    fs::write(temp.path().join(".codex/config.toml"), "model = \"test\"\n").unwrap();
    fs::create_dir_all(temp.path().join(".config/opencode")).unwrap();
    write_fake_cli(&bin, "codex");
    write_fake_cli(&bin, "opencode");
    write_fake_cli(&bin, "gemini");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "sync",
            "executors",
            "--home",
            temp.path().to_str().unwrap(),
            "--executor-path",
            bin.to_str().unwrap(),
            "--no-prompt",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["status"], "synced");
    assert_eq!(json["needs_human_approval"], true);
    assert!(json["usable"].as_array().unwrap().is_empty());

    let codex = find_executor(&json, "codex");
    assert_eq!(codex["installed"], true);
    assert_eq!(codex["configured"], true);
    assert_eq!(codex["allowed"], false);
    assert_eq!(codex["decision_source"], "pending_human_approval");

    let opencode = find_executor(&json, "opencode");
    assert_eq!(opencode["installed"], true);
    assert_eq!(opencode["configured"], true);
    assert_eq!(opencode["allowed"], false);
    assert_eq!(opencode["decision_source"], "pending_human_approval");

    let gemini = find_executor(&json, "gemini");
    assert_eq!(gemini["installed"], true);
    assert_eq!(gemini["configured"], false);
    assert_eq!(gemini["allowed"], false);
    assert_eq!(gemini["decision_source"], "unavailable");
}

#[test]
fn sync_persists_human_allowed_executor_policy() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let bin = temp.path().join("bin");
    fs::create_dir_all(temp.path().join(".codex")).unwrap();
    fs::write(temp.path().join(".codex/config.toml"), "model = \"test\"\n").unwrap();
    fs::create_dir_all(temp.path().join(".config/opencode")).unwrap();
    write_fake_cli(&bin, "codex");
    write_fake_cli(&bin, "opencode");

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "sync",
            "executors",
            "--home",
            temp.path().to_str().unwrap(),
            "--executor-path",
            bin.to_str().unwrap(),
            "--allow",
            "codex",
            "--deny",
            "opencode",
            "--no-prompt",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "executors",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["status"], "loaded");
    assert_eq!(json["usable"], serde_json::json!(["codex"]));
    assert_eq!(json["integrations"][0]["id"], "opencode_codex_bridge");
    assert_eq!(json["integrations"][0]["enabled"], false);
    let codex = find_executor(&json, "codex");
    assert_eq!(codex["allowed"], true);
    assert_eq!(codex["decision_source"], "human_allow");
    let opencode = find_executor(&json, "opencode");
    assert_eq!(opencode["allowed"], false);
    assert_eq!(opencode["decision_source"], "human_deny");
}

#[test]
fn sync_enables_opencode_codex_bridge_when_both_are_human_authorized() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let bin = temp.path().join("bin");
    fs::create_dir_all(temp.path().join(".codex")).unwrap();
    fs::write(temp.path().join(".codex/config.toml"), "model = \"test\"\n").unwrap();
    fs::create_dir_all(temp.path().join(".config/opencode")).unwrap();
    write_fake_cli(&bin, "codex");
    write_fake_cli(&bin, "opencode");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "sync",
            "executors",
            "--home",
            temp.path().to_str().unwrap(),
            "--executor-path",
            bin.to_str().unwrap(),
            "--allow",
            "codex",
            "--allow",
            "opencode",
            "--no-prompt",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["usable"], serde_json::json!(["codex", "opencode"]));
    assert_eq!(json["integrations"][0]["id"], "opencode_codex_bridge");
    assert_eq!(json["integrations"][0]["from"], "opencode");
    assert_eq!(json["integrations"][0]["to"], "codex");
    assert_eq!(json["integrations"][0]["enabled"], true);
}

#[test]
fn skill_install_runs_executor_sync_without_authorizing_unapproved_clis() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let bin = temp.path().join("bin");
    fs::create_dir_all(temp.path().join(".codex")).unwrap();
    fs::write(temp.path().join(".codex/config.toml"), "model = \"test\"\n").unwrap();
    write_fake_cli(&bin, "codex");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "skill",
            "install",
            "--home",
            temp.path().to_str().unwrap(),
            "--target",
            "codex",
            "--executor-path",
            bin.to_str().unwrap(),
            "--no-prompt",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["skill"], "forge-core");
    assert_eq!(json["executor_sync"]["status"], "synced");
    assert_eq!(json["executor_sync"]["needs_human_approval"], true);
    let codex = find_executor(&json["executor_sync"], "codex");
    assert_eq!(codex["configured"], true);
    assert_eq!(codex["allowed"], false);

    let saved = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "executors",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let saved_json: Value = serde_json::from_slice(&saved).unwrap();
    assert!(find_executor(&saved_json, "codex")["allowed"]
        .as_bool()
        .is_some());
}

#[test]
fn sync_detects_runtime_substrates_and_requires_human_authorization_before_use() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let bin = temp.path().join("bin");
    fs::create_dir_all(temp.path().join(".kube")).unwrap();
    fs::write(temp.path().join(".kube/config"), "apiVersion: v1\n").unwrap();
    write_fake_cli(&bin, "docker");
    write_fake_cli(&bin, "kubectl");
    write_fake_cli(&bin, "kn");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "sync",
            "runtimes",
            "--home",
            temp.path().to_str().unwrap(),
            "--runtime-path",
            bin.to_str().unwrap(),
            "--no-prompt",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["status"], "synced");
    assert_eq!(json["needs_human_approval"], true);
    assert!(json["usable"].as_array().unwrap().is_empty());

    let docker = find_runtime(&json, "docker");
    assert_eq!(docker["installed"], true);
    assert_eq!(docker["configured"], true);
    assert_eq!(docker["allowed"], false);
    assert_eq!(docker["async_capable"], true);

    let kubernetes = find_runtime(&json, "kubernetes");
    assert_eq!(kubernetes["installed"], true);
    assert_eq!(kubernetes["configured"], true);
    assert_eq!(kubernetes["allowed"], false);

    let knative = find_runtime(&json, "knative");
    assert_eq!(knative["installed"], true);
    assert_eq!(knative["configured"], true);
    assert_eq!(knative["allowed"], false);
}

#[test]
fn sync_suggests_knative_install_when_docker_and_kubernetes_exist_but_knative_is_missing() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let bin = temp.path().join("bin");
    fs::create_dir_all(temp.path().join(".kube")).unwrap();
    fs::write(temp.path().join(".kube/config"), "apiVersion: v1\n").unwrap();
    write_fake_cli(&bin, "docker");
    write_fake_cli(&bin, "kubectl");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "sync",
            "runtimes",
            "--home",
            temp.path().to_str().unwrap(),
            "--runtime-path",
            bin.to_str().unwrap(),
            "--allow",
            "docker",
            "--allow",
            "kubernetes",
            "--no-prompt",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["usable"], serde_json::json!(["docker", "kubernetes"]));
    assert!(json["install_suggestions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|suggestion| suggestion["id"] == "knative"
            && suggestion["requires_human_approval"] == true));
}

#[test]
fn runtime_scope_blocks_foreign_resource_mutation_without_authorization() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "runtime",
            "guard",
            "--substrate",
            "knative",
            "--resource",
            "service/existing-api",
            "--namespace",
            "default",
            "--action",
            "update",
            "--owner",
            "external",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["allowed"], false);
    assert_eq!(json["requires_human_approval"], true);
    assert_eq!(json["decision"], "blocked_external_resource");
}

#[test]
fn runtime_scope_allows_forge_owned_resources_and_explicit_external_authorization() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "runtime",
            "guard",
            "--substrate",
            "knative",
            "--resource",
            "service/forge-node",
            "--namespace",
            "forge",
            "--action",
            "delete",
            "--owner",
            "forge",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"allowed\": true"));

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "runtime",
            "guard",
            "--substrate",
            "knative",
            "--resource",
            "service/existing-api",
            "--namespace",
            "default",
            "--action",
            "update",
            "--owner",
            "external",
            "--allow-external",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("human_allow_external_resource"));
}

#[test]
fn planned_tasks_include_async_policy_for_runtime_execution() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run an asynchronous workflow on Kubernetes or Knative with Docker build steps",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert!(json["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|task| task["async_policy"]["mode"] == "async"
            && task["async_policy"]["resume_strategy"] == "event_or_poll"));
}

#[test]
fn workflow_goals_can_be_mutated_during_runtime_with_origin_trace() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Create a mutable workflow",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();

    let update_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "update-goal",
            "--workflow",
            workflow_id,
            "--goal",
            "Create a mutable workflow with Codex/OpenCode human interface",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let update: Value = serde_json::from_slice(&update_output).unwrap();
    assert_eq!(update["status"], "workflow_goal_updated");
    assert_eq!(update["origin"], "codex");
    assert_eq!(
        update["new_goal"],
        "Create a mutable workflow with Codex/OpenCode human interface"
    );
    assert!(update["revision"].as_u64().unwrap() >= 1);

    let status = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "status",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let status_json: Value = serde_json::from_slice(&status).unwrap();
    assert_eq!(
        status_json["goal"],
        "Create a mutable workflow with Codex/OpenCode human interface"
    );
    assert_eq!(status_json["revisions"].as_array().unwrap().len(), 1);
    assert_eq!(status_json["revisions"][0]["origin"], "codex");
}

#[test]
fn workflow_artifacts_can_be_attached_during_runtime_from_codex_or_opencode() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let artifact = temp.path().join("runtime-note.md");
    fs::write(
        &artifact,
        "# Runtime note\n\nArtifact attached while workflow is running.\n",
    )
    .unwrap();
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Create runtime artifact mutation",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();

    let attach_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "attach-artifact",
            "--workflow",
            workflow_id,
            "--path",
            artifact.to_str().unwrap(),
            "--kind",
            "runtime_note",
            "--origin",
            "opencode",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let attach: Value = serde_json::from_slice(&attach_output).unwrap();
    assert_eq!(attach["status"], "artifact_attached");
    assert_eq!(attach["origin"], "opencode");
    assert_eq!(attach["artifact"]["kind"], "runtime_note");
    assert!(attach["artifact"]["sha256"].as_str().unwrap().len() >= 64);

    let artifacts = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "artifacts",
            "--workflow",
            workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let artifacts_json: Value = serde_json::from_slice(&artifacts).unwrap();
    assert!(artifacts_json["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["path"].as_str().unwrap().contains("runtime-note.md")));
}

#[test]
fn self_run_dry_run_creates_bounded_self_evolution_workflow_and_artifacts() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let repo = temp.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    fs::write(repo.join("README.md"), "# Repo\n").unwrap();

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "self",
            "run",
            "--repo",
            repo.to_str().unwrap(),
            "--until",
            "2026-05-25T10:00:00-03:00",
            "--max-cycles",
            "1",
            "--executor",
            "codex",
            "--executor",
            "opencode",
            "--dry-run",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["status"], "planned");
    assert!(json["run_id"].as_str().unwrap().starts_with("run_"));
    assert_eq!(json["stop_at"], "2026-05-25T10:00:00-03:00");
    assert_eq!(json["executors"], serde_json::json!(["codex", "opencode"]));
    assert!(json["workflow_id"].as_str().unwrap().starts_with("wf_"));
    assert!(json["cycle_reports"].as_array().unwrap().len() == 1);
    let prompt_path = temp
        .path()
        .join(json["cycle_reports"][0]["prompt_path"].as_str().unwrap());
    assert!(prompt_path.exists());
    let prompt = fs::read_to_string(prompt_path).unwrap();
    assert!(prompt.contains("Improve Forge Core"));
    assert!(prompt.contains("Codex/OpenCode"));
    assert!(prompt.contains("Personality/Soul Routing"));
    assert!(prompt.contains("Codex handles developer/personality instructions"));
    assert!(prompt.contains("Paperclip models soul, voice, tone or persona"));
    assert!(prompt.contains("auditable in lineage and validation-gated"));
    assert!(prompt.contains("cargo test"));
    assert!(prompt.contains("Do not mutate external Docker/Kubernetes/Knative resources"));
}

#[test]
fn self_run_prompt_packet_is_versioned_and_checksummed_for_executor_replay() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let repo = temp.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    fs::write(repo.join("README.md"), "# Repo\n").unwrap();

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "self",
            "run",
            "--repo",
            repo.to_str().unwrap(),
            "--until",
            "2026-05-25T10:00:00-03:00",
            "--max-cycles",
            "1",
            "--executor",
            "codex",
            "--dry-run",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    let cycle_report = &json["cycle_reports"][0];
    assert_eq!(
        cycle_report["prompt_packet_version"],
        "forge.self_evolution.prompt.v2"
    );

    let prompt_path = temp
        .path()
        .join(cycle_report["prompt_path"].as_str().unwrap());
    let prompt = fs::read_to_string(prompt_path).unwrap();
    assert!(prompt.contains("Prompt packet version: `forge.self_evolution.prompt.v2`"));
    assert!(prompt.contains("Executor: `codex`"));
    assert_eq!(cycle_report["prompt_sha256"], hex_sha256(prompt.as_bytes()));
}

#[test]
fn self_run_declares_self_update_and_gh_publication_after_validation_contract() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let repo = temp.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    fs::write(repo.join("README.md"), "# Repo\n").unwrap();

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "self",
            "run",
            "--repo",
            repo.to_str().unwrap(),
            "--until",
            "2026-05-25T10:00:00-03:00",
            "--max-cycles",
            "1",
            "--executor",
            "codex",
            "--push",
            "--dry-run",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    let cycle_report = &json["cycle_reports"][0];
    assert_eq!(cycle_report["self_update"]["status"], "planned");
    assert_eq!(
        cycle_report["self_update"]["command"],
        serde_json::json!(["cargo", "install", "--path", ".", "--force"])
    );
    assert_eq!(cycle_report["public_project_update"]["status"], "planned");
    assert_eq!(cycle_report["public_project_update"]["uses_gh"], true);
    assert_eq!(
        cycle_report["public_project_update"]["gh_auth_command"],
        serde_json::json!(["timeout", "20", "gh", "auth", "token"])
    );
    assert_eq!(
        cycle_report["public_project_update"]["repo_view_command"],
        serde_json::json!(["git", "remote", "get-url", "origin"])
    );
    assert_eq!(
        cycle_report["public_project_update"]["push_command"],
        serde_json::json!(["timeout", "300", "git", "push"])
    );

    let prompt_path = temp
        .path()
        .join(cycle_report["prompt_path"].as_str().unwrap());
    let prompt = fs::read_to_string(prompt_path).unwrap();
    assert!(prompt.contains("After validation passes, update the local Forge installation"));
    assert!(prompt.contains("Publish validated commits through the GitHub CLI contract"));
}

#[test]
fn self_run_writes_validation_evidence_contract_artifact() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let repo = temp.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    fs::write(repo.join("README.md"), "# Repo\n").unwrap();

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "self",
            "run",
            "--repo",
            repo.to_str().unwrap(),
            "--until",
            "2026-05-25T10:00:00-03:00",
            "--max-cycles",
            "1",
            "--executor",
            "codex",
            "--dry-run",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    let cycle_report = &json["cycle_reports"][0];
    let validation_report_path = cycle_report["validation_report_path"].as_str().unwrap();
    assert!(validation_report_path.ends_with("self-evolution-cycle-001-validation.json"));
    assert!(
        cycle_report["validation_report_sha256"]
            .as_str()
            .unwrap()
            .len()
            >= 64
    );

    let validation_report_full_path = temp.path().join(validation_report_path);
    assert!(validation_report_full_path.exists());
    let validation_report_bytes = fs::read(&validation_report_full_path).unwrap();
    assert_eq!(
        cycle_report["validation_report_sha256"],
        hex_sha256(&validation_report_bytes)
    );

    let validation_report: Value = serde_json::from_slice(&validation_report_bytes).unwrap();
    assert_eq!(
        validation_report["schema_version"],
        "forge.self_evolution.validation.v1"
    );
    assert_eq!(
        validation_report["prompt_packet_version"],
        "forge.self_evolution.prompt.v2"
    );
    assert_eq!(validation_report["status"], "planned");
    assert_eq!(validation_report["validation_passed"], false);
    let commands = validation_report["commands"].as_array().unwrap();
    assert_eq!(commands.len(), 4);
    assert_eq!(commands[0]["command"], "cargo fmt --check");
    assert_eq!(commands[0]["status"], "planned");
    assert_eq!(commands[3]["command"], "cargo build --release");
}

#[test]
fn request_status_surfaces_latest_validation_evidence_for_async_callers() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let repo = temp.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    fs::write(repo.join("README.md"), "# Repo\n").unwrap();

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "self",
            "run",
            "--repo",
            repo.to_str().unwrap(),
            "--until",
            "2026-05-25T10:00:00-03:00",
            "--max-cycles",
            "1",
            "--executor",
            "codex",
            "--dry-run",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    let run_id = json["run_id"].as_str().unwrap();
    let cycle_report = &json["cycle_reports"][0];

    let status = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "status",
            "--run",
            run_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let status_json: Value = serde_json::from_slice(&status).unwrap();
    let validation = &status_json["latest_validation_evidence"];
    assert_eq!(
        validation["artifact_path"],
        cycle_report["validation_report_path"]
    );
    assert_eq!(
        validation["artifact_sha256"],
        cycle_report["validation_report_sha256"]
    );
    assert_eq!(
        validation["schema_version"],
        "forge.self_evolution.validation.v1"
    );
    assert_eq!(
        validation["prompt_packet_version"],
        "forge.self_evolution.prompt.v2"
    );
    assert_eq!(validation["status"], "planned");
    assert_eq!(validation["validation_passed"], false);
    assert_eq!(validation["cycle"], 1);
    assert_eq!(validation["executor"], "codex");
    assert_eq!(validation["command_summary"]["total"], 4);
    assert_eq!(validation["command_summary"]["planned"], 4);
    assert_eq!(validation["command_summary"]["passed"], 0);
    assert_eq!(validation["command_summary"]["failed"], 0);
    assert_eq!(validation["command_summary"]["skipped"], 0);
}

#[test]
fn request_start_returns_async_run_identifier_for_skill_callers() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "start",
            "--goal",
            "Improve Forge asynchronously from Codex skill",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["status"], "accepted");
    assert_eq!(json["async"], true);
    assert!(json["run_id"].as_str().unwrap().starts_with("run_"));
    assert!(json["workflow_id"].as_str().unwrap().starts_with("wf_"));

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "status",
            "--run",
            json["run_id"].as_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("Improve Forge asynchronously"));
}

#[test]
fn request_status_reflects_current_workflow_mutations_for_async_callers() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let artifact = temp.path().join("status-note.md");
    fs::write(
        &artifact,
        "# Status note\n\nA runtime artifact attached through Forge.\n",
    )
    .unwrap();

    let started = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "start",
            "--goal",
            "Improve Forge with async request status",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let started_json: Value = serde_json::from_slice(&started).unwrap();
    let run_id = started_json["run_id"].as_str().unwrap();
    let workflow_id = started_json["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "update-goal",
            "--workflow",
            workflow_id,
            "--goal",
            "Improve Forge with source-of-truth request status",
            "--origin",
            "opencode",
            "--output",
            "json",
        ])
        .assert()
        .success();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "attach-artifact",
            "--workflow",
            workflow_id,
            "--path",
            artifact.to_str().unwrap(),
            "--kind",
            "runtime_note",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let status = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "status",
            "--run",
            run_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let status_json: Value = serde_json::from_slice(&status).unwrap();
    assert_eq!(status_json["run_id"], run_id);
    assert_eq!(status_json["workflow_id"], workflow_id);
    assert_eq!(
        status_json["requested_goal"],
        "Improve Forge with async request status"
    );
    assert_eq!(
        status_json["goal"],
        "Improve Forge with source-of-truth request status"
    );
    assert_eq!(status_json["workflow_status"], "pending");
    assert_eq!(status_json["workflow_revision"], 2);
    assert_eq!(status_json["artifact_count"], 1);
    assert_eq!(
        status_json["task_summary"]["total"],
        status_json["task_summary"]["pending"]
    );
}

#[test]
fn inspect_and_request_status_project_context_handoff_readiness() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let started = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "start",
            "--goal",
            "Improve Forge context handoff visibility for async operators",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let started_json: Value = serde_json::from_slice(&started).unwrap();
    let run_id = started_json["run_id"].as_str().unwrap();
    let workflow_id = started_json["workflow_id"].as_str().unwrap();

    let inspect_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "inspect",
            workflow_id,
            "--verbose",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let inspection: Value = serde_json::from_slice(&inspect_output).unwrap();
    assert_eq!(
        inspection["handoff_summary"]["total"],
        inspection["task_count"]
    );
    assert!(
        inspection["handoff_summary"]["blocked_dependencies"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(inspection["diagram"].as_str().unwrap().contains(
        "task-002 Extract requirements [pending] depends_on task-001 handoff blocked_dependencies"
    ));

    let nodes = inspection["nodes"].as_array().unwrap();
    let requirements = find_task(nodes, "Extract requirements");
    assert_eq!(requirements["handoff_ready"], false);
    assert_eq!(requirements["handoff_status"], "blocked_dependencies");
    assert!(requirements["handoff_blockers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|blocker| {
            blocker["kind"] == "dependency_not_ready"
                && blocker["refs"] == serde_json::json!(["task-001"])
        }));

    let status = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "status",
            "--run",
            run_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let status_json: Value = serde_json::from_slice(&status).unwrap();
    assert_eq!(
        status_json["handoff_summary"]["total"],
        status_json["task_summary"]["total"]
    );
    assert!(
        status_json["handoff_summary"]["blocked_dependencies"]
            .as_u64()
            .unwrap()
            > 0
    );
    let handoff_tasks = status_json["handoff_summary"]["tasks"].as_array().unwrap();
    let requirements_handoff = find_task(handoff_tasks, "Extract requirements");
    assert_eq!(requirements_handoff["handoff_ready"], false);
    assert_eq!(
        requirements_handoff["handoff_status"],
        "blocked_dependencies"
    );
    assert_eq!(
        requirements_handoff["blocking_refs"],
        serde_json::json!(["task-001"])
    );
}

#[test]
fn list_surfaces_workflow_registry_with_lifecycle_and_initial_request() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let planned = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build a reusable invoice workflow",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let planned_json: Value = serde_json::from_slice(&planned).unwrap();
    let planned_workflow_id = planned_json["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "run",
            "--workflow",
            planned_workflow_id,
            "--simulate",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let started = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "start",
            "--goal",
            "Operate recurring fraud review",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let started_json: Value = serde_json::from_slice(&started).unwrap();
    let run_id = started_json["run_id"].as_str().unwrap();
    let async_workflow_id = started_json["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "update-goal",
            "--workflow",
            async_workflow_id,
            "--goal",
            "Operate recurring fraud review with supervisor alerts",
            "--origin",
            "opencode",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let listed = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "list",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let listed_json: Value = serde_json::from_slice(&listed).unwrap();
    assert_eq!(listed_json["status"], "loaded");
    assert_eq!(listed_json["summary"]["total"], 2);
    assert_eq!(listed_json["summary"]["running"], 0);
    assert_eq!(listed_json["summary"]["non_running"], 2);

    let planned_row = find_workflow(&listed_json, planned_workflow_id);
    assert_eq!(
        planned_row["initial_request"],
        "Build a reusable invoice workflow"
    );
    assert_eq!(
        planned_row["current_goal"],
        "Build a reusable invoice workflow"
    );
    assert_eq!(planned_row["lifecycle_state"], "scaled_to_zero");
    assert_eq!(planned_row["running"], false);
    assert!(planned_row["run_ids"].as_array().unwrap().is_empty());

    let async_row = find_workflow(&listed_json, async_workflow_id);
    assert_eq!(
        async_row["initial_request"],
        "Operate recurring fraud review"
    );
    assert_eq!(
        async_row["current_goal"],
        "Operate recurring fraud review with supervisor alerts"
    );
    assert_eq!(async_row["lifecycle_state"], "idle");
    assert_eq!(async_row["running"], false);
    assert_eq!(async_row["run_ids"], serde_json::json!([run_id]));
    assert_eq!(async_row["workflow_revision"], 1);
    assert_eq!(
        async_row["task_summary"]["total"],
        async_row["task_summary"]["pending"]
    );
}

#[test]
fn list_filters_workflow_registry_by_running_and_non_running_lifecycle() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let completed = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build a reusable completed workflow",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let completed_json: Value = serde_json::from_slice(&completed).unwrap();
    let completed_workflow_id = completed_json["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "run",
            "--workflow",
            completed_workflow_id,
            "--simulate",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let running = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build a currently running workflow",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let running_json: Value = serde_json::from_slice(&running).unwrap();
    let running_workflow_id = running_json["workflow_id"].as_str().unwrap();
    set_task_status_in_stored_workflow(&store, running_workflow_id, "task-001", "running");

    let running_list = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "list",
            "--lifecycle",
            "running",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let running_list_json: Value = serde_json::from_slice(&running_list).unwrap();
    assert_eq!(running_list_json["filter"]["lifecycle"], "running");
    assert_eq!(running_list_json["summary"]["total"], 1);
    assert_eq!(running_list_json["summary"]["running"], 1);
    assert_eq!(running_list_json["summary"]["non_running"], 0);
    assert_eq!(running_list_json["workflows"].as_array().unwrap().len(), 1);
    assert_eq!(
        running_list_json["workflows"][0]["workflow_id"],
        running_workflow_id
    );
    assert_eq!(
        running_list_json["workflows"][0]["lifecycle_state"],
        "running"
    );

    let non_running_list = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "list",
            "--lifecycle",
            "non-running",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let non_running_list_json: Value = serde_json::from_slice(&non_running_list).unwrap();
    assert_eq!(non_running_list_json["filter"]["lifecycle"], "non-running");
    assert_eq!(non_running_list_json["summary"]["total"], 1);
    assert_eq!(non_running_list_json["summary"]["running"], 0);
    assert_eq!(non_running_list_json["summary"]["non_running"], 1);
    assert_eq!(
        non_running_list_json["workflows"][0]["workflow_id"],
        completed_workflow_id
    );
    assert_eq!(
        non_running_list_json["workflows"][0]["lifecycle_state"],
        "scaled_to_zero"
    );
}

#[test]
fn list_projects_context_handoff_readiness_for_registry_rows() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let planned = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build registry-level context handoff routing visibility",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let planned_json: Value = serde_json::from_slice(&planned).unwrap();
    let workflow_id = planned_json["workflow_id"].as_str().unwrap();

    let listed = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "list",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let listed_json: Value = serde_json::from_slice(&listed).unwrap();
    assert_eq!(
        listed_json["summary"]["context_handoff"]["total_tasks"],
        planned_json["tasks"].as_array().unwrap().len()
    );
    assert_eq!(listed_json["summary"]["context_handoff"]["ready_tasks"], 1);
    assert!(
        listed_json["summary"]["context_handoff"]["blocked_dependencies"]
            .as_u64()
            .unwrap()
            > 0
    );

    let row = find_workflow(&listed_json, workflow_id);
    assert_eq!(
        row["context_handoff"]["total_tasks"],
        row["task_summary"]["total"]
    );
    assert_eq!(row["context_handoff"]["ready_tasks"], 1);
    assert_eq!(
        row["context_handoff"]["blocked_dependencies"],
        row["task_summary"]["total"].as_u64().unwrap() - 1
    );
    assert_eq!(row["context_handoff"]["blocked_missing_context"], 0);
    assert_eq!(
        row["context_handoff"]["schema_version"],
        "forge.registry_context_handoff.v1"
    );
}

#[test]
fn list_aggregates_context_next_actions_for_registry_rows() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let planned = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build registry-level context action routing visibility",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let planned_json: Value = serde_json::from_slice(&planned).unwrap();
    let workflow_id = planned_json["workflow_id"].as_str().unwrap();
    let task_count = planned_json["tasks"].as_array().unwrap().len() as u64;

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            "task-001",
            "--budget",
            "1200",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let context: Value = serde_json::from_slice(&context_output).unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "task",
            "checkpoint",
            "--workflow",
            workflow_id,
            "--task",
            "task-001",
            "--executor",
            "codex",
            "--state",
            "paused",
            "--summary",
            "Paused after registry context route",
            "--context-sha256",
            context["context_sha256"].as_str().unwrap(),
            "--context-routing-cache-key",
            context["routing_fingerprint"]["cache_key"]
                .as_str()
                .unwrap(),
            "--workflow-revision",
            "0",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let listed = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "list",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let listed_json: Value = serde_json::from_slice(&listed).unwrap();
    let summary = &listed_json["summary"]["context_actions"];
    assert_eq!(
        summary["schema_version"],
        "forge.registry_context_action.v1"
    );
    assert_eq!(summary["total_tasks"], task_count);
    assert_eq!(summary["ready_for_handoff"], 1);
    assert_eq!(summary["partial_retry_with_fresh_context"], 1);
    assert_eq!(summary["partial_retry_recommended"], 1);
    assert_eq!(summary["wait_for_dependencies"], task_count - 1);
    assert_eq!(summary["start_executor_handoff"], 0);

    let row = find_workflow(&listed_json, workflow_id);
    assert_eq!(row["context_actions"]["total_tasks"], task_count);
    assert_eq!(row["context_actions"]["ready_for_handoff"], 1);
    assert_eq!(
        row["context_actions"]["wait_for_dependencies"],
        task_count - 1
    );
    assert_eq!(
        row["context_actions"]["partial_retry_with_fresh_context"],
        1
    );
    assert_eq!(row["context_actions"]["partial_retry_recommended"], 1);

    let refs = row["context_action_refs"].as_array().unwrap();
    assert_eq!(refs.len() as u64, task_count);

    let retry_ref = refs
        .iter()
        .find(|entry| entry["task_id"] == "task-001")
        .unwrap();
    assert_eq!(
        retry_ref["schema_version"],
        "forge.registry_context_action_ref.v1"
    );
    assert_eq!(retry_ref["task_id"], "task-001");
    assert_eq!(retry_ref["title"], "Parse intent");
    assert_eq!(retry_ref["executor"], "command");
    assert_eq!(retry_ref["action"], "partial_retry_with_fresh_context");
    assert_eq!(retry_ref["ready_for_handoff"], true);
    assert_eq!(retry_ref["partial_retry_recommended"], true);
    assert_eq!(retry_ref["handoff_status"], "ready");
    assert_eq!(retry_ref["blocking_refs"], serde_json::json!([]));
    assert!(retry_ref["checkpoint_id"]
        .as_str()
        .unwrap()
        .starts_with("ckpt_"));
    assert_eq!(
        retry_ref["current_context_routing_cache_key"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert!(retry_ref["reason"]
        .as_str()
        .unwrap()
        .contains("checkpoint route differs"));

    let wait_ref = refs
        .iter()
        .find(|entry| entry["task_id"] == "task-002")
        .unwrap();
    assert_eq!(wait_ref["action"], "wait_for_dependencies");
    assert_eq!(wait_ref["ready_for_handoff"], false);
    assert_eq!(wait_ref["partial_retry_recommended"], false);
    assert_eq!(wait_ref["handoff_status"], "blocked_dependencies");
    assert_eq!(wait_ref["blocking_refs"], serde_json::json!(["task-001"]));
    assert_eq!(
        wait_ref["current_context_routing_cache_key"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
}

#[test]
fn list_filters_workflow_registry_by_context_action_and_lifecycle() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let completed = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build a completed workflow for context-action filtering",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let completed_json: Value = serde_json::from_slice(&completed).unwrap();
    let completed_workflow_id = completed_json["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "run",
            "--workflow",
            completed_workflow_id,
            "--simulate",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let running = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build a running workflow with dependency waits for context-action filtering",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let running_json: Value = serde_json::from_slice(&running).unwrap();
    let running_workflow_id = running_json["workflow_id"].as_str().unwrap();
    set_task_status_in_stored_workflow(&store, running_workflow_id, "task-001", "running");

    let listed = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "list",
            "--lifecycle",
            "running",
            "--context-action",
            "wait_for_dependencies",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let listed_json: Value = serde_json::from_slice(&listed).unwrap();
    assert_eq!(listed_json["filter"]["lifecycle"], "running");
    assert_eq!(
        listed_json["filter"]["context_action"],
        "wait_for_dependencies"
    );
    assert_eq!(listed_json["filter"]["quality_action"], Value::Null);
    assert_eq!(listed_json["summary"]["total"], 1);
    assert_eq!(listed_json["summary"]["running"], 1);
    assert_eq!(listed_json["summary"]["non_running"], 0);
    assert_eq!(
        listed_json["summary"]["context_actions"]["wait_for_dependencies"],
        running_json["tasks"].as_array().unwrap().len() as u64 - 1
    );

    let workflows = listed_json["workflows"].as_array().unwrap();
    assert_eq!(workflows.len(), 1);
    assert_eq!(workflows[0]["workflow_id"], running_workflow_id);
    assert_eq!(workflows[0]["lifecycle_state"], "running");
    assert!(
        workflows[0]["context_actions"]["wait_for_dependencies"]
            .as_u64()
            .unwrap()
            > 0
    );
}

#[test]
fn list_aggregates_context_quality_and_recommends_quality_actions_by_lifecycle() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let completed = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build a compact completed quality workflow",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let completed_json: Value = serde_json::from_slice(&completed).unwrap();
    let completed_workflow_id = completed_json["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "run",
            "--workflow",
            completed_workflow_id,
            "--simulate",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let long_tail = (0..120)
        .map(|index| format!("budget pressure evidence shard {index}"))
        .collect::<Vec<_>>()
        .join(" ");
    let running_goal =
        format!("Build registry context quality lifecycle visibility with {long_tail}");
    let running = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .arg("plan")
        .arg("--goal")
        .arg(&running_goal)
        .arg("--output")
        .arg("json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let running_json: Value = serde_json::from_slice(&running).unwrap();
    let running_workflow_id = running_json["workflow_id"].as_str().unwrap();
    set_task_status_in_stored_workflow(&store, running_workflow_id, "task-001", "running");

    let listed = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "list",
            "--lifecycle",
            "running",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let listed_json: Value = serde_json::from_slice(&listed).unwrap();
    assert_eq!(listed_json["filter"]["lifecycle"], "running");
    assert_eq!(listed_json["summary"]["total"], 1);
    assert_eq!(
        listed_json["summary"]["context_quality"]["schema_version"],
        "forge.registry_context_quality.v1"
    );
    assert_eq!(listed_json["summary"]["context_quality"]["workflows"], 1);
    assert_eq!(
        listed_json["summary"]["context_quality"]["total_tasks"],
        running_json["tasks"].as_array().unwrap().len()
    );
    assert!(
        listed_json["summary"]["context_quality"]["budget_pressure"]
            .as_u64()
            .unwrap()
            > 0
    );

    let row = find_workflow(&listed_json, running_workflow_id);
    assert_eq!(row["lifecycle_state"], "running");
    assert_eq!(
        row["context_quality"]["schema_version"],
        "forge.registry_context_quality.v1"
    );
    assert_eq!(
        row["context_quality"]["total_tasks"],
        row["task_summary"]["total"]
    );
    assert!(row["context_quality"]["budget_pressure"].as_u64().unwrap() > 0);
    assert_eq!(
        row["quality_action"]["schema_version"],
        "forge.registry_quality_action.v1"
    );
    assert_eq!(row["quality_action"]["action"], "increase_context_budget");
    assert_eq!(row["quality_action"]["priority"], "warning");
    assert!(row["quality_action"]["affected_tasks"].as_u64().unwrap() > 0);
}

#[test]
fn list_filters_workflow_registry_by_quality_action_and_lifecycle() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let completed = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build a compact completed workflow for quality-action filtering",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let completed_json: Value = serde_json::from_slice(&completed).unwrap();
    let completed_workflow_id = completed_json["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "run",
            "--workflow",
            completed_workflow_id,
            "--simulate",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let long_tail = (0..120)
        .map(|index| format!("quality action budget pressure shard {index}"))
        .collect::<Vec<_>>()
        .join(" ");
    let running_goal = format!("Build registry quality action filtering with {long_tail}");
    let running = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .arg("plan")
        .arg("--goal")
        .arg(&running_goal)
        .arg("--output")
        .arg("json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let running_json: Value = serde_json::from_slice(&running).unwrap();
    let running_workflow_id = running_json["workflow_id"].as_str().unwrap();
    set_task_status_in_stored_workflow(&store, running_workflow_id, "task-001", "running");

    let listed = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "list",
            "--lifecycle",
            "running",
            "--quality-action",
            "increase_context_budget",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let listed_json: Value = serde_json::from_slice(&listed).unwrap();
    assert_eq!(listed_json["filter"]["lifecycle"], "running");
    assert_eq!(
        listed_json["filter"]["quality_action"],
        "increase_context_budget"
    );
    assert_eq!(listed_json["summary"]["total"], 1);
    assert_eq!(listed_json["summary"]["running"], 1);
    assert_eq!(listed_json["summary"]["non_running"], 0);
    assert_eq!(listed_json["summary"]["context_quality"]["workflows"], 1);
    assert!(
        listed_json["summary"]["context_quality"]["budget_pressure"]
            .as_u64()
            .unwrap()
            > 0
    );

    let workflows = listed_json["workflows"].as_array().unwrap();
    assert_eq!(workflows.len(), 1);
    assert_eq!(workflows[0]["workflow_id"], running_workflow_id);
    assert_eq!(
        workflows[0]["quality_action"]["action"],
        "increase_context_budget"
    );
}

#[test]
fn list_surfaces_quality_action_catalog_for_filter_discovery() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let catalog = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "list",
            "--quality-actions",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let catalog_json: Value = serde_json::from_slice(&catalog).unwrap();
    assert_eq!(catalog_json["status"], "quality_actions_loaded");
    assert_eq!(
        catalog_json["schema_version"],
        "forge.registry_quality_action_catalog.v1"
    );
    assert_eq!(catalog_json["filter_field"], "quality_action");
    assert!(catalog_json["actions"].as_array().unwrap().len() >= 6);

    let increase_context_budget = find_quality_action(&catalog_json, "increase_context_budget");
    assert_eq!(
        increase_context_budget["filter_value"],
        "increase_context_budget"
    );
    assert!(increase_context_budget["possible_priorities"]
        .as_array()
        .unwrap()
        .contains(&Value::String("blocking".to_string())));
    assert!(increase_context_budget["possible_priorities"]
        .as_array()
        .unwrap()
        .contains(&Value::String("warning".to_string())));
    assert!(increase_context_budget["description"]
        .as_str()
        .unwrap()
        .contains("context budget"));

    let start_handoff = find_quality_action(&catalog_json, "start_executor_handoff");
    assert_eq!(start_handoff["filter_value"], "start_executor_handoff");
    assert_eq!(
        start_handoff["possible_priorities"],
        serde_json::json!(["ready"])
    );
    assert!(start_handoff["trigger"]
        .as_str()
        .unwrap()
        .contains("context quality and dependencies allow executor handoff"));
}

#[test]
fn list_surfaces_context_action_catalog_for_filter_discovery() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let catalog = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "list",
            "--context-actions",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let catalog_json: Value = serde_json::from_slice(&catalog).unwrap();
    assert_eq!(catalog_json["status"], "context_actions_loaded");
    assert_eq!(
        catalog_json["schema_version"],
        "forge.registry_context_action_catalog.v1"
    );
    assert_eq!(catalog_json["filter_field"], "context_action");
    assert!(catalog_json["actions"].as_array().unwrap().len() >= 10);

    let wait_for_dependencies = find_context_action(&catalog_json, "wait_for_dependencies");
    assert_eq!(
        wait_for_dependencies["filter_value"],
        "wait_for_dependencies"
    );
    assert_eq!(wait_for_dependencies["readiness"], "blocked");
    assert!(wait_for_dependencies["description"]
        .as_str()
        .unwrap()
        .contains("dependency tasks"));
    assert!(wait_for_dependencies["trigger"]
        .as_str()
        .unwrap()
        .contains("dependencies are not ready"));

    let start_handoff = find_context_action(&catalog_json, "start_executor_handoff");
    assert_eq!(start_handoff["filter_value"], "start_executor_handoff");
    assert_eq!(start_handoff["readiness"], "ready");
    assert!(start_handoff["description"]
        .as_str()
        .unwrap()
        .contains("executor handoff"));

    let partial_retry = find_context_action(&catalog_json, "partial_retry_with_fresh_context");
    assert_eq!(
        partial_retry["filter_value"],
        "partial_retry_with_fresh_context"
    );
    assert_eq!(partial_retry["readiness"], "retry");
    assert!(partial_retry["trigger"]
        .as_str()
        .unwrap()
        .contains("checkpoint route differs"));
}

#[test]
fn list_surfaces_reusable_code_node_subflows_with_compatibility_keys() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let planned = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run a cron workflow with repeated local Python cost calculations without AI and email ops@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let planned_json: Value = serde_json::from_slice(&planned).unwrap();
    let workflow_id = planned_json["workflow_id"].as_str().unwrap();

    let listed = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "list",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let listed_json: Value = serde_json::from_slice(&listed).unwrap();
    let row = find_workflow(&listed_json, workflow_id);
    assert_eq!(listed_json["summary"]["reusable_subflows"], 1);
    let reusable = row["reusable_subflows"].as_array().unwrap();
    assert_eq!(reusable.len(), 1);
    assert_eq!(reusable[0]["task_id"], "task-011");
    assert_eq!(reusable[0]["policy_mode"], "local_code_node");
    assert_eq!(reusable[0]["language"], "python");
    assert_eq!(
        reusable[0]["reuse_key"],
        "local_code_node:python:forge_local_python_code_node:deterministic_code_node_validation_required"
    );
    assert_eq!(
        reusable[0]["context_lineage_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
}

#[test]
fn list_aggregates_execution_policy_routes_for_registry_rows() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let planned = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run a cron workflow with repeated local Python cost calculations without AI and email ops@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let planned_json: Value = serde_json::from_slice(&planned).unwrap();
    let workflow_id = planned_json["workflow_id"].as_str().unwrap();
    let task_count = planned_json["tasks"].as_array().unwrap().len() as u64;

    let listed = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "list",
            "--lifecycle",
            "non-running",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let listed_json: Value = serde_json::from_slice(&listed).unwrap();
    let summary = &listed_json["summary"]["execution_policy"];
    assert_eq!(
        summary["schema_version"],
        "forge.registry_execution_policy.v1"
    );
    assert_eq!(summary["workflows"], 1);
    assert_eq!(summary["total_tasks"], task_count);
    assert_eq!(summary["ai_tasks"], 3);
    assert_eq!(summary["mixed_tasks"], 1);
    assert_eq!(summary["deterministic_tasks"], 8);
    assert_eq!(summary["model_call_required_tasks"], 4);
    assert_eq!(summary["model_call_avoided_tasks"], 8);
    assert_eq!(summary["local_code_nodes"], 1);
    assert_eq!(summary["reusable_local_code_nodes"], 1);
    assert_eq!(summary["command_tasks"], 6);
    assert_eq!(summary["wait_tasks"], 1);
    assert_eq!(summary["notification_tasks"], 1);

    let row = find_workflow(&listed_json, workflow_id);
    assert_eq!(
        row["execution_policy"]["schema_version"],
        "forge.registry_execution_policy.v1"
    );
    assert_eq!(row["execution_policy"]["workflows"], 1);
    assert_eq!(row["execution_policy"]["total_tasks"], task_count);
    assert_eq!(row["execution_policy"]["ai_tasks"], 3);
    assert_eq!(row["execution_policy"]["mixed_tasks"], 1);
    assert_eq!(row["execution_policy"]["deterministic_tasks"], 8);
    assert_eq!(row["execution_policy"]["local_code_nodes"], 1);
    assert_eq!(row["execution_policy"]["reusable_local_code_nodes"], 1);
    assert_eq!(row["reusable_subflows"].as_array().unwrap().len(), 1);
}

#[test]
fn plan_reports_compatible_reuse_candidates_before_creating_duplicate_code_nodes() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let first = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run a cron workflow with repeated local Python cost calculations without AI and email ops@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let first_json: Value = serde_json::from_slice(&first).unwrap();
    let first_workflow_id = first_json["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "run",
            "--workflow",
            first_workflow_id,
            "--simulate",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let second = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run a cron workflow with frequent local Python cost calculations without AI and email finance@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let second_json: Value = serde_json::from_slice(&second).unwrap();
    let candidates = second_json["reuse_candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0]["requested_task_id"], "task-011");
    assert_eq!(candidates[0]["candidate_workflow_id"], first_workflow_id);
    assert_eq!(candidates[0]["candidate_task_id"], "task-011");
    assert_eq!(candidates[0]["candidate_lifecycle_state"], "scaled_to_zero");
    assert_eq!(candidates[0]["attachable_as_child_subflow"], true);
    assert_eq!(
        candidates[0]["reuse_key"],
        "local_code_node:python:forge_local_python_code_node:deterministic_code_node_validation_required"
    );
    assert_eq!(
        candidates[0]["context_lineage_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
}

#[test]
fn plan_persists_reuse_candidates_as_proposed_child_subflows_for_inspection() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let first = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run a cron workflow with repeated local Python cost calculations without AI and email ops@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let first_json: Value = serde_json::from_slice(&first).unwrap();
    let first_workflow_id = first_json["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "run",
            "--workflow",
            first_workflow_id,
            "--simulate",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let second = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run a cron workflow with frequent local Python cost calculations without AI and email finance@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let second_json: Value = serde_json::from_slice(&second).unwrap();
    let second_workflow_id = second_json["workflow_id"].as_str().unwrap();
    assert_eq!(second_json["attached_subflows"], 1);
    let deterministic_task = find_task(
        second_json["tasks"].as_array().unwrap(),
        "Run deterministic non-AI step",
    );
    let child_subflows = deterministic_task["child_subflows"].as_array().unwrap();
    assert_eq!(child_subflows.len(), 1);
    assert_eq!(child_subflows[0]["workflow_id"], first_workflow_id);
    assert_eq!(child_subflows[0]["task_id"], "task-011");
    assert_eq!(child_subflows[0]["binding_status"], "proposed");
    assert_eq!(child_subflows[0]["lifecycle_state"], "scaled_to_zero");
    assert_eq!(
        child_subflows[0]["validation_gate"],
        "deterministic_code_node_validation_required"
    );

    let inspection = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "inspect",
            second_workflow_id,
            "--verbose",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let inspection_json: Value = serde_json::from_slice(&inspection).unwrap();
    assert_eq!(inspection_json["subflow_count"], 1);
    assert_eq!(
        inspection_json["subflows"][0]["workflow_id"],
        first_workflow_id
    );
    assert_eq!(inspection_json["subflows"][0]["task_id"], "task-011");
    assert_eq!(inspection_json["subflows"][0]["binding_status"], "proposed");
    assert!(inspection_json["diagram"]
        .as_str()
        .unwrap()
        .contains(&format!("subflows {first_workflow_id}/task-011:proposed")));

    let inspected_task = find_task(
        inspection_json["nodes"].as_array().unwrap(),
        "Run deterministic non-AI step",
    );
    assert_eq!(inspected_task["subflow_refs"].as_array().unwrap().len(), 1);
}

#[test]
fn inspect_expands_proposed_child_subflows_with_recursive_path_metadata() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let first = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run a cron workflow with repeated local Python cost calculations without AI and email ops@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let first_json: Value = serde_json::from_slice(&first).unwrap();
    let first_workflow_id = first_json["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "run",
            "--workflow",
            first_workflow_id,
            "--simulate",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let second = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run a cron workflow with frequent local Python cost calculations without AI and email finance@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let second_json: Value = serde_json::from_slice(&second).unwrap();
    let second_workflow_id = second_json["workflow_id"].as_str().unwrap();
    assert_eq!(second_json["attached_subflows"], 1);

    let inspection = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "inspect",
            second_workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let inspection_json: Value = serde_json::from_slice(&inspection).unwrap();
    assert_eq!(inspection_json["subflow_count"], 1);
    let subflow = &inspection_json["subflows"][0];
    assert_eq!(subflow["workflow_id"], first_workflow_id);
    assert_eq!(subflow["task_id"], "task-011");
    assert_eq!(subflow["parent_workflow_id"], second_workflow_id);
    assert_eq!(subflow["parent_task_id"], "task-011");
    assert_eq!(subflow["depth"], 1);
    assert_eq!(
        subflow["path"],
        serde_json::json!([
            format!("{second_workflow_id}/task-011"),
            format!("{first_workflow_id}/task-011")
        ])
    );
    assert_eq!(subflow["reachable"], true);
    assert_eq!(subflow["terminal"], true);
    assert_eq!(subflow["child_workflow_status"], "completed");
    assert_eq!(subflow["child_lifecycle_state"], "scaled_to_zero");
    assert!(subflow["child_task_count"].as_u64().unwrap() >= 8);
    assert_eq!(subflow["child_subflow_count"], 0);
    assert!(inspection_json["diagram"]
        .as_str()
        .unwrap()
        .contains(&format!(
            "subflow depth 1 {second_workflow_id}/task-011 -> {first_workflow_id}/task-011"
        )));
}

#[test]
fn inspect_marks_recursive_child_subflow_cycles_as_terminal() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let first = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run a cron workflow with repeated local Python cost calculations without AI and email ops@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let first_json: Value = serde_json::from_slice(&first).unwrap();
    let first_workflow_id = first_json["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "run",
            "--workflow",
            first_workflow_id,
            "--simulate",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let second = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run a cron workflow with frequent local Python cost calculations without AI and email finance@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let second_json: Value = serde_json::from_slice(&second).unwrap();
    let second_workflow_id = second_json["workflow_id"].as_str().unwrap();
    assert_eq!(second_json["attached_subflows"], 1);

    append_child_subflow_to_stored_workflow(
        &store,
        first_workflow_id,
        "task-011",
        serde_json::json!({
            "workflow_id": second_workflow_id,
            "task_id": "task-011",
            "title": "Run deterministic non-AI step",
            "binding_status": "proposed",
            "lifecycle_state": "idle",
            "reuse_key": "local_code_node:python:forge_local_python_code_node:deterministic_code_node_validation_required",
            "context_lineage_sha256": "0".repeat(64),
            "validation_gate": "deterministic_code_node_validation_required",
            "reason": "test-only recursive reuse edge"
        }),
    );

    let inspection = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "inspect",
            second_workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let inspection_json: Value = serde_json::from_slice(&inspection).unwrap();
    assert_eq!(inspection_json["subflow_count"], 2);
    let subflows = inspection_json["subflows"].as_array().unwrap();
    let recursive = subflows
        .iter()
        .find(|subflow| {
            subflow["workflow_id"] == second_workflow_id && subflow["task_id"] == "task-011"
        })
        .unwrap();
    assert_eq!(recursive["depth"], 2);
    assert_eq!(recursive["reachable"], true);
    assert_eq!(recursive["terminal"], true);
    assert_eq!(recursive["cycle_detected"], true);
    assert_eq!(
        recursive["cycle_ref"],
        format!("{second_workflow_id}/task-011")
    );
    assert_eq!(recursive["terminal_reason"], "recursive_subflow_cycle");
    assert_eq!(
        recursive["recursion_policy"],
        "stop_on_repeated_workflow_task_path"
    );
    assert_eq!(
        recursive["path"],
        serde_json::json!([
            format!("{second_workflow_id}/task-011"),
            format!("{first_workflow_id}/task-011"),
            format!("{second_workflow_id}/task-011")
        ])
    );
    assert!(inspection_json["diagram"]
        .as_str()
        .unwrap()
        .contains("cycle recursive_subflow_cycle"));
}

#[test]
fn request_start_reuses_compatible_subflows_before_persisting_async_workflow() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let first = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run a cron workflow with repeated local Python cost calculations without AI and email ops@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let first_json: Value = serde_json::from_slice(&first).unwrap();
    let first_workflow_id = first_json["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "run",
            "--workflow",
            first_workflow_id,
            "--simulate",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let started = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "start",
            "--goal",
            "Run a cron workflow with frequent local Python cost calculations without AI and email finance@example.com",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let started_json: Value = serde_json::from_slice(&started).unwrap();
    let second_workflow_id = started_json["workflow_id"].as_str().unwrap();
    assert_eq!(started_json["status"], "accepted");
    assert_eq!(started_json["attached_subflows"], 1);
    assert_eq!(
        started_json["reuse_candidates"][0]["candidate_workflow_id"],
        first_workflow_id
    );
    assert_eq!(
        started_json["reuse_candidates"][0]["requested_task_id"],
        "task-011"
    );
    assert_eq!(
        started_json["reuse_candidates"][0]["attachable_as_child_subflow"],
        true
    );

    let inspection = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "inspect",
            second_workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let inspection_json: Value = serde_json::from_slice(&inspection).unwrap();
    assert_eq!(inspection_json["subflow_count"], 1);
    assert_eq!(
        inspection_json["subflows"][0]["workflow_id"],
        first_workflow_id
    );
    assert_eq!(inspection_json["subflows"][0]["task_id"], "task-011");
    assert_eq!(inspection_json["subflows"][0]["binding_status"], "proposed");
}

#[test]
fn context_package_carries_proposed_child_subflow_routing_for_reused_nodes() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let first = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run a cron workflow with repeated local Python cost calculations without AI and email ops@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let first_json: Value = serde_json::from_slice(&first).unwrap();
    let first_workflow_id = first_json["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "run",
            "--workflow",
            first_workflow_id,
            "--simulate",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let second = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run a cron workflow with frequent local Python cost calculations without AI and email finance@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let second_json: Value = serde_json::from_slice(&second).unwrap();
    let second_workflow_id = second_json["workflow_id"].as_str().unwrap();
    let deterministic_task = find_task(
        second_json["tasks"].as_array().unwrap(),
        "Run deterministic non-AI step",
    );

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            second_workflow_id,
            "--task",
            deterministic_task["id"].as_str().unwrap(),
            "--budget",
            "1600",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let context: Value = serde_json::from_slice(&context_output).unwrap();
    assert_eq!(context["schema_version"], "forge.context.v30");
    assert_eq!(
        context["routing_policy"],
        "task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30"
    );
    assert_eq!(context["child_subflow_count"], 1);
    assert_eq!(
        context["child_subflows"][0]["workflow_id"],
        first_workflow_id
    );
    assert_eq!(context["child_subflows"][0]["task_id"], "task-011");
    assert_eq!(context["child_subflows"][0]["binding_status"], "proposed");
    assert!(context["included_sections"]
        .as_array()
        .unwrap()
        .contains(&Value::String("child_subflows".to_string())));
    assert!(context["shards"].as_array().unwrap().iter().any(|shard| {
        shard["section"] == "child_subflows"
            && shard["source"] == "subflow_registry"
            && shard["included"] == true
    }));
    assert!(context["content"]
        .as_str()
        .unwrap()
        .contains(&format!("Child subflow: {first_workflow_id}/task-011")));
    assert!(context["content"]
        .as_str()
        .unwrap()
        .contains("Binding status: proposed"));
}

#[test]
fn validation_blocks_promotion_until_child_subflow_binding_is_validated() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let first = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run a cron workflow with repeated local Python cost calculations without AI and email ops@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let first_json: Value = serde_json::from_slice(&first).unwrap();
    let first_workflow_id = first_json["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "run",
            "--workflow",
            first_workflow_id,
            "--simulate",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let second = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run a cron workflow with frequent local Python cost calculations without AI and email finance@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let second_json: Value = serde_json::from_slice(&second).unwrap();
    let second_workflow_id = second_json["workflow_id"].as_str().unwrap();
    assert_eq!(second_json["attached_subflows"], 1);

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "run",
            "--workflow",
            second_workflow_id,
            "--simulate",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let blocked = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "validate",
            "--workflow",
            second_workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let blocked_json: Value = serde_json::from_slice(&blocked).unwrap();
    assert_eq!(blocked_json["status"], "blocked");
    assert_eq!(blocked_json["promotable"], false);
    assert!(blocked_json["failed_rules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|rule| {
            rule["task_id"] == "task-011"
                && rule["kind"] == "child_subflow_validation"
                && rule["message"]
                    .as_str()
                    .unwrap()
                    .contains("binding status proposed")
                && rule["message"]
                    .as_str()
                    .unwrap()
                    .contains(&format!("{first_workflow_id}/task-011"))
        }));
    assert!(blocked_json["rework_tasks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|task| {
            task["task_id"] == "task-011"
                && task["reason"]
                    .as_str()
                    .unwrap()
                    .contains("child subflow binding")
        }));

    let validated = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "validate-subflow",
            "--workflow",
            second_workflow_id,
            "--task",
            "task-011",
            "--child-workflow",
            first_workflow_id,
            "--child-task",
            "task-011",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let validated_json: Value = serde_json::from_slice(&validated).unwrap();
    assert_eq!(validated_json["status"], "child_subflow_validated");
    assert_eq!(validated_json["workflow_id"], second_workflow_id);
    assert_eq!(validated_json["task_id"], "task-011");
    assert_eq!(validated_json["child_workflow_id"], first_workflow_id);
    assert_eq!(validated_json["child_task_id"], "task-011");
    assert_eq!(validated_json["previous_binding_status"], "proposed");
    assert_eq!(validated_json["binding_status"], "validated");
    assert_eq!(validated_json["lifecycle_state"], "scaled_to_zero");
    assert_eq!(validated_json["revision"], 1);

    let passed = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "validate",
            "--workflow",
            second_workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let passed_json: Value = serde_json::from_slice(&passed).unwrap();
    assert_eq!(passed_json["status"], "passed");
    assert_eq!(passed_json["promotable"], true);
}

#[test]
fn task_checkpoint_records_resumable_context_and_surfaces_request_status() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let started = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "start",
            "--goal",
            "Improve Forge with durable task checkpoints",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let started_json: Value = serde_json::from_slice(&started).unwrap();
    let run_id = started_json["run_id"].as_str().unwrap();
    let workflow_id = started_json["workflow_id"].as_str().unwrap();

    let context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            "task-002",
            "--budget",
            "1200",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let context: Value = serde_json::from_slice(&context_output).unwrap();
    let workflow_revision = context["workflow_revision"].as_u64().unwrap().to_string();

    let checkpoint_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "task",
            "checkpoint",
            "--workflow",
            workflow_id,
            "--task",
            "task-002",
            "--executor",
            "codex",
            "--state",
            "paused",
            "--summary",
            "Requirements extraction paused after bounded context routing",
            "--context-sha256",
            context["context_sha256"].as_str().unwrap(),
            "--workflow-revision",
            workflow_revision.as_str(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let checkpoint_json: Value = serde_json::from_slice(&checkpoint_output).unwrap();
    assert_eq!(checkpoint_json["status"], "checkpoint_recorded");
    assert!(checkpoint_json["checkpoint"]["checkpoint_id"]
        .as_str()
        .unwrap()
        .starts_with("ckpt_"));
    assert_eq!(checkpoint_json["checkpoint"]["workflow_id"], workflow_id);
    assert_eq!(checkpoint_json["checkpoint"]["task_id"], "task-002");
    assert_eq!(checkpoint_json["checkpoint"]["executor"], "codex");
    assert_eq!(checkpoint_json["checkpoint"]["state"], "paused");
    assert_eq!(
        checkpoint_json["checkpoint"]["context_sha256"],
        context["context_sha256"]
    );
    assert_eq!(checkpoint_json["checkpoint"]["workflow_revision"], 0);

    let status = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "status",
            "--run",
            run_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let status_json: Value = serde_json::from_slice(&status).unwrap();
    assert_eq!(status_json["checkpoint_count"], 1);
    assert_eq!(
        status_json["latest_checkpoint"]["checkpoint_id"],
        checkpoint_json["checkpoint"]["checkpoint_id"]
    );
    assert_eq!(status_json["latest_checkpoint"]["task_id"], "task-002");
    assert_eq!(status_json["latest_checkpoint"]["state"], "paused");
    assert_eq!(status_json["latest_checkpoint"]["workflow_revision"], 0);
}

#[test]
fn context_package_includes_latest_checkpoint_and_marks_stale_after_goal_mutation() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let planned = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build resumable context packets for async continuation",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let planned_json: Value = serde_json::from_slice(&planned).unwrap();
    let workflow_id = planned_json["workflow_id"].as_str().unwrap();

    let original_context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            "task-002",
            "--budget",
            "1600",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let original_context: Value = serde_json::from_slice(&original_context_output).unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "task",
            "checkpoint",
            "--workflow",
            workflow_id,
            "--task",
            "task-002",
            "--executor",
            "codex",
            "--state",
            "paused",
            "--summary",
            "Paused before requirement synthesis",
            "--context-sha256",
            original_context["context_sha256"].as_str().unwrap(),
            "--workflow-revision",
            "0",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let checkpointed_context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            "task-002",
            "--budget",
            "1600",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let checkpointed_context: Value = serde_json::from_slice(&checkpointed_context_output).unwrap();
    assert_eq!(checkpointed_context["schema_version"], "forge.context.v30");
    assert_eq!(
        checkpointed_context["routing_policy"],
        "task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30"
    );
    assert_eq!(
        checkpointed_context["latest_checkpoint"]["context_sha256"],
        original_context["context_sha256"]
    );
    assert_eq!(
        checkpointed_context["latest_checkpoint"]["workflow_revision"],
        checkpointed_context["workflow_revision"]
    );
    assert_eq!(
        checkpointed_context["resume_context_status"],
        "checkpoint_current"
    );
    assert!(checkpointed_context["included_sections"]
        .as_array()
        .unwrap()
        .contains(&Value::String("checkpoint".to_string())));
    assert!(checkpointed_context["shards"]
        .as_array()
        .unwrap()
        .iter()
        .any(|shard| {
            shard["section"] == "checkpoint"
                && shard["source"] == "checkpoint"
                && shard["included"] == true
        }));

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "update-goal",
            "--workflow",
            workflow_id,
            "--goal",
            "Build resumable context packets after a goal mutation",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let stale_context_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "context",
            "--workflow",
            workflow_id,
            "--task",
            "task-002",
            "--budget",
            "1600",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stale_context: Value = serde_json::from_slice(&stale_context_output).unwrap();
    assert_eq!(stale_context["workflow_revision"], 1);
    assert_eq!(stale_context["latest_checkpoint"]["workflow_revision"], 0);
    assert_eq!(stale_context["resume_context_status"], "checkpoint_stale");
    assert_eq!(
        stale_context["resume_context_reason"],
        "checkpoint workflow revision differs from current workflow revision"
    );
}

#[test]
fn list_loads_legacy_workflows_without_async_policy_or_revisions() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let planned = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build a legacy-compatible workflow registry",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let planned_json: Value = serde_json::from_slice(&planned).unwrap();
    let workflow_id = planned_json["workflow_id"].as_str().unwrap();

    remove_legacy_fields_from_stored_workflow(&store, workflow_id);

    let listed = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "list",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let listed_json: Value = serde_json::from_slice(&listed).unwrap();
    let row = find_workflow(&listed_json, workflow_id);

    assert_eq!(
        row["initial_request"],
        "Build a legacy-compatible workflow registry"
    );
    assert_eq!(row["lifecycle_state"], "idle");
    assert_eq!(row["task_summary"]["pending"], row["task_summary"]["total"]);
}

#[test]
fn task_lease_prevents_two_executors_from_acquiring_same_task() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run async task leasing for multiple executors",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let task_id = json["tasks"][0]["id"].as_str().unwrap();

    let acquired = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "task",
            "acquire",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--executor",
            "codex",
            "--ttl-seconds",
            "600",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let acquired_json: Value = serde_json::from_slice(&acquired).unwrap();
    assert_eq!(acquired_json["status"], "lease_acquired");
    assert_eq!(acquired_json["allowed"], true);
    assert_eq!(acquired_json["lease"]["executor"], "codex");
    assert_eq!(acquired_json["lease"]["workflow_id"], workflow_id);
    assert_eq!(acquired_json["lease"]["task_id"], task_id);

    let conflict = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "task",
            "acquire",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--executor",
            "opencode",
            "--ttl-seconds",
            "600",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let conflict_json: Value = serde_json::from_slice(&conflict).unwrap();
    assert_eq!(conflict_json["status"], "lease_conflict");
    assert_eq!(conflict_json["allowed"], false);
    assert_eq!(conflict_json["current_lease"]["executor"], "codex");

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "task",
            "release",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--lease",
            acquired_json["lease"]["lease_id"].as_str().unwrap(),
            "--executor",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("lease_released"));

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "task",
            "acquire",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--executor",
            "opencode",
            "--ttl-seconds",
            "600",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"executor\": \"opencode\""));
}

#[test]
fn task_handoff_packet_acquires_lease_and_wraps_strict_context_for_ready_executor() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Handoff",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let task_id = json["tasks"][0]["id"].as_str().unwrap();

    let handoff = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "task",
            "handoff",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--executor",
            "codex",
            "--ttl-seconds",
            "600",
            "--budget",
            "1200",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let handoff_json: Value = serde_json::from_slice(&handoff).unwrap();
    assert_eq!(handoff_json["status"], "handoff_ready");
    assert_eq!(handoff_json["allowed"], true);
    assert_eq!(handoff_json["workflow_id"], workflow_id);
    assert_eq!(handoff_json["task_id"], task_id);
    assert_eq!(handoff_json["selected_executor"], "codex");
    assert_eq!(handoff_json["task_executor"], "command");
    assert_eq!(handoff_json["context"]["handoff_ready"], true);
    assert_eq!(handoff_json["context"]["handoff_status"], "ready");

    let packet = &handoff_json["packet"];
    assert_eq!(packet["schema_version"], "forge.executor_handoff.v8");
    assert_eq!(packet["workflow_id"], workflow_id);
    assert_eq!(packet["task_id"], task_id);
    assert_eq!(packet["selected_executor"], "codex");
    assert_eq!(packet["task_executor"], "command");
    assert_eq!(packet["lease_required"], true);
    assert_eq!(packet["lease_status"], "lease_acquired");
    assert_eq!(
        packet["lease_id"],
        handoff_json["lease"]["lease_id"].as_str().unwrap()
    );
    assert_eq!(packet["context_schema_version"], "forge.context.v30");
    assert_eq!(
        packet["context_sha256"],
        handoff_json["context"]["context_sha256"]
    );
    assert_eq!(
        packet["context_routing_fingerprint_schema_version"],
        "forge.context.routing_fingerprint.v1"
    );
    assert_eq!(
        packet["context_routing_cache_key"],
        handoff_json["context"]["routing_fingerprint"]["cache_key"]
    );
    assert_eq!(
        packet["context_routing_cache_key"].as_str().unwrap().len(),
        64
    );
    assert_eq!(
        packet["context_routing_lineage_sha256"],
        handoff_json["context"]["routing_fingerprint"]["lineage_sha256"]
    );
    assert_eq!(
        packet["context_bytes"],
        handoff_json["context"]["context_bytes"]
    );
    assert_eq!(packet["handoff_ready"], true);
    assert_eq!(packet["handoff_status"], "ready");
    assert_eq!(packet["expected_output"], "IntentSpec JSON");
    assert_eq!(packet["validation_gate"], "task_validation_rules");
    assert!(packet["validation_rules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|rule| rule["kind"] == "schema"));

    let conflict = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "task",
            "handoff",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--executor",
            "opencode",
            "--ttl-seconds",
            "600",
            "--budget",
            "1200",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let conflict_json: Value = serde_json::from_slice(&conflict).unwrap();
    assert_eq!(conflict_json["status"], "lease_conflict");
    assert_eq!(conflict_json["allowed"], false);
    assert_eq!(conflict_json["packet"]["lease_status"], "lease_conflict");
    assert_eq!(conflict_json["packet"]["handoff_ready"], true);
    assert_eq!(conflict_json["current_lease"]["executor"], "codex");
}

#[test]
fn task_handoff_packet_carries_full_execution_policy_for_deterministic_code_nodes() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run a cron workflow with repeated local Python cost calculations without AI and email ops@example.com",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let code_task = find_task(
        json["tasks"].as_array().unwrap(),
        "Run deterministic non-AI step",
    );
    let task_id = code_task["id"].as_str().unwrap();

    let handoff = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "task",
            "handoff",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--executor",
            "local-python",
            "--ttl-seconds",
            "600",
            "--budget",
            "1600",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let handoff_json: Value = serde_json::from_slice(&handoff).unwrap();
    let packet = &handoff_json["packet"];

    assert_eq!(packet["schema_version"], "forge.executor_handoff.v8");
    assert_eq!(packet["task_executor"], "command");
    assert_eq!(packet["execution_policy_mode"], "local_code_node");
    assert_eq!(packet["execution_policy"]["mode"], "local_code_node");
    assert_eq!(packet["execution_policy"]["ai_allowed"], false);
    assert_eq!(packet["execution_policy"]["deterministic"], true);
    assert_eq!(
        packet["execution_policy"]["reuse_hint"],
        "reuse_compatible_code_node"
    );
    assert_eq!(
        packet["execution_policy"]["validation_gate"],
        "deterministic_code_node_validation_required"
    );
    assert_eq!(
        packet["execution_policy"]["code_runtime"]["language"],
        "python"
    );
    assert_eq!(
        packet["execution_policy"]["code_runtime"]["entrypoint"],
        "forge_local_python_code_node"
    );
    assert_eq!(
        packet["execution_policy"]["code_runtime"]["sandbox"],
        "local_process_no_network"
    );
    assert_eq!(packet["lease_status"], "not_requested");
    assert_eq!(packet["handoff_ready"], false);
}

#[test]
fn task_handoff_packet_exposes_resume_plan_from_checkpoint_route_key() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Handoff resumable context route",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let task_id = json["tasks"][0]["id"].as_str().unwrap();

    let first_handoff = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "task",
            "handoff",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--executor",
            "codex",
            "--ttl-seconds",
            "600",
            "--budget",
            "1200",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let first_handoff_json: Value = serde_json::from_slice(&first_handoff).unwrap();
    let first_packet = &first_handoff_json["packet"];
    let first_lease_id = first_handoff_json["lease"]["lease_id"].as_str().unwrap();
    let checkpoint_context_sha256 = first_packet["context_sha256"].as_str().unwrap();
    let checkpoint_route_key = first_packet["context_routing_cache_key"].as_str().unwrap();
    let checkpoint_revision = first_handoff_json["context"]["workflow_revision"]
        .as_u64()
        .unwrap()
        .to_string();

    let checkpoint_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "task",
            "checkpoint",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--executor",
            "codex",
            "--state",
            "paused",
            "--summary",
            "paused after first executor packet",
            "--context-sha256",
            checkpoint_context_sha256,
            "--context-routing-cache-key",
            checkpoint_route_key,
            "--workflow-revision",
            &checkpoint_revision,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let checkpoint_json: Value = serde_json::from_slice(&checkpoint_output).unwrap();
    assert_eq!(
        checkpoint_json["checkpoint"]["context_routing_cache_key"],
        checkpoint_route_key
    );

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "task",
            "release",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--lease",
            first_lease_id,
            "--executor",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let resumed_handoff = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "task",
            "handoff",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--executor",
            "codex",
            "--ttl-seconds",
            "600",
            "--budget",
            "1200",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let resumed_handoff_json: Value = serde_json::from_slice(&resumed_handoff).unwrap();
    let resumed_packet = &resumed_handoff_json["packet"];
    assert_eq!(
        resumed_packet["schema_version"],
        "forge.executor_handoff.v8"
    );
    assert_eq!(
        resumed_packet["resume_context_status"],
        "checkpoint_current"
    );
    assert_eq!(
        resumed_packet["resume_plan"]["checkpoint_id"],
        checkpoint_json["checkpoint"]["checkpoint_id"]
    );
    assert_eq!(
        resumed_packet["resume_plan"]["checkpoint_context_sha256"],
        checkpoint_context_sha256
    );
    assert_eq!(
        resumed_packet["resume_plan"]["checkpoint_context_routing_cache_key"],
        checkpoint_route_key
    );
    assert_eq!(
        resumed_packet["resume_plan"]["current_context_routing_cache_key"],
        resumed_packet["context_routing_cache_key"]
    );
    assert_eq!(
        resumed_packet["resume_plan"]["status"],
        "checkpoint_route_changed"
    );
    assert_eq!(
        resumed_packet["resume_plan"]["action"],
        "partial_retry_with_fresh_context"
    );
    assert_eq!(
        resumed_packet["resume_plan"]["partial_retry_recommended"],
        true
    );
    assert_eq!(
        resumed_packet["resume_plan"]["reason"],
        "checkpoint route differs from current context route"
    );
    assert_eq!(
        resumed_packet["context_delta"]["schema_version"],
        "forge.context.delta.v1"
    );
    assert_eq!(resumed_packet["context_delta"]["status"], "route_changed");
    assert_eq!(
        resumed_packet["context_delta"]["partial_retry_recommended"],
        true
    );
}

#[test]
fn task_handoff_packet_carries_node_scoped_persona_contract() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Create an operator report with auditable persona routing",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let documentation_task = find_task(json["tasks"].as_array().unwrap(), "Generate documentation");
    let task_id = documentation_task["id"].as_str().unwrap();

    for dependency in documentation_task["dependencies"].as_array().unwrap() {
        set_task_status_in_stored_workflow(
            &store,
            workflow_id,
            dependency.as_str().unwrap(),
            "completed",
        );
    }

    let handoff = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "task",
            "handoff",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--executor",
            "codex",
            "--ttl-seconds",
            "600",
            "--budget",
            "2400",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let handoff_json: Value = serde_json::from_slice(&handoff).unwrap();
    let packet = &handoff_json["packet"];
    let contract = &packet["persona_contract"];

    assert_eq!(packet["schema_version"], "forge.executor_handoff.v8");
    assert_eq!(packet["persona_mode"], "operator_report");
    assert_eq!(packet["persona_profile_id"], "persona.operator_report.v1");
    assert_eq!(contract["schema_version"], "forge.persona_handoff.v2");
    assert_eq!(contract["profile_id"], "persona.operator_report.v1");
    assert_eq!(contract["mode"], "operator_report");
    assert_eq!(contract["scope"], "node");
    assert_eq!(
        contract["instruction_source"],
        "forge_personality_soul_routing_v1"
    );
    assert_eq!(contract["validation_gate"], "persona_routing_required");
    assert_eq!(contract["auditable"], true);
    assert_eq!(
        contract["lineage_sha256"],
        handoff_json["context"]["lineage"]["lineage_sha256"]
    );
    assert_eq!(
        contract["persona_mode_sha256"],
        handoff_json["context"]["lineage"]["persona_mode_sha256"]
    );
    assert_eq!(
        packet["persona_profile_sha256"],
        handoff_json["context"]["persona_profile"]["profile_sha256"]
    );
    assert_eq!(
        contract["profile_sha256"],
        handoff_json["context"]["lineage"]["persona_profile_sha256"]
    );
    assert!(contract["routing_rationale"]
        .as_str()
        .unwrap()
        .contains("node-scoped human-facing artifact"));
    assert!(contract["source_models"]
        .as_array()
        .unwrap()
        .contains(&Value::String(
            "codex_developer_personality_instructions".to_string()
        )));
    assert!(contract["source_models"]
        .as_array()
        .unwrap()
        .contains(&Value::String(
            "paperclip_soul_voice_tone_persona".to_string()
        )));
    assert!(contract["source_model_summaries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|source| source["model_id"] == "codex_developer_personality_instructions"));
    assert!(contract["source_model_summaries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|source| source["model_id"] == "paperclip_soul_voice_tone_persona"));
}

#[test]
fn task_handoff_does_not_acquire_lease_when_strict_context_is_blocked() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run executor handoff packets with a deliberately tiny context budget",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let task_id = json["tasks"][0]["id"].as_str().unwrap();

    let blocked = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "task",
            "handoff",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--executor",
            "codex",
            "--ttl-seconds",
            "600",
            "--budget",
            "128",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let blocked_json: Value = serde_json::from_slice(&blocked).unwrap();
    assert_eq!(blocked_json["status"], "handoff_blocked");
    assert_eq!(blocked_json["allowed"], false);
    assert_eq!(blocked_json["lease"], Value::Null);
    assert_eq!(blocked_json["current_lease"], Value::Null);
    assert_eq!(blocked_json["packet"]["lease_status"], "not_requested");
    assert_eq!(blocked_json["packet"]["handoff_ready"], false);
    assert!(blocked_json["context"]["missing_required_sections"]
        .as_array()
        .unwrap()
        .iter()
        .any(|section| section == "context_requirements"));

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "task",
            "acquire",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--executor",
            "opencode",
            "--ttl-seconds",
            "600",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("\"executor\": \"opencode\""));
}

#[test]
fn task_validate_response_accepts_completed_executor_response_with_passing_evidence() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Validate executor response contracts",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let task_id = json["tasks"][0]["id"].as_str().unwrap();
    let response_path = temp.path().join("executor-response.json");
    fs::write(
        &response_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": "forge.executor_response.v1",
            "task_id": task_id,
            "status": "completed",
            "artifacts": ["artifacts/executor-response-summary.md"],
            "trace_ref": "traces/task-001.jsonl",
            "cost": {
                "estimated_usd": 0.12,
                "tokens_in": 1200,
                "tokens_out": 220
            },
            "validation_evidence": [
                {
                    "command": "cargo test",
                    "exit_code": 0,
                    "summary": "tests passed"
                }
            ]
        }))
        .unwrap(),
    )
    .unwrap();

    let validation_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "task",
            "validate-response",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--response",
            response_path.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let validation: Value = serde_json::from_slice(&validation_output).unwrap();
    assert_eq!(
        validation["schema_version"],
        "forge.executor_response_validation.v1"
    );
    assert_eq!(validation["status"], "accepted");
    assert_eq!(validation["accepted"], true);
    assert_eq!(validation["workflow_id"], workflow_id);
    assert_eq!(validation["task_id"], task_id);
    assert_eq!(validation["response_status"], "completed");
    assert_eq!(
        validation["response_schema_version"],
        "forge.executor_response.v1"
    );
    assert_eq!(validation["validation_summary"]["total"], 1);
    assert_eq!(validation["validation_summary"]["passing"], 1);
    assert_eq!(validation["validation_summary"]["failing"], 0);
    assert_eq!(validation["violations"], serde_json::json!([]));
    assert_eq!(validation["response_sha256"].as_str().unwrap().len(), 64);
}

#[test]
fn task_validate_response_rejects_completed_executor_response_without_passing_evidence() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Reject invalid executor response contracts",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let task_id = json["tasks"][0]["id"].as_str().unwrap();
    let response_path = temp.path().join("bad-executor-response.json");
    fs::write(
        &response_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": "forge.executor_response.v1",
            "task_id": task_id,
            "status": "completed",
            "artifacts": [],
            "trace_ref": "",
            "cost": {
                "estimated_usd": -1.0,
                "tokens_in": 10,
                "tokens_out": 4
            },
            "validation_evidence": [
                {
                    "command": "cargo test",
                    "exit_code": 1,
                    "summary": "tests failed"
                }
            ]
        }))
        .unwrap(),
    )
    .unwrap();

    let validation_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "task",
            "validate-response",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--response",
            response_path.to_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();

    let validation: Value = serde_json::from_slice(&validation_output).unwrap();
    assert_eq!(validation["status"], "rejected");
    assert_eq!(validation["accepted"], false);
    assert_eq!(validation["validation_summary"]["failing"], 1);
    assert!(validation["violations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|violation| violation["code"] == "trace_ref_required"));
    assert!(validation["violations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|violation| violation["code"] == "cost_estimated_usd_non_negative"));
    assert!(validation["violations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|violation| violation["code"] == "completed_requires_passing_validation_evidence"));
}

#[test]
fn self_run_rejects_stop_date_in_the_past() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "self",
            "run",
            "--repo",
            temp.path().to_str().unwrap(),
            "--until",
            "2000-01-01T00:00:00-03:00",
            "--dry-run",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("stop date is in the past"));
}

#[test]
fn self_run_prompt_uses_persisted_self_evolution_goal_updates() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let repo = temp.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    let base_goal =
        "Improve Forge Core autonomously with bounded executor cycles, validation gates, artifacts and changelog";
    let updated_goal = format!(
        "{base_goal}. Prioritize persisted goal propagation, clusterization and n8n node research before generic context-routing backlog."
    );

    let planned_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            base_goal,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let planned: Value = serde_json::from_slice(&planned_output).unwrap();
    let strategic_workflow_id = planned["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "update-goal",
            "--workflow",
            strategic_workflow_id,
            "--origin",
            "codex",
            "--goal",
            &updated_goal,
            "--output",
            "json",
        ])
        .assert()
        .success();

    let self_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "self",
            "run",
            "--repo",
            repo.to_str().unwrap(),
            "--until",
            "2999-01-01T00:00:00-03:00",
            "--dry-run",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let self_report: Value = serde_json::from_slice(&self_output).unwrap();
    assert_eq!(self_report["status"], "planned");
    assert_eq!(
        self_report["cycle_reports"][0]["prompt_packet_version"],
        "forge.self_evolution.prompt.v2"
    );

    let prompt_path = self_report["cycle_reports"][0]["prompt_path"]
        .as_str()
        .unwrap();
    let prompt = fs::read_to_string(temp.path().join(prompt_path)).unwrap();
    assert!(prompt.contains("Persisted Forge workflow goal (authoritative):"));
    assert!(prompt.contains(&updated_goal));
    assert!(prompt.contains("future self-evolution cycles must honor it before generic guidance"));

    let self_workflow_id = self_report["workflow_id"].as_str().unwrap();
    let status_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "status",
            "--workflow",
            self_workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let status: Value = serde_json::from_slice(&status_output).unwrap();
    assert_eq!(status["goal"], updated_goal);
}

#[test]
fn cluster_registry_records_nodes_and_places_deterministic_code_task_by_capability() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let linux_register = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "cluster",
            "register",
            "--node-id",
            "lan-linux-ai",
            "--name",
            "LAN Linux AI Worker",
            "--endpoint",
            "ssh://forge@lan-linux",
            "--os",
            "linux",
            "--arch",
            "x86_64",
            "--cpu-cores",
            "16",
            "--memory-gb",
            "64",
            "--gpu",
            "nvidia-rtx-4090",
            "--software",
            "python3",
            "--software",
            "node",
            "--capability",
            "python",
            "--capability",
            "nodejs",
            "--capability",
            "docker",
            "--capability",
            "gpu",
            "--python",
            "--node",
            "--docker",
            "--gpu-available",
            "--network-reachable",
            "--status",
            "online",
            "--trust",
            "trusted_lan",
            "--sandbox",
            "local_process_no_network",
            "--sandbox",
            "ssh_command_no_sudo",
            "--cost-per-hour-usd",
            "0.42",
            "--latency-ms",
            "4",
            "--reliability",
            "0.99",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let linux_register: Value = serde_json::from_slice(&linux_register).unwrap();
    assert_eq!(linux_register["status"], "registered");
    assert_eq!(
        linux_register["node"]["schema_version"],
        "forge.cluster_node.v1"
    );
    assert_eq!(linux_register["node"]["node_id"], "lan-linux-ai");
    assert_eq!(linux_register["node"]["python_available"], true);

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "cluster",
            "register",
            "--node-id",
            "lan-windows-mt5",
            "--name",
            "LAN Windows MT5 Terminal",
            "--endpoint",
            "ssh://forge@lan-windows",
            "--os",
            "windows",
            "--arch",
            "x86_64",
            "--cpu-cores",
            "8",
            "--memory-gb",
            "32",
            "--software",
            "MetaTrader 5",
            "--capability",
            "metatrader5",
            "--network-reachable",
            "--status",
            "online",
            "--trust",
            "trusted_lan",
            "--sandbox",
            "windows_desktop_user_session",
            "--cost-per-hour-usd",
            "0.20",
            "--latency-ms",
            "11",
            "--reliability",
            "0.97",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let cluster_list = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "cluster",
            "list",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let cluster_list: Value = serde_json::from_slice(&cluster_list).unwrap();
    assert_eq!(cluster_list["schema_version"], "forge.cluster_registry.v1");
    assert_eq!(cluster_list["summary"]["total_nodes"], 2);
    assert_eq!(cluster_list["summary"]["online_nodes"], 2);
    assert_eq!(cluster_list["summary"]["python_nodes"], 1);
    assert_eq!(cluster_list["summary"]["windows_nodes"], 1);
    assert_eq!(cluster_list["summary"]["metatrader5_nodes"], 1);

    let planned_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run repeated local Python calculations without AI",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let planned: Value = serde_json::from_slice(&planned_output).unwrap();
    let workflow_id = planned["workflow_id"].as_str().unwrap();
    let code_task = find_task(
        planned["tasks"].as_array().unwrap(),
        "Run deterministic non-AI step",
    );

    let placement = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "cluster",
            "place",
            "--workflow",
            workflow_id,
            "--task",
            code_task["id"].as_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let placement: Value = serde_json::from_slice(&placement).unwrap();
    assert_eq!(placement["schema_version"], "forge.cluster_placement.v1");
    assert_eq!(placement["status"], "placement_selected");
    assert_eq!(placement["requirements"]["executor"], "command");
    assert_eq!(placement["requirements"]["policy_mode"], "local_code_node");
    assert_eq!(placement["requirements"]["mutation_allowed"], false);
    assert!(placement["requirements"]["required_capabilities"]
        .as_array()
        .unwrap()
        .contains(&Value::String("python".to_string())));
    assert_eq!(placement["selected_node"]["node_id"], "lan-linux-ai");

    let rejected_windows = placement["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|candidate| candidate["node_id"] == "lan-windows-mt5")
        .unwrap();
    assert_eq!(rejected_windows["eligible"], false);
    assert!(rejected_windows["reasons"]
        .as_array()
        .unwrap()
        .contains(&Value::String("missing capability python".to_string())));
}

fn find_executor<'a>(json: &'a Value, id: &str) -> &'a Value {
    json["executors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|executor| executor["id"] == id)
        .unwrap()
}

fn find_runtime<'a>(json: &'a Value, id: &str) -> &'a Value {
    json["runtimes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|runtime| runtime["id"] == id)
        .unwrap()
}

fn find_workflow<'a>(json: &'a Value, id: &str) -> &'a Value {
    json["workflows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|workflow| workflow["workflow_id"] == id)
        .unwrap()
}

fn find_quality_action<'a>(json: &'a Value, action: &str) -> &'a Value {
    json["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["action"] == action)
        .unwrap()
}

fn find_context_action<'a>(json: &'a Value, action: &str) -> &'a Value {
    json["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["action"] == action)
        .unwrap()
}

fn find_task<'a>(tasks: &'a [Value], title: &str) -> &'a Value {
    tasks.iter().find(|task| task["title"] == title).unwrap()
}

fn remove_legacy_fields_from_stored_workflow(store: &Path, workflow_id: &str) {
    let connection = Connection::open(store).unwrap();
    let data_json: String = connection
        .query_row(
            "SELECT data_json FROM workflows WHERE id = ?1",
            [workflow_id],
            |row| row.get(0),
        )
        .unwrap();
    let mut workflow: Value = serde_json::from_str(&data_json).unwrap();
    workflow.as_object_mut().unwrap().remove("revisions");
    for task in workflow["tasks"].as_array_mut().unwrap() {
        task.as_object_mut().unwrap().remove("async_policy");
    }
    let patched = serde_json::to_string(&workflow).unwrap();
    connection
        .execute(
            "UPDATE workflows SET data_json = ?1 WHERE id = ?2",
            (&patched, workflow_id),
        )
        .unwrap();
}

fn make_persona_routing_non_auditable(store: &Path, workflow_id: &str, title: &str) {
    let connection = Connection::open(store).unwrap();
    let data_json: String = connection
        .query_row(
            "SELECT data_json FROM workflows WHERE id = ?1",
            [workflow_id],
            |row| row.get(0),
        )
        .unwrap();
    let mut workflow: Value = serde_json::from_str(&data_json).unwrap();
    let task = workflow["tasks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|task| task["title"] == title)
        .unwrap();
    let persona = task["persona"].as_object_mut().unwrap();
    persona.insert("scope".to_string(), Value::String("workflow".to_string()));
    persona.insert("auditable".to_string(), Value::Bool(false));
    persona.insert(
        "validation_gate".to_string(),
        Value::String("manual_review_optional".to_string()),
    );
    persona.insert(
        "source_models".to_string(),
        Value::Array(vec![Value::String(
            "codex_developer_personality_instructions".to_string(),
        )]),
    );
    let patched = serde_json::to_string(&workflow).unwrap();
    connection
        .execute(
            "UPDATE workflows SET data_json = ?1 WHERE id = ?2",
            (&patched, workflow_id),
        )
        .unwrap();
}

fn append_child_subflow_to_stored_workflow(
    store: &Path,
    workflow_id: &str,
    task_id: &str,
    child_subflow: Value,
) {
    let connection = Connection::open(store).unwrap();
    let data_json: String = connection
        .query_row(
            "SELECT data_json FROM workflows WHERE id = ?1",
            [workflow_id],
            |row| row.get(0),
        )
        .unwrap();
    let mut workflow: Value = serde_json::from_str(&data_json).unwrap();
    let task = workflow["tasks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|task| task["id"] == task_id)
        .unwrap();
    task["child_subflows"]
        .as_array_mut()
        .unwrap()
        .push(child_subflow);
    let patched = serde_json::to_string(&workflow).unwrap();
    connection
        .execute(
            "UPDATE workflows SET data_json = ?1 WHERE id = ?2",
            (&patched, workflow_id),
        )
        .unwrap();
}

fn set_task_status_in_stored_workflow(
    store: &Path,
    workflow_id: &str,
    task_id: &str,
    status: &str,
) {
    let connection = Connection::open(store).unwrap();
    let data_json: String = connection
        .query_row(
            "SELECT data_json FROM workflows WHERE id = ?1",
            [workflow_id],
            |row| row.get(0),
        )
        .unwrap();
    let mut workflow: Value = serde_json::from_str(&data_json).unwrap();
    let task = workflow["tasks"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|task| task["id"] == task_id)
        .unwrap();
    task.as_object_mut()
        .unwrap()
        .insert("status".to_string(), Value::String(status.to_string()));
    let patched = serde_json::to_string(&workflow).unwrap();
    connection
        .execute(
            "UPDATE workflows SET data_json = ?1 WHERE id = ?2",
            (&patched, workflow_id),
        )
        .unwrap();
}

fn write_fake_cli(bin: &Path, name: &str) {
    fs::create_dir_all(bin).unwrap();
    let path = bin.join(name);
    fs::write(&path, "#!/usr/bin/env sh\nexit 0\n").unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).unwrap();
    }
}
