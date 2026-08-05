use chrono::Utc;
use foundry_core::graph::{self, ExecutorKind, TaskImpediment, TaskStatus};
use foundry_core::intent::parse_intent;
use foundry_core::lease::acquire_task_lease;
use foundry_core::request::{
    complete_ready_task, create_run_record, drive_request, load_run_record, save_run_record,
    start_async_request_with_project, step_request, RequestTaskCompletionInput,
};
use foundry_core::storage::FoundryStore;
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
    let store_path = temp.path().join("foundry.sqlite");
    let project_root = temp.path().join("project");
    std::fs::create_dir_all(&project_root).unwrap();

    let store = FoundryStore::open(&store_path).unwrap();
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
    let store = FoundryStore::open(temp.path().join("foundry.sqlite")).unwrap();
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
    let store = FoundryStore::open(temp.path().join("foundry.sqlite")).unwrap();
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
            evidence_exit_code: None,
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
            .contains("explicit non-empty caller-attested evidence command"),
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
fn complete_task_rejects_a_missing_preexisting_lease_before_any_mutation() {
    let temp = tempdir().unwrap();
    let store = FoundryStore::open(temp.path().join("foundry.sqlite")).unwrap();
    let mut workflow = graph::create_workflow(parse_intent(
        "Require lease acquisition before accepting completion evidence",
    ));
    workflow.tasks = vec![graph::task(
        "command-task",
        "Run leased deterministic command",
        &[],
        &[],
        vec![],
        "command receipt",
        (ExecutorKind::Command, 0.0),
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
            task_id: "command-task",
            executor: "external-worker",
            summary: "claimed completion without acquiring the offered lease",
            artifact_paths: &[],
            evidence_command: Some("true"),
            evidence_exit_code: Some(0),
            evidence_summary: Some("caller supplied evidence without a dispatch lease"),
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
            .contains("must already have an active executor lease"),
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
    assert!(store
        .load_task_lease(&workflow.id, "command-task")
        .unwrap()
        .is_none());
    assert!(directory_inventory(&temp.path().join("artifacts")).is_empty());
    assert!(store.load_workflow_events(&workflow.id).unwrap().is_empty());
}

#[test]
fn dispatch_wave_event_failure_rolls_back_heartbeat_and_all_acquired_leases() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("foundry.sqlite");
    let store = FoundryStore::open(&store_path).unwrap();
    let mut workflow = graph::create_workflow(parse_intent(
        "Acquire a dispatch wave and its leases as one atomic mutation",
    ));
    workflow.tasks = vec![graph::task(
        "dispatch-task",
        "Dispatch atomically",
        &[],
        &[],
        vec![],
        "dispatch receipt",
        (ExecutorKind::Command, 0.0),
    )];
    store.save_workflow(&workflow).unwrap();
    let run = create_run_record(&workflow, "test", "accepted");
    save_run_record(&store, &run).unwrap();
    let run_before = serde_json::to_value(load_run_record(&store, &run.run_id).unwrap()).unwrap();
    let workflow_before = serde_json::to_value(store.load_workflow(&workflow.id).unwrap()).unwrap();

    let connection = Connection::open(&store_path).unwrap();
    connection
        .execute_batch(
            r#"
            CREATE TRIGGER reject_request_dispatch_wave
            BEFORE INSERT ON events
            WHEN NEW.kind = 'request_dispatch_wave_created'
            BEGIN
                SELECT RAISE(ABORT, 'injected dispatch wave event failure');
            END;
            "#,
        )
        .unwrap();
    drop(connection);

    let error = drive_request(&store, &run.run_id, "external-worker", 120, "test").unwrap_err();
    assert!(
        error
            .to_string()
            .contains("injected dispatch wave event failure"),
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
    assert!(store
        .load_task_lease(&workflow.id, "dispatch-task")
        .unwrap()
        .is_none());
    assert!(store.load_workflow_events(&workflow.id).unwrap().is_empty());
}

#[test]
fn bounded_frontier_finds_a_runnable_task_behind_a_higher_priority_impediment() {
    let temp = tempdir().unwrap();
    let store = FoundryStore::open(temp.path().join("foundry.sqlite")).unwrap();
    let mut workflow = graph::create_workflow(parse_intent(
        "Do not let one impeded priority task hide the runnable frontier",
    ));
    workflow.core_orchestration.max_parallel_tasks = 1;
    let mut impeded = graph::task(
        "task-impeded",
        "Wait for an external dependency",
        &[],
        &[],
        vec![],
        "dependency receipt",
        (ExecutorKind::Command, 0.0),
    );
    impeded.work_item.priority = "p0".to_string();
    impeded.active_impediments.push(TaskImpediment {
        id: "impediment-external".to_string(),
        kind: "external".to_string(),
        reason: "external dependency is not ready".to_string(),
        origin: "test".to_string(),
        created_at: Utc::now(),
    });
    let mut runnable = graph::task(
        "task-runnable",
        "Run independent work",
        &[],
        &[],
        vec![],
        "independent receipt",
        (ExecutorKind::Command, 0.0),
    );
    runnable.work_item.priority = "p1".to_string();
    workflow.tasks = vec![impeded, runnable];
    store.save_workflow(&workflow).unwrap();
    let run = create_run_record(&workflow, "test", "accepted");
    save_run_record(&store, &run).unwrap();

    let drive = drive_request(&store, &run.run_id, "external-worker", 120, "test").unwrap();
    let frontier = drive.dispatch_frontier.as_ref().unwrap();
    assert_eq!(frontier.max_parallel_tasks, 1);
    assert_eq!(frontier.wave.assignments.len(), 1);
    assert_eq!(frontier.wave.assignments[0].task_id, "task-runnable");
    assert!(!frontier.wave.execution_started);
    assert!(!frontier.wave.assignments[0].execution_started);
    assert!(frontier
        .wave
        .deferred
        .iter()
        .any(|task| { task.task_id == "task-impeded" && task.status == "blocked_impediments" }));
}

#[test]
fn missing_quota_preserves_an_existing_lease_without_admitting_more_parallel_work() {
    let temp = tempdir().unwrap();
    let store = FoundryStore::open(temp.path().join("foundry.sqlite")).unwrap();
    let mut workflow = graph::create_workflow(parse_intent(
        "Preserve leased work while unknown quota prevents new fan-out",
    ));
    workflow.core_orchestration.max_parallel_tasks = 2;
    workflow.tasks = vec![
        graph::task(
            "task-leased",
            "Continue leased work",
            &[],
            &[],
            vec![],
            "leased receipt",
            (ExecutorKind::Command, 0.0),
        ),
        graph::task(
            "task-new",
            "Wait for quota evidence",
            &[],
            &[],
            vec![],
            "new receipt",
            (ExecutorKind::Command, 0.0),
        ),
    ];
    store.save_workflow(&workflow).unwrap();
    let run = create_run_record(&workflow, "test", "accepted");
    save_run_record(&store, &run).unwrap();
    let acquired =
        acquire_task_lease(&store, &workflow.id, "task-leased", "external-worker", 120).unwrap();
    let lease_id = acquired.lease.unwrap().lease_id;

    let drive = drive_request(&store, &run.run_id, "external-worker", 120, "test").unwrap();
    let frontier = drive.dispatch_frontier.as_ref().unwrap();
    assert_eq!(frontier.admission.quota_status, "missing");
    assert_eq!(frontier.admission.existing_active_leases, 1);
    assert_eq!(frontier.admission.admitted_new_handoffs, 0);
    assert_eq!(frontier.wave.assignments.len(), 1);
    assert_eq!(frontier.wave.assignments[0].task_id, "task-leased");
    assert_eq!(frontier.wave.assignments[0].lease_id, lease_id);
    assert_eq!(frontier.wave.assignments[0].lease_state, "reused_active");
    assert!(!frontier.wave.execution_started);
    assert!(store
        .load_task_lease(&workflow.id, "task-new")
        .unwrap()
        .is_none());
}

#[test]
fn nested_request_step_defers_final_delivery_until_after_outer_sqlite_commit() {
    let temp = tempdir().unwrap();
    let store = FoundryStore::open(temp.path().join("foundry.sqlite")).unwrap();
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
