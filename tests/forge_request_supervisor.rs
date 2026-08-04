use chrono::{Duration, Utc};
use forge_core::graph::{self, ExecutorKind, TaskStatus, Workflow};
use forge_core::intent::parse_intent;
use forge_core::request::{
    cancel_request, create_run_record, heartbeat_request, load_run_record, recover_stale_request,
    resume_async_request, save_run_record, step_request, switch_request_executor,
    update_run_status, RequestExecutorSwitchInput, RunRecord,
};
use forge_core::request_supervisor::{
    supervise_request_once, supervise_requests_once, RequestSupervisorOptions,
    RequestSupervisorRunOutcome, MAX_REQUEST_SUPERVISOR_STEPS_PER_RUN,
};
use forge_core::storage::ForgeStore;
use rusqlite::Connection;
use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;
use tempfile::TempDir;

fn open_store() -> (TempDir, ForgeStore) {
    let temporary = tempfile::tempdir().unwrap();
    let store = ForgeStore::open(temporary.path().join("forge.sqlite")).unwrap();
    (temporary, store)
}

fn workflow_with_tasks(store: &ForgeStore, executors: &[ExecutorKind]) -> Workflow {
    let mut workflow = graph::create_workflow(parse_intent("Supervise a bounded request safely"));
    workflow.tasks = executors
        .iter()
        .enumerate()
        .map(|(index, executor)| {
            let id = format!("task-{}", index + 1);
            let dependency = (index > 0).then(|| format!("task-{index}"));
            let dependencies = dependency
                .as_deref()
                .map(|dependency| vec![dependency])
                .unwrap_or_default();
            graph::task(
                &id,
                &format!("Bounded task {}", index + 1),
                &dependencies,
                &[],
                vec![],
                &format!("bounded output {}", index + 1),
                (executor.clone(), 0.0),
            )
        })
        .collect();
    store.save_workflow(&workflow).unwrap();
    workflow
}

fn save_run(
    store: &ForgeStore,
    workflow: &Workflow,
    status: &str,
    configure: impl FnOnce(&mut RunRecord),
) -> RunRecord {
    let mut run = create_run_record(workflow, "test", status);
    configure(&mut run);
    save_run_record(store, &run).unwrap();
    run
}

fn options() -> RequestSupervisorOptions {
    RequestSupervisorOptions {
        executor: "forge-request-supervisor".to_string(),
        origin: "request-supervisor-test".to_string(),
        instance_id: "request-supervisor-test-instance".to_string(),
        ttl_seconds: 120,
        max_steps_per_run: 1,
    }
}

#[test]
fn cancel_and_resume_reject_a_live_supervisor_lease_without_mutation() {
    let (_temporary, store) = open_store();
    let workflow = workflow_with_tasks(&store, &[ExecutorKind::Ai]);
    let configure_live_lease = |run: &mut RunRecord| {
        run.supervisor_instance_id = Some("live-supervisor".to_string());
        run.supervisor_lease_expires_at = Some(Utc::now() + Duration::minutes(5));
        run.supervisor_fencing_token = 7;
    };
    let cancel_run = save_run(&store, &workflow, "accepted", configure_live_lease);
    let resume_run = save_run(&store, &workflow, "accepted", configure_live_lease);

    let cancel_before = serde_json::to_value(&cancel_run).unwrap();
    let cancel_error = cancel_request(&store, &cancel_run.run_id, "test").unwrap_err();
    assert!(cancel_error.to_string().contains("live supervisor lease"));
    assert!(cancel_error
        .to_string()
        .contains("recover or reconcile the run after the lease expires"));
    assert_eq!(
        serde_json::to_value(load_run_record(&store, &cancel_run.run_id).unwrap()).unwrap(),
        cancel_before
    );

    let resume_before = serde_json::to_value(&resume_run).unwrap();
    let resume_error = resume_async_request(&store, &resume_run.run_id, "test").unwrap_err();
    assert!(resume_error.to_string().contains("live supervisor lease"));
    assert!(resume_error
        .to_string()
        .contains("recover or reconcile the run after the lease expires"));
    assert_eq!(
        serde_json::to_value(load_run_record(&store, &resume_run.run_id).unwrap()).unwrap(),
        resume_before
    );
}

#[test]
fn public_run_persistence_is_insert_only_and_preserves_a_live_lease() {
    let (_temporary, store) = open_store();
    let workflow = workflow_with_tasks(&store, &[ExecutorKind::Ai]);
    let run = save_run(&store, &workflow, "accepted", |run| {
        run.active_executor = Some("library-owner".to_string());
        run.supervisor_instance_id = Some("library-owner".to_string());
        run.supervisor_lease_expires_at = Some(Utc::now() + Duration::minutes(10));
        run.supervisor_fencing_token = 77;
    });
    let before = serde_json::to_value(&run).unwrap();
    let mut unfenced_update = run.clone();
    unfenced_update.status = "completed".to_string();
    unfenced_update.active_executor = Some("unfenced-library-writer".to_string());
    unfenced_update.supervisor_instance_id = None;
    unfenced_update.supervisor_lease_expires_at = None;

    let error = save_run_record(&store, &unfenced_update).unwrap_err();

    assert!(error.to_string().contains("insert-only"));
    assert_eq!(
        serde_json::to_value(load_run_record(&store, &run.run_id).unwrap()).unwrap(),
        before
    );
}

#[test]
fn stale_recovery_and_supervisor_scan_preserve_a_live_supervisor_lease() {
    let (_temporary, store) = open_store();
    let mut workflow = workflow_with_tasks(&store, &[ExecutorKind::Ai]);
    workflow.status = "running".to_string();
    store.save_workflow(&workflow).unwrap();
    let run = save_run(&store, &workflow, "running", |run| {
        run.active_executor = Some("external-worker".to_string());
        run.last_heartbeat_at = Some(Utc::now() - Duration::minutes(2));
        run.heartbeat_expires_at = Some(Utc::now() - Duration::minutes(1));
        run.heartbeat_ttl_seconds = Some(30);
        run.supervisor_instance_id = Some("live-supervisor-audit".to_string());
        run.supervisor_lease_expires_at = Some(Utc::now() + Duration::minutes(10));
        run.supervisor_fencing_token = 41;
    });
    let before = serde_json::to_value(&run).unwrap();

    let recovery_error = recover_stale_request(&store, &run.run_id, "test").unwrap_err();
    assert!(recovery_error.to_string().contains("live supervisor lease"));
    assert_eq!(
        serde_json::to_value(load_run_record(&store, &run.run_id).unwrap()).unwrap(),
        before
    );

    let supervised = supervise_request_once(&store, &run.run_id, &options()).unwrap();
    assert_eq!(
        supervised.outcome,
        RequestSupervisorRunOutcome::SkippedLeaseContended
    );
    assert_eq!(supervised.final_status, "running");
    assert_eq!(
        serde_json::to_value(load_run_record(&store, &run.run_id).unwrap()).unwrap(),
        before
    );
    assert_eq!(store.load_workflow(&workflow.id).unwrap().status, "running");
}

#[test]
fn cancel_and_resume_remain_available_without_a_supervisor_lease() {
    let (_temporary, store) = open_store();
    let workflow = workflow_with_tasks(&store, &[ExecutorKind::Ai]);
    let cancel_run = save_run(&store, &workflow, "accepted", |_| {});
    let resume_run = save_run(&store, &workflow, "accepted", |_| {});

    let cancelled = cancel_request(&store, &cancel_run.run_id, "test").unwrap();
    assert_eq!(cancelled.status, "cancelled");
    assert_eq!(
        load_run_record(&store, &cancel_run.run_id).unwrap().status,
        "cancelled"
    );

    let resumed = resume_async_request(&store, &resume_run.run_id, "test").unwrap();
    assert_eq!(resumed.status, "resumed");
    assert_eq!(
        load_run_record(&store, &resume_run.run_id).unwrap().status,
        "resumed"
    );
}

#[test]
fn status_update_and_executor_switch_reject_a_live_lease_without_mutation() {
    let (_temporary, store) = open_store();
    let workflow = workflow_with_tasks(&store, &[ExecutorKind::Ai]);
    let configure_live_lease = |run: &mut RunRecord| {
        run.supervisor_instance_id = Some("live-supervisor".to_string());
        run.supervisor_lease_expires_at = Some(Utc::now() + Duration::minutes(5));
        run.supervisor_fencing_token = 13;
    };
    let status_run = save_run(&store, &workflow, "accepted", configure_live_lease);
    let switch_run = save_run(&store, &workflow, "accepted", configure_live_lease);

    let status_before = serde_json::to_value(&status_run).unwrap();
    let status_events_before = store.load_workflow_events(&workflow.id).unwrap().len();
    let status_error =
        update_run_status(&store, &status_run.run_id, "needs_attention", "test").unwrap_err();
    assert!(status_error.to_string().contains("live supervisor lease"));
    assert_eq!(
        serde_json::to_value(load_run_record(&store, &status_run.run_id).unwrap()).unwrap(),
        status_before
    );
    assert_eq!(
        store.load_workflow_events(&workflow.id).unwrap().len(),
        status_events_before
    );

    let switch_before = serde_json::to_value(&switch_run).unwrap();
    let workflow_before = serde_json::to_value(store.load_workflow(&workflow.id).unwrap()).unwrap();
    let switch_events_before = store.load_workflow_events(&workflow.id).unwrap().len();
    let switch_error = switch_request_executor(
        &store,
        &switch_run.run_id,
        RequestExecutorSwitchInput {
            executor: "codex".to_string(),
            fallback_executors: vec!["agy".to_string()],
            summary: "attempt fenced switch".to_string(),
            ttl_seconds: 300,
            pid: None,
            origin: "test".to_string(),
            reason: "regression test".to_string(),
        },
    )
    .unwrap_err();
    assert!(switch_error.to_string().contains("live supervisor lease"));
    assert_eq!(
        serde_json::to_value(load_run_record(&store, &switch_run.run_id).unwrap()).unwrap(),
        switch_before
    );
    assert_eq!(
        serde_json::to_value(store.load_workflow(&workflow.id).unwrap()).unwrap(),
        workflow_before
    );
    assert_eq!(
        store.load_workflow_events(&workflow.id).unwrap().len(),
        switch_events_before
    );
}

#[test]
fn status_update_and_executor_switch_remain_available_without_a_supervisor_lease() {
    let (_temporary, store) = open_store();
    let workflow = workflow_with_tasks(&store, &[ExecutorKind::Ai]);
    let status_run = save_run(&store, &workflow, "accepted", |_| {});
    let switch_run = save_run(&store, &workflow, "accepted", |_| {});

    let updated = update_run_status(&store, &status_run.run_id, "needs_attention", "test").unwrap();
    assert_eq!(updated.status, "needs_attention");

    let switched = switch_request_executor(
        &store,
        &switch_run.run_id,
        RequestExecutorSwitchInput {
            executor: "codex".to_string(),
            fallback_executors: vec!["agy".to_string()],
            summary: "normal switch".to_string(),
            ttl_seconds: 300,
            pid: None,
            origin: "test".to_string(),
            reason: "normal path regression test".to_string(),
        },
    )
    .unwrap();
    assert_eq!(switched.status, "running");
    assert_eq!(switched.new_executor, "codex");
    assert_eq!(switched.fallback_executors, vec!["agy"]);
}

#[test]
fn parks_handoff_boundary_once_with_structured_reason_and_cleared_lease() {
    let (_temporary, store) = open_store();
    let workflow = workflow_with_tasks(&store, &[ExecutorKind::Command]);
    let run = save_run(&store, &workflow, "accepted", |_| {});

    let first = supervise_requests_once(&store, &options()).unwrap();

    assert_eq!(first.status, "request_supervisor_completed");
    assert_eq!(first.counts.scanned, 1);
    assert_eq!(first.counts.needs_attention, 1);
    assert_eq!(first.counts.advanced, 0);
    assert_eq!(first.counts.failures, 0);
    assert_eq!(
        first.runs[0].outcome,
        RequestSupervisorRunOutcome::NeedsAttention
    );
    assert_eq!(first.runs[0].steps_attempted, 1);
    let attention_reason = first.runs[0].attention_reason.as_ref().unwrap();
    assert_eq!(attention_reason.status, "handoff_required");
    assert_eq!(attention_reason.source, "step");

    let parked = load_run_record(&store, &run.run_id).unwrap();
    assert_eq!(parked.status, "needs_attention");
    assert_eq!(parked.active_executor, None);
    assert_eq!(parked.executor_pid, None);
    assert_eq!(parked.last_heartbeat_at, None);
    assert_eq!(parked.heartbeat_expires_at, None);
    assert_eq!(parked.heartbeat_ttl_seconds, None);
    let stored_reason: serde_json::Value =
        serde_json::from_str(parked.progress_summary.as_deref().unwrap()).unwrap();
    assert_eq!(stored_reason["status"], "handoff_required");
    assert_eq!(stored_reason["source"], "step");

    let parked_workflow = store.load_workflow(&workflow.id).unwrap();
    assert_eq!(parked_workflow.status, "needs_attention");
    assert_eq!(parked_workflow.tasks[0].status, TaskStatus::Pending);

    let attention_events = store
        .load_workflow_events(&workflow.id)
        .unwrap()
        .into_iter()
        .filter(|event| event.kind == "async_request_needs_attention")
        .collect::<Vec<_>>();
    assert_eq!(attention_events.len(), 1);
    assert_eq!(
        attention_events[0].data["reason_code"],
        "supervisor_manual_boundary"
    );
    assert_eq!(
        attention_events[0].data["reason"]["status"],
        "handoff_required"
    );

    let parked_updated_at = parked.updated_at;
    let second = supervise_requests_once(&store, &options()).unwrap();
    assert_eq!(second.counts.needs_attention, 0);
    assert_eq!(second.counts.skipped_inactive, 1);
    assert_eq!(second.runs[0].steps_attempted, 0);
    assert_eq!(
        second.runs[0].outcome,
        RequestSupervisorRunOutcome::SkippedInactive
    );
    let still_parked = load_run_record(&store, &run.run_id).unwrap();
    assert_eq!(still_parked.updated_at, parked_updated_at);
    assert_eq!(
        store
            .load_workflow_events(&workflow.id)
            .unwrap()
            .into_iter()
            .filter(|event| event.kind == "async_request_needs_attention")
            .count(),
        1,
        "a parked run must not be implicitly resumed or emit duplicate attention events"
    );
}

#[test]
fn recovers_stale_runs_and_does_not_take_over_fresh_external_executor() {
    let (_temporary, store) = open_store();
    let mut stale_workflow = workflow_with_tasks(&store, &[ExecutorKind::Ai]);
    stale_workflow.status = "running".to_string();
    store.save_workflow(&stale_workflow).unwrap();
    let stale_run = save_run(&store, &stale_workflow, "running", |run| {
        run.active_executor = Some("external-worker".to_string());
        run.executor_pid = Some(u32::MAX);
        run.progress_summary = Some("external worker stopped".to_string());
        run.last_heartbeat_at = Some(Utc::now() - Duration::seconds(120));
        run.heartbeat_expires_at = Some(Utc::now() - Duration::seconds(60));
        run.heartbeat_ttl_seconds = Some(30);
        run.supervisor_instance_id = Some("crashed-supervisor-instance".to_string());
        run.supervisor_lease_expires_at = Some(Utc::now() - Duration::seconds(60));
        run.supervisor_fencing_token = 7;
    });

    let mut fresh_workflow = workflow_with_tasks(&store, &[ExecutorKind::Ai]);
    fresh_workflow.status = "running".to_string();
    store.save_workflow(&fresh_workflow).unwrap();
    let fresh_run = save_run(&store, &fresh_workflow, "running", |run| {
        run.active_executor = Some("external-worker".to_string());
        run.progress_summary = Some("external worker is healthy".to_string());
        run.last_heartbeat_at = Some(Utc::now());
        run.heartbeat_expires_at = Some(Utc::now() + Duration::seconds(300));
        run.heartbeat_ttl_seconds = Some(300);
    });
    let fresh_before = serde_json::to_value(load_run_record(&store, &fresh_run.run_id).unwrap())
        .expect("fresh run serializes");

    let report = supervise_requests_once(&store, &options()).unwrap();

    assert_eq!(report.counts.scanned, 2);
    assert_eq!(report.counts.recovered, 1);
    assert_eq!(report.counts.needs_attention, 1);
    assert_eq!(report.counts.skipped_external_active, 1);
    assert_eq!(report.counts.advanced, 0);
    assert_eq!(report.counts.failures, 0);

    let recovered = load_run_record(&store, &stale_run.run_id).unwrap();
    assert_eq!(recovered.status, "needs_attention");
    assert_eq!(recovered.active_executor, None);
    assert_eq!(recovered.executor_pid, None);
    assert_eq!(recovered.last_heartbeat_at, None);
    assert_eq!(recovered.heartbeat_expires_at, None);
    assert_eq!(recovered.heartbeat_ttl_seconds, None);
    assert_eq!(recovered.supervisor_instance_id, None);
    assert_eq!(recovered.supervisor_lease_expires_at, None);
    assert_eq!(
        recovered.supervisor_fencing_token, 7,
        "stale recovery clears ownership but preserves the monotonic fencing counter"
    );
    assert_eq!(
        store.load_workflow(&stale_workflow.id).unwrap().status,
        "needs_attention"
    );

    let fresh_after = serde_json::to_value(load_run_record(&store, &fresh_run.run_id).unwrap())
        .expect("fresh run serializes");
    assert_eq!(
        fresh_after, fresh_before,
        "fresh external executor state must remain byte-for-byte unchanged"
    );
    assert_eq!(
        store.load_workflow(&fresh_workflow.id).unwrap().status,
        "running"
    );
}

#[test]
fn live_supervisor_lease_rejects_unfenced_step_and_heartbeat_but_owner_can_advance() {
    let (_temporary, store) = open_store();
    let workflow = workflow_with_tasks(&store, &[ExecutorKind::Command]);
    let run = save_run(&store, &workflow, "accepted", |run| {
        run.supervisor_instance_id = Some("request-supervisor-test-instance".to_string());
        run.supervisor_lease_expires_at = Some(Utc::now() + Duration::seconds(120));
        run.supervisor_fencing_token = 41;
    });
    let run_before = serde_json::to_value(load_run_record(&store, &run.run_id).unwrap()).unwrap();
    let workflow_before = serde_json::to_value(store.load_workflow(&workflow.id).unwrap()).unwrap();

    let step_error = step_request(&store, &run.run_id, "external-worker", 120, "test").unwrap_err();
    assert!(
        step_error.to_string().contains("live supervisor lease"),
        "{step_error:#}"
    );
    let heartbeat_error = heartbeat_request(
        &store,
        &run.run_id,
        "external-worker",
        "must not clear or bypass supervisor fencing",
        120,
        None,
        "test",
    )
    .unwrap_err();
    assert!(
        heartbeat_error
            .to_string()
            .contains("live supervisor lease"),
        "{heartbeat_error:#}"
    );
    assert_eq!(
        serde_json::to_value(load_run_record(&store, &run.run_id).unwrap()).unwrap(),
        run_before
    );
    assert_eq!(
        serde_json::to_value(store.load_workflow(&workflow.id).unwrap()).unwrap(),
        workflow_before
    );

    let supervised = supervise_request_once(&store, &run.run_id, &options()).unwrap();
    assert_eq!(
        supervised.outcome,
        RequestSupervisorRunOutcome::NeedsAttention
    );
    let parked = load_run_record(&store, &run.run_id).unwrap();
    assert_eq!(parked.status, "needs_attention");
    assert_eq!(parked.supervisor_fencing_token, 41);
    assert_eq!(parked.supervisor_instance_id, None);
    assert_eq!(
        store.load_workflow(&workflow.id).unwrap().tasks[0].status,
        TaskStatus::Pending
    );
}

#[test]
fn parks_command_wait_and_notification_tasks_until_real_execution_receipts_exist() {
    let (_temporary, store) = open_store();
    let executor_kinds = [
        ExecutorKind::Command,
        ExecutorKind::Wait,
        ExecutorKind::Notification,
    ];
    for executor_kind in executor_kinds {
        let workflow = workflow_with_tasks(&store, std::slice::from_ref(&executor_kind));
        let run = save_run(&store, &workflow, "accepted", |_| {});

        let supervised = supervise_request_once(&store, &run.run_id, &options()).unwrap();

        assert_eq!(
            supervised.outcome,
            RequestSupervisorRunOutcome::NeedsAttention
        );
        assert_eq!(supervised.steps_attempted, 1);
        assert_eq!(supervised.steps_advanced, 0);
        assert_eq!(
            supervised
                .attention_reason
                .as_ref()
                .map(|reason| reason.status.as_str()),
            Some("handoff_required")
        );
        let current_workflow = store.load_workflow(&workflow.id).unwrap();
        assert_eq!(current_workflow.tasks[0].status, TaskStatus::Pending);
        assert!(
            current_workflow.artifacts.is_empty(),
            "{executor_kind:?} must not gain fabricated output artifacts"
        );
        let current_run = load_run_record(&store, &run.run_id).unwrap();
        assert_eq!(current_run.status, "needs_attention");
        assert_eq!(current_run.supervisor_instance_id, None);
        assert_eq!(current_run.supervisor_lease_expires_at, None);
    }
}

#[test]
fn rejects_unbounded_or_zero_step_configuration_before_scanning() {
    let (_temporary, store) = open_store();
    let mut invalid = options();
    invalid.max_steps_per_run = 0;
    assert!(supervise_requests_once(&store, &invalid)
        .unwrap_err()
        .to_string()
        .contains("must be at least 1"));

    invalid.max_steps_per_run = MAX_REQUEST_SUPERVISOR_STEPS_PER_RUN + 1;
    assert!(supervise_requests_once(&store, &invalid)
        .unwrap_err()
        .to_string()
        .contains("cannot exceed"));
}

#[test]
fn concurrent_supervisors_use_exclusive_fenced_ownership_for_one_step() {
    let (temporary, store) = open_store();
    let workflow = workflow_with_tasks(
        &store,
        &[
            ExecutorKind::Command,
            ExecutorKind::Command,
            ExecutorKind::Command,
        ],
    );
    let run = save_run(&store, &workflow, "accepted", |_| {});
    let store_path = store.path().to_path_buf();
    drop(store);

    let barrier = Arc::new(Barrier::new(2));
    let handles = ["supervisor-instance-a", "supervisor-instance-b"]
        .into_iter()
        .map(|instance_id| {
            let barrier = Arc::clone(&barrier);
            let store_path = store_path.clone();
            let run_id = run.run_id.clone();
            thread::spawn(move || {
                let store = ForgeStore::open(store_path).unwrap();
                let options = RequestSupervisorOptions {
                    executor: "forge-request-supervisor".to_string(),
                    origin: "concurrent-supervisor-test".to_string(),
                    instance_id: instance_id.to_string(),
                    ttl_seconds: 120,
                    max_steps_per_run: 1,
                };
                barrier.wait();
                supervise_request_once(&store, &run_id, &options).unwrap()
            })
        })
        .collect::<Vec<_>>();
    let results = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect::<Vec<_>>();

    assert_eq!(
        results
            .iter()
            .filter(|result| result.outcome == RequestSupervisorRunOutcome::NeedsAttention)
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| {
                matches!(
                    result.outcome,
                    RequestSupervisorRunOutcome::SkippedLeaseContended
                        | RequestSupervisorRunOutcome::SkippedInactive
                )
            })
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .map(|result| result.steps_advanced)
            .sum::<usize>(),
        0
    );

    let store = ForgeStore::open(&store_path).unwrap();
    let current_workflow = store.load_workflow(&workflow.id).unwrap();
    assert_eq!(current_workflow.tasks[0].status, TaskStatus::Pending);
    assert_eq!(current_workflow.tasks[1].status, TaskStatus::Pending);
    assert_eq!(current_workflow.tasks[2].status, TaskStatus::Pending);
    let current_run = load_run_record(&store, &run.run_id).unwrap();
    assert_eq!(current_run.supervisor_fencing_token, 1);
    assert_eq!(current_run.status, "needs_attention");
    assert_eq!(current_run.supervisor_instance_id, None);
    assert_eq!(current_run.supervisor_lease_expires_at, None);
    assert_eq!(
        store
            .load_workflow_events(&workflow.id)
            .unwrap()
            .into_iter()
            .filter(|event| event.kind == "async_request_supervisor_lease_acquired")
            .count(),
        1
    );

    drop(temporary);
}

#[test]
fn supervisor_final_delivery_failure_keeps_sqlite_and_filesystem_consistent() {
    let (temporary, store) = open_store();
    let mut workflow = workflow_with_tasks(&store, &[ExecutorKind::Command]);
    workflow.tasks[0].status = TaskStatus::Completed;
    store.save_workflow(&workflow).unwrap();
    let run = save_run(&store, &workflow, "accepted", |_| {});
    let fault_connection = Connection::open(store.path()).unwrap();
    fault_connection
        .execute_batch(
            r#"
            CREATE TRIGGER reject_supervised_final_delivery
            BEFORE INSERT ON events
            WHEN NEW.kind = 'final_delivery_package_created'
            BEGIN
                SELECT RAISE(ABORT, 'supervised final delivery rejected');
            END;
            "#,
        )
        .unwrap();

    let failed = supervise_request_once(&store, &run.run_id, &options()).unwrap();

    assert_eq!(failed.outcome, RequestSupervisorRunOutcome::Failed);
    assert!(failed
        .error
        .as_deref()
        .is_some_and(|error| error.contains("supervised final delivery rejected")));
    let current_run = load_run_record(&store, &run.run_id).unwrap();
    assert_eq!(current_run.status, "accepted");
    assert_eq!(
        current_run.supervisor_instance_id.as_deref(),
        Some("request-supervisor-test-instance")
    );
    assert_eq!(current_run.supervisor_fencing_token, 1);
    let current_workflow = store.load_workflow(&workflow.id).unwrap();
    assert_ne!(current_workflow.status, "completed");
    assert!(current_workflow
        .artifacts
        .iter()
        .all(|artifact| !artifact.kind.starts_with("final_delivery_package")));
    let artifact_dir = temporary.path().join("artifacts").join(&workflow.id);
    assert!(
        !artifact_dir.exists() || fs::read_dir(&artifact_dir).unwrap().next().is_none(),
        "failed supervised completion must not publish final delivery files"
    );
    let staging_root = temporary
        .path()
        .join("tmp")
        .join(&workflow.id)
        .join(".final-delivery-staging");
    assert!(
        !staging_root.exists() || fs::read_dir(&staging_root).unwrap().next().is_none(),
        "failed supervised completion must clean staged final delivery files"
    );

    fault_connection
        .execute_batch("DROP TRIGGER reject_supervised_final_delivery")
        .unwrap();
    let completed = supervise_request_once(&store, &run.run_id, &options()).unwrap();
    assert_eq!(completed.outcome, RequestSupervisorRunOutcome::Completed);
    assert_eq!(
        load_run_record(&store, &run.run_id).unwrap().status,
        "completed"
    );
    assert_eq!(
        store
            .load_workflow(&workflow.id)
            .unwrap()
            .artifacts
            .iter()
            .filter(|artifact| artifact.kind.starts_with("final_delivery_package"))
            .count(),
        2
    );
}
