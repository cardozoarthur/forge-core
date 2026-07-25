use forge_core::graph::{self, ExecutorKind, TaskStatus};
use forge_core::intent::parse_intent;
use forge_core::request::{
    complete_ready_task, create_run_record, drive_request, load_run_record, save_run_record,
    start_async_request_with_project, step_request, RequestTaskCompletionInput,
};
use forge_core::storage::ForgeStore;
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

fn directory_inventory(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn collect(root: &Path, current: &Path, entries: &mut Vec<(PathBuf, Vec<u8>)>) {
        if !current.exists() {
            return;
        }
        for entry in fs::read_dir(current).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                collect(root, &path, entries);
            } else {
                entries.push((
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(path).unwrap(),
                ));
            }
        }
    }

    let mut entries = Vec::new();
    collect(root, root, &mut entries);
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries
}

#[test]
fn request_start_rolls_back_workflow_and_run_when_event_persistence_fails() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let project_root = temp.path().join("project");
    std::fs::create_dir_all(&project_root).unwrap();

    let store = ForgeStore::open(&store_path).unwrap();
    let connection = Connection::open(&store_path).unwrap();
    connection
        .execute_batch(
            r#"
            CREATE TRIGGER reject_async_request_started
            BEFORE INSERT ON events
            WHEN NEW.kind = 'async_request_started'
            BEGIN
                SELECT RAISE(ABORT, 'injected async request event failure');
            END;
            "#,
        )
        .unwrap();
    drop(connection);

    let error = start_async_request_with_project(
        &store,
        "Prove request start transaction rollback",
        "atomicity-test",
        &project_root,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("injected async request event failure"),
        "{error:#}"
    );
    assert!(store.load_workflows().unwrap().is_empty());
    assert!(store.load_runs().unwrap().is_empty());
}

#[test]
fn request_step_outer_rollback_leaves_artifact_directory_byte_identical() {
    let temp = tempdir().unwrap();
    let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();
    let mut workflow = graph::create_workflow(parse_intent(
        "Require a real receipt before a deterministic task can complete",
    ));
    workflow.tasks = vec![graph::task(
        "command-task",
        "Run deterministic command",
        &[],
        &[],
        vec![],
        "command receipt",
        (ExecutorKind::Command, 0.0),
    )];
    store.save_workflow(&workflow).unwrap();
    let run = create_run_record(&workflow, "test", "accepted");
    save_run_record(&store, &run).unwrap();
    let artifact_root = temp.path().join("artifacts");
    let before = directory_inventory(&artifact_root);

    let result: anyhow::Result<()> = store.with_transaction(|| {
        let step = step_request(&store, &run.run_id, "external-worker", 120, "test")?;
        assert_eq!(step.status, "handoff_required");
        assert!(step.output_artifact.is_none());
        assert!(step.response_artifact_path.is_none());
        anyhow::bail!("injected outer request step rollback")
    });

    let error = result.unwrap_err();
    assert!(error
        .to_string()
        .contains("injected outer request step rollback"));
    assert_eq!(directory_inventory(&artifact_root), before);
    assert_eq!(
        load_run_record(&store, &run.run_id).unwrap().status,
        "accepted"
    );
    let current_workflow = store.load_workflow(&workflow.id).unwrap();
    assert_eq!(current_workflow.tasks[0].status, TaskStatus::Pending);
    assert!(current_workflow.artifacts.is_empty());
    assert!(store.load_workflow_events(&workflow.id).unwrap().is_empty());
}

#[test]
fn deterministic_complete_task_requires_explicit_execution_receipt_before_mutation() {
    let temp = tempdir().unwrap();
    let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();
    let mut workflow = graph::create_workflow(parse_intent(
        "Reject deterministic completion without an execution receipt",
    ));
    workflow.tasks = vec![graph::task(
        "wait-task",
        "Wait for bounded condition",
        &[],
        &[],
        vec![],
        "wait receipt",
        (ExecutorKind::Wait, 0.0),
    )];
    store.save_workflow(&workflow).unwrap();
    let run = create_run_record(&workflow, "test", "accepted");
    save_run_record(&store, &run).unwrap();
    let run_before = serde_json::to_value(load_run_record(&store, &run.run_id).unwrap()).unwrap();
    let workflow_before = serde_json::to_value(store.load_workflow(&workflow.id).unwrap()).unwrap();

    let error = complete_ready_task(
        &store,
        &run.run_id,
        RequestTaskCompletionInput {
            task_id: "wait-task",
            executor: "external-worker",
            summary: "claimed completion without a receipt",
            artifact_paths: &[],
            evidence_command: None,
            evidence_summary: Some("missing execution command"),
            estimated_usd: 0.0,
            tokens_in: 0,
            tokens_out: 0,
            ttl_seconds: 120,
            context_budget: None,
            origin: "test",
        },
    )
    .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("explicit non-empty evidence command receipt"),
        "{error:#}"
    );
    assert_eq!(
        serde_json::to_value(load_run_record(&store, &run.run_id).unwrap()).unwrap(),
        run_before
    );
    assert_eq!(
        serde_json::to_value(store.load_workflow(&workflow.id).unwrap()).unwrap(),
        workflow_before
    );
    assert!(directory_inventory(&temp.path().join("artifacts")).is_empty());
    assert!(store.load_workflow_events(&workflow.id).unwrap().is_empty());
}

#[test]
fn nested_request_step_defers_final_delivery_until_after_outer_sqlite_commit() {
    let temp = tempdir().unwrap();
    let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();
    let mut workflow = graph::create_workflow(parse_intent(
        "Finish run state when every task is already validated",
    ));
    for task in &mut workflow.tasks {
        task.status = TaskStatus::Completed;
    }
    let initial_workflow_status = workflow.status.clone();
    store.save_workflow(&workflow).unwrap();
    let run = create_run_record(&workflow, "test", "accepted");
    save_run_record(&store, &run).unwrap();
    let artifact_root = temp.path().join("artifacts");
    let staging_root = temp.path().join("tmp");
    let artifacts_before = directory_inventory(&artifact_root);
    let staging_before = directory_inventory(&staging_root);

    let result: anyhow::Result<()> = store.with_transaction(|| {
        let step = step_request(&store, &run.run_id, "external-worker", 120, "test")?;
        assert_eq!(step.status, "completion_ready");
        assert_eq!(step.action, "finalize_request");
        assert!(step.drive_before.final_delivery_package.is_none());
        anyhow::bail!("injected failure before outer SQLite commit")
    });

    let error = result.unwrap_err();
    assert!(error
        .to_string()
        .contains("injected failure before outer SQLite commit"));
    assert_eq!(directory_inventory(&artifact_root), artifacts_before);
    assert_eq!(directory_inventory(&staging_root), staging_before);
    let rolled_back_run = load_run_record(&store, &run.run_id).unwrap();
    assert_eq!(rolled_back_run.status, "accepted");
    let rolled_back_workflow = store.load_workflow(&workflow.id).unwrap();
    assert_eq!(rolled_back_workflow.status, initial_workflow_status);
    assert!(rolled_back_workflow
        .artifacts
        .iter()
        .all(|artifact| !artifact.kind.starts_with("final_delivery_package")));

    let completed = drive_request(&store, &run.run_id, "external-worker", 120, "test").unwrap();
    assert_eq!(completed.status, "complete");
    assert!(completed.final_delivery_package.is_some());
    assert_eq!(
        load_run_record(&store, &run.run_id).unwrap().status,
        "completed"
    );
}
