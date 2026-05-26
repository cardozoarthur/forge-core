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
fn milestone_status_surfaces_05_boundary_and_promotion_gate() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "milestone",
            "status",
            "--version",
            "0.5",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["schema_version"], "forge.milestone.status.v1");
    assert_eq!(json["milestone"], "0.5");
    assert_eq!(
        json["status_vocabulary"],
        serde_json::json!([
            "implemented",
            "validated",
            "groundwork",
            "planned",
            "blocked"
        ])
    );
    assert_eq!(json["promotion_decision"]["decision"], "promote");
    assert_eq!(json["promotion_decision"]["promotable"], true);
    let blocked_by = json["promotion_decision"]["blocked_by"].as_array().unwrap();
    assert!(
        !blocked_by.contains(&serde_json::json!("creative_artifact_ir")),
        "creative_artifact_ir is now validated and should not block"
    );
    assert!(
        !blocked_by.contains(&serde_json::json!("export_demo_baseline")),
        "export_demo_baseline is now validated and should not block"
    );

    let capabilities = json["capabilities"].as_array().unwrap();
    let scheduler = capabilities
        .iter()
        .find(|capability| capability["id"] == "scheduler_loop_subflow_foundation")
        .unwrap();
    assert_eq!(scheduler["status"], "validated");

    let creative_ir = capabilities
        .iter()
        .find(|capability| capability["id"] == "creative_artifact_ir")
        .unwrap();
    assert_eq!(creative_ir["status"], "validated");
    assert!(creative_ir["gap_before_promotion"]
        .as_str()
        .unwrap()
        .contains("0.5"));
    assert_eq!(json["summary"]["validated"].as_u64().unwrap(), 9);
    assert_eq!(json["summary"]["groundwork"].as_u64().unwrap(), 0);
    assert_eq!(json["summary"]["planned"].as_u64().unwrap(), 0);
}

#[test]
fn mcp_exposes_milestone_status_for_agent_runtime_boundaries() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let tools = forge()
        .args(["mcp", "tools", "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let manifest: Value = serde_json::from_slice(&tools).unwrap();
    assert!(manifest["tools"].as_array().unwrap().iter().any(|tool| {
        tool["name"] == "forge.milestone.status"
            && tool["output_schema"] == "forge.milestone.status.v1"
            && tool["async_safe"] == true
            && tool["mutates_workflow"] == false
    }));

    let call = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.milestone.status"])
        .arg("--input")
        .arg(r#"{"version":"0.5"}"#)
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&call).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(
        json["result"]["schema_version"],
        "forge.milestone.status.v1"
    );
    assert_eq!(json["result"]["milestone"], "0.5");
    assert_eq!(json["result"]["promotion_decision"]["decision"], "promote");
    assert!(json["result"]["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|capability| capability["id"] == "export_demo_baseline"
            && capability["status"] == "validated"));
    assert!(json["result"]["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |capability| capability["id"] == "design_tokens" && capability["status"] == "validated"
        ));
    assert!(json["result"]["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|capability| capability["id"] == "live_collaboration"
            && capability["status"] == "validated"));
}

#[test]
fn milestone_manifest_surfaces_requirements_evidence_gaps_and_promotion_decision() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "milestone",
            "manifest",
            "--version",
            "0.5",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["schema_version"], "forge.milestone.manifest.v1");
    assert_eq!(json["milestone"], "0.5");
    assert!(json["release_line_boundary"]
        .as_str()
        .unwrap()
        .contains("0.4.x"));
    assert!(json["requirements"].as_array().unwrap().len() >= 9);
    assert!(json["completed_capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |capability| capability["id"] == "scheduler_loop_subflow_foundation"
                && capability["promotion_ready"] == true
        ));
    assert!(json["completed_capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|capability| capability["id"] == "export_demo_baseline"
            && capability["promotion_ready"] == true));
    assert!(json["completed_capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |capability| capability["id"] == "research_artifact_baseline"
                && capability["promotion_ready"] == true
        ));
    assert!(json["validation_evidence"]
        .as_array()
        .unwrap()
        .iter()
        .any(|evidence| evidence["capability_id"] == "creative_artifact_ir"));
    assert!(json["demos"]
        .as_array()
        .unwrap()
        .iter()
        .any(|demo| demo["capability_id"] == "export_demo_baseline"));
    assert!(json["known_gaps"]
        .as_array()
        .unwrap()
        .iter()
        .all(|gap| gap["capability_id"] != "live_collaboration"));
    assert_eq!(json["promotion_decision"]["decision"], "promote");
    assert_eq!(json["promotion_decision"]["promotable"], true);
    assert!(
        json["promotion_decision"]["blocked_by"]
            .as_array()
            .unwrap()
            .is_empty(),
        "all 9 capabilities are now validated; promotion should be blocked_by empty"
    );
}

#[test]
fn milestone_boundary_document_matches_validated_export_demo_runtime_state() {
    let docs = fs::read_to_string("docs/forge-0.5-milestone.md").unwrap();

    assert!(
        docs.contains("| Export/demo baseline | validated |"),
        "the visible 0.5 milestone boundary must match the runtime manifest after export-demo validation"
    );
    assert!(
        docs.contains("forge milestone export-demo"),
        "the milestone boundary should expose the native export-demo command as validation evidence"
    );
    assert!(
        docs.contains("promotion decision"),
        "the milestone boundary should explain that promotion is a gated runtime decision"
    );
}

#[test]
fn packaged_skill_mentions_export_demo_agent_surface() {
    assert!(
        forge_core::skill::SKILL_MD.contains("forge milestone export-demo"),
        "the packaged Forge skill should teach agents how to generate export/demo evidence"
    );
    assert!(
        forge_core::skill::SKILL_MD.contains("forge.milestone.export_demo"),
        "the packaged Forge skill should expose the MCP export-demo tool to agent callers"
    );
}

#[test]
fn packaged_skill_mentions_experimental_multimodal_agent_surface() {
    assert!(
        forge_core::skill::SKILL_MD.contains("forge multimodal status"),
        "the packaged Forge skill should teach agents how to inspect experimental multimodal status"
    );
    assert!(
        forge_core::skill::SKILL_MD.contains("forge.multimodal.status"),
        "the packaged Forge skill should expose the MCP multimodal status tool to agent callers"
    );
    assert!(
        forge_core::skill::SKILL_MD.contains("forge multimodal guard"),
        "the packaged Forge skill should teach agents to route camera/screen/input access through runtime guards"
    );
}

#[test]
fn multimodal_status_is_experimental_disabled_by_default() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "multimodal",
            "status",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["schema_version"], "forge.multimodal.status.v1");
    assert_eq!(json["status"], "experimental_disabled");
    assert_eq!(json["feature_flag"]["enabled"], false);
    assert_eq!(json["installs_performed"], false);
    assert!(json["capabilities"].as_array().unwrap().iter().any(|cap| {
        cap["id"] == "screen_understanding"
            && cap["permission_scope"] == "screen"
            && cap["state"] == "missing"
    }));
    assert!(json["capabilities"].as_array().unwrap().iter().any(|cap| {
        cap["id"] == "blender_asset_processing" && cap["permission_scope"] == "filesystem"
    }));
    assert!(json["runtime_guards"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("kill_switch")));
}

#[test]
fn multimodal_install_plan_is_plan_only_and_mcp_visible() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "multimodal",
            "install-plan",
            "--capability",
            "audio_transcription",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["schema_version"], "forge.multimodal.install_plan.v1");
    assert_eq!(json["status"], "plan_only");
    assert_eq!(json["capability_id"], "audio_transcription");
    assert_eq!(json["installs_performed"], false);
    assert_eq!(json["requires_human_approval"], true);
    assert!(json["rollback_steps"].as_array().unwrap().len() >= 2);

    let tools = forge()
        .args(["mcp", "tools", "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let manifest: Value = serde_json::from_slice(&tools).unwrap();
    assert!(manifest["tools"].as_array().unwrap().iter().any(|tool| {
        tool["name"] == "forge.multimodal.status"
            && tool["output_schema"] == "forge.multimodal.status.v1"
            && tool["async_safe"] == true
            && tool["mutates_workflow"] == false
    }));
    assert!(manifest["tools"].as_array().unwrap().iter().any(|tool| {
        tool["name"] == "forge.multimodal.install_plan"
            && tool["output_schema"] == "forge.multimodal.install_plan.v1"
            && tool["async_safe"] == true
            && tool["mutates_workflow"] == false
    }));
}

#[test]
fn multimodal_runtime_guard_requires_feature_flag_and_explicit_allow() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let denied = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "multimodal",
            "guard",
            "--capability",
            "camera",
            "--action",
            "access",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let denied_json: Value = serde_json::from_slice(&denied).unwrap();
    assert_eq!(denied_json["schema_version"], "forge.multimodal.guard.v1");
    assert_eq!(denied_json["allowed"], false);
    assert_eq!(denied_json["feature_flag_enabled"], false);
    assert_eq!(denied_json["requires_human_approval"], true);
    assert_eq!(denied_json["audit_required"], true);

    let allowed = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "multimodal",
            "guard",
            "--capability",
            "camera",
            "--action",
            "access",
            "--enable-experimental",
            "--allow",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let allowed_json: Value = serde_json::from_slice(&allowed).unwrap();
    assert_eq!(allowed_json["allowed"], true);
    assert_eq!(allowed_json["feature_flag_enabled"], true);
    assert_eq!(allowed_json["explicit_allow"], true);
    assert!(allowed_json["guardrails"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("dry_run_or_simulation_first")));
}

#[test]
fn mcp_can_call_multimodal_status_without_enabling_runtime_access() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.multimodal.status"])
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(
        json["result"]["schema_version"],
        "forge.multimodal.status.v1"
    );
    assert_eq!(json["result"]["feature_flag"]["enabled"], false);
    assert_eq!(json["result"]["installs_performed"], false);
}

#[test]
fn mcp_exposes_milestone_manifest_for_agent_release_gates() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let tools = forge()
        .args(["mcp", "tools", "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let manifest: Value = serde_json::from_slice(&tools).unwrap();
    assert!(manifest["tools"].as_array().unwrap().iter().any(|tool| {
        tool["name"] == "forge.milestone.manifest"
            && tool["output_schema"] == "forge.milestone.manifest.v1"
            && tool["async_safe"] == true
            && tool["mutates_workflow"] == false
    }));

    let call = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.milestone.manifest"])
        .arg("--input")
        .arg(r#"{"version":"0.5"}"#)
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&call).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(
        json["result"]["schema_version"],
        "forge.milestone.manifest.v1"
    );
    assert_eq!(json["result"]["milestone"], "0.5");
    assert!(json["result"]["completed_capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|capability| capability["id"] == "research_artifact_baseline"));
    assert!(json["result"]["completed_capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|capability| capability["id"] == "export_demo_baseline"));
    assert!(json["result"]["missing_capabilities"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn milestone_research_baseline_is_source_grounded_and_agent_visible() {
    use forge_core::milestone::build_milestone_research;

    let report = build_milestone_research("0.5").unwrap();
    assert_eq!(report.schema_version, "forge.milestone.research.v1");
    assert_eq!(report.status, "validated");
    assert!(report.sources.len() >= 8);
    assert!(report
        .sources
        .iter()
        .any(|source| source.label.contains("Penpot")));
    assert!(report
        .validation_gates
        .iter()
        .any(|gate| gate.id == "creative_ir_round_trip_fidelity"));
    assert!(report
        .workflow_templates
        .iter()
        .any(|template| template.id == "ai_first_whiteboard_brainstorm"));

    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "milestone",
            "research",
            "--version",
            "0.5",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["schema_version"], "forge.milestone.research.v1");
    assert_eq!(json["status"], "validated");

    let tools = forge()
        .args(["mcp", "tools", "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let manifest: Value = serde_json::from_slice(&tools).unwrap();
    assert!(manifest["tools"].as_array().unwrap().iter().any(|tool| {
        tool["name"] == "forge.milestone.research"
            && tool["output_schema"] == "forge.milestone.research.v1"
            && tool["async_safe"] == true
            && tool["mutates_workflow"] == false
    }));
}

#[test]
fn milestone_export_demo_creates_workflow_with_creative_artifacts_and_tokens() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "milestone",
            "export-demo",
            "--origin",
            "test",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["schema_version"], "forge.milestone.export_demo.v1");
    assert_eq!(json["status"], "export_demo_generated");
    assert!(json["workflow_id"].as_str().unwrap().starts_with("wf_"));
    assert!(json["screen_artifact_id"]
        .as_str()
        .unwrap()
        .starts_with("ca_"));
    assert!(json["document_artifact_id"]
        .as_str()
        .unwrap()
        .starts_with("ca_"));
    assert_eq!(json["token_collection_name"], "export_demo_tokens");
    assert_eq!(json["goal"], "hackathon");
    let artifacts = json["demo_artifacts"].as_array().unwrap();
    assert!(artifacts.iter().any(|a| a["kind"] == "scheduled_workflow"));
    assert!(artifacts.iter().any(|a| a["kind"] == "creative_screen"));
    assert!(artifacts.iter().any(|a| a["kind"] == "creative_document"));
    assert!(artifacts.iter().any(|a| a["kind"] == "design_tokens"));
    let lineage = json["lineage_chain"].as_array().unwrap();
    assert!(lineage.len() >= 3);
    assert!(json["export_evidence"]
        .as_str()
        .unwrap()
        .contains("forge.milestone.export_demo.v1"));
}

#[test]
fn mcp_exposes_milestone_export_demo_tool() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let tools = forge()
        .args(["mcp", "tools", "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let manifest: Value = serde_json::from_slice(&tools).unwrap();
    assert!(manifest["tools"].as_array().unwrap().iter().any(|tool| {
        tool["name"] == "forge.milestone.export_demo"
            && tool["output_schema"] == "forge.milestone.export_demo.v1"
            && tool["async_safe"] == false
            && tool["mutates_workflow"] == true
    }));

    let call = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.milestone.export_demo"])
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&call).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(
        json["result"]["schema_version"],
        "forge.milestone.export_demo.v1"
    );
    assert_eq!(json["result"]["status"], "export_demo_generated");
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
fn plan_models_daily_goal_research_as_native_cron_loop_subflow_graph() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Create daily Goal research workflow for Goals: hackathon in America/Sao_Paulo",
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

    let schedule = find_task(tasks, "Schedule daily Goal research");
    assert_eq!(schedule["executor"], "wait");
    assert_eq!(schedule["schedule"]["schema_version"], "forge.schedule.v1");
    assert_eq!(schedule["schedule"]["kind"], "cron");
    assert_eq!(schedule["schedule"]["cron"], "0 8 * * *");
    assert_eq!(schedule["schedule"]["timezone"], "America/Sao_Paulo");
    assert_eq!(
        schedule["schedule"]["missed_run_policy"],
        "run_once_then_resume"
    );
    assert_eq!(schedule["schedule"]["scale_to_zero_when_idle"], true);
    assert!(schedule["schedule"]["next_run_at"]
        .as_str()
        .unwrap()
        .contains('T'));
    assert!(schedule["schedule"]["run_history"]
        .as_array()
        .unwrap()
        .is_empty());

    let loop_node = find_task(tasks, "Loop over configured Goals");
    assert_eq!(loop_node["loop_control"]["schema_version"], "forge.loop.v1");
    assert_eq!(loop_node["loop_control"]["kind"], "loop_over_items");
    assert_eq!(
        loop_node["loop_control"]["items"],
        serde_json::json!(["hackathon"])
    );
    assert_eq!(loop_node["loop_control"]["subflow_mode"], "finite_per_item");
    assert_eq!(
        loop_node["loop_control"]["stop_policy"],
        "pause_mutate_or_stop"
    );

    let search = find_task(tasks, "Search hackathon opportunities with DuckDuckGo");
    assert_eq!(search["executor"], "command");
    assert_eq!(search["execution_policy"]["ai_allowed"], false);
    assert_eq!(
        search["native_subflow"]["subflow_id"],
        "goal_research:hackathon"
    );
    assert_eq!(search["native_subflow"]["mode"], "finite");
    assert_eq!(search["native_subflow"]["triggered_by"], "loop:task-010");
    assert_eq!(
        search["native_subflow"]["lineage"]["workflow_id_policy"],
        "inherit_parent_workflow_id"
    );
    assert_eq!(
        search["native_subflow"]["lineage"]["artifact_lineage_policy"],
        "attach_to_parent_run_and_goal"
    );

    let evaluation = find_task(tasks, "Evaluate hackathon Goal fit");
    assert_eq!(evaluation["executor"], "ai");
    assert_eq!(evaluation["execution_policy"]["ai_allowed"], true);

    for title in [
        "Generate hackathon Markdown report",
        "Generate hackathon PDF report",
        "Record hackathon Telegram delivery",
    ] {
        let task = find_task(tasks, title);
        assert_eq!(
            task["native_subflow"]["subflow_id"],
            "goal_research:hackathon"
        );
    }
}

#[test]
fn inspect_and_list_surface_schedule_and_loop_visibility() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let planned = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Create daily Goal research workflow for Goals: hackathon in America/Sao_Paulo",
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

    let inspected = forge()
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
    let inspected_json: Value = serde_json::from_slice(&inspected).unwrap();
    assert_eq!(
        inspected_json["schedule_summary"]["schema_version"],
        "forge.schedule.summary.v1"
    );
    assert_eq!(inspected_json["schedule_summary"]["scheduled_nodes"], 1);
    assert_eq!(
        inspected_json["schedule_summary"]["scale_to_zero_when_idle_nodes"],
        1
    );
    assert_eq!(
        inspected_json["loop_summary"]["schema_version"],
        "forge.loop.summary.v1"
    );
    assert_eq!(inspected_json["loop_summary"]["loop_nodes"], 1);
    assert_eq!(inspected_json["loop_summary"]["loop_over_items_nodes"], 1);
    assert!(inspected_json["diagram"]
        .as_str()
        .unwrap()
        .contains("schedule cron 0 8 * * * America/Sao_Paulo"));
    assert!(inspected_json["diagram"]
        .as_str()
        .unwrap()
        .contains("loop loop_over_items items hackathon"));

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
    assert_eq!(row["schedule_summary"]["scheduled_nodes"], 1);
    assert_eq!(row["loop_summary"]["loop_nodes"], 1);
}

#[test]
fn schedule_list_surfaces_only_scheduled_or_looping_workflows_for_cli_and_mcp() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let regular = forge()
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
    let regular_json: Value = serde_json::from_slice(&regular).unwrap();
    let regular_workflow_id = regular_json["workflow_id"].as_str().unwrap().to_string();

    let scheduled = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "create-daily-goal-research",
            "--goal",
            "hackathon",
            "--timezone",
            "America/Sao_Paulo",
            "--cron",
            "0 8 * * *",
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
    let scheduled_json: Value = serde_json::from_slice(&scheduled).unwrap();
    let scheduled_workflow_id = scheduled_json["workflow_id"].as_str().unwrap().to_string();

    let listed = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
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
    assert_eq!(listed_json["summary"]["total"], 1);
    assert!(workflow_ids(&listed_json).contains(&scheduled_workflow_id));
    assert!(!workflow_ids(&listed_json).contains(&regular_workflow_id));

    let mcp_listed = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.schedule.list"])
        .arg("--input")
        .arg(r#"{}"#)
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let mcp_json: Value = serde_json::from_slice(&mcp_listed).unwrap();
    assert_eq!(mcp_json["status"], "ok");
    assert_eq!(mcp_json["result"]["summary"]["total"], 1);
    assert!(workflow_ids(&mcp_json["result"]).contains(&scheduled_workflow_id));
    assert!(!workflow_ids(&mcp_json["result"]).contains(&regular_workflow_id));
}

#[test]
fn mcp_creates_daily_goal_research_workflow_and_exposes_schedule_loop_tools() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let manifest = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "tools",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let manifest_json: Value = serde_json::from_slice(&manifest).unwrap();
    for name in [
        "forge.schedule.create_daily_goal_research",
        "forge.schedule.update",
        "forge.schedule.list",
        "forge.loop.inspect",
    ] {
        find_mcp_tool(&manifest_json, name);
    }

    let input = serde_json::json!({
        "goals": ["hackathon"],
        "timezone": "America/Sao_Paulo",
        "cron": "0 8 * * *",
        "origin": "codex"
    })
    .to_string();
    let created = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.schedule.create_daily_goal_research"])
        .arg("--input")
        .arg(&input)
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let created_json: Value = serde_json::from_slice(&created).unwrap();
    assert_eq!(created_json["status"], "ok");
    assert_eq!(
        created_json["result"]["status"],
        "daily_goal_research_workflow_created"
    );
    assert_eq!(
        created_json["result"]["workflow"]["tasks"][8]["schedule"]["timezone"],
        "America/Sao_Paulo"
    );
    assert_eq!(
        created_json["result"]["workflow"]["tasks"][9]["loop_control"]["items"],
        serde_json::json!(["hackathon"])
    );
}

#[test]
fn run_daily_goal_research_smoke_generates_reports_and_telegram_record() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let created = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "create-daily-goal-research",
            "--goal",
            "hackathon",
            "--timezone",
            "America/Sao_Paulo",
            "--cron",
            "0 8 * * *",
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
    let created_json: Value = serde_json::from_slice(&created).unwrap();
    let workflow_id = created_json["workflow_id"].as_str().unwrap();

    let run = forge()
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
    let run_json: Value = serde_json::from_slice(&run).unwrap();
    assert_eq!(
        run_json["daily_goal_research"]["status"],
        "smoke_artifacts_generated"
    );
    assert_eq!(
        run_json["daily_goal_research"]["goals"][0]["goal"],
        "hackathon"
    );
    assert_eq!(
        run_json["daily_goal_research"]["goals"][0]["telegram_delivery"]["secret_exposed"],
        false
    );

    let inspected = forge()
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
    let inspected_json: Value = serde_json::from_slice(&inspected).unwrap();
    let schedule_node = inspected_json["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["schedule"].is_object())
        .unwrap();
    let run_history = schedule_node["schedule"]["run_history"].as_array().unwrap();
    assert_eq!(run_history.len(), 1);
    assert!(run_history[0]["run_id"]
        .as_str()
        .unwrap()
        .starts_with("run_"));
    assert_eq!(run_history[0]["status"], "completed");
    assert_eq!(run_history[0]["missed"], false);
    assert!(run_history[0]["scheduled_at"]
        .as_str()
        .unwrap()
        .contains('T'));
    assert!(run_history[0]["started_at"].as_str().unwrap().contains('T'));
    assert!(run_history[0]["finished_at"]
        .as_str()
        .unwrap()
        .contains('T'));

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
    let paths = artifacts_json["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|artifact| artifact["path"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert!(paths
        .iter()
        .any(|path| path.ends_with("goal-hackathon-report.md")));
    assert!(paths
        .iter()
        .any(|path| path.ends_with("goal-hackathon-report.pdf")));
    let delivery_path = paths
        .iter()
        .find(|path| path.ends_with("telegram-delivery-hackathon.json"))
        .unwrap();
    let delivery = fs::read_to_string(temp.path().join(delivery_path)).unwrap();
    assert!(delivery.contains("configured_telegram_chat_ref"));
    assert!(!delivery.contains("bot_token"));
    assert!(!delivery.contains("chat_id"));
}

#[test]
fn run_daily_goal_research_smoke_reports_bounded_parallel_goal_execution() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let created = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "create-daily-goal-research",
            "--goal",
            "hackathon",
            "--goal",
            "competition",
            "--goal",
            "blockchain",
            "--timezone",
            "America/Sao_Paulo",
            "--cron",
            "0 8 * * *",
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
    let created_json: Value = serde_json::from_slice(&created).unwrap();
    let workflow_id = created_json["workflow_id"].as_str().unwrap();

    let run = forge()
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
    let run_json: Value = serde_json::from_slice(&run).unwrap();
    let smoke = &run_json["daily_goal_research"];

    assert_eq!(smoke["artifact_count"], 9);
    assert_eq!(
        smoke["goals"]
            .as_array()
            .unwrap()
            .iter()
            .map(|goal| goal["goal"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["blockchain", "competition", "hackathon"]
    );

    let execution = &smoke["execution"];
    assert_eq!(
        execution["schema_version"],
        "forge.daily_goal_research.execution.v1"
    );
    assert_eq!(execution["mode"], "bounded_parallel_goal_artifacts");
    assert_eq!(execution["max_workers"], 4);
    assert_eq!(execution["worker_count"], 3);
    assert_eq!(execution["total_goals"], 3);
    assert_eq!(execution["bounded"], true);
    assert_eq!(execution["concurrency_used"], true);
    assert_eq!(execution["deterministic_output_order"], true);
    assert_eq!(
        execution["goal_order"],
        serde_json::json!(["blockchain", "competition", "hackathon"])
    );
    let waves = execution["waves"].as_array().unwrap();
    assert_eq!(waves.len(), 1);
    assert_eq!(waves[0]["level"], 1);
    assert_eq!(waves[0]["worker_count"], 3);
    assert_eq!(
        waves[0]["goals"],
        serde_json::json!(["blockchain", "competition", "hackathon"])
    );
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
fn plan_for_hackathon_mvp_software_factory_gates_idea_pdf_telegram_and_improvement() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Create a hackathon MVP software factory for Ideathon Energia para Todos. Input includes the regulation, the user wants to build GreenRoute AI using OSM and OSRM for collaborative logistics, the official final deadline is 2026-05-31T23:59:00-03:00, use a customizable 36 hour buffer, generate the final idea PDF and send the explanation to Telegram.",
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
        .contains(&Value::String(
            "hackathon regulation compliance matrix".to_string()
        )));
    assert!(json["intent"]["deliverables"]
        .as_array()
        .unwrap()
        .contains(&Value::String("final idea PDF artifact".to_string())));
    assert!(json["intent"]["constraints"]
        .as_array()
        .unwrap()
        .contains(&Value::String(
            "final package deadline buffer before official submission".to_string()
        )));

    let tasks = json["tasks"].as_array().unwrap();
    let regulation = find_task(tasks, "Parse hackathon regulation");
    let deadline = find_task(tasks, "Calculate buffered hackathon deadline");
    let viability = find_task(tasks, "Evaluate user idea viability against regulation");
    let brainstorm = find_task(tasks, "Brainstorm and score hackathon MVP concepts");
    let final_idea = find_task(tasks, "Select final idea and MVP scope");
    let pdf = find_task(tasks, "Generate final idea PDF and explanation artifact");
    let telegram = find_task(tasks, "Send final idea PDF to Telegram");
    let backlog = find_task(tasks, "Build hackathon MVP software factory backlog");
    let osrm_plan = find_task(tasks, "Prepare OSM OSRM MVP build plan");
    let validation = find_task(tasks, "Validate MVP, pitch and judging package");
    let improvement = find_task(tasks, "Run continuous improvement until buffered deadline");

    assert_eq!(regulation["executor"], "ai");
    assert_eq!(deadline["executor"], "command");
    assert_eq!(viability["executor"], "ai");
    assert_eq!(brainstorm["executor"], "ai");
    assert_eq!(final_idea["executor"], "ai");
    assert_eq!(pdf["executor"], "command");
    assert_eq!(telegram["executor"], "notification");
    assert_eq!(backlog["executor"], "command");
    assert_eq!(osrm_plan["executor"], "mixed");
    assert_eq!(validation["executor"], "command");
    assert_eq!(improvement["executor"], "wait");

    assert!(deadline["validation_rules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|rule| rule["kind"] == "deadline_buffer"
            && rule["expected"].as_str().unwrap().contains("customizable")));
    assert!(viability["validation_rules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|rule| rule["kind"] == "viability_gate"
            && rule["expected"]
                .as_str()
                .unwrap()
                .contains("not_viable_with_alternative")));
    assert!(brainstorm["validation_rules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|rule| rule["kind"] == "weighted_scoring"
            && rule["expected"].as_str().unwrap().contains("20% pitch")));
    assert_eq!(telegram["notification"]["channel"], "telegram");
    assert_eq!(telegram["notification"]["to"], "configured_telegram_chat");
    assert_eq!(telegram["notification"]["include_cost_report"], false);
    assert_eq!(improvement["schedule"]["timezone"], "America/Sao_Paulo");
    assert_eq!(improvement["schedule"]["cron"], "0 */6 * * *");
    assert!(osrm_plan["context_requirements"]
        .as_array()
        .unwrap()
        .contains(&Value::String("OSRM routing requirements".to_string())));
    assert!(validation["validation_rules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|rule| rule["kind"] == "pitch_validation"
            && rule["expected"].as_str().unwrap().contains("five minutes")));
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
    assert!(run["parallel_plan"].is_object());
    assert_eq!(
        run["parallel_plan"]["schema_version"],
        "forge.scheduler.parallel_plan.v1"
    );
    assert!(run["parallel_plan"]["total_waves"].as_u64().unwrap() >= 1);
    assert!(!run["parallel_plan"]["waves"].as_array().unwrap().is_empty());
    assert!(run["parallel_plan"]["total_tasks"].as_u64().unwrap() >= 1);
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
    assert!(skill.contains("forge mcp tools"));
    assert!(skill.contains("forge mcp call forge.run.start"));
    assert!(skill.contains("forge.workflow.attach_artifact"));
    assert!(skill.contains("forge.context.request"));
    assert!(skill.contains("forge.task.handoff"));
    assert!(skill.contains("forge.schedule.summary"));
    assert!(skill.contains("forge.schedule.loop_summary"));
    assert!(skill.contains("forge.schedule.worker_status"));
    assert!(skill.contains("forge schedule worker-status"));
}

#[test]
fn mcp_tools_manifest_exposes_stable_agent_runtime_surface() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "tools",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["status"], "mcp_tools_loaded");
    assert_eq!(json["schema_version"], "forge.mcp.tools.v1");
    assert_eq!(json["protocol"], "model_context_protocol");
    assert!(json["tools"].as_array().unwrap().len() >= 10);

    for tool_name in [
        "forge.workflow.list",
        "forge.workflow.inspect",
        "forge.run.start",
        "forge.run.resume",
        "forge.run.status",
        "forge.workflow.update_goal",
        "forge.workflow.attach_artifact",
        "forge.context.request",
        "forge.task.handoff",
        "forge.validation.status",
        "forge.artifact.fetch",
    ] {
        let tool = find_mcp_tool(&json, tool_name);
        assert_eq!(tool["name"], tool_name);
        assert_eq!(tool["input_schema"]["type"], "object");
        assert!(tool["output_schema"]
            .as_str()
            .unwrap()
            .starts_with("forge."));
        assert!(tool["forge_command"].as_array().unwrap().len() >= 2);
    }

    let run_start = find_mcp_tool(&json, "forge.run.start");
    assert_eq!(run_start["async_safe"], true);
    assert_eq!(run_start["mutates_workflow"], true);
    assert!(run_start["description"]
        .as_str()
        .unwrap()
        .contains("return a run_id"));

    let update_goal = find_mcp_tool(&json, "forge.workflow.update_goal");
    assert_eq!(update_goal["mutates_workflow"], true);
    assert!(update_goal["description"]
        .as_str()
        .unwrap()
        .contains("revision"));

    let task_handoff = find_mcp_tool(&json, "forge.task.handoff");
    assert_eq!(task_handoff["async_safe"], true);
    assert_eq!(task_handoff["mutates_workflow"], true);
    assert_eq!(task_handoff["output_schema"], "forge.executor_handoff.v8");
    assert!(task_handoff["description"]
        .as_str()
        .unwrap()
        .contains("Acquire a bounded executor handoff"));
}

#[test]
fn mcp_call_starts_resumes_and_polls_async_run_for_agent_handoff() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let started = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.run.start"])
        .arg("--input")
        .arg(r#"{"goal":"Improve Forge through MCP async handoff","origin":"codex"}"#)
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let started_json: Value = serde_json::from_slice(&started).unwrap();
    assert_eq!(started_json["schema_version"], "forge.mcp.call.v1");
    assert_eq!(started_json["status"], "ok");
    assert_eq!(started_json["tool_name"], "forge.run.start");
    assert_eq!(started_json["result"]["status"], "accepted");
    assert_eq!(started_json["result"]["async"], true);
    let run_id = started_json["result"]["run_id"].as_str().unwrap();
    let workflow_id = started_json["result"]["workflow_id"].as_str().unwrap();
    assert!(run_id.starts_with("run_"));
    assert!(workflow_id.starts_with("wf_"));
    assert_eq!(
        started_json["result"]["handoff_contract"]["schema_version"],
        "forge.agent_handoff_contract.v1"
    );
    assert_eq!(
        started_json["result"]["handoff_contract"]["run_id"],
        started_json["result"]["run_id"]
    );
    assert_eq!(
        started_json["result"]["handoff_contract"]["status_poll"]["tool"],
        "forge.run.status"
    );
    assert_eq!(
        started_json["result"]["handoff_contract"]["allowed_context"]["tool"],
        "forge.context.request"
    );
    assert!(
        started_json["result"]["handoff_contract"]["validation_rules"]
            .as_array()
            .unwrap()
            .contains(&Value::String("validate-before-promotion".to_string()))
    );

    let resume_input = serde_json::json!({
        "run_id": run_id,
        "origin": "opencode"
    })
    .to_string();
    let resumed = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.run.resume"])
        .arg("--input")
        .arg(&resume_input)
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let resumed_json: Value = serde_json::from_slice(&resumed).unwrap();
    assert_eq!(resumed_json["status"], "ok");
    assert_eq!(resumed_json["result"]["status"], "resumed");
    assert_eq!(resumed_json["result"]["run_id"], run_id);
    assert_eq!(resumed_json["result"]["workflow_id"], workflow_id);
    assert_eq!(resumed_json["result"]["origin"], "opencode");
    assert_eq!(
        resumed_json["result"]["request_status"]["status"],
        "resumed"
    );

    let status_input = serde_json::json!({ "run_id": run_id }).to_string();
    let status = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.run.status"])
        .arg("--input")
        .arg(&status_input)
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let status_json: Value = serde_json::from_slice(&status).unwrap();
    assert_eq!(status_json["result"]["run_id"], run_id);
    assert_eq!(status_json["result"]["workflow_id"], workflow_id);
    assert_eq!(status_json["result"]["status"], "resumed");
    assert_eq!(
        status_json["result"]["handoff_summary"]["schema_version"],
        "forge.context_handoff_summary.v1"
    );
}

#[test]
fn mcp_call_mutates_workflow_and_fetches_bounded_artifact_content() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let artifact = temp.path().join("mcp-note.md");
    fs::write(
        &artifact,
        "# MCP artifact\n\nAttached through the Forge MCP tool surface.\n",
    )
    .unwrap();

    let started = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.run.start"])
        .arg("--input")
        .arg(r#"{"goal":"Exercise MCP mutation tools","origin":"codex"}"#)
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let started_json: Value = serde_json::from_slice(&started).unwrap();
    let workflow_id = started_json["result"]["workflow_id"].as_str().unwrap();

    let update_input = serde_json::json!({
        "workflow_id": workflow_id,
        "goal": "Exercise MCP mutation tools with revision tracking",
        "origin": "codex"
    })
    .to_string();
    let updated = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.workflow.update_goal"])
        .arg("--input")
        .arg(&update_input)
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let updated_json: Value = serde_json::from_slice(&updated).unwrap();
    assert_eq!(updated_json["result"]["status"], "workflow_goal_updated");
    assert_eq!(updated_json["result"]["revision"], 1);

    let attach_input = serde_json::json!({
        "workflow_id": workflow_id,
        "path": artifact.to_str().unwrap(),
        "kind": "mcp_note",
        "origin": "opencode"
    })
    .to_string();
    let attached = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.workflow.attach_artifact"])
        .arg("--input")
        .arg(&attach_input)
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let attached_json: Value = serde_json::from_slice(&attached).unwrap();
    assert_eq!(attached_json["result"]["status"], "artifact_attached");
    assert_eq!(attached_json["result"]["revision"], 2);
    let artifact_path = attached_json["result"]["artifact"]["path"]
        .as_str()
        .unwrap();

    let fetch_input = serde_json::json!({
        "workflow_id": workflow_id,
        "path": artifact_path,
        "max_bytes": 200
    })
    .to_string();
    let fetched = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.artifact.fetch"])
        .arg("--input")
        .arg(&fetch_input)
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let fetched_json: Value = serde_json::from_slice(&fetched).unwrap();
    assert_eq!(
        fetched_json["result"]["schema_version"],
        "forge.mcp.artifact_fetch.v1"
    );
    assert_eq!(fetched_json["result"]["workflow_id"], workflow_id);
    assert_eq!(fetched_json["result"]["artifact"]["path"], artifact_path);
    assert_eq!(fetched_json["result"]["truncated"], false);
    assert!(fetched_json["result"]["content_utf8"]
        .as_str()
        .unwrap()
        .contains("Attached through the Forge MCP tool surface"));

    let validation_input = serde_json::json!({ "workflow_id": workflow_id }).to_string();
    let validation = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.validation.status"])
        .arg("--input")
        .arg(&validation_input)
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let validation_json: Value = serde_json::from_slice(&validation).unwrap();
    assert_eq!(
        validation_json["result"]["schema_version"],
        "forge.mcp.validation_status.v1"
    );
    assert_eq!(validation_json["result"]["validation"]["status"], "blocked");
    assert_eq!(validation_json["result"]["validation"]["promotable"], false);
}

#[test]
fn mcp_call_acquires_bounded_task_handoff_packet_for_agent_executor() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let planned = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "MCP bounded executor handoff",
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
    let task_id = planned_json["tasks"][0]["id"].as_str().unwrap();

    let handoff_input = serde_json::json!({
        "workflow_id": workflow_id,
        "task_id": task_id,
        "executor": "codex",
        "budget": 1200,
        "ttl_seconds": 600
    })
    .to_string();
    let handoff = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.task.handoff"])
        .arg("--input")
        .arg(&handoff_input)
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let handoff_json: Value = serde_json::from_slice(&handoff).unwrap();
    assert_eq!(handoff_json["schema_version"], "forge.mcp.call.v1");
    assert_eq!(handoff_json["status"], "ok");
    assert_eq!(handoff_json["tool_name"], "forge.task.handoff");
    assert_eq!(handoff_json["result"]["status"], "handoff_ready");
    assert_eq!(handoff_json["result"]["allowed"], true);
    assert_eq!(handoff_json["result"]["workflow_id"], workflow_id);
    assert_eq!(handoff_json["result"]["task_id"], task_id);
    assert_eq!(handoff_json["result"]["selected_executor"], "codex");
    assert_eq!(
        handoff_json["result"]["packet"]["schema_version"],
        "forge.executor_handoff.v8"
    );
    assert_eq!(
        handoff_json["result"]["packet"]["lease_id"],
        handoff_json["result"]["lease"]["lease_id"]
    );
    assert_eq!(
        handoff_json["result"]["packet"]["context_sha256"],
        handoff_json["result"]["context"]["context_sha256"]
    );
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
            "2999-01-01T00:00:00-03:00",
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
    assert_eq!(json["stop_at"], "2999-01-01T00:00:00-03:00");
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
            "2999-01-01T00:00:00-03:00",
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
            "2999-01-01T00:00:00-03:00",
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
            "2999-01-01T00:00:00-03:00",
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
fn self_run_exposes_operating_mode_overhead_ledger_and_decision_gate() {
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
            "2999-01-01T00:00:00-03:00",
            "--max-cycles",
            "1",
            "--executor",
            "codex",
            "--mode",
            "balanced",
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
    assert_eq!(json["operating_mode"], "balanced");
    assert_eq!(json["status"], "planned");
    assert_eq!(
        json["decision_gate"]["schema_version"],
        "forge.self_evolution.decision_gate.v1"
    );
    assert_eq!(json["decision_gate"]["decision"], "run_cycle");
    assert_eq!(json["decision_gate"]["stop_loop"], false);
    assert!(
        json["decision_gate"]["expected_value_score"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(
        json["decision_gate"]["orchestration_cost_score"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(
        json["overhead_ledger"]["schema_version"],
        "forge.self_evolution.overhead_ledger.v1"
    );
    assert_eq!(json["overhead_ledger"]["operating_mode"], "balanced");
    assert_eq!(json["overhead_ledger"]["cycle_count"], 1);
    assert!(json["overhead_ledger"]["prompt_bytes"].as_u64().unwrap() > 0);
    assert!(
        json["overhead_ledger"]["estimated_prompt_tokens"]
            .as_u64()
            .unwrap()
            > 0
    );

    let cycle_report = &json["cycle_reports"][0];
    assert_eq!(
        cycle_report["overhead_ledger"]["schema_version"],
        "forge.self_evolution.overhead_ledger.v1"
    );
    assert_eq!(cycle_report["decision_gate"]["decision"], "run_cycle");
    let prompt_path = temp
        .path()
        .join(cycle_report["prompt_path"].as_str().unwrap());
    let prompt = fs::read_to_string(prompt_path).unwrap();
    assert!(prompt.contains("Operating mode: `balanced`"));
    assert!(prompt.contains("Lean overhead ledger"));
    assert!(prompt.contains("Automated self-evolution decision gate"));
}

#[test]
fn self_run_stops_when_terminal_final_goal_contract_is_satisfied() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let repo = temp.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    let base_goal =
        "Improve Forge Core autonomously with bounded executor cycles, validation gates, artifacts and changelog";
    let terminal_goal = format!(
        "{base_goal}. Stopping rule: when Forge has a validated lean/balanced/strict mode boundary, a measurable overhead ledger, and an automated self-evolution decision gate that can reject or stop cycles whose expected value is lower than orchestration cost, the self-evolution loop should mark the terminal goal reached, stop proposing new architecture by default, avoid further interaction, and only resume if a human gives a new explicit goal."
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
            &terminal_goal,
            "--output",
            "json",
        ])
        .assert()
        .success();

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "self",
            "run",
            "--repo",
            repo.to_str().unwrap(),
            "--until",
            "2999-01-01T00:00:00-03:00",
            "--max-cycles",
            "1",
            "--mode",
            "balanced",
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
    assert_eq!(json["status"], "terminal_goal_reached");
    assert!(json["cycle_reports"].as_array().unwrap().is_empty());
    assert_eq!(
        json["decision_gate"]["decision"],
        "stop_terminal_goal_reached"
    );
    assert_eq!(json["decision_gate"]["terminal_goal_reached"], true);
    assert_eq!(json["decision_gate"]["stop_loop"], true);
    assert!(json["decision_gate"]["reason"]
        .as_str()
        .unwrap()
        .contains("terminal self-evolution goal is already satisfied"));
}

#[test]
fn self_run_uses_latest_self_evolution_goal_instead_of_longest_goal() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let repo = temp.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    let base_goal =
        "Improve Forge Core autonomously with bounded executor cycles, validation gates, artifacts and changelog";
    let terminal_goal = format!(
        "{base_goal}. Stopping rule: when Forge has a validated lean/balanced/strict mode boundary, a measurable overhead ledger, and an automated self-evolution decision gate that can reject or stop cycles whose expected value is lower than orchestration cost, the self-evolution loop should mark the terminal goal reached, stop proposing new architecture by default, avoid further interaction, and only resume if a human gives a new explicit goal."
    );
    let latest_goal = format!(
        "{base_goal}. New explicit human goal: prioritize MCP workflow invocation, reusable Codex/OpenCode skills, async agent handoff with run_id, bounded context contracts and validation artifacts."
    );

    let old_output = forge()
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
    let old_plan: Value = serde_json::from_slice(&old_output).unwrap();
    let old_workflow_id = old_plan["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "update-goal",
            "--workflow",
            old_workflow_id,
            "--origin",
            "codex",
            "--goal",
            &terminal_goal,
            "--output",
            "json",
        ])
        .assert()
        .success();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            &latest_goal,
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
            "--max-cycles",
            "1",
            "--mode",
            "balanced",
            "--dry-run",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&self_output).unwrap();
    assert_eq!(json["status"], "planned");
    assert_eq!(json["decision_gate"]["decision"], "run_cycle");

    let prompt_path = json["cycle_reports"][0]["prompt_path"].as_str().unwrap();
    let prompt = fs::read_to_string(temp.path().join(prompt_path)).unwrap();
    assert!(prompt.contains(&latest_goal));
    assert!(!prompt.contains("terminal self-evolution goal is already satisfied"));
}

#[test]
fn self_run_keeps_running_for_explicit_forge_05_continuation_goal() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let repo = temp.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    let base_goal =
        "Improve Forge Core autonomously with bounded executor cycles, validation gates, artifacts and changelog";
    let old_terminal_goal = format!(
        "{base_goal}. Stopping rule: when Forge has a validated lean/balanced/strict mode boundary, a measurable overhead ledger, and an automated self-evolution decision gate that can reject or stop cycles whose expected value is lower than orchestration cost, the self-evolution loop should mark the terminal goal reached."
    );
    let forge_05_goal = format!(
        "{old_terminal_goal} Terminal goal correction from Codex: do not stop this self-evolution loop merely because the cron/loop/daily-goal-research phase is satisfied. The current active terminal phase is Forge 0.5 agent-integration and creative-runtime readiness. Continue until Forge has validated and reported MCP/skill/agent integration surfaces, first-class no-argument interactive Forge CLI/TUI with slash commands and direct-chat routing, creative runtime IR for design/doc/slide/video/whiteboard artifacts, design tokens, live human+AI collaboration, lean-governance evidence and a Forge 0.5 milestone manifest."
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
    let workflow_id = planned["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "update-goal",
            "--workflow",
            workflow_id,
            "--origin",
            "codex",
            "--goal",
            &forge_05_goal,
            "--output",
            "json",
        ])
        .assert()
        .success();

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "self",
            "run",
            "--repo",
            repo.to_str().unwrap(),
            "--until",
            "2999-01-01T00:00:00-03:00",
            "--max-cycles",
            "1",
            "--mode",
            "balanced",
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
    assert_eq!(json["decision_gate"]["decision"], "run_cycle");
    assert_eq!(json["decision_gate"]["terminal_goal_reached"], false);
    assert_eq!(json["decision_gate"]["stop_loop"], false);
    assert!(
        json["decision_gate"]["expected_value_score"]
            .as_u64()
            .unwrap()
            >= json["decision_gate"]["orchestration_cost_score"]
                .as_u64()
                .unwrap()
    );

    let prompt_path = json["cycle_reports"][0]["prompt_path"].as_str().unwrap();
    let prompt = fs::read_to_string(temp.path().join(prompt_path)).unwrap();
    assert!(prompt.contains("Forge 0.5 agent-integration"));
    assert!(prompt.contains("creative-runtime readiness"));
}

#[test]
fn self_run_rejects_low_value_bloat_cycle_in_lean_mode() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let repo = temp.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    let base_goal =
        "Improve Forge Core autonomously with bounded executor cycles, validation gates, artifacts and changelog";
    let bloat_goal = format!(
        "{base_goal}. Add governance schemas, receipts, hashes, manifests and projections for every self-evolution detail without changing useful throughput, cost, validation, retries, deterministic execution or artifact delivery."
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
            &bloat_goal,
            "--output",
            "json",
        ])
        .assert()
        .success();

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "self",
            "run",
            "--repo",
            repo.to_str().unwrap(),
            "--until",
            "2999-01-01T00:00:00-03:00",
            "--max-cycles",
            "1",
            "--mode",
            "lean",
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
    assert_eq!(json["status"], "rejected");
    assert!(json["cycle_reports"].as_array().unwrap().is_empty());
    assert_eq!(json["decision_gate"]["decision"], "reject_low_value_cycle");
    assert_eq!(json["decision_gate"]["stop_loop"], true);
    assert_eq!(json["decision_gate"]["operating_mode"], "lean");
    assert!(
        json["decision_gate"]["expected_value_score"]
            .as_u64()
            .unwrap()
            < json["decision_gate"]["orchestration_cost_score"]
                .as_u64()
                .unwrap()
    );
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
            "2999-01-01T00:00:00-03:00",
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
    assert_eq!(status_json["tasks"][0]["status"], "completed");
    assert_eq!(
        status_json["tasks"][0]["work_item"]["backlog_state"],
        "done"
    );
    assert_eq!(
        status_json["tasks"][0]["work_item"]["goal_validation"]["definitively_ready"],
        true
    );
    assert!(status_json["tasks"][0]["work_item"]["subtasks"]
        .as_array()
        .unwrap()
        .iter()
        .all(|subtask| subtask["status"] == "completed"));
    assert_eq!(status_json["revisions"].as_array().unwrap().len(), 1);
    assert_eq!(status_json["revisions"][0]["origin"], "executor_response");
    assert_eq!(
        status_json["revisions"][0]["change_type"],
        "executor_response_promoted"
    );
    assert!(status_json["revisions"][0]["summary"]
        .as_str()
        .unwrap()
        .contains(task_id));
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
    assert_eq!(cluster_list["schema_version"], "forge.cluster_registry.v2");
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
    assert_eq!(
        placement["placement_policy"]["schema_version"],
        "forge.cluster_placement_policy.v1"
    );
    assert_eq!(
        placement["placement_policy"]["authorized_execution_scope"],
        "placement_metadata_only"
    );
    assert_eq!(
        placement["placement_policy"]["trust_policy"],
        "explicit_trust_required_no_external_mutation"
    );
    assert_eq!(
        placement["placement_policy"]["required_trust"],
        placement["requirements"]["required_trust"]
    );
    assert_eq!(
        placement["placement_policy"]["remote_execution_enabled"],
        false
    );
    assert_eq!(
        placement["placement_policy"]["remote_ai_execution_allowed"],
        false
    );
    assert_eq!(
        placement["placement_policy"]["external_mutation_allowed"],
        false
    );
    assert_eq!(
        placement["placement_policy"]["authorization_required"],
        "explicit_authorization_required_before_remote_execution_or_external_mutation"
    );
    assert_eq!(
        placement["placement_policy"]["requirements_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert_eq!(
        placement["placement_policy"]["policy_sha256"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
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

#[test]
fn cluster_placement_routes_metatrader5_work_to_windows_software_node() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "cluster",
            "register",
            "--node-id",
            "lan-linux-command",
            "--name",
            "LAN Linux Command Worker",
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
            "--software",
            "python3",
            "--capability",
            "command",
            "--capability",
            "python",
            "--python",
            "--network-reachable",
            "--status",
            "online",
            "--trust",
            "trusted_lan",
            "--sandbox",
            "local_process_no_network",
            "--latency-ms",
            "3",
            "--reliability",
            "0.99",
            "--output",
            "json",
        ])
        .assert()
        .success();

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

    let planned_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Run repeated MetaTrader 5 backtests on the real Windows machine without AI or external mutation",
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
    let mt5_task = find_task(
        planned["tasks"].as_array().unwrap(),
        "Run MetaTrader 5 deterministic step",
    );

    assert_eq!(mt5_task["executor"], "command");
    assert_eq!(
        mt5_task["execution_policy"]["mode"],
        "windows_software_node"
    );
    assert_eq!(mt5_task["execution_policy"]["ai_allowed"], false);
    assert_eq!(mt5_task["execution_policy"]["deterministic"], true);
    assert_eq!(
        mt5_task["execution_policy"]["code_runtime"]["language"],
        "metatrader5"
    );
    assert_eq!(
        mt5_task["execution_policy"]["code_runtime"]["sandbox"],
        "windows_desktop_user_session"
    );

    let placement_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "cluster",
            "place",
            "--workflow",
            workflow_id,
            "--task",
            mt5_task["id"].as_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let placement: Value = serde_json::from_slice(&placement_output).unwrap();

    assert_eq!(placement["schema_version"], "forge.cluster_placement.v1");
    assert_eq!(placement["status"], "placement_selected");
    assert_eq!(
        placement["requirements"]["schema_version"],
        "forge.cluster_placement_requirements.v3"
    );
    assert_eq!(placement["requirements"]["required_os"], "windows");
    assert_eq!(
        placement["requirements"]["required_capabilities"][0],
        "metatrader5"
    );
    assert_eq!(
        placement["requirements"]["required_software"][0],
        "metatrader5"
    );
    assert!(placement["requirements"]["required_sandbox_permissions"]
        .as_array()
        .unwrap()
        .contains(&Value::String("windows_desktop_user_session".to_string())));
    assert_eq!(placement["selected_node"]["node_id"], "lan-windows-mt5");

    let rejected_linux = placement["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|candidate| candidate["node_id"] == "lan-linux-command")
        .unwrap();
    assert_eq!(rejected_linux["eligible"], false);
    let reasons = rejected_linux["reasons"].as_array().unwrap();
    assert!(reasons.contains(&Value::String("missing capability metatrader5".to_string())));
    assert!(reasons.contains(&Value::String(
        "os linux does not satisfy required os windows".to_string()
    )));
    assert!(reasons.contains(&Value::String("missing software metatrader5".to_string())));
    assert!(reasons.contains(&Value::String(
        "missing sandbox permission windows_desktop_user_session".to_string()
    )));
}

#[test]
fn cluster_placement_blocks_remote_ai_tasks_without_explicit_authorization() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "cluster",
            "register",
            "--node-id",
            "lan-ai-worker",
            "--name",
            "LAN AI Worker",
            "--endpoint",
            "ssh://forge@lan-ai",
            "--os",
            "linux",
            "--arch",
            "x86_64",
            "--cpu-cores",
            "32",
            "--memory-gb",
            "128",
            "--gpu",
            "nvidia-rtx-4090",
            "--software",
            "python3",
            "--capability",
            "ai",
            "--capability",
            "gpu",
            "--capability",
            "python",
            "--python",
            "--gpu-available",
            "--network-reachable",
            "--status",
            "online",
            "--trust",
            "trusted_lan",
            "--sandbox",
            "local_process_no_network",
            "--latency-ms",
            "3",
            "--reliability",
            "0.99",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let planned_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Research a distributed runtime strategy with AI analysis",
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
    let ai_task = find_task(planned["tasks"].as_array().unwrap(), "Extract requirements");

    let placement_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "cluster",
            "place",
            "--workflow",
            workflow_id,
            "--task",
            ai_task["id"].as_str().unwrap(),
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .stdout
        .clone();
    let placement: Value = serde_json::from_slice(&placement_output).unwrap();

    assert_eq!(placement["schema_version"], "forge.cluster_placement.v1");
    assert_eq!(placement["status"], "placement_blocked");
    assert_eq!(
        placement["requirements"]["schema_version"],
        "forge.cluster_placement_requirements.v3"
    );
    assert_eq!(placement["requirements"]["executor"], "ai");
    assert_eq!(placement["requirements"]["policy_mode"], "model_executor");
    assert_eq!(placement["requirements"]["reasoning_required"], true);
    assert_eq!(
        placement["requirements"]["remote_ai_execution_allowed"],
        false
    );
    assert_eq!(placement["selected_node"], Value::Null);

    let candidate = placement["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|candidate| candidate["node_id"] == "lan-ai-worker")
        .unwrap();
    assert_eq!(candidate["eligible"], false);
    assert!(candidate["reasons"]
        .as_array()
        .unwrap()
        .contains(&Value::String(
            "remote AI execution requires explicit authorization".to_string()
        )));
}

#[test]
fn cluster_handoff_selects_node_leases_task_and_returns_hash_sync_manifest() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "cluster",
            "register",
            "--node-id",
            "lan-linux-command",
            "--name",
            "LAN Linux Command Worker",
            "--endpoint",
            "ssh://forge@lan-linux",
            "--os",
            "linux",
            "--arch",
            "x86_64",
            "--cpu-cores",
            "8",
            "--memory-gb",
            "32",
            "--software",
            "python3",
            "--capability",
            "command",
            "--capability",
            "python",
            "--python",
            "--network-reachable",
            "--status",
            "online",
            "--trust",
            "trusted_lan",
            "--sandbox",
            "local_process_no_network",
            "--latency-ms",
            "5",
            "--reliability",
            "0.98",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let planned_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Prepare distributed cluster handoff without external mutation",
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
    let task_id = planned["tasks"][0]["id"].as_str().unwrap();

    let handoff = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "cluster",
            "handoff",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--ttl-seconds",
            "600",
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
    let handoff_json: Value = serde_json::from_slice(&handoff).unwrap();
    assert_eq!(
        handoff_json["schema_version"],
        "forge.cluster_task_handoff.v1"
    );
    assert_eq!(handoff_json["status"], "cluster_handoff_ready");
    assert_eq!(handoff_json["allowed"], true);
    assert_eq!(handoff_json["selected_node_id"], "lan-linux-command");
    assert_eq!(handoff_json["remote_execution_enabled"], false);
    assert_eq!(handoff_json["external_mutation_allowed"], false);
    assert_eq!(
        handoff_json["placement"]["selected_node"]["node_id"],
        "lan-linux-command"
    );
    assert_eq!(
        handoff_json["task_handoff"]["selected_executor"],
        "lan-linux-command"
    );

    let node_lease = &handoff_json["cluster_node_lease"];
    assert_eq!(node_lease["node_id"], "lan-linux-command");
    assert_eq!(node_lease["workflow_id"], workflow_id);
    assert_eq!(node_lease["task_id"], task_id);
    assert_eq!(node_lease["lease_scope"], "task_on_cluster_node");
    assert_eq!(
        node_lease["lease_id"],
        handoff_json["task_handoff"]["lease"]["lease_id"]
    );

    let sync_manifest = &handoff_json["sync_manifest"];
    assert_eq!(
        sync_manifest["schema_version"],
        "forge.cluster_sync_manifest.v1"
    );
    assert_eq!(sync_manifest["workflow_id"], workflow_id);
    assert_eq!(sync_manifest["task_id"], task_id);
    assert_eq!(sync_manifest["selected_node_id"], "lan-linux-command");
    assert_eq!(
        sync_manifest["context_sha256"],
        handoff_json["task_handoff"]["context"]["context_sha256"]
    );
    assert_eq!(
        sync_manifest["context_routing_cache_key"],
        handoff_json["task_handoff"]["context"]["routing_fingerprint"]["cache_key"]
    );
    assert_eq!(
        sync_manifest["sync_mode"],
        "content_addressed_hash_manifest_only"
    );
    let manifest_hash = sync_manifest["manifest_sha256"].as_str().unwrap();
    assert_eq!(manifest_hash.len(), 64);
    let manifest_hash_input = serde_json::json!([
        sync_manifest["schema_version"],
        sync_manifest["workflow_id"],
        sync_manifest["task_id"],
        sync_manifest["selected_node_id"],
        sync_manifest["lease_id"],
        sync_manifest["context_sha256"],
        sync_manifest["context_routing_cache_key"],
        sync_manifest["context_routing_lineage_sha256"],
        sync_manifest["checkpoint_ref"],
        sync_manifest["shard_refs"],
        sync_manifest["artifact_refs"],
        sync_manifest["sync_mode"],
        sync_manifest["remote_execution_enabled"],
        sync_manifest["external_mutation_allowed"]
    ]);
    assert_eq!(
        manifest_hash,
        hex_sha256(&serde_json::to_vec(&manifest_hash_input).unwrap())
    );
    assert_eq!(sync_manifest["external_mutation_allowed"], false);
    assert!(sync_manifest["shard_refs"].as_array().unwrap().len() >= 5);
    assert!(sync_manifest["shard_refs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|shard| shard["section"] == "execution_policy"
            && shard["content_sha256"].as_str().unwrap().len() == 64));
    assert_eq!(sync_manifest["artifact_refs"], serde_json::json!([]));

    let conflict = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "cluster",
            "handoff",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
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
    let conflict_json: Value = serde_json::from_slice(&conflict).unwrap();
    assert_eq!(conflict_json["status"], "lease_conflict");
    assert_eq!(conflict_json["allowed"], false);
    assert_eq!(
        conflict_json["task_handoff"]["current_lease"]["executor"],
        "lan-linux-command"
    );
}

#[test]
fn cluster_lease_registry_lists_node_scoped_leases_without_remote_execution() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "cluster",
            "register",
            "--node-id",
            "lan-linux-lease-worker",
            "--name",
            "LAN Linux Lease Worker",
            "--endpoint",
            "ssh://forge@lan-lease-worker",
            "--os",
            "linux",
            "--arch",
            "x86_64",
            "--cpu-cores",
            "8",
            "--memory-gb",
            "32",
            "--software",
            "python3",
            "--capability",
            "command",
            "--capability",
            "python",
            "--python",
            "--network-reachable",
            "--status",
            "online",
            "--trust",
            "trusted_lan",
            "--sandbox",
            "local_process_no_network",
            "--latency-ms",
            "5",
            "--reliability",
            "0.98",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let planned_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Prepare distributed cluster handoff lease inspection without external mutation",
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
    let task = &planned["tasks"][0];
    let task_id = task["id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "cluster",
            "handoff",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--ttl-seconds",
            "600",
            "--budget",
            "1600",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let lease_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "cluster",
            "leases",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let leases: Value = serde_json::from_slice(&lease_output).unwrap();

    assert_eq!(
        leases["schema_version"],
        "forge.cluster_node_lease_registry.v1"
    );
    assert_eq!(leases["status"], "listed");
    assert_eq!(leases["summary"]["total_leases"], 1);
    assert_eq!(leases["summary"]["active_leases"], 1);
    assert_eq!(leases["summary"]["expired_leases"], 0);
    assert_eq!(leases["summary"]["registered_node_leases"], 1);
    assert_eq!(leases["summary"]["unregistered_executor_leases"], 0);

    let lease = &leases["leases"][0];
    assert_eq!(lease["schema_version"], "forge.cluster_node_lease.v1");
    assert_eq!(lease["node_id"], "lan-linux-lease-worker");
    assert_eq!(lease["node_name"], "LAN Linux Lease Worker");
    assert_eq!(lease["workflow_id"], workflow_id);
    assert_eq!(lease["task_id"], task_id);
    assert_eq!(lease["task_title"], task["title"]);
    assert_eq!(lease["lease_scope"], "task_on_cluster_node");
    assert_eq!(lease["lease_status"], "active");
    assert_eq!(lease["trust_level"], "trusted_lan");
    assert_eq!(lease["network_reachable"], true);
    assert_eq!(lease["remote_execution_enabled"], false);
    assert_eq!(lease["external_mutation_allowed"], false);
    assert_eq!(
        lease["trust_policy"],
        "explicit_trust_required_no_external_mutation"
    );
    assert!(lease["sandbox_permissions"]
        .as_array()
        .unwrap()
        .contains(&Value::String("local_process_no_network".to_string())));

    let filtered_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "cluster",
            "leases",
            "--node-id",
            "lan-linux-lease-worker",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let filtered: Value = serde_json::from_slice(&filtered_output).unwrap();
    assert_eq!(filtered["filter"]["node_id"], "lan-linux-lease-worker");
    assert_eq!(filtered["leases"].as_array().unwrap().len(), 1);
}

#[test]
fn cluster_placement_prefers_idle_eligible_node_over_node_with_active_lease() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "cluster",
            "register",
            "--node-id",
            "lan-linux-busy",
            "--name",
            "LAN Linux Busy Worker",
            "--endpoint",
            "ssh://forge@lan-busy",
            "--os",
            "linux",
            "--arch",
            "x86_64",
            "--cpu-cores",
            "8",
            "--memory-gb",
            "32",
            "--software",
            "python3",
            "--capability",
            "command",
            "--capability",
            "python",
            "--python",
            "--network-reachable",
            "--status",
            "online",
            "--trust",
            "trusted_lan",
            "--sandbox",
            "local_process_no_network",
            "--latency-ms",
            "1",
            "--reliability",
            "0.99",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let first_plan_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Prepare first distributed cluster handoff lease-aware placement",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let first_plan: Value = serde_json::from_slice(&first_plan_output).unwrap();
    let first_workflow_id = first_plan["workflow_id"].as_str().unwrap();
    let first_task_id = first_plan["tasks"][0]["id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "cluster",
            "handoff",
            "--workflow",
            first_workflow_id,
            "--task",
            first_task_id,
            "--ttl-seconds",
            "600",
            "--budget",
            "1600",
            "--output",
            "json",
        ])
        .assert()
        .success();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "cluster",
            "register",
            "--node-id",
            "lan-linux-idle",
            "--name",
            "LAN Linux Idle Worker",
            "--endpoint",
            "ssh://forge@lan-idle",
            "--os",
            "linux",
            "--arch",
            "x86_64",
            "--cpu-cores",
            "8",
            "--memory-gb",
            "32",
            "--software",
            "python3",
            "--capability",
            "command",
            "--capability",
            "python",
            "--python",
            "--network-reachable",
            "--status",
            "online",
            "--trust",
            "trusted_lan",
            "--sandbox",
            "local_process_no_network",
            "--latency-ms",
            "10",
            "--reliability",
            "0.96",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let second_plan_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Prepare second distributed cluster handoff lease-aware placement",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let second_plan: Value = serde_json::from_slice(&second_plan_output).unwrap();
    let second_workflow_id = second_plan["workflow_id"].as_str().unwrap();
    let second_task_id = second_plan["tasks"][0]["id"].as_str().unwrap();

    let placement_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "cluster",
            "place",
            "--workflow",
            second_workflow_id,
            "--task",
            second_task_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let placement: Value = serde_json::from_slice(&placement_output).unwrap();

    assert_eq!(placement["schema_version"], "forge.cluster_placement.v1");
    assert_eq!(placement["selected_node"]["node_id"], "lan-linux-idle");

    let busy = placement["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|candidate| candidate["node_id"] == "lan-linux-busy")
        .unwrap();
    let idle = placement["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|candidate| candidate["node_id"] == "lan-linux-idle")
        .unwrap();

    assert_eq!(busy["eligible"], true);
    assert_eq!(busy["active_lease_count"], 1);
    assert!(busy["reasons"]
        .as_array()
        .unwrap()
        .contains(&Value::String("active leases 1".to_string())));
    assert_eq!(idle["eligible"], true);
    assert_eq!(idle["active_lease_count"], 0);
}

#[test]
fn cluster_list_exposes_node_scheduling_posture_from_task_leases() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "cluster",
            "register",
            "--node-id",
            "lan-linux-scheduling-busy",
            "--name",
            "LAN Linux Scheduling Busy",
            "--endpoint",
            "ssh://forge@lan-scheduling-busy",
            "--os",
            "linux",
            "--arch",
            "x86_64",
            "--cpu-cores",
            "8",
            "--memory-gb",
            "32",
            "--software",
            "python3",
            "--capability",
            "command",
            "--capability",
            "python",
            "--python",
            "--network-reachable",
            "--status",
            "online",
            "--trust",
            "trusted_lan",
            "--sandbox",
            "local_process_no_network",
            "--latency-ms",
            "5",
            "--reliability",
            "0.98",
            "--output",
            "json",
        ])
        .assert()
        .success();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "cluster",
            "register",
            "--node-id",
            "lan-linux-scheduling-idle",
            "--name",
            "LAN Linux Scheduling Idle",
            "--endpoint",
            "ssh://forge@lan-scheduling-idle",
            "--os",
            "linux",
            "--arch",
            "x86_64",
            "--cpu-cores",
            "8",
            "--memory-gb",
            "32",
            "--software",
            "python3",
            "--capability",
            "command",
            "--capability",
            "python",
            "--python",
            "--network-reachable",
            "--status",
            "online",
            "--trust",
            "trusted_lan",
            "--sandbox",
            "local_process_no_network",
            "--latency-ms",
            "8",
            "--reliability",
            "0.97",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let planned_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Prepare cluster list scheduling posture audit without remote execution",
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
    let task_id = planned["tasks"][0]["id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "cluster",
            "handoff",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--ttl-seconds",
            "600",
            "--budget",
            "1600",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let cluster_list_output = forge()
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
    let cluster_list: Value = serde_json::from_slice(&cluster_list_output).unwrap();

    assert_eq!(cluster_list["schema_version"], "forge.cluster_registry.v2");
    assert_eq!(cluster_list["summary"]["total_nodes"], 2);
    assert_eq!(cluster_list["summary"]["active_leases"], 1);
    assert_eq!(cluster_list["summary"]["expired_leases"], 0);
    assert_eq!(cluster_list["summary"]["schedulable_nodes"], 2);
    assert_eq!(cluster_list["summary"]["busy_schedulable_nodes"], 1);
    assert_eq!(cluster_list["summary"]["idle_schedulable_nodes"], 1);

    let busy = cluster_list["scheduling"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["node_id"] == "lan-linux-scheduling-busy")
        .unwrap();
    assert_eq!(busy["schema_version"], "forge.cluster_node_scheduling.v1");
    assert_eq!(busy["schedulable"], true);
    assert_eq!(busy["scheduling_status"], "busy");
    assert_eq!(busy["active_lease_count"], 1);
    assert_eq!(busy["expired_lease_count"], 0);
    assert_eq!(busy["remote_execution_enabled"], false);
    assert_eq!(busy["external_mutation_allowed"], false);

    let idle = cluster_list["scheduling"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["node_id"] == "lan-linux-scheduling-idle")
        .unwrap();
    assert_eq!(idle["schedulable"], true);
    assert_eq!(idle["scheduling_status"], "idle");
    assert_eq!(idle["active_lease_count"], 0);
    assert_eq!(idle["expired_lease_count"], 0);
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

#[test]
fn parallel_execution_reports_concurrent_wave_metrics() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Build three independent features at once: feature-a, feature-b, feature-c without any sequential dependencies",
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
    assert_eq!(run["mode"], "simulate_parallel");
    assert!(run["concurrent_wave_count"].as_u64().unwrap() >= 1);
    assert!(run["max_concurrent_tasks"].as_u64().unwrap() >= 1);
    assert_eq!(run["status"], "completed");
    assert!(
        run["cost_report"]["total_estimated_cost_usd"]
            .as_f64()
            .unwrap()
            > 0.0
    );
}

#[test]
fn dag_with_independent_branches_executes_concurrent_waves() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Research three independent topics: topic-alpha, topic-beta, topic-gamma then generate combined report",
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
    assert!(run["completed_tasks"].as_u64().unwrap() >= 8);
    assert!(
        run["cost_report"]["total_estimated_cost_usd"]
            .as_f64()
            .unwrap()
            > 0.0
    );
    assert_eq!(run["mode"], "simulate_parallel");
    assert!(run["concurrent_wave_count"].as_u64().unwrap() >= 1);
}

fn find_workflow<'a>(json: &'a Value, id: &str) -> &'a Value {
    json["workflows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|workflow| workflow["workflow_id"] == id)
        .unwrap()
}

fn workflow_ids(json: &Value) -> Vec<String> {
    json["workflows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|workflow| workflow["workflow_id"].as_str().unwrap().to_string())
        .collect()
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

fn find_mcp_tool<'a>(json: &'a Value, name: &str) -> &'a Value {
    json["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == name)
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

fn set_workflow_status_in_stored_workflow(store: &Path, workflow_id: &str, status: &str) {
    let connection = Connection::open(store).unwrap();
    let data_json: String = connection
        .query_row(
            "SELECT data_json FROM workflows WHERE id = ?1",
            [workflow_id],
            |row| row.get(0),
        )
        .unwrap();
    let mut workflow: Value = serde_json::from_str(&data_json).unwrap();
    workflow
        .as_object_mut()
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

#[test]
fn request_list_lists_all_requests_without_filter() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let first = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "start",
            "--goal",
            "First async request",
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
    let first_json: Value = serde_json::from_slice(&first).unwrap();
    let first_run_id = first_json["run_id"].as_str().unwrap().to_string();

    let second = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "start",
            "--goal",
            "Second async request",
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
    let second_json: Value = serde_json::from_slice(&second).unwrap();
    let second_run_id = second_json["run_id"].as_str().unwrap().to_string();

    let list_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "list",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let list: Value = serde_json::from_slice(&list_output).unwrap();
    assert_eq!(list["status"], "loaded");
    assert_eq!(list["schema_version"], "forge.request_list.v1");
    assert_eq!(list["total"], 2);
    let run_ids: Vec<&str> = list["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|run| run["run_id"].as_str().unwrap())
        .collect();
    assert!(run_ids.contains(&first_run_id.as_str()));
    assert!(run_ids.contains(&second_run_id.as_str()));
    assert!(list["runs"][0]["workflow_id"]
        .as_str()
        .unwrap()
        .starts_with("wf_"));
    assert_eq!(list["runs"][0]["status"], "accepted");
}

#[test]
fn request_list_filters_by_status() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let first = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "start",
            "--goal",
            "First request for filter test",
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
    let first_json: Value = serde_json::from_slice(&first).unwrap();
    let first_run_id = first_json["run_id"].as_str().unwrap().to_string();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "cancel",
            "--run",
            &first_run_id,
            "--origin",
            "opencode",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let second = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "start",
            "--goal",
            "Second request for filter test",
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
    let second_json: Value = serde_json::from_slice(&second).unwrap();
    let second_run_id = second_json["run_id"].as_str().unwrap().to_string();

    let accepted_list = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "list",
            "--status",
            "accepted",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let accepted: Value = serde_json::from_slice(&accepted_list).unwrap();
    assert_eq!(accepted["total"], 1);
    assert_eq!(
        accepted["runs"][0]["run_id"].as_str().unwrap(),
        second_run_id
    );
    assert_eq!(accepted["runs"][0]["status"], "accepted");

    let cancelled_list = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "list",
            "--status",
            "cancelled",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let cancelled: Value = serde_json::from_slice(&cancelled_list).unwrap();
    assert_eq!(cancelled["total"], 1);
    assert_eq!(
        cancelled["runs"][0]["run_id"].as_str().unwrap(),
        first_run_id
    );
    assert_eq!(cancelled["runs"][0]["status"], "cancelled");
}

#[test]
fn request_cancel_marks_run_as_cancelled_and_records_event() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let started = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "start",
            "--goal",
            "Cancel this request test",
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

    let cancel_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "cancel",
            "--run",
            run_id,
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

    let cancel: Value = serde_json::from_slice(&cancel_output).unwrap();
    assert_eq!(cancel["status"], "cancelled");
    assert_eq!(cancel["run_id"], run_id);
    assert_eq!(cancel["workflow_id"], workflow_id);
    assert_eq!(cancel["previous_status"], "accepted");
    assert_eq!(cancel["origin"], "opencode");
    assert!(!cancel["cancelled_at"].as_str().unwrap().is_empty());

    let status_output = forge()
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

    let status: Value = serde_json::from_slice(&status_output).unwrap();
    assert_eq!(status["status"], "cancelled");
}

#[test]
fn request_heartbeat_marks_async_run_active_and_surfaces_it_in_status_list_and_inspect() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let started = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "start",
            "--goal",
            "Self-evolution executor heartbeat visibility",
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

    let heartbeat = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "heartbeat",
            "--run",
            run_id,
            "--executor",
            "codex",
            "--summary",
            "cycle 11 executor is applying a bounded patch",
            "--ttl-seconds",
            "600",
            "--pid",
            "4242",
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

    let heartbeat_json: Value = serde_json::from_slice(&heartbeat).unwrap();
    assert_eq!(heartbeat_json["status"], "running");
    assert_eq!(heartbeat_json["previous_status"], "accepted");
    assert_eq!(heartbeat_json["activity"]["active"], true);
    assert_eq!(heartbeat_json["activity"]["heartbeat_status"], "fresh");
    assert_eq!(heartbeat_json["activity"]["executor"], "codex");
    assert_eq!(
        heartbeat_json["activity"]["summary"],
        "cycle 11 executor is applying a bounded patch"
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
    assert_eq!(status_json["status"], "running");
    assert_eq!(status_json["activity"]["active"], true);
    assert_eq!(status_json["activity"]["heartbeat_status"], "fresh");
    assert_eq!(
        status_json["activity"]["summary"],
        "cycle 11 executor is applying a bounded patch"
    );

    let running_requests = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "list",
            "--status",
            "running",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let running_json: Value = serde_json::from_slice(&running_requests).unwrap();
    assert_eq!(running_json["total"], 1);
    assert_eq!(running_json["runs"][0]["run_id"], run_id);
    assert_eq!(running_json["runs"][0]["activity"]["active"], true);
    assert_eq!(running_json["runs"][0]["activity"]["executor"], "codex");

    let inspected = forge()
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
    let inspected_json: Value = serde_json::from_slice(&inspected).unwrap();
    assert_eq!(inspected_json["lifecycle_state"], "running");
    assert_eq!(inspected_json["active_run_count"], 1);
    assert_eq!(
        inspected_json["run_statuses"],
        serde_json::json!(["running"])
    );
    assert!(inspected_json["diagram"]
        .as_str()
        .unwrap()
        .contains("runs:"));
}

#[test]
fn mcp_run_heartbeat_tool_keeps_agent_handoff_observable() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let tools = forge()
        .args(["mcp", "tools", "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let manifest: Value = serde_json::from_slice(&tools).unwrap();
    assert!(manifest["tools"].as_array().unwrap().iter().any(|tool| {
        tool["name"] == "forge.run.heartbeat"
            && tool["output_schema"] == "forge.request_heartbeat.v1"
            && tool["async_safe"] == true
            && tool["mutates_workflow"] == true
    }));

    assert!(
        forge_core::skill::SKILL_MD.contains("forge request heartbeat"),
        "the packaged skill must teach executors to keep active runs observable"
    );
    assert!(
        forge_core::skill::SKILL_MD.contains("forge.run.heartbeat"),
        "the packaged skill must expose the MCP heartbeat tool"
    );

    let started = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "start",
            "--goal",
            "MCP heartbeat test",
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
    let input = serde_json::json!({
        "run_id": run_id,
        "executor": "opencode",
        "summary": "agent heartbeat through MCP",
        "ttl_seconds": 600,
        "origin": "mcp"
    })
    .to_string();

    let heartbeat = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.run.heartbeat",
            "--input",
            &input,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let heartbeat_json: Value = serde_json::from_slice(&heartbeat).unwrap();
    assert_eq!(heartbeat_json["status"], "ok");
    assert_eq!(heartbeat_json["result"]["status"], "running");
    assert_eq!(heartbeat_json["result"]["activity"]["active"], true);
    assert_eq!(heartbeat_json["result"]["activity"]["executor"], "opencode");
}

#[test]
fn request_cancel_with_mcp_tool_works_through_mcp_protocol() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let started = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "start",
            "--goal",
            "Cancel through MCP test",
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

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.request.cancel",
            "--input",
            &format!(r#"{{"run_id":"{}","origin":"mcp"}}"#, run_id),
            "--output",
            "json",
        ])
        .assert()
        .success();
}

#[test]
fn request_list_through_mcp_tool_returns_runs() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "request",
            "start",
            "--goal",
            "MCP list test",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let mcp_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.request.list",
            "--input",
            r#"{"status":"accepted"}"#,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let mcp: Value = serde_json::from_slice(&mcp_output).unwrap();
    assert_eq!(mcp["status"], "ok");
    assert_eq!(mcp["tool_name"], "forge.request.list");
    assert_eq!(mcp["result"]["total"], 1);
    assert_eq!(mcp["result"]["runs"][0]["status"], "accepted");
}

#[test]
fn schedule_summary_and_loop_summary_report_aggregate_state_across_workflows() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "summary",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        json["schema_version"],
        "forge.schedule.aggregate_summary.v1"
    );
    assert_eq!(json["workflow_count"], 0);
    assert_eq!(json["summary"]["scheduled_nodes"], 0);

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
    let plan: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(plan["status"], "planned");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "create-daily-goal-research",
            "--goal",
            "hackathon",
            "--timezone",
            "America/Sao_Paulo",
            "--cron",
            "0 8 * * *",
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
    let research: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(research["status"], "daily_goal_research_workflow_created");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "summary",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        json["schema_version"],
        "forge.schedule.aggregate_summary.v1"
    );
    assert_eq!(json["workflow_count"], 2);
    assert!(json["summary"]["scheduled_nodes"].as_u64().unwrap_or(0) >= 1);
    assert!(json["summary"]["cron_nodes"].as_u64().unwrap_or(0) >= 1);
    assert_eq!(json["loop_summary"]["loop_nodes"], 1);
    assert_eq!(json["loop_summary"]["loop_over_items_nodes"], 1);
    assert!(json["scale_to_zero_workflows"].as_u64().unwrap_or(0) >= 1);

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "loop-summary",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        json["schema_version"],
        "forge.schedule.aggregate_summary.v1"
    );
    assert!(json["loop_summary"]["loop_nodes"].as_u64().unwrap_or(0) >= 1);
    assert!(json["loop_summary"]["total_items"].as_u64().unwrap_or(0) >= 1);
}

#[test]
fn mcp_schedule_summary_tools_return_aggregate_state_for_agents() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let manifest = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "tools",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let manifest_json: Value = serde_json::from_slice(&manifest).unwrap();
    let summary_tool = find_mcp_tool(&manifest_json, "forge.schedule.summary");
    assert_eq!(summary_tool["async_safe"], true);
    assert_eq!(summary_tool["mutates_workflow"], false);
    let loop_summary_tool = find_mcp_tool(&manifest_json, "forge.schedule.loop_summary");
    assert_eq!(loop_summary_tool["async_safe"], true);
    assert_eq!(loop_summary_tool["mutates_workflow"], false);

    let created = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "create-daily-goal-research",
            "--goal",
            "hackathon",
            "--timezone",
            "America/Sao_Paulo",
            "--cron",
            "0 8 * * *",
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
    let created_json: Value = serde_json::from_slice(&created).unwrap();
    assert_eq!(
        created_json["status"],
        "daily_goal_research_workflow_created"
    );

    let summary = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.schedule.summary"])
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let summary_json: Value = serde_json::from_slice(&summary).unwrap();
    assert_eq!(summary_json["status"], "ok");
    assert_eq!(summary_json["tool_name"], "forge.schedule.summary");
    assert_eq!(
        summary_json["result"]["schema_version"],
        "forge.schedule.aggregate_summary.v1"
    );
    assert_eq!(summary_json["result"]["workflow_count"], 1);
    assert_eq!(summary_json["result"]["summary"]["cron_nodes"], 1);
    assert_eq!(summary_json["result"]["loop_summary"]["loop_nodes"], 1);

    let loop_summary = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.schedule.loop_summary"])
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let loop_summary_json: Value = serde_json::from_slice(&loop_summary).unwrap();
    assert_eq!(loop_summary_json["status"], "ok");
    assert_eq!(
        loop_summary_json["tool_name"],
        "forge.schedule.loop_summary"
    );
    assert_eq!(loop_summary_json["result"]["workflow_count"], 1);
    assert_eq!(
        loop_summary_json["result"]["loop_summary"]["loop_over_items_nodes"],
        1
    );
}

#[test]
fn schedule_worker_status_reports_sleep_backpressure_and_scale_to_zero_plan() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let idle = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "create-daily-goal-research",
            "--goal",
            "hackathon",
            "--cron",
            "0 8 * * *",
            "--timezone",
            "America/Sao_Paulo",
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
    let idle_json: Value = serde_json::from_slice(&idle).unwrap();
    let idle_workflow_id = idle_json["workflow_id"].as_str().unwrap();

    let sleeping = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "worker-status",
            "--executor",
            "forge-scheduler",
            "--max-workers",
            "2",
            "--ttl-seconds",
            "120",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let sleeping_json: Value = serde_json::from_slice(&sleeping).unwrap();
    assert_eq!(
        sleeping_json["schema_version"],
        "forge.schedule.worker_status.v1"
    );
    assert_eq!(sleeping_json["status"], "sleeping_until_next_wakeup");
    assert_eq!(sleeping_json["executor"], "forge-scheduler");
    assert_eq!(sleeping_json["worker_pool"]["max_workers"], 2);
    assert_eq!(sleeping_json["worker_pool"]["assignable_due_workflows"], 0);
    assert_eq!(sleeping_json["sleep"]["sleep_until_next_wakeup"], true);
    assert!(sleeping_json["sleep"]["next_wakeup_at"]
        .as_str()
        .unwrap()
        .contains('T'));
    assert_eq!(sleeping_json["summary"]["scale_to_zero_workflows"], 1);
    assert_eq!(
        sleeping_json["workflows"][0]["workflow_id"],
        idle_workflow_id
    );
    assert_eq!(
        sleeping_json["workflows"][0]["scale_to_zero_eligible"],
        true
    );

    for goal in ["marathon", "competition"] {
        let created = forge()
            .args([
                "--store",
                store.to_str().unwrap(),
                "schedule",
                "create-daily-goal-research",
                "--goal",
                goal,
                "--cron",
                "0 8 * * *",
                "--timezone",
                "America/Sao_Paulo",
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
        let created_json: Value = serde_json::from_slice(&created).unwrap();
        let workflow_id = created_json["workflow_id"].as_str().unwrap();
        let schedule_task_id = created_json["workflow"]["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|task| task["schedule"].is_object())
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();

        forge()
            .args([
                "--store",
                store.to_str().unwrap(),
                "schedule",
                "update",
                "--workflow",
                workflow_id,
                "--task",
                &schedule_task_id,
                "--next-run-at",
                "2000-01-01T00:00:00Z",
                "--origin",
                "codex",
                "--output",
                "json",
            ])
            .assert()
            .success();
    }

    let ready = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "worker-status",
            "--executor",
            "forge-scheduler",
            "--max-workers",
            "1",
            "--ttl-seconds",
            "120",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let ready_json: Value = serde_json::from_slice(&ready).unwrap();
    assert_eq!(ready_json["status"], "ready_due_work");
    assert_eq!(ready_json["summary"]["scanned_workflows"], 3);
    assert_eq!(ready_json["summary"]["due_workflows"], 2);
    assert_eq!(ready_json["summary"]["idle_workflows"], 1);
    assert_eq!(ready_json["worker_pool"]["max_workers"], 1);
    assert_eq!(ready_json["worker_pool"]["assignable_due_workflows"], 1);
    assert_eq!(
        ready_json["worker_pool"]["assignment_plan"]["schema_version"],
        "forge.schedule.assignment_plan.v1"
    );
    assert_eq!(
        ready_json["worker_pool"]["assignment_plan"]["max_workers"],
        1
    );
    assert_eq!(
        ready_json["worker_pool"]["assignment_plan"]["assigned"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        ready_json["worker_pool"]["assignment_plan"]["queued"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        ready_json["worker_pool"]["assignment_plan"]["assigned"][0]["lease_scope"],
        "schedule_task"
    );
    assert_eq!(
        ready_json["worker_pool"]["assignment_plan"]["assigned"][0]["wave"],
        1
    );
    let assigned_workflow = ready_json["worker_pool"]["assignment_plan"]["assigned"][0]
        ["workflow_id"]
        .as_str()
        .unwrap();
    let queued_workflow = ready_json["worker_pool"]["assignment_plan"]["queued"][0]["workflow_id"]
        .as_str()
        .unwrap();
    assert!(assigned_workflow < queued_workflow);
    assert!(
        ready_json["worker_pool"]["assignment_plan"]["deterministic_ordering"]
            .as_bool()
            .unwrap()
    );
    assert_eq!(ready_json["backpressure"]["active"], true);
    assert_eq!(ready_json["backpressure"]["queued_due_workflows"], 1);
    assert_eq!(ready_json["sleep"]["sleep_until_next_wakeup"], false);
    assert_eq!(ready_json["sleep"]["sleep_seconds"], 0);
    assert_eq!(ready_json["cancellation"]["supported"], true);
    assert_eq!(ready_json["cancellation"]["lease_ttl_seconds"], 120);
}

#[test]
fn mcp_schedule_worker_status_tool_exposes_native_scheduler_worker_surface() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let manifest = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "tools",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let manifest_json: Value = serde_json::from_slice(&manifest).unwrap();
    let tool = find_mcp_tool(&manifest_json, "forge.schedule.worker_status");
    assert_eq!(tool["output_schema"], "forge.schedule.worker_status.v1");
    assert_eq!(tool["async_safe"], true);
    assert_eq!(tool["mutates_workflow"], false);

    let output = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.schedule.worker_status"])
        .arg("--input")
        .arg(r#"{"executor":"mcp-scheduler","max_workers":3,"ttl_seconds":90}"#)
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["tool_name"], "forge.schedule.worker_status");
    assert_eq!(
        json["result"]["schema_version"],
        "forge.schedule.worker_status.v1"
    );
    assert_eq!(json["result"]["executor"], "mcp-scheduler");
    assert_eq!(json["result"]["worker_pool"]["max_workers"], 3);
    assert_eq!(
        json["result"]["worker_pool"]["assignment_plan"]["schema_version"],
        "forge.schedule.assignment_plan.v1"
    );
    assert!(
        json["result"]["worker_pool"]["assignment_plan"]["deterministic_ordering"]
            .as_bool()
            .unwrap()
    );
    assert_eq!(json["result"]["cancellation"]["lease_ttl_seconds"], 90);
}

#[test]
fn schedule_create_cli_models_daily_goal_research_with_multiple_goals() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "create-daily-goal-research",
            "--goal",
            "hackathon",
            "--goal",
            "blockchain",
            "--timezone",
            "America/Sao_Paulo",
            "--cron",
            "0 8 * * *",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["status"], "daily_goal_research_workflow_created");
    let workflow_id = json["workflow_id"].as_str().unwrap();
    assert!(workflow_id.starts_with("wf_"));

    assert_eq!(
        json["goals"],
        serde_json::json!(["blockchain", "hackathon"])
    );
    assert_eq!(json["schedule_summary"]["scheduled_nodes"], 1);
    assert_eq!(json["loop_summary"]["loop_nodes"], 1);
    assert_eq!(json["loop_summary"]["total_items"], 2);
    assert_eq!(json["attached_subflows"], 0);

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
    assert_eq!(inspection["loop_summary"]["total_items"], 2);
    assert!(inspection["diagram"]
        .as_str()
        .unwrap()
        .contains("loop loop_over_items items blockchain,hackathon"));
}

#[test]
fn mcp_schedule_update_mutates_cron_and_timezone() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let created = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "create-daily-goal-research",
            "--goal",
            "hackathon",
            "--cron",
            "0 8 * * *",
            "--timezone",
            "America/Sao_Paulo",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let created_json: Value = serde_json::from_slice(&created).unwrap();
    let workflow_id = created_json["workflow_id"].as_str().unwrap();
    let tasks = created_json["workflow"]["tasks"].as_array().unwrap();
    let schedule_task = tasks
        .iter()
        .find(|task| task["executor"] == "wait")
        .unwrap();
    let task_id = schedule_task["id"].as_str().unwrap();

    let due_at = "2000-01-01T00:00:00Z";
    let update_input = serde_json::json!({
        "workflow_id": workflow_id,
        "task_id": task_id,
        "cron": "0 9 * * 1",
        "timezone": "America/New_York",
        "next_run_at": due_at,
        "origin": "codex"
    })
    .to_string();

    let updated = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.schedule.update"])
        .arg("--input")
        .arg(&update_input)
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let updated_json: Value = serde_json::from_slice(&updated).unwrap();
    assert_eq!(updated_json["status"], "ok");
    assert_eq!(updated_json["result"]["status"], "schedule_updated");
    assert_eq!(updated_json["result"]["schedule"]["cron"], "0 9 * * 1");
    assert_eq!(
        updated_json["result"]["schedule"]["timezone"],
        "America/New_York"
    );
    assert_eq!(updated_json["result"]["schedule"]["next_run_at"], due_at);
    assert!(updated_json["result"]["revision"].as_u64().unwrap() >= 1);
}

#[test]
fn plan_models_loop_kinds_from_goal_text() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    for (goal_suffix, expected_kind) in [
        ("loop over items", "loop_over_items"),
        ("bounded repeat", "bounded_repeat"),
        ("retry backoff", "retry_backoff"),
        ("retry with backoff", "retry_backoff"),
        ("while condition", "while_until"),
        ("while/until policy", "while_until"),
        ("infinite recurring subflow", "infinite_recurring_subflow"),
        ("recurring subflow", "infinite_recurring_subflow"),
    ] {
        let goal = format!("Run a {goal_suffix} workflow task");
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
        let tasks = json["tasks"].as_array().unwrap();
        let loop_task = tasks
            .iter()
            .find(|task| {
                task["loop_control"].is_object() && task["loop_control"]["kind"] == expected_kind
            })
            .unwrap_or_else(|| {
                panic!("no task with loop_control kind {expected_kind} in goal {goal}")
            });

        assert_eq!(
            loop_task["loop_control"]["kind"], expected_kind,
            "goal: {goal}"
        );

        let subflow_task = tasks
            .iter()
            .find(|task| {
                task["title"]
                    .as_str()
                    .map(|title| title.contains(expected_kind))
                    .unwrap_or(false)
            })
            .unwrap_or_else(|| {
                panic!("no subflow task for loop kind {expected_kind} in goal {goal}")
            });

        assert!(
            subflow_task["loop_control"].is_object(),
            "subflow task should have loop_control in goal {goal}"
        );
    }
}

#[test]
fn inspect_scheduled_workflow_diagram_exposes_loop_and_cron_details() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "create-daily-goal-research",
            "--goal",
            "hackathon",
            "--cron",
            "0 8 * * *",
            "--timezone",
            "America/Sao_Paulo",
            "--origin",
            "forge_cli",
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
            "schedule",
            "inspect",
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

    let inspection: Value = serde_json::from_slice(&inspect_output).unwrap();
    assert_eq!(inspection["status"], "inspected");
    assert_eq!(inspection["schedule_summary"]["cron_nodes"], 1);
    assert_eq!(inspection["loop_summary"]["loop_over_items_nodes"], 1);
    assert_eq!(
        inspection["schedule_summary"]["scale_to_zero_when_idle_nodes"],
        1
    );

    let diagram = inspection["diagram"].as_str().unwrap();
    assert!(diagram.contains("cron 0 8 * * * America/Sao_Paulo"));
    assert!(diagram.contains("loop loop_over_items"));
    assert!(diagram.contains("scale_to_zero true"));
    assert!(diagram.contains("native_subflow goal_research:hackathon"));

    let nodes = inspection["nodes"].as_array().unwrap();
    let schedule_node = nodes
        .iter()
        .find(|node| node["schedule"].is_object())
        .unwrap();
    assert_eq!(schedule_node["schedule"]["kind"], "cron");
    assert_eq!(schedule_node["schedule"]["cron"], "0 8 * * *");
    assert_eq!(schedule_node["schedule"]["timezone"], "America/Sao_Paulo");
    assert_eq!(
        schedule_node["schedule"]["missed_run_policy"],
        "run_once_then_resume"
    );
    assert_eq!(schedule_node["schedule"]["scale_to_zero_when_idle"], true);

    let loop_node = nodes
        .iter()
        .find(|node| node["loop_control"].is_object())
        .unwrap();
    assert_eq!(loop_node["loop_control"]["kind"], "loop_over_items");
    assert_eq!(
        loop_node["loop_control"]["items"],
        serde_json::json!(["hackathon"])
    );
    assert_eq!(loop_node["loop_control"]["subflow_mode"], "finite_per_item");
}

#[test]
fn schedule_pause_resume_stop_controls_loop_node_state() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let created = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "create-daily-goal-research",
            "--goal",
            "hackathon",
            "--cron",
            "0 8 * * *",
            "--timezone",
            "America/Sao_Paulo",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let created_json: Value = serde_json::from_slice(&created).unwrap();
    let workflow_id = created_json["workflow_id"].as_str().unwrap().to_string();

    let tasks = created_json["workflow"]["tasks"].as_array().unwrap();
    let loop_task = tasks
        .iter()
        .find(|task| task["loop_control"].is_object())
        .unwrap();
    let loop_task_id = loop_task["id"].as_str().unwrap();

    let paused = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "pause",
            "--workflow",
            &workflow_id,
            "--task",
            loop_task_id,
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let paused_json: Value = serde_json::from_slice(&paused).unwrap();
    assert_eq!(paused_json["status"], "loop_state_updated");
    assert_eq!(paused_json["previous_state"], "waiting_for_schedule");
    assert_eq!(paused_json["new_state"], "paused");

    let resumed = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "resume",
            "--workflow",
            &workflow_id,
            "--task",
            loop_task_id,
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let resumed_json: Value = serde_json::from_slice(&resumed).unwrap();
    assert_eq!(resumed_json["status"], "loop_state_updated");
    assert_eq!(resumed_json["previous_state"], "paused");
    assert_eq!(resumed_json["new_state"], "active");

    let stopped = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "stop",
            "--workflow",
            &workflow_id,
            "--task",
            loop_task_id,
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stopped_json: Value = serde_json::from_slice(&stopped).unwrap();
    assert_eq!(stopped_json["status"], "loop_state_updated");
    assert_eq!(stopped_json["previous_state"], "active");
    assert_eq!(stopped_json["new_state"], "stopped");
}

#[test]
fn schedule_run_due_reports_no_due_when_next_run_is_in_future() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let created = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "create-daily-goal-research",
            "--goal",
            "hackathon",
            "--cron",
            "0 8 * * *",
            "--timezone",
            "America/Sao_Paulo",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let created_json: Value = serde_json::from_slice(&created).unwrap();
    let workflow_id = created_json["workflow_id"].as_str().unwrap().to_string();

    let run_due = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "run-due",
            "--workflow",
            &workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let run_due_json: Value = serde_json::from_slice(&run_due).unwrap();
    assert_eq!(run_due_json["status"], "no_due_cron_nodes");
    assert_eq!(run_due_json["due_executed"], false);
    assert_eq!(run_due_json["goal"], "Create daily Goal research workflow for Goals: hackathon in America/Sao_Paulo cron 0 8 * * *");
    assert_eq!(
        run_due_json["scale_to_zero"]["schema_version"],
        "forge.scale_to_zero_decision.v1"
    );
    assert_eq!(run_due_json["scale_to_zero"]["applied"], true);
    assert_eq!(
        run_due_json["scale_to_zero"]["reason"],
        "finite_workflow_has_no_due_scheduled_work"
    );
    assert!(run_due_json["scale_to_zero"]["next_wakeup_at"]
        .as_str()
        .unwrap()
        .contains('T'));
    assert_eq!(run_due_json["schedule_summary"]["due_nodes"], 0);

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
    let row = find_workflow(&listed_json, &workflow_id);
    assert_eq!(row["lifecycle_state"], "scaled_to_zero");
    assert_eq!(row["running"], false);

    let inspected = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "inspect",
            &workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let inspected_json: Value = serde_json::from_slice(&inspected).unwrap();
    assert_eq!(inspected_json["lifecycle_state"], "scaled_to_zero");
}

#[test]
fn schedule_scan_due_executes_due_workflows_with_lease_and_scales_idle_workflows_to_zero() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let due_created = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "create-daily-goal-research",
            "--goal",
            "hackathon",
            "--cron",
            "0 8 * * *",
            "--timezone",
            "America/Sao_Paulo",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let due_created_json: Value = serde_json::from_slice(&due_created).unwrap();
    let due_workflow_id = due_created_json["workflow_id"]
        .as_str()
        .unwrap()
        .to_string();
    let due_schedule_task_id = due_created_json["workflow"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["schedule"].is_object())
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let idle_created = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "create-daily-goal-research",
            "--goal",
            "grant",
            "--cron",
            "0 9 * * *",
            "--timezone",
            "America/Sao_Paulo",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let idle_created_json: Value = serde_json::from_slice(&idle_created).unwrap();
    let idle_workflow_id = idle_created_json["workflow_id"]
        .as_str()
        .unwrap()
        .to_string();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "update",
            "--workflow",
            &due_workflow_id,
            "--task",
            &due_schedule_task_id,
            "--next-run-at",
            "2000-01-01T00:00:00Z",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let scanned = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "scan-due",
            "--executor",
            "forge-scheduler",
            "--ttl-seconds",
            "60",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let scanned_json: Value = serde_json::from_slice(&scanned).unwrap();
    assert_eq!(scanned_json["schema_version"], "forge.schedule.scan_due.v1");
    assert_eq!(scanned_json["status"], "schedule_scan_completed");
    assert_eq!(scanned_json["summary"]["scanned_workflows"], 2);
    assert_eq!(scanned_json["summary"]["due_workflows"], 1);
    assert_eq!(scanned_json["summary"]["executed_workflows"], 1);
    assert_eq!(scanned_json["summary"]["scale_to_zero_workflows"], 1);
    assert_eq!(scanned_json["results"].as_array().unwrap().len(), 2);

    let due_result = scanned_json["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|result| result["workflow_id"] == due_workflow_id)
        .unwrap();
    assert_eq!(due_result["status"], "due_workflow_executed");
    assert_eq!(due_result["schedule_task_id"], due_schedule_task_id);
    assert_eq!(due_result["lease_status"], "lease_acquired");
    assert!(due_result["lease_id"]
        .as_str()
        .unwrap()
        .starts_with("lease_"));
    assert_eq!(due_result["run_due"]["status"], "due_workflow_executed");
    assert_eq!(
        due_result["run_due"]["daily_goal_research"]["artifact_count"],
        3
    );

    let idle_result = scanned_json["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|result| result["workflow_id"] == idle_workflow_id)
        .unwrap();
    assert_eq!(idle_result["status"], "no_due_cron_nodes");
    assert_eq!(idle_result["lease_status"], "not_required");
    assert!(idle_result["lease_id"].is_null());
    assert_eq!(idle_result["run_due"]["scale_to_zero"]["applied"], true);
    assert_eq!(
        idle_result["run_due"]["scale_to_zero"]["reason"],
        "finite_workflow_has_no_due_scheduled_work"
    );
}

#[test]
fn schedule_run_due_executes_after_simulate_advances_next_run() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let created = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "create-daily-goal-research",
            "--goal",
            "hackathon",
            "--cron",
            "0 8 * * *",
            "--timezone",
            "America/Sao_Paulo",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let created_json: Value = serde_json::from_slice(&created).unwrap();
    let workflow_id = created_json["workflow_id"].as_str().unwrap().to_string();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "run",
            "--workflow",
            &workflow_id,
            "--simulate",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let run_due = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "run-due",
            "--workflow",
            &workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let run_due_json: Value = serde_json::from_slice(&run_due).unwrap();
    assert_eq!(run_due_json["status"], "no_due_cron_nodes");
    assert_eq!(run_due_json["due_executed"], false);
}

#[test]
fn schedule_update_next_run_at_and_run_due_generates_goal_artifacts() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let created = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "create-daily-goal-research",
            "--goal",
            "hackathon",
            "--cron",
            "0 8 * * *",
            "--timezone",
            "America/Sao_Paulo",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let created_json: Value = serde_json::from_slice(&created).unwrap();
    let workflow_id = created_json["workflow_id"].as_str().unwrap().to_string();
    let schedule_task_id = created_json["workflow"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["schedule"].is_object())
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let loop_task_id = created_json["workflow"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["loop_control"].is_object())
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let due_at = "2000-01-01T00:00:00Z";
    let updated = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "update",
            "--workflow",
            &workflow_id,
            "--task",
            &schedule_task_id,
            "--next-run-at",
            due_at,
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
    let updated_json: Value = serde_json::from_slice(&updated).unwrap();
    assert_eq!(updated_json["status"], "schedule_updated");
    assert_eq!(updated_json["schedule"]["next_run_at"], due_at);

    let run_due = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "run-due",
            "--workflow",
            &workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let run_due_json: Value = serde_json::from_slice(&run_due).unwrap();
    assert_eq!(run_due_json["status"], "due_workflow_executed");
    assert_eq!(run_due_json["due_executed"], true);
    assert_eq!(
        run_due_json["daily_goal_research"]["status"],
        "smoke_artifacts_generated"
    );
    assert_eq!(run_due_json["daily_goal_research"]["artifact_count"], 3);
    assert_eq!(
        run_due_json["daily_goal_research"]["goals"][0]["goal"],
        "hackathon"
    );
    assert_eq!(
        run_due_json["daily_goal_research"]["goals"][0]["telegram_delivery"]["secret_exposed"],
        false
    );
    let goal_lineage = &run_due_json["daily_goal_research"]["goals"][0]["lineage"];
    let run_id = goal_lineage["run_id"].as_str().unwrap();
    assert!(run_id.starts_with("run_"));
    assert_eq!(goal_lineage["workflow_id"], workflow_id);
    assert_eq!(goal_lineage["schedule_task_id"], schedule_task_id);
    assert_eq!(goal_lineage["loop_task_id"], loop_task_id);
    assert_eq!(goal_lineage["goal"], "hackathon");
    assert_eq!(goal_lineage["subflow_id"], "goal_research:hackathon");
    assert_eq!(goal_lineage["triggered_by"], format!("loop:{loop_task_id}"));

    let inspected = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "inspect",
            &workflow_id,
            "--verbose",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let inspected_json: Value = serde_json::from_slice(&inspected).unwrap();
    let schedule_node = inspected_json["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["schedule"].is_object())
        .unwrap();
    let run_history = schedule_node["schedule"]["run_history"].as_array().unwrap();
    assert_eq!(run_history.len(), 1);
    assert_eq!(run_history[0]["scheduled_at"], due_at);
    assert_eq!(run_history[0]["status"], "completed");
    assert_eq!(run_history[0]["missed"], true);
    assert_eq!(inspected_json["schedule_summary"]["missed_run_nodes"], 1);

    let artifacts = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "artifacts",
            "--workflow",
            &workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let artifacts_json: Value = serde_json::from_slice(&artifacts).unwrap();
    let paths = artifacts_json["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|artifact| artifact["path"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert!(paths
        .iter()
        .any(|path| path.ends_with("goal-hackathon-report.md")));
    assert!(paths
        .iter()
        .any(|path| path.ends_with("goal-hackathon-report.pdf")));
    let delivery_path = paths
        .iter()
        .find(|path| path.ends_with("telegram-delivery-hackathon.json"))
        .unwrap();
    let delivery = fs::read_to_string(temp.path().join(delivery_path)).unwrap();
    assert!(delivery.contains("configured_telegram_chat_ref"));
    assert!(delivery.contains(&workflow_id));
    assert!(delivery.contains(run_id));
    assert!(!delivery.contains("bot_token"));
    assert!(!delivery.contains("chat_id"));

    let status = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "status",
            "--workflow",
            &workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let status_json: Value = serde_json::from_slice(&status).unwrap();
    for kind in ["markdown_report", "pdf_report", "telegram_delivery"] {
        let artifact = status_json["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|artifact| artifact["kind"] == kind)
            .unwrap_or_else(|| panic!("missing artifact kind {kind}"));
        assert_eq!(artifact["lineage"]["workflow_id"], workflow_id);
        assert_eq!(artifact["lineage"]["run_id"], run_id);
        assert_eq!(artifact["lineage"]["goal"], "hackathon");
        assert_eq!(artifact["lineage"]["subflow_id"], "goal_research:hackathon");
    }
}

#[test]
fn schedule_run_due_skip_missed_policy_records_history_without_artifacts() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let created = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "create-daily-goal-research",
            "--goal",
            "hackathon",
            "--cron",
            "0 8 * * *",
            "--timezone",
            "America/Sao_Paulo",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let created_json: Value = serde_json::from_slice(&created).unwrap();
    let workflow_id = created_json["workflow_id"].as_str().unwrap().to_string();
    let schedule_task_id = created_json["workflow"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["schedule"].is_object())
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let due_at = "2000-01-01T00:00:00Z";
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "update",
            "--workflow",
            &workflow_id,
            "--task",
            &schedule_task_id,
            "--next-run-at",
            due_at,
            "--missed-run-policy",
            "skip_missed",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let run_due = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "run-due",
            "--workflow",
            &workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let run_due_json: Value = serde_json::from_slice(&run_due).unwrap();
    assert_eq!(run_due_json["status"], "missed_runs_skipped");
    assert_eq!(run_due_json["due_executed"], false);
    assert!(run_due_json["daily_goal_research"].is_null());
    assert_eq!(run_due_json["schedule_summary"]["missed_run_nodes"], 1);

    let inspected = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "inspect",
            &workflow_id,
            "--verbose",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let inspected_json: Value = serde_json::from_slice(&inspected).unwrap();
    let schedule_node = inspected_json["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["schedule"].is_object())
        .unwrap();
    let schedule = &schedule_node["schedule"];
    assert_eq!(schedule["missed_run_policy"], "skip_missed");
    assert_ne!(schedule["next_run_at"], due_at);
    let run_history = schedule["run_history"].as_array().unwrap();
    assert_eq!(run_history.len(), 1);
    assert_eq!(run_history[0]["scheduled_at"], due_at);
    assert_eq!(run_history[0]["status"], "skipped_missed");
    assert_eq!(run_history[0]["missed"], true);

    let artifacts = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "artifacts",
            "--workflow",
            &workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let artifacts_json: Value = serde_json::from_slice(&artifacts).unwrap();
    assert!(artifacts_json["artifacts"].as_array().unwrap().is_empty());
}

#[test]
fn schedule_run_due_reports_missed_run_reconciliation_for_cli_list_and_inspect() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let created = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "create-daily-goal-research",
            "--goal",
            "hackathon",
            "--cron",
            "0 8 * * *",
            "--timezone",
            "America/Sao_Paulo",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let created_json: Value = serde_json::from_slice(&created).unwrap();
    let workflow_id = created_json["workflow_id"].as_str().unwrap().to_string();
    let schedule_task_id = created_json["workflow"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["schedule"].is_object())
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let due_at = "2000-01-01T00:00:00Z";
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "update",
            "--workflow",
            &workflow_id,
            "--task",
            &schedule_task_id,
            "--next-run-at",
            due_at,
            "--missed-run-policy",
            "skip_missed",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let run_due = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "run-due",
            "--workflow",
            &workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let run_due_json: Value = serde_json::from_slice(&run_due).unwrap();
    assert_eq!(run_due_json["status"], "missed_runs_skipped");
    let reconciliation = run_due_json["missed_run_reconciliation"]
        .as_array()
        .unwrap();
    assert_eq!(reconciliation.len(), 1);
    assert_eq!(
        reconciliation[0]["schema_version"],
        "forge.missed_run_reconciliation.v1"
    );
    assert_eq!(reconciliation[0]["task_id"], schedule_task_id);
    assert_eq!(reconciliation[0]["policy"], "skip_missed");
    assert_eq!(reconciliation[0]["action"], "skipped_missed");
    assert_eq!(reconciliation[0]["scheduled_at"], due_at);
    assert_eq!(reconciliation[0]["run_status"], "skipped_missed");
    assert_eq!(reconciliation[0]["artifacts_allowed"], false);

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
    let row = listed_json["workflows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["workflow_id"] == workflow_id)
        .unwrap();
    assert!(row["schedule_summary"]["missed_run_policies"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("skip_missed")));
    assert!(row["schedule_summary"]["missed_run_reconciliation_actions"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("skipped_missed")));

    let inspected = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "inspect",
            &workflow_id,
            "--verbose",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let inspected_json: Value = serde_json::from_slice(&inspected).unwrap();
    assert!(inspected_json["schedule_summary"]["missed_run_policies"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("skip_missed")));
    assert!(
        inspected_json["schedule_summary"]["missed_run_reconciliation_actions"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("skipped_missed"))
    );
    let schedule_node = inspected_json["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["schedule"].is_object())
        .unwrap();
    let run_history = schedule_node["schedule"]["run_history"].as_array().unwrap();
    assert_eq!(run_history[0]["missed_run_policy"], "skip_missed");
    assert_eq!(run_history[0]["reconciliation_action"], "skipped_missed");
}

#[test]
fn schedule_run_due_skips_paused_loop_nodes_without_artifacts() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let created = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "create-daily-goal-research",
            "--goal",
            "hackathon",
            "--cron",
            "0 8 * * *",
            "--timezone",
            "America/Sao_Paulo",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let created_json: Value = serde_json::from_slice(&created).unwrap();
    let workflow_id = created_json["workflow_id"].as_str().unwrap().to_string();
    let tasks = created_json["workflow"]["tasks"].as_array().unwrap();
    let schedule_task_id = tasks
        .iter()
        .find(|task| task["schedule"].is_object())
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let loop_task_id = tasks
        .iter()
        .find(|task| task["loop_control"].is_object())
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "pause",
            "--workflow",
            &workflow_id,
            "--task",
            &loop_task_id,
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
            "schedule",
            "update",
            "--workflow",
            &workflow_id,
            "--task",
            &schedule_task_id,
            "--next-run-at",
            "2000-01-01T00:00:00Z",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let run_due = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "run-due",
            "--workflow",
            &workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let run_due_json: Value = serde_json::from_slice(&run_due).unwrap();
    assert_eq!(run_due_json["status"], "loop_not_runnable");
    assert_eq!(run_due_json["due_executed"], false);
    assert_eq!(run_due_json["blocked_loop_state"], "paused");
    assert_eq!(run_due_json["blocked_loop_task_id"], loop_task_id);

    let inspected = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "inspect",
            &workflow_id,
            "--verbose",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let inspected_json: Value = serde_json::from_slice(&inspected).unwrap();
    let schedule_node = inspected_json["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["schedule"].is_object())
        .unwrap();
    assert!(schedule_node["schedule"]["run_history"]
        .as_array()
        .unwrap()
        .is_empty());

    let artifacts = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "artifacts",
            "--workflow",
            &workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let artifacts_json: Value = serde_json::from_slice(&artifacts).unwrap();
    assert!(artifacts_json["artifacts"].as_array().unwrap().is_empty());
}

#[test]
fn mcp_schedule_pause_resume_stop_exposes_loop_state_control_tools() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let manifest = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "tools",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let manifest_json: Value = serde_json::from_slice(&manifest).unwrap();
    for name in [
        "forge.schedule.pause",
        "forge.schedule.resume",
        "forge.schedule.stop",
        "forge.schedule.run_due",
        "forge.schedule.scan_due",
    ] {
        find_mcp_tool(&manifest_json, name);
    }
}

#[test]
fn mcp_call_schedule_scan_due_runs_native_scheduler_scan() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let created = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "create-daily-goal-research",
            "--goal",
            "hackathon",
            "--cron",
            "0 8 * * *",
            "--timezone",
            "America/Sao_Paulo",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let created_json: Value = serde_json::from_slice(&created).unwrap();
    let workflow_id = created_json["workflow_id"].as_str().unwrap().to_string();
    let schedule_task_id = created_json["workflow"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["schedule"].is_object())
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "update",
            "--workflow",
            &workflow_id,
            "--task",
            &schedule_task_id,
            "--next-run-at",
            "2000-01-01T00:00:00Z",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let input = serde_json::json!({
        "executor": "mcp-scheduler",
        "ttl_seconds": 60
    })
    .to_string();
    let scanned = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.schedule.scan_due"])
        .arg("--input")
        .arg(&input)
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let scanned_json: Value = serde_json::from_slice(&scanned).unwrap();
    assert_eq!(scanned_json["status"], "ok");
    assert_eq!(
        scanned_json["result"]["schema_version"],
        "forge.schedule.scan_due.v1"
    );
    assert_eq!(scanned_json["result"]["summary"]["due_workflows"], 1);
    assert_eq!(
        scanned_json["result"]["results"][0]["workflow_id"],
        workflow_id
    );
    assert_eq!(
        scanned_json["result"]["results"][0]["lease_status"],
        "lease_acquired"
    );
}

#[test]
fn mcp_call_schedule_pause_and_resume_toggles_loop_state() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let created = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "create-daily-goal-research",
            "--goal",
            "hackathon",
            "--cron",
            "0 8 * * *",
            "--timezone",
            "America/Sao_Paulo",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let created_json: Value = serde_json::from_slice(&created).unwrap();
    let workflow_id = created_json["workflow_id"].as_str().unwrap();
    let tasks = created_json["workflow"]["tasks"].as_array().unwrap();
    let loop_task_id = tasks
        .iter()
        .find(|task| task["loop_control"].is_object())
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let pause_input = serde_json::json!({
        "workflow_id": workflow_id,
        "task_id": loop_task_id,
        "origin": "mcp"
    })
    .to_string();
    let paused = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.schedule.pause"])
        .arg("--input")
        .arg(&pause_input)
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let paused_json: Value = serde_json::from_slice(&paused).unwrap();
    assert_eq!(paused_json["status"], "ok");
    assert_eq!(paused_json["result"]["status"], "loop_state_updated");
    assert_eq!(paused_json["result"]["new_state"], "paused");

    let resume_input = serde_json::json!({
        "workflow_id": workflow_id,
        "task_id": loop_task_id,
        "origin": "mcp"
    })
    .to_string();
    let resumed = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.schedule.resume"])
        .arg("--input")
        .arg(&resume_input)
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let resumed_json: Value = serde_json::from_slice(&resumed).unwrap();
    assert_eq!(resumed_json["status"], "ok");
    assert_eq!(resumed_json["result"]["status"], "loop_state_updated");
    assert_eq!(resumed_json["result"]["new_state"], "active");

    let stop_input = serde_json::json!({
        "workflow_id": workflow_id,
        "task_id": loop_task_id,
        "origin": "mcp"
    })
    .to_string();
    let stopped = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.schedule.stop"])
        .arg("--input")
        .arg(&stop_input)
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stopped_json: Value = serde_json::from_slice(&stopped).unwrap();
    assert_eq!(stopped_json["status"], "ok");
    assert_eq!(stopped_json["result"]["status"], "loop_state_updated");
    assert_eq!(stopped_json["result"]["new_state"], "stopped");
}

#[test]
fn mcp_call_schedule_run_due_returns_no_due_for_future_schedule() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let created = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "create-daily-goal-research",
            "--goal",
            "hackathon",
            "--cron",
            "0 8 * * *",
            "--timezone",
            "America/Sao_Paulo",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let created_json: Value = serde_json::from_slice(&created).unwrap();
    let workflow_id = created_json["workflow_id"].as_str().unwrap();

    let run_due_input = serde_json::json!({
        "workflow_id": workflow_id,
    })
    .to_string();
    let run_due = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.schedule.run_due"])
        .arg("--input")
        .arg(&run_due_input)
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let run_due_json: Value = serde_json::from_slice(&run_due).unwrap();
    assert_eq!(run_due_json["status"], "ok");
    assert_eq!(run_due_json["result"]["status"], "no_due_cron_nodes");
    assert_eq!(run_due_json["result"]["due_executed"], false);
}

#[test]
fn simulated_run_includes_parallel_dag_schedule_plan() {
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
    assert!(run["parallel_plan"].is_object());
    assert_eq!(
        run["parallel_plan"]["schema_version"],
        "forge.scheduler.parallel_plan.v1"
    );
    let waves = run["parallel_plan"]["waves"].as_array().unwrap();
    assert!(!waves.is_empty());
    assert!(run["parallel_plan"]["total_waves"].as_u64().unwrap() >= 1);
    assert!(run["parallel_plan"]["total_tasks"].as_u64().unwrap() >= 1);
    for wave in waves {
        assert!(!wave["task_ids"].as_array().unwrap().is_empty());
        assert!(!wave["task_titles"].as_array().unwrap().is_empty());
        assert!(wave["level"].as_u64().unwrap() >= 1);
    }
}

#[test]
fn parallel_scheduling_detects_independent_tasks_in_same_wave() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Daily goal research for Goals: hackathon, competition, marathon in America/Sao_Paulo cron 0 8 * * *",
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

    let plan = forge()
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

    let run: Value = serde_json::from_slice(&plan).unwrap();
    let waves = run["parallel_plan"]["waves"].as_array().unwrap();
    let parallel_waves: Vec<&Value> = waves
        .iter()
        .filter(|w| w["concurrent"].as_bool().unwrap_or(false))
        .collect();
    assert!(
        !parallel_waves.is_empty(),
        "should have at least one wave with concurrent tasks for multi-Goal research"
    );
    assert!(run["parallel_plan"]["parallel_opportunity"]
        .as_bool()
        .unwrap_or(false));
}

#[test]
fn interactive_home_renders_anvil_forge_and_operational_dashboard_sections() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args(["--store", store.to_str().unwrap(), "interactive", "home"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("forge"));
    assert!(text.contains("Active runs"));
    assert!(text.contains("Scheduled workflows"));
    assert!(text.contains("Paused/idle workflows"));
    assert!(text.contains("Recent artifacts"));
    assert!(text.contains("Pending approvals"));
    assert!(text.contains("Validation failures"));
    assert!(text.contains("Executor availability"));
    assert!(text.contains("Runtime/node status"));
    assert!(text.contains("Scheduler worker status"));
    assert!(text.contains("Repository context"));
    assert!(text.contains("Estimated costs"));
    assert!(text.contains("Quick actions"));
    assert!(text.contains("/status"));
    assert!(text.contains("/workflows"));
}

#[test]
fn no_args_non_tty_stays_script_safe_and_does_not_open_dashboard() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "Forge Core workflow runtime -- use `forge --help`",
        ));

    assert!(!store.exists());
}

#[test]
fn no_args_tty_renders_interactive_home_when_pseudo_terminal_is_available() {
    let script = if Path::new("/usr/bin/script").exists() {
        "/usr/bin/script"
    } else if Path::new("/bin/script").exists() {
        "/bin/script"
    } else {
        return;
    };
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let binary = assert_cmd::cargo::cargo_bin("forge");
    let command = format!("{} --store {}", binary.display(), store.display());

    let output = std::process::Command::new(script)
        .args(["-q", "-c", &command, "/dev/null"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("forge"));
    assert!(stdout.contains("Active runs"));
    assert!(stdout.contains("Quick actions"));
    assert!(stdout.contains("/status"));
}

#[test]
fn interactive_slash_command_catalog_is_discoverable_and_scriptable() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "interactive",
            "slash-commands",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["status"], "slash_commands_loaded");
    assert_eq!(
        json["schema_version"],
        "forge.interactive.slash_commands.v1"
    );
    for name in [
        "/help",
        "/status",
        "/list",
        "/inspect",
        "/runs",
        "/workflows",
        "/artifacts",
        "/costs",
        "/config",
        "/sync",
        "/executors",
        "/runtimes",
        "/validate",
        "/approve",
        "/reject",
        "/goal",
        "/attach",
        "/resume",
        "/pause",
        "/stop",
        "/delete",
        "/export",
        "/logs",
        "/update",
        "/workers",
    ] {
        let command = find_slash_command(&json, name);
        assert_eq!(command["name"], name);
        assert_eq!(command["scriptable"], true);
        assert!(command["equivalent_command"].as_array().unwrap().len() >= 2);
    }

    let status = find_slash_command(&json, "/status");
    assert_eq!(status["risk_level"], "low");
    assert_eq!(status["mutates_workflow"], false);
    assert!(status["equivalent_command"]
        .as_array()
        .unwrap()
        .contains(&Value::String("status".to_string())));
}

#[test]
fn interactive_route_answers_simple_question_without_creating_workflow_state() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "interactive",
            "route",
            "--input",
            "What is the current Forge status?",
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
    assert_eq!(json["status"], "routed");
    assert_eq!(json["input_kind"], "chat");
    assert_eq!(json["routing_decision"], "direct_answer");
    assert_eq!(json["workflow_created"], false);
    assert_eq!(json["run_id"], Value::Null);
    assert_eq!(json["workflow_id"], Value::Null);
    assert!(json["answer"]
        .as_str()
        .unwrap()
        .contains("Forge can answer this from current runtime state"));
    assert_eq!(json["retention_decision"]["action"], "none");

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
    assert_eq!(listed_json["summary"]["total"], 0);
}

#[test]
fn interactive_route_complex_request_creates_async_workflow_and_retains_it() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let request = "Research upcoming hackathons every day, validate regulations, generate Markdown/PDF artifacts and send the report to Telegram";

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "interactive",
            "route",
            "--input",
            request,
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
    assert_eq!(json["status"], "routed");
    assert_eq!(json["input_kind"], "chat");
    assert_eq!(json["routing_decision"], "new_workflow");
    assert_eq!(json["workflow_created"], true);
    assert!(json["run_id"].as_str().unwrap().starts_with("run_"));
    assert!(json["workflow_id"].as_str().unwrap().starts_with("wf_"));
    assert_eq!(json["retention_decision"]["action"], "retain");
    assert_eq!(json["retention_decision"]["requires_human_approval"], false);
    assert!(json["routing_explanation"]
        .as_str()
        .unwrap()
        .contains("scheduled work"));

    let status = forge()
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
        .get_output()
        .stdout
        .clone();
    let status_json: Value = serde_json::from_slice(&status).unwrap();
    assert_eq!(status_json["status"], "accepted");
    assert_eq!(status_json["workflow_id"], json["workflow_id"]);
    assert_eq!(status_json["requested_goal"], request);
}

#[test]
fn interactive_route_slash_command_stays_command_mode_without_workflow() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "interactive",
            "route",
            "--input",
            "/status --workflow wf_demo",
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
    assert_eq!(json["input_kind"], "slash_command");
    assert_eq!(json["routing_decision"], "slash_command");
    assert_eq!(json["workflow_created"], false);
    assert_eq!(json["slash_command"]["name"], "/status");
    assert_eq!(json["slash_command"]["recognized"], true);
    assert!(json["slash_command"]["equivalent_command"]
        .as_array()
        .unwrap()
        .contains(&Value::String("status".to_string())));
}

#[test]
fn interactive_retention_requires_approval_before_deleting_artifact_workflow() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "interactive",
            "route",
            "--input",
            "Create a one-off PDF artifact, send it to Telegram, then delete the workflow after the answer",
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
    assert_eq!(json["routing_decision"], "new_workflow");
    assert_eq!(json["workflow_created"], true);
    assert_eq!(json["retention_decision"]["action"], "keep_until_approved");
    assert_eq!(json["retention_decision"]["requires_human_approval"], true);
    assert!(json["retention_decision"]["reason"]
        .as_str()
        .unwrap()
        .contains("artifact"));
    assert!(json["retention_decision"]["reason"]
        .as_str()
        .unwrap()
        .contains("external side effect"));
}

#[test]
fn human_interaction_choice_gate_pauses_run_and_surfaces_pending_state() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let planned = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Prepare a risky deployment that needs a human approve refine or combine decision",
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
    let task = find_task(
        planned_json["tasks"].as_array().unwrap(),
        "Extract requirements",
    );
    let task_id = task["id"].as_str().unwrap();

    let created = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "interaction",
            "create-choice",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--kind",
            "approve_reject_refine_combine",
            "--prompt",
            "Choose the deployment direction before execution continues",
            "--choice",
            "approve=Approve",
            "--choice",
            "refine=Refine",
            "--choice",
            "combine=Combine",
            "--timeout-seconds",
            "3600",
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
    let created_json: Value = serde_json::from_slice(&created).unwrap();
    assert_eq!(created_json["status"], "human_interaction_created");
    assert_eq!(
        created_json["interaction"]["schema_version"],
        "forge.human_interaction.v1"
    );
    assert_eq!(created_json["interaction"]["state"], "pending");
    assert_eq!(created_json["task_status"], "blocked");
    assert_eq!(
        created_json["interaction"]["choices"]
            .as_array()
            .unwrap()
            .len(),
        3
    );

    let run = forge()
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
        .failure()
        .get_output()
        .stdout
        .clone();
    let run_json: Value = serde_json::from_slice(&run).unwrap();
    assert_eq!(run_json["status"], "blocked_on_human_interaction");
    assert_eq!(run_json["blocked_interaction"]["task_id"], task_id);
    assert_eq!(run_json["blocked_interaction"]["state"], "pending");
    assert_eq!(run_json["completed_tasks"], 0);

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
    assert_eq!(status_json["human_interaction_summary"]["pending"], 1);
    assert_eq!(status_json["human_interaction_summary"]["answered"], 0);

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
    assert_eq!(row["human_interaction_summary"]["pending"], 1);

    let inspected = forge()
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
    let inspected_json: Value = serde_json::from_slice(&inspected).unwrap();
    assert_eq!(inspected_json["human_interaction_summary"]["pending"], 1);
    let inspected_task = find_task(
        inspected_json["nodes"].as_array().unwrap(),
        "Extract requirements",
    );
    assert_eq!(inspected_task["human_interaction"]["state"], "pending");
    assert!(inspected_json["diagram"]
        .as_str()
        .unwrap()
        .contains("human_interaction approve_reject_refine_combine pending"));
}

#[test]
fn human_interaction_form_answer_validates_required_fields_and_resumes_workflow() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let planned = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Configure notification and budget inputs before running a workflow",
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
    let task = find_task(planned_json["tasks"].as_array().unwrap(), "Parse intent");
    let task_id = task["id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "interaction",
            "create-form",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--prompt",
            "Provide notification and budget settings",
            "--field",
            "telegram_channel_ref:text:required:configured_telegram_destination",
            "--field",
            "budget_usd:number:required:5",
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
            "interaction",
            "answer",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--field",
            "telegram_channel_ref=configured_telegram_destination",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "missing required form field: budget_usd",
        ));

    let answered = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "interaction",
            "answer",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--field",
            "telegram_channel_ref=configured_telegram_destination",
            "--field",
            "budget_usd=5",
            "--rationale",
            "Use the configured Telegram destination and a bounded budget",
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
    let answered_json: Value = serde_json::from_slice(&answered).unwrap();
    assert_eq!(answered_json["status"], "human_interaction_answered");
    assert_eq!(answered_json["interaction"]["state"], "answered");
    assert_eq!(answered_json["task_status"], "pending");
    assert_eq!(answered_json["decision"]["field_values"]["budget_usd"], "5");
    assert_eq!(
        answered_json["decision"]["affected_tasks"],
        serde_json::json!([task_id])
    );
    assert_eq!(answered_json["decision"]["origin"], "codex");

    let run = forge()
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
    let run_json: Value = serde_json::from_slice(&run).unwrap();
    assert_eq!(run_json["status"], "completed");
}

#[test]
fn human_interaction_timeout_keeps_workflow_blocked_with_audit_state() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let planned = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Request a risk acknowledgement before deleting temporary state",
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
    let task = find_task(planned_json["tasks"].as_array().unwrap(), "Validate build");
    let task_id = task["id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "interaction",
            "create-choice",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
            "--kind",
            "risk_acknowledgement",
            "--prompt",
            "Acknowledge deletion risk before continuing",
            "--choice",
            "acknowledge=Acknowledge",
            "--timeout-seconds",
            "0",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let expired = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "interaction",
            "expire",
            "--workflow",
            workflow_id,
            "--task",
            task_id,
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
    let expired_json: Value = serde_json::from_slice(&expired).unwrap();
    assert_eq!(expired_json["status"], "human_interaction_timed_out");
    assert_eq!(expired_json["interaction"]["state"], "timed_out");
    assert_eq!(
        expired_json["interaction"]["on_timeout"],
        "keep_blocked_and_notify"
    );
    assert_eq!(expired_json["task_status"], "blocked");

    let run = forge()
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
        .failure()
        .get_output()
        .stdout
        .clone();
    let run_json: Value = serde_json::from_slice(&run).unwrap();
    assert_eq!(run_json["status"], "blocked_on_human_interaction");
    assert_eq!(run_json["blocked_interaction"]["state"], "timed_out");

    let inspected = forge()
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
    let inspected_json: Value = serde_json::from_slice(&inspected).unwrap();
    assert_eq!(inspected_json["human_interaction_summary"]["timed_out"], 1);
}

#[test]
fn mcp_exposes_human_interaction_bridge_tools() {
    let manifest = forge()
        .args(["mcp", "tools", "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let manifest_json: Value = serde_json::from_slice(&manifest).unwrap();

    for (name, output_schema, mutates_workflow) in [
        (
            "forge.interaction.create_choice",
            "forge.human_interaction.v1",
            true,
        ),
        (
            "forge.interaction.create_form",
            "forge.human_interaction.v1",
            true,
        ),
        (
            "forge.interaction.answer",
            "forge.human_interaction.v1",
            true,
        ),
        (
            "forge.interaction.expire",
            "forge.human_interaction.v1",
            true,
        ),
        (
            "forge.interaction.list",
            "forge.human_interaction.list.v1",
            false,
        ),
    ] {
        let tool = find_mcp_tool(&manifest_json, name);
        assert_eq!(tool["output_schema"], output_schema);
        assert_eq!(tool["async_safe"], true);
        assert_eq!(tool["mutates_workflow"], mutates_workflow);
    }
}

#[test]
fn mcp_human_interaction_choice_answer_round_trip_preserves_audit_state() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let planned = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Prepare a deployment plan that requires agent-visible human approval",
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
    let task = find_task(
        planned_json["tasks"].as_array().unwrap(),
        "Extract requirements",
    );
    let task_id = task["id"].as_str().unwrap();

    let create_input = serde_json::json!({
        "workflow_id": workflow_id,
        "task_id": task_id,
        "kind": "approve_reject_refine_combine",
        "prompt": "Choose the safe deployment direction",
        "choices": [
            "approve=Approve|Proceed with current scope|resume workflow",
            "refine=Refine|Request a narrower scope|keep workflow paused"
        ],
        "timeout_seconds": 3600,
        "origin": "mcp"
    })
    .to_string();
    let created = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.interaction.create_choice"])
        .arg("--input")
        .arg(&create_input)
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let created_json: Value = serde_json::from_slice(&created).unwrap();
    assert_eq!(created_json["status"], "ok");
    assert_eq!(
        created_json["result"]["status"],
        "human_interaction_created"
    );
    assert_eq!(created_json["result"]["workflow_id"], workflow_id);
    assert_eq!(created_json["result"]["task_id"], task_id);
    assert_eq!(created_json["result"]["interaction"]["state"], "pending");
    assert_eq!(created_json["result"]["origin"], "mcp");

    let listed = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.interaction.list"])
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let listed_json: Value = serde_json::from_slice(&listed).unwrap();
    assert_eq!(listed_json["result"]["summary"]["pending_required"], 1);
    assert_eq!(
        listed_json["result"]["interactions"][0]["interaction"]["kind"],
        "approve_reject_refine_combine"
    );

    let answer_input = serde_json::json!({
        "workflow_id": workflow_id,
        "task_id": task_id,
        "selected_options": ["approve"],
        "rationale": "Approved through the MCP human approval bridge",
        "origin": "mcp"
    })
    .to_string();
    let answered = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.interaction.answer"])
        .arg("--input")
        .arg(&answer_input)
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let answered_json: Value = serde_json::from_slice(&answered).unwrap();
    assert_eq!(
        answered_json["result"]["status"],
        "human_interaction_answered"
    );
    assert_eq!(answered_json["result"]["interaction"]["state"], "answered");
    assert_eq!(answered_json["result"]["decision"]["origin"], "mcp");
    assert_eq!(
        answered_json["result"]["decision"]["selected_options"],
        serde_json::json!(["approve"])
    );
    assert_eq!(answered_json["result"]["task_status"], "pending");
}

#[test]
fn mcp_human_interaction_form_and_expire_validate_like_cli_surface() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let planned = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Collect budget settings before running a scheduled workflow",
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
    let task_id = planned_json["tasks"][0]["id"].as_str().unwrap();

    let form_input = serde_json::json!({
        "workflow_id": workflow_id,
        "task_id": task_id,
        "prompt": "Provide budget settings",
        "fields": [
            "budget_usd:number:required:5",
            "telegram_channel_ref:text:optional:configured_telegram_destination"
        ],
        "timeout_seconds": 0,
        "origin": "mcp"
    })
    .to_string();
    let created = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.interaction.create_form"])
        .arg("--input")
        .arg(&form_input)
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let created_json: Value = serde_json::from_slice(&created).unwrap();
    assert_eq!(created_json["result"]["interaction"]["kind"], "form");
    assert_eq!(
        created_json["result"]["interaction"]["form"]["fields"][0]["id"],
        "budget_usd"
    );

    let invalid_answer = serde_json::json!({
        "workflow_id": workflow_id,
        "task_id": task_id,
        "field_values": ["telegram_channel_ref=configured_telegram_destination"],
        "origin": "mcp"
    })
    .to_string();
    forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.interaction.answer"])
        .arg("--input")
        .arg(&invalid_answer)
        .args(["--output", "json"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "missing required form field: budget_usd",
        ));

    let expired = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["mcp", "call", "forge.interaction.expire"])
        .arg("--input")
        .arg(
            serde_json::json!({
                "workflow_id": workflow_id,
                "task_id": task_id,
                "origin": "mcp"
            })
            .to_string(),
        )
        .args(["--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let expired_json: Value = serde_json::from_slice(&expired).unwrap();
    assert_eq!(
        expired_json["result"]["status"],
        "human_interaction_timed_out"
    );
    assert_eq!(expired_json["result"]["interaction"]["state"], "timed_out");
    assert_eq!(expired_json["result"]["task_status"], "blocked");
}

fn find_slash_command<'a>(json: &'a Value, name: &str) -> &'a Value {
    json["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|command| command["name"] == name)
        .unwrap()
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

// -- Creative artifact IR tests --

#[test]
fn creative_artifact_attach_screen_is_listed_and_inspectable() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    // plan a workflow
    let plan_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Design a landing page",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let plan: Value = serde_json::from_slice(&plan_output).unwrap();
    let workflow_id = plan["workflow_id"].as_str().unwrap();

    // attach a screen creative artifact
    let attach_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "attach-creative",
            "--workflow",
            workflow_id,
            "--title",
            "Landing Page Hero",
            "--kind",
            "screen",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let attach: Value = serde_json::from_slice(&attach_output).unwrap();
    assert_eq!(attach["status"], "creative_artifact_attached");
    assert_eq!(attach["origin"], "forge_cli");
    assert_eq!(attach["artifact"]["kind"], "Screen");
    assert_eq!(attach["artifact"]["title"], "Landing Page Hero");
    let artifact_id = attach["artifact"]["id"].as_str().unwrap();

    // list creative artifacts
    let list_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "list-creative",
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
    let list: Value = serde_json::from_slice(&list_output).unwrap();
    assert_eq!(list["status"], "creative_artifacts_listed");
    let artifacts = list["artifacts"].as_array().unwrap();
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0]["id"], artifact_id);
    assert_eq!(artifacts[0]["kind"], "Screen");

    // inspect creative artifact
    let inspect_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "inspect-creative",
            "--workflow",
            workflow_id,
            "--artifact",
            artifact_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let inspect: Value = serde_json::from_slice(&inspect_output).unwrap();
    assert_eq!(inspect["status"], "creative_artifact_inspected");
    assert_eq!(inspect["artifact"]["id"], artifact_id);
    assert_eq!(inspect["artifact"]["kind"], "screen");
    assert_eq!(inspect["artifact"]["content"]["type"], "screen");
    assert_eq!(inspect["artifact"]["content"]["width_px"], 1440);
    assert_eq!(inspect["artifact"]["content"]["height_px"], 900);
}

#[test]
fn creative_artifact_all_kinds_can_be_attached() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let plan_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Creative suite demo",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let plan: Value = serde_json::from_slice(&plan_output).unwrap();
    let workflow_id = plan["workflow_id"].as_str().unwrap();

    for (kind, title) in [
        ("screen", "App Screen"),
        ("whiteboard", "Brainstorm Board"),
        ("document", "Requirements Doc"),
        ("slide_deck", "Pitch Deck"),
        ("component", "Button Component"),
    ] {
        let attach_output = forge()
            .args([
                "--store",
                store.to_str().unwrap(),
                "workflow",
                "attach-creative",
                "--workflow",
                workflow_id,
                "--title",
                title,
                "--kind",
                kind,
                "--origin",
                "forge_cli",
                "--output",
                "json",
            ])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let attach: Value = serde_json::from_slice(&attach_output).unwrap();
        assert_eq!(attach["status"], "creative_artifact_attached");
        assert_eq!(attach["artifact"]["title"], title);
    }

    // verify all 5 are listed
    let list_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "list-creative",
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
    let list: Value = serde_json::from_slice(&list_output).unwrap();
    assert_eq!(list["artifacts"].as_array().unwrap().len(), 5);
}

#[test]
fn creative_artifact_unknown_kind_is_rejected() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let plan_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Test rejection",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let plan: Value = serde_json::from_slice(&plan_output).unwrap();
    let workflow_id = plan["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "attach-creative",
            "--workflow",
            workflow_id,
            "--title",
            "Bad",
            "--kind",
            "hologram",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .failure();
}

#[test]
fn creative_artifact_inspect_missing_artifact_returns_error() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let plan_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Test missing artifact",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let plan: Value = serde_json::from_slice(&plan_output).unwrap();
    let workflow_id = plan["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "inspect-creative",
            "--workflow",
            workflow_id,
            "--artifact",
            "ca_nonexistent",
            "--output",
            "json",
        ])
        .assert()
        .failure();
}

#[test]
fn token_collection_can_be_set_and_retrieved() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let plan_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Design system setup",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let plan: Value = serde_json::from_slice(&plan_output).unwrap();
    let workflow_id = plan["workflow_id"].as_str().unwrap();

    // set token collection
    let set_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "set-tokens",
            "--workflow",
            workflow_id,
            "--name",
            "Brand Tokens",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let set: Value = serde_json::from_slice(&set_output).unwrap();
    assert_eq!(set["status"], "token_collection_set");

    // get token collection
    let get_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "get-tokens",
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
    let get: Value = serde_json::from_slice(&get_output).unwrap();
    assert_eq!(get["status"], "token_collection_loaded");
    assert_eq!(get["token_collection"]["name"], "Brand Tokens");
    assert!(get["token_collection"]["tokens"].as_array().unwrap().len() >= 2);
}

#[test]
fn status_surfaces_creative_artifacts_and_token_presence() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let plan_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Status creative check",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let plan: Value = serde_json::from_slice(&plan_output).unwrap();
    let workflow_id = plan["workflow_id"].as_str().unwrap();

    // no creative artifacts yet
    let status_output = forge()
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
    let status: Value = serde_json::from_slice(&status_output).unwrap();
    assert_eq!(status["creative_artifacts"].as_array().unwrap().len(), 0);
    assert!(!status["has_token_collection"].as_bool().unwrap());

    // add a creative artifact
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "attach-creative",
            "--workflow",
            workflow_id,
            "--title",
            "Hero Screen",
            "--kind",
            "screen",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success();

    // status now shows creative artifact
    let status_output2 = forge()
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
    let status2: Value = serde_json::from_slice(&status_output2).unwrap();
    assert_eq!(status2["creative_artifacts"].as_array().unwrap().len(), 1);
    assert_eq!(status2["creative_artifacts"][0]["title"], "Hero Screen");

    // set tokens
    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "set-tokens",
            "--workflow",
            workflow_id,
            "--name",
            "Theme",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success();

    // status now shows tokens
    let status_output3 = forge()
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
    let status3: Value = serde_json::from_slice(&status_output3).unwrap();
    assert!(status3["has_token_collection"].as_bool().unwrap());
    assert_eq!(
        status3["token_summary"]["schema_version"],
        "forge.tokens.workflow_summary.v1"
    );
    assert_eq!(status3["token_summary"]["collection_name"], "Theme");
    assert_eq!(status3["token_summary"]["token_count"], 2);
    assert_eq!(status3["token_summary"]["semantic_alias_count"], 1);
    assert_eq!(
        status3["token_summary"]["resolution_schema_version"],
        "forge.tokens.resolution.v1"
    );
}

#[test]
fn creative_artifact_round_trip_serialization_preserves_full_screen_content() {
    use forge_core::ir::{
        Breakpoint, CreativeArtifact, InteractionFlow, ScreenElement, ScreenSpec,
    };
    use std::collections::BTreeMap;

    let spec = ScreenSpec {
        schema_version: forge_core::ir::ir_schema_version(),
        width_px: 1440,
        height_px: 900,
        background: "#1a1a2e".to_string(),
        breakpoints: vec![
            Breakpoint {
                name: "tablet".to_string(),
                max_width_px: 768,
            },
            Breakpoint {
                name: "mobile".to_string(),
                max_width_px: 375,
            },
        ],
        elements: vec![
            ScreenElement {
                id: "el_1".to_string(),
                component_ref: "header".to_string(),
                x: 0.0,
                y: 0.0,
                width: 1440.0,
                height: 80.0,
                props: BTreeMap::new(),
                visible: true,
                locked: false,
                layer: 0,
            },
            ScreenElement {
                id: "el_2".to_string(),
                component_ref: "hero".to_string(),
                x: 0.0,
                y: 80.0,
                width: 1440.0,
                height: 600.0,
                props: BTreeMap::from([("heading".to_string(), "Welcome".to_string())]),
                visible: true,
                locked: false,
                layer: 1,
            },
        ],
        interactions: vec![InteractionFlow {
            trigger: "click".to_string(),
            action: "navigate".to_string(),
            target_id: "el_2".to_string(),
        }],
    };

    let artifact = CreativeArtifact::new_screen("Full Screen", spec.clone());

    // round-trip through JSON
    let json = serde_json::to_value(&artifact).unwrap();
    let restored: CreativeArtifact = serde_json::from_value(json.clone()).unwrap();

    assert_eq!(restored.title, "Full Screen");
    assert_eq!(restored.tags.len(), 0);
    assert_eq!(restored.patches.len(), 0);

    match &restored.content {
        forge_core::ir::CreativeContent::Screen(s) => {
            assert_eq!(s.width_px, 1440);
            assert_eq!(s.height_px, 900);
            assert_eq!(s.background, "#1a1a2e");
            assert_eq!(s.breakpoints.len(), 2);
            assert_eq!(s.elements.len(), 2);
            assert_eq!(s.interactions.len(), 1);
            assert_eq!(s.elements[1].props.get("heading").unwrap(), "Welcome");
        }
        _ => panic!("expected Screen content"),
    }
}

#[test]
fn creative_artifact_document_with_sections_round_trips() {
    use forge_core::ir::{CreativeArtifact, DocumentContent, DocumentSection, DocumentSpec};

    let spec = DocumentSpec {
        schema_version: forge_core::ir::ir_schema_version(),
        title: "API Spec".to_string(),
        author: "forge".to_string(),
        front_matter: std::collections::BTreeMap::from([
            ("version".to_string(), "1.0".to_string()),
            ("status".to_string(), "draft".to_string()),
        ]),
        sections: vec![
            DocumentSection {
                id: "sec_1".to_string(),
                heading: "Introduction".to_string(),
                level: 1,
                content: vec![DocumentContent::Text {
                    value: "This API provides...".to_string(),
                }],
                children: vec![],
            },
            DocumentSection {
                id: "sec_2".to_string(),
                heading: "Endpoints".to_string(),
                level: 1,
                content: vec![
                    DocumentContent::Code {
                        language: "http".to_string(),
                        code: "GET /api/v1/users".to_string(),
                    },
                    DocumentContent::Table {
                        headers: vec!["Method".to_string(), "Path".to_string()],
                        rows: vec![
                            vec!["GET".to_string(), "/users".to_string()],
                            vec!["POST".to_string(), "/users".to_string()],
                        ],
                    },
                ],
                children: vec![],
            },
        ],
    };

    let artifact = CreativeArtifact::new_document("API Spec v1", spec);
    let json = serde_json::to_value(&artifact).unwrap();
    let restored: CreativeArtifact = serde_json::from_value(json).unwrap();

    match &restored.content {
        forge_core::ir::CreativeContent::Document(d) => {
            assert_eq!(d.title, "API Spec");
            assert_eq!(d.author, "forge");
            assert_eq!(d.front_matter.get("version").unwrap(), "1.0");
            assert_eq!(d.sections.len(), 2);
            assert_eq!(d.sections[1].content.len(), 2);
            match &d.sections[1].content[1] {
                DocumentContent::Table { headers, .. } => {
                    assert_eq!(headers.len(), 2);
                }
                _ => panic!("expected Table content"),
            }
        }
        _ => panic!("expected Document content"),
    }
}

#[test]
fn creative_artifact_component_with_variants_states_tokens_round_trips() {
    use forge_core::ir::{
        ComponentProp, ComponentSlot, ComponentSpec, ComponentState, ComponentVariant,
        CreativeArtifact,
    };

    let spec = ComponentSpec {
        schema_version: forge_core::ir::ir_schema_version(),
        name: "Button".to_string(),
        description: "Primary action button".to_string(),
        props: vec![
            ComponentProp {
                name: "label".to_string(),
                prop_type: "string".to_string(),
                required: true,
                default_value: None,
                description: "Button label text".to_string(),
            },
            ComponentProp {
                name: "variant".to_string(),
                prop_type: "string".to_string(),
                required: false,
                default_value: Some("primary".to_string()),
                description: "Visual variant".to_string(),
            },
        ],
        variants: vec![
            ComponentVariant {
                name: "primary".to_string(),
                props_override: std::collections::BTreeMap::new(),
            },
            ComponentVariant {
                name: "secondary".to_string(),
                props_override: std::collections::BTreeMap::new(),
            },
        ],
        states: vec![
            ComponentState {
                name: "hover".to_string(),
                styling: std::collections::BTreeMap::from([(
                    "background".to_string(),
                    "blue-600".to_string(),
                )]),
            },
            ComponentState {
                name: "disabled".to_string(),
                styling: std::collections::BTreeMap::from([(
                    "opacity".to_string(),
                    "0.5".to_string(),
                )]),
            },
        ],
        slots: vec![ComponentSlot {
            name: "icon".to_string(),
            description: "Optional icon slot".to_string(),
            required: false,
        }],
        token_dependencies: vec!["color.primary".to_string(), "spacing.md".to_string()],
        code_template: Some("<button class=\"{variant}\">{label}</button>".to_string()),
    };

    let artifact = CreativeArtifact::new_component("Button", spec);
    let json = serde_json::to_value(&artifact).unwrap();
    let restored: CreativeArtifact = serde_json::from_value(json).unwrap();

    match &restored.content {
        forge_core::ir::CreativeContent::Component(c) => {
            assert_eq!(c.name, "Button");
            assert_eq!(c.props.len(), 2);
            assert_eq!(c.variants.len(), 2);
            assert_eq!(c.states.len(), 2);
            assert_eq!(c.slots.len(), 1);
            assert_eq!(c.token_dependencies.len(), 2);
            assert!(c.code_template.is_some());
            assert_eq!(c.states[0].styling.get("background").unwrap(), "blue-600");
        }
        _ => panic!("expected Component content"),
    }
}

#[test]
fn design_token_serialization_round_trips_all_types() {
    use forge_core::ir::{DesignToken, SemanticAlias, TokenCollection, TokenType};
    use std::collections::BTreeMap;

    let collection = TokenCollection {
        schema_version: forge_core::ir::ir_schema_version(),
        name: "Brand".to_string(),
        description: "Brand design tokens".to_string(),
        tokens: vec![
            DesignToken {
                name: "color.primary".to_string(),
                value: "#3B82F6".to_string(),
                token_type: TokenType::Color,
                description: "Primary brand color".to_string(),
                group: "color".to_string(),
                extensions: BTreeMap::new(),
            },
            DesignToken {
                name: "spacing.md".to_string(),
                value: "16px".to_string(),
                token_type: TokenType::Spacing,
                description: "Medium spacing".to_string(),
                group: "spacing".to_string(),
                extensions: BTreeMap::from([("scaling".to_string(), "1.5".to_string())]),
            },
            DesignToken {
                name: "font.family.body".to_string(),
                value: "Inter".to_string(),
                token_type: TokenType::FontFamily,
                description: "Body font".to_string(),
                group: "typography".to_string(),
                extensions: BTreeMap::new(),
            },
        ],
        semantic_aliases: vec![SemanticAlias {
            name: "semantic.brand".to_string(),
            resolves_to: "color.primary".to_string(),
            description: "Semantic brand color".to_string(),
        }],
        modes: Vec::new(),
    };

    let json = serde_json::to_value(&collection).unwrap();
    let restored: TokenCollection = serde_json::from_value(json).unwrap();

    assert_eq!(restored.tokens.len(), 3);
    assert_eq!(restored.semantic_aliases.len(), 1);
    assert_eq!(restored.tokens[0].token_type, TokenType::Color);
    assert_eq!(restored.tokens[1].token_type, TokenType::Spacing);
    assert_eq!(restored.tokens[2].token_type, TokenType::FontFamily);
    assert_eq!(restored.tokens[1].extensions.get("scaling").unwrap(), "1.5");
    assert_eq!(restored.semantic_aliases[0].resolves_to, "color.primary");
}

#[test]
fn design_token_resolution_handles_aliases_modes_and_artifact_impact() {
    use forge_core::ir::{
        resolve_token_collection, ComponentSpec, CreativeArtifact, CreativeContent, DesignToken,
        ScreenElement, ScreenSpec, SemanticAlias, TokenCollection, TokenMode, TokenOverride,
        TokenType,
    };
    use std::collections::BTreeMap;

    let collection = TokenCollection {
        schema_version: forge_core::ir::ir_schema_version(),
        name: "Product Tokens".to_string(),
        description: "Mode-aware token test".to_string(),
        tokens: vec![
            DesignToken {
                name: "color.brand.primary".to_string(),
                value: "#2563EB".to_string(),
                token_type: TokenType::Color,
                description: String::new(),
                group: "color".to_string(),
                extensions: BTreeMap::new(),
            },
            DesignToken {
                name: "spacing.md".to_string(),
                value: "16px".to_string(),
                token_type: TokenType::Spacing,
                description: String::new(),
                group: "spacing".to_string(),
                extensions: BTreeMap::new(),
            },
        ],
        semantic_aliases: vec![SemanticAlias {
            name: "button.background.default".to_string(),
            resolves_to: "color.brand.primary".to_string(),
            description: String::new(),
        }],
        modes: vec![TokenMode {
            name: "dark".to_string(),
            overrides: vec![TokenOverride {
                token_name: "color.brand.primary".to_string(),
                value: "#60A5FA".to_string(),
                reason: "dark mode contrast".to_string(),
            }],
        }],
    };
    let component = CreativeArtifact::new_component(
        "Action Button",
        ComponentSpec {
            schema_version: forge_core::ir::ir_schema_version(),
            name: "Button".to_string(),
            description: String::new(),
            props: Vec::new(),
            variants: Vec::new(),
            states: Vec::new(),
            slots: Vec::new(),
            token_dependencies: vec![
                "button.background.default".to_string(),
                "spacing.md".to_string(),
            ],
            code_template: None,
        },
    );
    let screen = CreativeArtifact::new_screen(
        "Home",
        ScreenSpec {
            schema_version: forge_core::ir::ir_schema_version(),
            width_px: 1440,
            height_px: 900,
            background: "{token:color.brand.primary}".to_string(),
            breakpoints: Vec::new(),
            elements: vec![ScreenElement {
                id: "cta".to_string(),
                component_ref: "Button".to_string(),
                x: 100.0,
                y: 100.0,
                width: 160.0,
                height: 48.0,
                props: BTreeMap::from([
                    (
                        "background".to_string(),
                        "{token:button.background.default}".to_string(),
                    ),
                    ("label".to_string(), "Apply now".to_string()),
                ]),
                visible: true,
                locked: false,
                layer: 1,
            }],
            interactions: Vec::new(),
        },
    );
    assert!(matches!(screen.content, CreativeContent::Screen(_)));

    let report = resolve_token_collection(&collection, Some("dark"), &[component, screen]);

    assert_eq!(report.schema_version, "forge.tokens.resolution.v1");
    assert_eq!(report.mode.as_deref(), Some("dark"));
    assert_eq!(report.unresolved_aliases.len(), 0);
    let alias = report
        .resolved_tokens
        .iter()
        .find(|token| token.name == "button.background.default")
        .unwrap();
    assert_eq!(alias.value, "#60A5FA");
    assert_eq!(alias.resolves_to.as_deref(), Some("color.brand.primary"));
    assert_eq!(alias.applied_overrides, vec!["mode:dark"]);
    assert_eq!(report.impact_preview.affected_artifact_count, 2);
    assert!(report
        .impact_preview
        .references
        .iter()
        .any(
            |reference| reference.reference_kind == "component_dependency"
                && reference.token_name == "button.background.default"
        ));
    assert!(report
        .impact_preview
        .references
        .iter()
        .any(|reference| reference.path == "content.elements[0].props.background"));
}

#[test]
fn workflow_patch_token_by_intent_updates_only_tokens_and_preserves_creative_artifacts() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let plan_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Token patch demo",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let plan: Value = serde_json::from_slice(&plan_output).unwrap();
    let workflow_id = plan["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "set-tokens",
            "--workflow",
            workflow_id,
            "--name",
            "Brand",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success();
    let attached = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "attach-creative",
            "--workflow",
            workflow_id,
            "--title",
            "Hero Screen",
            "--kind",
            "screen",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let attach_json: Value = serde_json::from_slice(&attached).unwrap();
    let artifact_id = attach_json["artifact"]["id"].as_str().unwrap();
    let before = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "inspect-creative",
            "--workflow",
            workflow_id,
            "--artifact",
            artifact_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let before_json: Value = serde_json::from_slice(&before).unwrap();

    let patched = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "patch-token",
            "--workflow",
            workflow_id,
            "--token",
            "color.primary",
            "--value",
            "#111827",
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
    let patch_json: Value = serde_json::from_slice(&patched).unwrap();
    assert_eq!(patch_json["status"], "token_patched");
    assert_eq!(patch_json["token_name"], "color.primary");
    assert_eq!(patch_json["old_value"], "#3B82F6");
    assert_eq!(patch_json["new_value"], "#111827");
    assert_eq!(patch_json["creative_artifacts_rewritten"], false);
    assert_eq!(
        patch_json["patch"]["changes"][0]["path"],
        "token_collection.tokens[color.primary].value"
    );
    assert_eq!(
        patch_json["impact_preview"]["schema_version"],
        "forge.tokens.impact_preview.v1"
    );

    let resolved = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "resolve-tokens",
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
    let resolved_json: Value = serde_json::from_slice(&resolved).unwrap();
    assert_eq!(resolved_json["status"], "token_resolution_ready");
    assert!(resolved_json["resolution"]["resolved_tokens"]
        .as_array()
        .unwrap()
        .iter()
        .any(|token| token["name"] == "color.primary" && token["value"] == "#111827"));
    assert!(resolved_json["resolution"]["resolved_tokens"]
        .as_array()
        .unwrap()
        .iter()
        .any(|token| token["name"] == "semantic.Brand" && token["value"] == "#111827"));

    let after = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "inspect-creative",
            "--workflow",
            workflow_id,
            "--artifact",
            artifact_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let after_json: Value = serde_json::from_slice(&after).unwrap();
    assert_eq!(after_json["artifact"], before_json["artifact"]);
}

#[test]
fn creative_collaboration_events_are_durable_and_status_visible() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let plan_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Live collaboration screen demo",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let plan: Value = serde_json::from_slice(&plan_output).unwrap();
    let workflow_id = plan["workflow_id"].as_str().unwrap();

    let attach_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "attach-creative",
            "--workflow",
            workflow_id,
            "--title",
            "Collaborative Screen",
            "--kind",
            "screen",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let attach: Value = serde_json::from_slice(&attach_output).unwrap();
    let artifact_id = attach["artifact"]["id"].as_str().unwrap();

    let presence = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "collaboration-event",
            "--workflow",
            workflow_id,
            "--artifact",
            artifact_id,
            "--kind",
            "presence",
            "--actor",
            "human:arthur",
            "--summary",
            "editing hero headline",
            "--target",
            "cursor:120,240",
            "--selection",
            "hero.headline",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let presence_json: Value = serde_json::from_slice(&presence).unwrap();
    assert_eq!(
        presence_json["status"],
        "creative_collaboration_event_recorded"
    );
    assert_eq!(presence_json["summary"]["active_presence_count"], 1);

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "collaboration-event",
            "--workflow",
            workflow_id,
            "--artifact",
            artifact_id,
            "--kind",
            "comment",
            "--actor",
            "codex",
            "--summary",
            "Hero copy needs stronger user decision framing.",
            "--target",
            "hero.headline",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success();
    let patch = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "collaboration-event",
            "--workflow",
            workflow_id,
            "--artifact",
            artifact_id,
            "--kind",
            "patch",
            "--actor",
            "codex",
            "--summary",
            "Patch hero headline by intent without rewriting the full screen.",
            "--target",
            "content.elements[hero].props.heading",
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
    let patch_json: Value = serde_json::from_slice(&patch).unwrap();
    let patch_id = patch_json["event_id"].as_str().unwrap();
    assert_eq!(patch_json["summary"]["patch_event_count"], 1);

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "collaboration-event",
            "--workflow",
            workflow_id,
            "--artifact",
            artifact_id,
            "--kind",
            "rollback",
            "--actor",
            "human:arthur",
            "--summary",
            "Rollback weak hero headline patch after review.",
            "--target",
            patch_id,
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let inspect_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "inspect-creative",
            "--workflow",
            workflow_id,
            "--artifact",
            artifact_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let inspect: Value = serde_json::from_slice(&inspect_output).unwrap();
    let collaboration = &inspect["artifact"]["collaboration"];
    assert_eq!(
        collaboration["schema_version"],
        "forge.creative_collaboration.v1"
    );
    assert_eq!(collaboration["presences"][0]["actor"], "human:arthur");
    assert_eq!(
        collaboration["presences"][0]["selections"][0],
        "hero.headline"
    );
    assert_eq!(
        collaboration["comments"][0]["body"],
        "Hero copy needs stronger user decision framing."
    );
    assert_eq!(
        collaboration["patch_stream"][0]["instruction"],
        "Patch hero headline by intent without rewriting the full screen."
    );
    assert_eq!(collaboration["rollbacks"][0]["target_event_id"], patch_id);
    assert_eq!(collaboration["audit_history"].as_array().unwrap().len(), 4);
    assert_eq!(inspect["artifact"]["patches"].as_array().unwrap().len(), 1);

    let status_output = forge()
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
    let status: Value = serde_json::from_slice(&status_output).unwrap();
    let summary = &status["creative_artifacts"][0]["collaboration_summary"];
    assert_eq!(summary["active_presence_count"], 1);
    assert_eq!(summary["comment_count"], 1);
    assert_eq!(summary["patch_event_count"], 1);
    assert_eq!(summary["rollback_count"], 1);
    assert_eq!(summary["audit_event_count"], 4);
}

#[test]
fn mcp_exposes_creative_collaboration_status_and_event_recording() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let plan_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Live collaboration document demo",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let plan: Value = serde_json::from_slice(&plan_output).unwrap();
    let workflow_id = plan["workflow_id"].as_str().unwrap();

    let attach_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "attach-creative",
            "--workflow",
            workflow_id,
            "--title",
            "Collaborative Brief",
            "--kind",
            "document",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let attach: Value = serde_json::from_slice(&attach_output).unwrap();
    let artifact_id = attach["artifact"]["id"].as_str().unwrap();

    let manifest = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "tools",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let manifest_json: Value = serde_json::from_slice(&manifest).unwrap();
    find_mcp_tool(&manifest_json, "forge.creative.collaboration_event");
    find_mcp_tool(&manifest_json, "forge.creative.collaboration_status");

    let event_input = format!(
        r#"{{"workflow_id":"{workflow_id}","artifact_id":"{artifact_id}","kind":"comment","actor":"opencode","summary":"Document needs a clearer approval decision.","target":"section:intro","origin":"mcp_test","selections":["section:intro"]}}"#
    );
    let event_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.creative.collaboration_event",
            "--input",
            &event_input,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let event: Value = serde_json::from_slice(&event_output).unwrap();
    assert_eq!(
        event["result"]["status"],
        "creative_collaboration_event_recorded"
    );
    assert_eq!(event["result"]["summary"]["comment_count"], 1);

    let status_input =
        format!(r#"{{"workflow_id":"{workflow_id}","artifact_id":"{artifact_id}"}}"#);
    let status_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.creative.collaboration_status",
            "--input",
            &status_input,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let status: Value = serde_json::from_slice(&status_output).unwrap();
    assert_eq!(
        status["result"]["status"],
        "creative_collaboration_status_loaded"
    );
    assert_eq!(status["result"]["summary"]["comment_count"], 1);
    assert_eq!(
        status["result"]["collaboration"]["comments"][0]["body"],
        "Document needs a clearer approval decision."
    );
}

#[test]
fn mcp_tools_manifest_and_calls_expose_token_resolution_and_patch() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let plan_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "MCP token patch demo",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let plan: Value = serde_json::from_slice(&plan_output).unwrap();
    let workflow_id = plan["workflow_id"].as_str().unwrap();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "workflow",
            "set-tokens",
            "--workflow",
            workflow_id,
            "--name",
            "AgentTokens",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let manifest = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "tools",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let manifest_json: Value = serde_json::from_slice(&manifest).unwrap();
    for name in ["forge.tokens.resolve", "forge.tokens.patch"] {
        find_mcp_tool(&manifest_json, name);
    }

    let patch_input = format!(
        r##"{{"workflow_id":"{workflow_id}","token_name":"color.primary","value":"#0F172A","origin":"mcp_test"}}"##
    );
    let patched = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.tokens.patch",
            "--input",
            &patch_input,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let patched_json: Value = serde_json::from_slice(&patched).unwrap();
    assert_eq!(patched_json["result"]["status"], "token_patched");
    assert_eq!(patched_json["result"]["new_value"], "#0F172A");

    let resolve_input = format!(r#"{{"workflow_id":"{workflow_id}"}}"#);
    let resolved = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.tokens.resolve",
            "--input",
            &resolve_input,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let resolved_json: Value = serde_json::from_slice(&resolved).unwrap();
    assert_eq!(resolved_json["result"]["status"], "token_resolution_ready");
    assert!(resolved_json["result"]["resolution"]["resolved_tokens"]
        .as_array()
        .unwrap()
        .iter()
        .any(|token| token["name"] == "color.primary" && token["value"] == "#0F172A"));
}

#[test]
fn milestone_status_reports_creative_artifact_ir_capability_as_validated() {
    use forge_core::milestone::build_milestone_status;

    let report = build_milestone_status("0.5").unwrap();
    let creative_ir = report
        .capabilities
        .iter()
        .find(|c| c.id == "creative_artifact_ir")
        .expect("creative_artifact_ir capability should exist");
    let design_tokens = report
        .capabilities
        .iter()
        .find(|c| c.id == "design_tokens")
        .expect("design_tokens capability should exist");
    let componentization = report
        .capabilities
        .iter()
        .find(|c| c.id == "componentization_ai_surfaces")
        .expect("componentization_ai_surfaces capability should exist");
    let export_demo = report
        .capabilities
        .iter()
        .find(|c| c.id == "export_demo_baseline")
        .expect("export_demo_baseline capability should exist");

    assert_eq!(
        creative_ir.status, "validated",
        "creative_artifact_ir should be validated"
    );
    assert_eq!(
        design_tokens.status, "validated",
        "design_tokens should be validated"
    );
    assert_eq!(
        componentization.status, "validated",
        "componentization_ai_surfaces should be validated"
    );
    assert_eq!(
        export_demo.status, "validated",
        "export_demo_baseline is now validated after forge milestone export-demo"
    );
}

#[test]
fn mcp_tools_manifest_includes_creative_artifact_and_token_tools() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "tools",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let manifest: Value = serde_json::from_slice(&output).unwrap();
    let tool_names: Vec<&str> = manifest["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();

    assert!(tool_names.contains(&"forge.creative.list"));
    assert!(tool_names.contains(&"forge.creative.inspect"));
    assert!(tool_names.contains(&"forge.creative.attach"));
    assert!(tool_names.contains(&"forge.tokens.get"));
    assert!(tool_names.contains(&"forge.tokens.set"));
}

#[test]
fn mcp_creative_tools_list_inspect_and_attach_workflow_creative_artifacts() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let plan_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Test creative MCP tools",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let plan: Value = serde_json::from_slice(&plan_output).unwrap();
    let workflow_id = plan["workflow_id"].as_str().unwrap();

    let list_empty = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.creative.list",
            "--input",
            &format!(r#"{{"workflow_id":"{workflow_id}"}}"#),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let list_report: Value = serde_json::from_slice(&list_empty).unwrap();
    assert_eq!(list_report["result"]["status"], "creative_artifacts_listed");
    assert_eq!(
        list_report["result"]["artifacts"].as_array().unwrap().len(),
        0
    );

    let attach_output = forge()
        .args(["--store", store.to_str().unwrap(), "mcp", "call", "forge.creative.attach", "--input", &format!(r#"{{"workflow_id":"{workflow_id}","title":"Test Screen","kind":"screen","origin":"mcp_test"}}"#), "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let attach_report: Value = serde_json::from_slice(&attach_output).unwrap();
    assert_eq!(
        attach_report["result"]["status"],
        "creative_artifact_attached"
    );
    let artifact_id = attach_report["result"]["artifact"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(attach_report["result"]["artifact"]["kind"], "Screen");

    let list_after = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.creative.list",
            "--input",
            &format!(r#"{{"workflow_id":"{workflow_id}"}}"#),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let list_report: Value = serde_json::from_slice(&list_after).unwrap();
    assert_eq!(
        list_report["result"]["artifacts"].as_array().unwrap().len(),
        1
    );

    let inspect_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.creative.inspect",
            "--input",
            &format!(r#"{{"workflow_id":"{workflow_id}","artifact_id":"{artifact_id}"}}"#),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let inspect_report: Value = serde_json::from_slice(&inspect_output).unwrap();
    assert_eq!(
        inspect_report["result"]["status"],
        "creative_artifact_inspected"
    );
    assert_eq!(inspect_report["result"]["artifact"]["id"], artifact_id);
    assert_eq!(
        inspect_report["result"]["artifact"]["content"]["width_px"],
        1440
    );
}

#[test]
fn mcp_token_tools_get_set_design_tokens_on_workflows() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let plan_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Test token MCP tools",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let plan: Value = serde_json::from_slice(&plan_output).unwrap();
    let workflow_id = plan["workflow_id"].as_str().unwrap();

    let get_empty = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.tokens.get",
            "--input",
            &format!(r#"{{"workflow_id":"{workflow_id}"}}"#),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let get_report: Value = serde_json::from_slice(&get_empty).unwrap();
    assert_eq!(get_report["result"]["status"], "token_collection_loaded");
    assert!(get_report["result"]["token_collection"].is_null());

    let set_output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.tokens.set",
            "--input",
            &format!(
                r#"{{"workflow_id":"{workflow_id}","name":"MyTestTokens","origin":"mcp_test"}}"#
            ),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let set_report: Value = serde_json::from_slice(&set_output).unwrap();
    assert_eq!(set_report["result"]["status"], "token_collection_set");
    let tokens = set_report["result"]["token_collection"]
        .as_object()
        .unwrap();
    assert_eq!(tokens["name"], "MyTestTokens");

    let get_after = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "mcp",
            "call",
            "forge.tokens.get",
            "--input",
            &format!(r#"{{"workflow_id":"{workflow_id}"}}"#),
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let get_report: Value = serde_json::from_slice(&get_after).unwrap();
    assert_eq!(get_report["result"]["status"], "token_collection_loaded");
    assert_eq!(
        get_report["result"]["token_collection"]["name"],
        "MyTestTokens"
    );
    assert!(
        get_report["result"]["token_collection"]["tokens"]
            .as_array()
            .unwrap()
            .len()
            >= 2
    );
}

#[test]
fn parallel_scan_due_dispatches_multiple_due_workflows_with_max_workers() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let goal_names = ["hackathon", "grant", "competition"];
    let mut workflow_ids = Vec::new();
    let mut schedule_task_ids = Vec::new();

    for goal in &goal_names {
        let created = forge()
            .args([
                "--store",
                store.to_str().unwrap(),
                "schedule",
                "create-daily-goal-research",
                "--goal",
                goal,
                "--cron",
                "0 8 * * *",
                "--timezone",
                "America/Sao_Paulo",
                "--origin",
                "forge_cli",
                "--output",
                "json",
            ])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let created_json: Value = serde_json::from_slice(&created).unwrap();
        workflow_ids.push(created_json["workflow_id"].as_str().unwrap().to_string());
        let task_id = created_json["workflow"]["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|task| task["schedule"].is_object())
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        schedule_task_ids.push(task_id);
    }

    for (i, workflow_id) in workflow_ids.iter().enumerate() {
        forge()
            .args([
                "--store",
                store.to_str().unwrap(),
                "schedule",
                "update",
                "--workflow",
                workflow_id,
                "--task",
                &schedule_task_ids[i],
                "--next-run-at",
                "2000-01-01T00:00:00Z",
                "--origin",
                "codex",
                "--output",
                "json",
            ])
            .assert()
            .success();
    }

    let scanned = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "scan-due",
            "--executor",
            "forge-scheduler-parallel",
            "--max-workers",
            "3",
            "--ttl-seconds",
            "90",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let scanned_json: Value = serde_json::from_slice(&scanned).unwrap();
    assert_eq!(scanned_json["schema_version"], "forge.schedule.scan_due.v1");
    assert!(scanned_json["status"]
        .as_str()
        .unwrap()
        .contains("scan_completed"));
    assert_eq!(scanned_json["summary"]["scanned_workflows"], 3);
    assert_eq!(scanned_json["summary"]["due_workflows"], 3);
    assert_eq!(scanned_json["summary"]["executed_workflows"], 3);
    assert_eq!(scanned_json["summary"]["parallel"], true);
    assert_eq!(scanned_json["summary"]["max_workers"], 3);
    assert!(scanned_json["summary"]["wave_count"].as_u64().unwrap() >= 1);
    assert!(scanned_json["summary"]["duration_ms"].as_i64().unwrap_or(0) >= 0);
    assert_eq!(
        scanned_json["summary"]["executed_workflows"],
        scanned_json["results"].as_array().unwrap().len()
    );
    for result in scanned_json["results"].as_array().unwrap() {
        let status = result["status"].as_str().unwrap();
        assert!(status == "executed", "unexpected status: {status}");
        assert!(!result["lease_id"].is_null());
        if let Some(lease_id) = result["lease_id"].as_str() {
            assert!(lease_id.starts_with("lease_"), "lease_id: {lease_id}");
        }
    }
}

#[test]
fn parallel_scan_due_with_max_workers_one_is_sequential() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let created = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "create-daily-goal-research",
            "--goal",
            "hackathon",
            "--cron",
            "0 8 * * *",
            "--timezone",
            "America/Sao_Paulo",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let created_json: Value = serde_json::from_slice(&created).unwrap();
    let workflow_id = created_json["workflow_id"].as_str().unwrap().to_string();
    let task_id = created_json["workflow"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["schedule"].is_object())
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "update",
            "--workflow",
            &workflow_id,
            "--task",
            &task_id,
            "--next-run-at",
            "2000-01-01T00:00:00Z",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let scanned = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "scan-due",
            "--executor",
            "forge-scheduler-single",
            "--max-workers",
            "1",
            "--ttl-seconds",
            "60",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let scanned_json: Value = serde_json::from_slice(&scanned).unwrap();
    assert_eq!(scanned_json["schema_version"], "forge.schedule.scan_due.v1");
    assert_eq!(scanned_json["summary"]["parallel"], false);
    assert_eq!(scanned_json["summary"]["scanned_workflows"], 1);
    assert_eq!(scanned_json["summary"]["due_workflows"], 1);
    assert_eq!(scanned_json["summary"]["executed_workflows"], 1);
}

#[test]
fn parallel_scan_due_reports_idle_workflows_without_due_nodes() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let created = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "create-daily-goal-research",
            "--goal",
            "hackathon",
            "--cron",
            "0 8 * * *",
            "--timezone",
            "America/Sao_Paulo",
            "--origin",
            "forge_cli",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let created_json: Value = serde_json::from_slice(&created).unwrap();
    let workflow_id = created_json["workflow_id"].as_str().unwrap().to_string();

    let scanned = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "scan-due",
            "--executor",
            "forge-scheduler-idle",
            "--max-workers",
            "3",
            "--ttl-seconds",
            "60",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let scanned_json: Value = serde_json::from_slice(&scanned).unwrap();
    assert_eq!(scanned_json["schema_version"], "forge.schedule.scan_due.v1");
    assert!(
        scanned_json["summary"]["idle_workflows"]
            .as_u64()
            .unwrap_or(0)
            >= 1
    );
    assert!(
        scanned_json["summary"]["executed_workflows"]
            .as_u64()
            .unwrap_or(0)
            == 0
    );
    assert!(
        scanned_json["summary"]["due_workflows"]
            .as_u64()
            .unwrap_or(0)
            == 0
    );
    assert_eq!(scanned_json["summary"]["scale_to_zero_workflows"], 1);
    assert_eq!(
        scanned_json["worker_pool"]["schema_version"],
        "forge.worker_pool.v1"
    );
    assert_eq!(scanned_json["worker_pool"]["total_jobs"], 0);
    assert_eq!(scanned_json["worker_pool"]["completed_jobs"], 0);

    let results = scanned_json["results"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["workflow_id"], workflow_id);
    assert_eq!(results[0]["status"], "no_due_cron_nodes");
    assert_eq!(results[0]["run_due"]["scale_to_zero"]["applied"], true);

    let status = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "status",
            "--workflow",
            &workflow_id,
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let status_json: Value = serde_json::from_slice(&status).unwrap();
    assert_eq!(status_json["status"], "scaled_to_zero");
}

#[test]
fn worker_pool_parallel_scan_due_preserves_workflow_state_consistency() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let goal_names = ["hackathon", "grant"];
    let mut workflow_ids = Vec::new();
    let mut task_ids = Vec::new();

    for goal in &goal_names {
        let created = forge()
            .args([
                "--store",
                store.to_str().unwrap(),
                "schedule",
                "create-daily-goal-research",
                "--goal",
                goal,
                "--cron",
                "0 8 * * *",
                "--timezone",
                "America/Sao_Paulo",
                "--origin",
                "forge_cli",
                "--output",
                "json",
            ])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let created_json: Value = serde_json::from_slice(&created).unwrap();
        workflow_ids.push(created_json["workflow_id"].as_str().unwrap().to_string());
        task_ids.push(
            created_json["workflow"]["tasks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|task| task["schedule"].is_object())
                .unwrap()["id"]
                .as_str()
                .unwrap()
                .to_string(),
        );
    }

    for (i, wf_id) in workflow_ids.iter().enumerate() {
        forge()
            .args([
                "--store",
                store.to_str().unwrap(),
                "schedule",
                "update",
                "--workflow",
                wf_id,
                "--task",
                &task_ids[i],
                "--next-run-at",
                "2000-01-01T00:00:00Z",
                "--origin",
                "codex",
                "--output",
                "json",
            ])
            .assert()
            .success();
    }

    forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "schedule",
            "scan-due",
            "--executor",
            "forge-scheduler-consistency",
            "--max-workers",
            "4",
            "--ttl-seconds",
            "120",
            "--output",
            "json",
        ])
        .assert()
        .success();

    for wf_id in &workflow_ids {
        let inspect = forge()
            .args([
                "--store",
                store.to_str().unwrap(),
                "status",
                "--workflow",
                wf_id,
                "--output",
                "json",
            ])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let inspect_json: Value = serde_json::from_slice(&inspect).unwrap();
        assert_eq!(
            inspect_json["workflow_id"].as_str().unwrap(),
            wf_id.as_str()
        );
        assert_eq!(inspect_json["status"].as_str().unwrap(), "pending");
        assert!(
            inspect_json["artifacts"]
                .as_array()
                .map(|a| a.len() >= 3)
                .unwrap_or(false),
            "parallel scan-due should preserve at least 3 artifacts per workflow"
        );
    }
}

#[test]
fn list_filters_workflow_registry_by_workflow_level_running_status() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let planned = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "plan",
            "--goal",
            "Workflow-level running lifecycle visibility test",
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

    let all_list = forge()
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
    let all_list_json: Value = serde_json::from_slice(&all_list).unwrap();
    assert_eq!(all_list_json["summary"]["total"], 1);
    assert_eq!(all_list_json["summary"]["running"], 0);

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
    assert_eq!(running_list_json["summary"]["total"], 0);

    set_workflow_status_in_stored_workflow(&store, workflow_id, "running");

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
    assert_eq!(running_list_json["summary"]["total"], 1);
    assert_eq!(running_list_json["summary"]["running"], 1);
    assert_eq!(running_list_json["summary"]["non_running"], 0);
    assert_eq!(
        running_list_json["workflows"][0]["workflow_id"],
        workflow_id
    );
    assert_eq!(
        running_list_json["workflows"][0]["lifecycle_state"],
        "running"
    );
    assert_eq!(
        running_list_json["workflows"][0]["workflow_status"],
        "running"
    );
    assert!(running_list_json["workflows"][0]["running"]
        .as_bool()
        .unwrap());

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
    assert_eq!(non_running_list_json["summary"]["total"], 0);

    set_workflow_status_in_stored_workflow(&store, workflow_id, "completed");

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
    assert_eq!(non_running_list_json["summary"]["total"], 1);
    assert_ne!(
        non_running_list_json["workflows"][0]["lifecycle_state"],
        "running"
    );
    assert!(!non_running_list_json["workflows"][0]["running"]
        .as_bool()
        .unwrap());

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
    assert_eq!(running_list_json["summary"]["total"], 0);
}
