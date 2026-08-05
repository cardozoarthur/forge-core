use assert_cmd::Command;
use foundry_core::mcp::{call_mcp_tool, mcp_tools_manifest};
use foundry_core::storage::FoundryStore;
use serde_json::json;
use tempfile::tempdir;

fn foundry() -> Command {
    Command::cargo_bin("foundry").expect("foundry binary should build")
}

#[test]
fn cli_materializes_three_agy_frontend_and_five_codex_backend_agents() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("foundry.sqlite");
    let output = foundry()
        .arg("--store")
        .arg(&store_path)
        .args([
            "teamwork",
            "--goal",
            "Deliver frontend and backend independently",
            "--lane",
            "frontend=agy:3",
            "--lane",
            "backend=codex:5",
            "--max-parallel-agents",
            "8",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["strategy"]["parallelism"]["max_parallel_agents"], 8);
    assert_eq!(
        report["strategy"]["parallelism"]["lanes"][0]["brain"],
        "agy"
    );
    assert_eq!(
        report["strategy"]["parallelism"]["lanes"][0]["agent_count"],
        3
    );
    assert_eq!(
        report["strategy"]["parallelism"]["lanes"][1]["brain"],
        "codex"
    );
    assert_eq!(
        report["strategy"]["parallelism"]["lanes"][1]["agent_count"],
        5
    );
    assert_eq!(report["roster"]["agent_count"], 12);
    assert_eq!(
        report["workspace_isolation"]["status"],
        "task_worktree_bindings_required"
    );
    assert_eq!(
        report["workspace_isolation"]["task_scoped_worktree_required"],
        true
    );
    assert_eq!(
        report["workspace_isolation"]["shared_checkout_parallel_mutation_allowed"],
        false
    );
    assert_eq!(
        report["workspace_isolation"]["branch_task_ids"]
            .as_array()
            .unwrap()
            .len(),
        8
    );
    let task_ids = report["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|task| task["id"].as_str())
        .collect::<Vec<_>>();
    assert!(task_ids.contains(&"task-005-frontend-003"));
    assert!(task_ids.contains(&"task-005-backend-005"));
    assert!(task_ids.contains(&"task-005-frontend-join"));
    assert!(task_ids.contains(&"task-005-backend-join"));

    let workflow_id = report["workflow_id"].as_str().unwrap();
    let store = FoundryStore::open(&store_path).unwrap();
    let workflow = store.load_workflow(workflow_id).unwrap();
    let auditor = workflow
        .tasks
        .iter()
        .find(|task| task.id == "task-006")
        .unwrap();
    assert_eq!(
        auditor.node_brain_routing.default_brain.as_deref(),
        Some("codex")
    );
    assert_eq!(
        auditor.node_brain_routing.allowed_brains,
        vec!["codex".to_string()]
    );
}

#[test]
fn mcp_exposes_and_persists_structured_elastic_teamwork_lanes() {
    let temp = tempdir().unwrap();
    let store = FoundryStore::open(temp.path().join("foundry.sqlite")).unwrap();
    let tool = mcp_tools_manifest()
        .tools
        .into_iter()
        .find(|tool| tool.name == "foundry.teamwork.plan")
        .expect("elastic teamwork must be exposed over MCP");
    assert!(tool.async_safe);
    assert!(tool.mutates_workflow);
    assert_eq!(tool.output_schema, "foundry.teamwork.plan.v1");

    let call = call_mcp_tool(
        &store,
        "foundry.teamwork.plan",
        json!({
            "goal": "Deliver frontend and backend independently",
            "max_parallel_agents": 8,
            "lanes": [
                {
                    "id": "frontend",
                    "brain": "agy",
                    "agent_count": 3,
                    "responsibility": "Implement isolated frontend slices"
                },
                {
                    "id": "backend",
                    "brain": "codex",
                    "agent_count": 5,
                    "responsibility": "Implement isolated backend slices"
                }
            ]
        }),
    )
    .unwrap();

    assert_eq!(call.result["schema_version"], "foundry.teamwork.plan.v1");
    assert_eq!(call.result["roster"]["agent_count"], 12);
    assert_eq!(
        call.result["workspace_isolation"]["task_scoped_worktree_required"],
        true
    );
    assert_eq!(
        call.result["strategy"]["parallelism"]["lanes"][0]["agent_count"],
        3
    );
    assert_eq!(
        call.result["strategy"]["parallelism"]["lanes"][1]["agent_count"],
        5
    );
    let workflow_id = call.result["workflow_id"].as_str().unwrap();
    let workflow = store.load_workflow(workflow_id).unwrap();
    assert!(workflow
        .tasks
        .iter()
        .any(|task| task.id == "task-005-frontend-003"));
    assert!(workflow
        .tasks
        .iter()
        .any(|task| task.id == "task-005-backend-005"));
    assert!(store
        .load_workflow_events(workflow_id)
        .unwrap()
        .iter()
        .any(|event| event.kind == "teamwork_planned"));
}
