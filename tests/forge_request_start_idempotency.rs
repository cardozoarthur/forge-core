use forge_core::mcp::{call_mcp_tool, mcp_tools_manifest};
use forge_core::request::start_async_request_with_project_and_idempotency;
use forge_core::storage::ForgeStore;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Barrier};
use std::thread;
use tempfile::tempdir;

fn initialize_store(path: &Path) {
    drop(ForgeStore::open(path).unwrap());
}

fn count_rows(path: &Path, query: &str) -> i64 {
    Connection::open(path)
        .unwrap()
        .query_row(query, [], |row| row.get(0))
        .unwrap()
}

fn forge() -> Command {
    Command::new(env!("CARGO_BIN_EXE_forge"))
}

#[test]
fn request_start_replays_exact_response_and_scopes_key_to_origin() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let project_root = temp.path().join("project");
    std::fs::create_dir_all(&project_root).unwrap();
    let store = ForgeStore::open(&store_path).unwrap();

    let first = start_async_request_with_project_and_idempotency(
        &store,
        "Ship the retry-safe workflow",
        "codex",
        &project_root,
        Some("opaque-retry-key"),
    )
    .unwrap();
    let replay = start_async_request_with_project_and_idempotency(
        &store,
        "Ship the retry-safe workflow",
        "codex",
        &project_root,
        Some("opaque-retry-key"),
    )
    .unwrap();

    assert!(!first.idempotent_replay);
    assert!(replay.idempotent_replay);
    assert_eq!(first.run_id, replay.run_id);
    assert_eq!(first.workflow_id, replay.workflow_id);
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(&replay).unwrap()
    );
    assert_eq!(count_rows(&store_path, "SELECT COUNT(*) FROM runs"), 1);
    assert_eq!(count_rows(&store_path, "SELECT COUNT(*) FROM workflows"), 1);
    assert_eq!(
        count_rows(
            &store_path,
            "SELECT COUNT(*) FROM events WHERE kind = 'async_request_started'"
        ),
        1
    );

    let stored_run = store.load_run(&first.run_id).unwrap();
    let metadata = &stored_run["request_start_idempotency"];
    assert_eq!(
        metadata["schema_version"],
        "forge.request_start_idempotency.v1"
    );
    assert_eq!(metadata["origin"], "codex");
    assert_eq!(metadata["key_sha256"].as_str().unwrap().len(), 64);
    assert!(!stored_run.to_string().contains("opaque-retry-key"));

    let different_origin = start_async_request_with_project_and_idempotency(
        &store,
        "Ship the retry-safe workflow",
        "mcp",
        &project_root,
        Some("opaque-retry-key"),
    )
    .unwrap();
    assert_ne!(different_origin.run_id, first.run_id);
    assert_ne!(different_origin.workflow_id, first.workflow_id);
    assert_eq!(count_rows(&store_path, "SELECT COUNT(*) FROM runs"), 2);
}

#[test]
fn request_start_rejects_goal_or_project_context_conflicts() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let first_project = temp.path().join("project-a");
    let second_project = temp.path().join("project-b");
    std::fs::create_dir_all(&first_project).unwrap();
    std::fs::create_dir_all(&second_project).unwrap();
    let store = ForgeStore::open(&store_path).unwrap();

    let first = start_async_request_with_project_and_idempotency(
        &store,
        "Original goal",
        "codex",
        &first_project,
        Some("conflict-key"),
    )
    .unwrap();

    let goal_error = start_async_request_with_project_and_idempotency(
        &store,
        "Divergent goal",
        "codex",
        &first_project,
        Some("conflict-key"),
    )
    .unwrap_err();
    assert!(
        goal_error
            .to_string()
            .contains("different goal or project/worktree context"),
        "{goal_error:#}"
    );

    let project_error = start_async_request_with_project_and_idempotency(
        &store,
        "Original goal",
        "codex",
        &second_project,
        Some("conflict-key"),
    )
    .unwrap_err();
    assert!(
        project_error
            .to_string()
            .contains("different goal or project/worktree context"),
        "{project_error:#}"
    );

    assert_eq!(count_rows(&store_path, "SELECT COUNT(*) FROM runs"), 1);
    assert_eq!(count_rows(&store_path, "SELECT COUNT(*) FROM workflows"), 1);
    assert_eq!(
        store.load_run(&first.run_id).unwrap()["goal"],
        "Original goal"
    );
}

#[test]
fn concurrent_request_start_retries_create_one_run_and_workflow() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let project_root = temp.path().join("project");
    std::fs::create_dir_all(&project_root).unwrap();
    initialize_store(&store_path);

    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            let store_path = store_path.clone();
            let project_root = project_root.clone();
            thread::spawn(move || {
                let store = ForgeStore::open(store_path).unwrap();
                barrier.wait();
                start_async_request_with_project_and_idempotency(
                    &store,
                    "Converge concurrent retries",
                    "mcp",
                    &project_root,
                    Some("concurrent-key"),
                )
                .unwrap()
            })
        })
        .collect();
    let reports: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();

    assert_eq!(reports[0].run_id, reports[1].run_id);
    assert_eq!(reports[0].workflow_id, reports[1].workflow_id);
    assert_eq!(
        reports
            .iter()
            .filter(|report| report.idempotent_replay)
            .count(),
        1
    );
    assert_eq!(count_rows(&store_path, "SELECT COUNT(*) FROM runs"), 1);
    assert_eq!(count_rows(&store_path, "SELECT COUNT(*) FROM workflows"), 1);
    assert_eq!(
        count_rows(
            &store_path,
            "SELECT COUNT(*) FROM events WHERE kind = 'async_request_started'"
        ),
        1
    );
}

#[test]
fn failed_request_start_rolls_back_idempotency_reservation() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let project_root = temp.path().join("project");
    std::fs::create_dir_all(&project_root).unwrap();
    let store = ForgeStore::open(&store_path).unwrap();
    let connection = Connection::open(&store_path).unwrap();
    connection
        .execute_batch(
            r#"
            CREATE TRIGGER reject_idempotent_request_start
            BEFORE INSERT ON events
            WHEN NEW.kind = 'async_request_started'
            BEGIN
                SELECT RAISE(ABORT, 'injected idempotent request start failure');
            END;
            "#,
        )
        .unwrap();
    drop(connection);

    let error = start_async_request_with_project_and_idempotency(
        &store,
        "Retry after transactional rollback",
        "codex",
        &project_root,
        Some("rollback-key"),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("injected idempotent request start failure"),
        "{error:#}"
    );
    assert_eq!(count_rows(&store_path, "SELECT COUNT(*) FROM runs"), 0);
    assert_eq!(count_rows(&store_path, "SELECT COUNT(*) FROM workflows"), 0);

    Connection::open(&store_path)
        .unwrap()
        .execute_batch("DROP TRIGGER reject_idempotent_request_start")
        .unwrap();
    let retried = start_async_request_with_project_and_idempotency(
        &store,
        "Retry after transactional rollback",
        "codex",
        &project_root,
        Some("rollback-key"),
    )
    .unwrap();
    let replay = start_async_request_with_project_and_idempotency(
        &store,
        "Retry after transactional rollback",
        "codex",
        &project_root,
        Some("rollback-key"),
    )
    .unwrap();
    assert_eq!(retried.run_id, replay.run_id);
    assert_eq!(count_rows(&store_path, "SELECT COUNT(*) FROM runs"), 1);
}

#[test]
fn cli_and_mcp_expose_idempotent_request_start() {
    let temp = tempdir().unwrap();
    let cli_store = temp.path().join("cli.sqlite");

    let first_output = forge()
        .args([
            "--store",
            cli_store.to_str().unwrap(),
            "request",
            "start",
            "--goal",
            "Retry the CLI request safely",
            "--origin",
            "codex",
            "--idempotency-key",
            "cli-key",
            "--output",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        first_output.status.success(),
        "{}",
        String::from_utf8_lossy(&first_output.stderr)
    );
    let first: Value = serde_json::from_slice(&first_output.stdout).unwrap();

    let replay_output = forge()
        .args([
            "--store",
            cli_store.to_str().unwrap(),
            "request",
            "start",
            "--goal",
            "Retry the CLI request safely",
            "--origin",
            "codex",
            "--idempotency-key",
            "cli-key",
            "--output",
            "json",
        ])
        .output()
        .unwrap();
    assert!(
        replay_output.status.success(),
        "{}",
        String::from_utf8_lossy(&replay_output.stderr)
    );
    let replay: Value = serde_json::from_slice(&replay_output.stdout).unwrap();
    assert_eq!(first, replay);

    let mcp_store_path = temp.path().join("mcp.sqlite");
    let mcp_store = ForgeStore::open(&mcp_store_path).unwrap();
    let mcp_first = call_mcp_tool(
        &mcp_store,
        "forge.run.start",
        json!({
            "goal": "Retry the MCP request safely",
            "origin": "mcp",
            "idempotency_key": "mcp-key",
        }),
    )
    .unwrap();
    let mcp_replay = call_mcp_tool(
        &mcp_store,
        "forge.run.start",
        json!({
            "goal": "Retry the MCP request safely",
            "origin": "mcp",
            "idempotency_key": "mcp-key",
        }),
    )
    .unwrap();
    assert_eq!(mcp_first.result, mcp_replay.result);

    let manifest = serde_json::to_value(mcp_tools_manifest()).unwrap();
    let run_start = manifest["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "forge.run.start")
        .unwrap();
    assert_eq!(
        run_start["input_schema"]["properties"]["idempotency_key"]["type"],
        "string"
    );
    assert!(!run_start["input_schema"]["required"]
        .as_array()
        .unwrap()
        .contains(&json!("idempotency_key")));
}

#[test]
fn detached_cli_replay_launches_exactly_one_logical_driver() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("detached.sqlite");
    let store_arg = store_path.to_str().unwrap();
    let args = [
        "--store",
        store_arg,
        "request",
        "start",
        "--goal",
        "Run one retry-safe detached request",
        "--origin",
        "codex",
        "--idempotency-key",
        "detached-retry-key",
        "--detached",
        "--output",
        "json",
    ];

    let first_output = forge().args(args).output().unwrap();
    assert!(
        first_output.status.success(),
        "{}",
        String::from_utf8_lossy(&first_output.stderr)
    );
    let first: Value = serde_json::from_slice(&first_output.stdout).unwrap();

    let replay_output = forge().args(args).output().unwrap();
    assert!(
        replay_output.status.success(),
        "{}",
        String::from_utf8_lossy(&replay_output.stderr)
    );
    let replay: Value = serde_json::from_slice(&replay_output.stdout).unwrap();

    assert_eq!(first, replay);
    assert_eq!(count_rows(&store_path, "SELECT COUNT(*) FROM runs"), 1);
    assert_eq!(count_rows(&store_path, "SELECT COUNT(*) FROM workflows"), 1);
    assert_eq!(
        count_rows(
            &store_path,
            "SELECT COUNT(*) FROM events WHERE kind = 'async_request_detached_driver_spawned'"
        ),
        1,
        "an idempotent detached replay must not launch a second logical worker"
    );

    let run_id = first["run_id"].as_str().unwrap();
    for _ in 0..100 {
        let store = ForgeStore::open(&store_path).unwrap();
        let status = store.load_run(run_id).unwrap()["status"]
            .as_str()
            .unwrap()
            .to_string();
        if !matches!(status.as_str(), "accepted" | "resumed" | "running") {
            break;
        }
        thread::sleep(std::time::Duration::from_millis(20));
    }
}
