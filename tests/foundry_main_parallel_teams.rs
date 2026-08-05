use assert_cmd::Command;
use foundry_core::graph::{CoreParallelLaneSpec, CoreParallelTeamSpec};
use foundry_core::mcp::{call_mcp_tool, mcp_tools_manifest};
use foundry_core::request::start_async_request_with_project_idempotency_and_parallel_team;
use foundry_core::storage::FoundryStore;
use rusqlite::Connection;
use serde_json::{json, Value};
use tempfile::tempdir;

fn foundry() -> Command {
    Command::cargo_bin("foundry").expect("foundry binary should build")
}

fn assert_three_agy_frontend_five_codex_backend(store: &FoundryStore, workflow_id: &str) {
    let workflow = store.load_workflow(workflow_id).unwrap();
    let parallel_team = workflow
        .core_orchestration
        .parallel_team
        .expect("explicit parallel team should be persisted");
    assert_eq!(
        parallel_team.schema_version,
        "foundry.core.parallel_team.v1"
    );
    assert_eq!(parallel_team.max_parallel_agents, 8);
    assert_eq!(parallel_team.lanes.len(), 2);
    assert_eq!(parallel_team.lanes[0].id, "frontend");
    assert_eq!(parallel_team.lanes[0].executor_id, "agy");
    assert_eq!(parallel_team.lanes[0].agent_count, 3);
    assert_eq!(parallel_team.lanes[1].id, "backend");
    assert_eq!(parallel_team.lanes[1].executor_id, "codex");
    assert_eq!(parallel_team.lanes[1].agent_count, 5);

    let frontend = workflow
        .tasks
        .iter()
        .find(|task| task.id == "task-005-frontend-003")
        .expect("third frontend branch should exist");
    assert_eq!(
        frontend.node_brain_routing.default_brain.as_deref(),
        Some("agy")
    );
    let backend = workflow
        .tasks
        .iter()
        .find(|task| task.id == "task-005-backend-005")
        .expect("fifth backend branch should exist");
    assert_eq!(
        backend.node_brain_routing.default_brain.as_deref(),
        Some("codex")
    );
    assert!(workflow
        .tasks
        .iter()
        .any(|task| task.id == "task-005-frontend-join"));
    assert!(workflow
        .tasks
        .iter()
        .any(|task| task.id == "task-005-backend-join"));
    let final_join = workflow
        .tasks
        .iter()
        .find(|task| task.id == "task-006")
        .expect("final join should exist");
    assert_eq!(
        final_join.dependencies,
        vec![
            "task-005-frontend-join".to_string(),
            "task-005-backend-join".to_string()
        ]
    );
}

#[test]
fn plan_main_flow_materializes_and_persists_explicit_parallel_team() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("plan.sqlite");
    let output = foundry()
        .current_dir(temp.path())
        .arg("--store")
        .arg(&store_path)
        .args([
            "plan",
            "--goal",
            "Deliver frontend and backend independently",
            "--lane",
            "frontend=Antigravity-CLI:3",
            "--lane",
            "backend=CODEX:5",
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
    let report: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        report["core_orchestration"]["parallel_team"]["lanes"][0]["executor_id"],
        "agy"
    );
    let workflow_id = report["workflow_id"].as_str().unwrap();
    let store = FoundryStore::open(&store_path).unwrap();
    assert_three_agy_frontend_five_codex_backend(&store, workflow_id);
}

#[test]
fn plan_without_lanes_preserves_the_serial_default() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("serial.sqlite");
    let output = foundry()
        .current_dir(temp.path())
        .arg("--store")
        .arg(&store_path)
        .args([
            "plan",
            "--goal",
            "Keep the ordinary serial workflow",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&output).unwrap();
    assert!(report["core_orchestration"]["parallel_team"].is_null());
    let task_006 = report["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["id"] == "task-006")
        .unwrap();
    assert_eq!(task_006["dependencies"], json!(["task-005"]));
}

#[test]
fn request_start_main_flow_materializes_team_and_fingerprints_its_composition() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("request.sqlite");
    let base_args = [
        "request",
        "start",
        "--goal",
        "Deliver frontend and backend independently",
        "--origin",
        "codex",
        "--idempotency-key",
        "parallel-team-request",
        "--lane",
        "frontend=agy:3",
        "--lane",
        "backend=codex:5",
        "--max-parallel-agents",
        "8",
        "--output",
        "json",
    ];
    let first = foundry()
        .current_dir(temp.path())
        .arg("--store")
        .arg(&store_path)
        .args(base_args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: Value = serde_json::from_slice(&first).unwrap();
    assert_eq!(report["parallel_team"]["max_parallel_agents"], 8);
    let workflow_id = report["workflow_id"].as_str().unwrap();
    let store = FoundryStore::open(&store_path).unwrap();
    assert_three_agy_frontend_five_codex_backend(&store, workflow_id);

    let replay = foundry()
        .current_dir(temp.path())
        .arg("--store")
        .arg(&store_path)
        .args(base_args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(first, replay);

    foundry()
        .current_dir(temp.path())
        .arg("--store")
        .arg(&store_path)
        .args([
            "request",
            "start",
            "--goal",
            "Deliver frontend and backend independently",
            "--origin",
            "codex",
            "--idempotency-key",
            "parallel-team-request",
            "--lane",
            "frontend=agy:2",
            "--lane",
            "backend=codex:5",
            "--max-parallel-agents",
            "7",
            "--output",
            "json",
        ])
        .assert()
        .failure();
}

#[test]
fn mcp_run_start_uses_the_same_persisted_parallel_team_contract() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("mcp.sqlite");
    let store = FoundryStore::open(&store_path).unwrap();
    let call = call_mcp_tool(
        &store,
        "foundry.run.start",
        json!({
            "goal": "Deliver frontend and backend independently",
            "origin": "mcp",
            "lanes": [
                {"id": "frontend", "brain": "agy", "agent_count": 3},
                {"id": "backend", "brain": "codex", "agent_count": 5}
            ],
            "max_parallel_agents": 8
        }),
    )
    .unwrap();
    assert_eq!(call.result["parallel_team"]["max_parallel_agents"], 8);
    let workflow_id = call.result["workflow_id"].as_str().unwrap();
    assert_three_agy_frontend_five_codex_backend(&store, workflow_id);

    let manifest = serde_json::to_value(mcp_tools_manifest()).unwrap();
    let run_start = manifest["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "foundry.run.start")
        .unwrap();
    assert_eq!(
        run_start["input_schema"]["properties"]["lanes"]["type"],
        "array"
    );
    assert_eq!(
        run_start["input_schema"]["properties"]["max_parallel_agents"]["type"],
        "integer"
    );
    assert_eq!(
        run_start["input_schema"]["properties"]["lanes"]["items"]["required"],
        json!(["id", "brain", "agent_count"])
    );
    assert_eq!(
        run_start["input_schema"]["properties"]["lanes"]["items"]["properties"]["agent_count"]
            ["minimum"],
        1
    );
}

#[test]
fn request_idempotency_is_semantic_across_sources_and_lane_order() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("semantic-idempotency.sqlite");
    let project_root = temp.path().join("project");
    std::fs::create_dir_all(&project_root).unwrap();
    let store = FoundryStore::open(&store_path).unwrap();
    let lane = |id: &str, executor_id: &str, agent_count: usize| CoreParallelLaneSpec {
        id: id.to_string(),
        executor_id: executor_id.to_string(),
        agent_count,
        parallel_group: "implementation-wave-001".to_string(),
        responsibility: format!("Deliver independent bounded work for the {id} lane."),
    };
    let cli_spec = CoreParallelTeamSpec::explicit(
        "foundry.request.start.cli",
        vec![lane("frontend", "agy", 3), lane("backend", "codex", 5)],
        8,
    );
    let mcp_spec = CoreParallelTeamSpec::explicit(
        "foundry.run.start.mcp",
        vec![lane("backend", "codex", 5), lane("frontend", "agy", 3)],
        8,
    );

    let first = start_async_request_with_project_idempotency_and_parallel_team(
        &store,
        "Deliver frontend and backend independently",
        "codex",
        &project_root,
        Some("semantic-parallel-team"),
        Some(cli_spec),
    )
    .unwrap();
    let replay = start_async_request_with_project_idempotency_and_parallel_team(
        &store,
        "Deliver frontend and backend independently",
        "codex",
        &project_root,
        Some("semantic-parallel-team"),
        Some(mcp_spec),
    )
    .unwrap();

    assert_eq!(first.run_id, replay.run_id);
    assert_eq!(first.workflow_id, replay.workflow_id);
    assert!(replay.idempotent_replay);
}

#[test]
fn plan_rolls_back_workflow_event_and_detached_run_together() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("plan-atomicity.sqlite");
    drop(FoundryStore::open(&store_path).unwrap());
    let connection = Connection::open(&store_path).unwrap();
    connection
        .execute_batch(
            r#"
            CREATE TRIGGER reject_parallel_plan_event
            BEFORE INSERT ON events
            WHEN NEW.kind = 'workflow_planned'
            BEGIN
                SELECT RAISE(ABORT, 'injected parallel plan event failure');
            END;
            "#,
        )
        .unwrap();
    drop(connection);

    foundry()
        .current_dir(temp.path())
        .arg("--store")
        .arg(&store_path)
        .args([
            "plan",
            "--goal",
            "Deliver frontend and backend independently",
            "--lane",
            "frontend=agy:3",
            "--lane",
            "backend=codex:5",
            "--max-parallel-agents",
            "8",
            "--detached",
            "--output",
            "json",
        ])
        .assert()
        .failure();

    let connection = Connection::open(&store_path).unwrap();
    let count = |table: &str| {
        connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap()
    };
    assert_eq!(count("workflows"), 0);
    assert_eq!(count("runs"), 0);
    assert_eq!(count("events"), 0);
}
