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
    let task_id = json["tasks"][0]["id"].as_str().unwrap();

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
    let task_id = json["tasks"][0]["id"].as_str().unwrap();

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
    assert_eq!(context["schema_version"], "forge.context.v3");
    assert_eq!(
        context["routing_policy"],
        "task_local_revisioned_persona_budget_v3"
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
    assert_eq!(context["schema_version"], "forge.context.v3");
    assert_eq!(
        context["routing_policy"],
        "task_local_revisioned_persona_budget_v3"
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
    assert_eq!(context["schema_version"], "forge.context.v3");
    assert_eq!(
        context["routing_policy"],
        "task_local_revisioned_persona_budget_v3"
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
        "forge.self_evolution.prompt.v1"
    );

    let prompt_path = temp
        .path()
        .join(cycle_report["prompt_path"].as_str().unwrap());
    let prompt = fs::read_to_string(prompt_path).unwrap();
    assert!(prompt.contains("Prompt packet version: `forge.self_evolution.prompt.v1`"));
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
        "forge.self_evolution.prompt.v1"
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
        "forge.self_evolution.prompt.v1"
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
