use chrono::{Duration, Utc};
use forge_core::schedule::{
    create_daily_goal_research_workflow, run_daily_goal_research_smoke, run_due_workflow,
    scan_due_workflows, update_loop_state, update_workflow_schedule, ScheduleUpdateOptions,
};
use forge_core::storage::ForgeStore;
use rusqlite::Connection;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration as StdDuration;
use tempfile::tempdir;

fn files_below(path: &Path) -> Vec<PathBuf> {
    if !path.exists() {
        return Vec::new();
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(files_below(&path));
        } else {
            files.push(path);
        }
    }
    files.sort();
    files
}

#[test]
fn schedule_creation_rolls_back_workflow_when_event_persistence_fails() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let store = ForgeStore::open(&store_path).unwrap();
    let connection = Connection::open(&store_path).unwrap();
    connection
        .execute_batch(
            r#"
            CREATE TRIGGER reject_schedule_creation_event
            BEFORE INSERT ON events
            WHEN NEW.kind = 'daily_goal_research_workflow_created'
            BEGIN
                SELECT RAISE(ABORT, 'injected schedule creation event failure');
            END;
            "#,
        )
        .unwrap();
    drop(connection);

    let error = create_daily_goal_research_workflow(
        &store,
        vec!["Atomic schedule creation".to_string()],
        "UTC",
        "0 9 * * *",
        "atomicity-test",
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("injected schedule creation event failure"),
        "{error:#}"
    );
    assert!(store.load_workflows().unwrap().is_empty());
}

#[test]
fn due_schedule_rolls_back_workflow_when_event_persistence_fails() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let store = ForgeStore::open(&store_path).unwrap();

    let created = create_daily_goal_research_workflow(
        &store,
        vec!["Atomic schedule evidence".to_string()],
        "UTC",
        "0 9 * * *",
        "atomicity-test",
    )
    .unwrap();
    let mut workflow = store.load_workflow(&created.workflow_id).unwrap();
    let due_at = Utc::now() - Duration::seconds(1);
    for schedule in workflow
        .tasks
        .iter_mut()
        .filter_map(|task| task.schedule.as_mut())
    {
        schedule.next_run_at = Some(due_at);
        schedule.missed_run_policy = "run_latest".to_string();
    }
    store.save_workflow(&workflow).unwrap();
    let expected = serde_json::to_value(&workflow).unwrap();
    let artifact_root = temp.path().join("artifacts").join(&workflow.id);
    fs::create_dir_all(&artifact_root).unwrap();
    let sentinel = artifact_root.join("previous-artifact.txt");
    fs::write(&sentinel, b"previous committed bytes").unwrap();
    let orphan = artifact_root.join("schedule-run-run_orphan--goal-stale-worker-report.md");
    fs::write(&orphan, b"uncommitted crash residue").unwrap();

    let connection = Connection::open(&store_path).unwrap();
    connection
        .execute_batch(
            r#"
            CREATE TRIGGER reject_due_workflow_event
            BEFORE INSERT ON events
            WHEN NEW.kind = 'due_workflow_executed'
            BEGIN
                SELECT RAISE(ABORT, 'injected due workflow event failure');
            END;
            "#,
        )
        .unwrap();
    drop(connection);

    let error = run_due_workflow(&store, &workflow.id).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("injected due workflow event failure"),
        "{error:#}"
    );
    let persisted = store.load_workflow(&workflow.id).unwrap();
    assert_eq!(serde_json::to_value(persisted).unwrap(), expected);
    assert_eq!(
        fs::read(&sentinel).unwrap(),
        b"previous committed bytes",
        "a failed schedule transaction must preserve previous artifacts"
    );
    assert_eq!(
        files_below(&artifact_root),
        vec![sentinel],
        "new schedule artifacts must be compensated when SQLite rolls back"
    );
}

#[test]
fn failed_due_schedule_releases_its_lease_and_allows_immediate_retry() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let store = ForgeStore::open(&store_path).unwrap();

    let created = create_daily_goal_research_workflow(
        &store,
        vec!["Retry failed schedule without waiting for TTL".to_string()],
        "UTC",
        "0 9 * * *",
        "atomicity-test",
    )
    .unwrap();
    let mut workflow = store.load_workflow(&created.workflow_id).unwrap();
    let task_id = workflow
        .tasks
        .iter_mut()
        .find_map(|task| {
            task.schedule.as_mut().map(|schedule| {
                schedule.next_run_at = Some(Utc::now() - Duration::seconds(1));
                task.id.clone()
            })
        })
        .unwrap();
    store.save_workflow(&workflow).unwrap();

    let connection = Connection::open(&store_path).unwrap();
    connection
        .execute_batch(
            r#"
            CREATE TRIGGER reject_leased_due_workflow_event
            BEFORE INSERT ON events
            WHEN NEW.kind = 'due_workflow_executed'
            BEGIN
                SELECT RAISE(ABORT, 'injected leased due workflow event failure');
            END;
            "#,
        )
        .unwrap();
    drop(connection);

    let error = scan_due_workflows(&store, "atomicity-worker", 300).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("injected leased due workflow event failure"),
        "{error:#}"
    );
    assert!(
        store
            .load_task_lease(&created.workflow_id, &task_id)
            .unwrap()
            .is_none(),
        "a rolled-back due workflow must release the exact lease it acquired"
    );

    let connection = Connection::open(&store_path).unwrap();
    connection
        .execute_batch("DROP TRIGGER reject_leased_due_workflow_event")
        .unwrap();
    drop(connection);

    let retry = scan_due_workflows(&store, "retry-worker", 300).unwrap();
    assert_eq!(retry.summary.executed_workflows, 1);
    assert_eq!(retry.summary.lease_conflicts, 0);
    assert!(
        store
            .load_task_lease(&created.workflow_id, &task_id)
            .unwrap()
            .is_none(),
        "the successful immediate retry must also release its lease"
    );
}

#[test]
fn concurrent_schedule_updates_keep_both_revisions_and_fields() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let store = ForgeStore::open(&store_path).unwrap();
    let created = create_daily_goal_research_workflow(
        &store,
        vec!["Concurrent schedule update".to_string()],
        "UTC",
        "0 9 * * *",
        "atomicity-test",
    )
    .unwrap();
    let before = store.load_workflow(&created.workflow_id).unwrap();
    let task_id = before
        .tasks
        .iter()
        .find(|task| task.schedule.is_some())
        .unwrap()
        .id
        .clone();
    let revision_count = before.revisions.len();
    let first_store = ForgeStore::open(&store_path).unwrap();
    let second_store = ForgeStore::open(&store_path).unwrap();
    let blocker = Connection::open(&store_path).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
    let barrier = Arc::new(Barrier::new(3));

    let first_barrier = Arc::clone(&barrier);
    let first_workflow_id = created.workflow_id.clone();
    let first_task_id = task_id.clone();
    let first = thread::spawn(move || {
        first_barrier.wait();
        update_workflow_schedule(
            &first_store,
            &first_workflow_id,
            &first_task_id,
            ScheduleUpdateOptions {
                cron: Some("5 10 * * *"),
                timezone: None,
                missed_run_policy: None,
                next_run_at: None,
                origin: "concurrent-cron",
            },
        )
    });

    let second_barrier = Arc::clone(&barrier);
    let second_workflow_id = created.workflow_id.clone();
    let second_task_id = task_id.clone();
    let second = thread::spawn(move || {
        second_barrier.wait();
        update_workflow_schedule(
            &second_store,
            &second_workflow_id,
            &second_task_id,
            ScheduleUpdateOptions {
                cron: None,
                timezone: Some("America/Sao_Paulo"),
                missed_run_policy: None,
                next_run_at: None,
                origin: "concurrent-timezone",
            },
        )
    });

    barrier.wait();
    thread::sleep(StdDuration::from_millis(75));
    blocker.execute_batch("COMMIT").unwrap();
    first.join().unwrap().unwrap();
    second.join().unwrap().unwrap();

    let persisted = store.load_workflow(&created.workflow_id).unwrap();
    let schedule = persisted
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .unwrap()
        .schedule
        .as_ref()
        .unwrap();
    assert_eq!(schedule.cron, "5 10 * * *");
    assert_eq!(schedule.timezone, "America/Sao_Paulo");
    assert_eq!(persisted.revisions.len(), revision_count + 2);
}

#[test]
fn concurrent_loop_updates_keep_both_revisions() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let store = ForgeStore::open(&store_path).unwrap();
    let created = create_daily_goal_research_workflow(
        &store,
        vec!["Concurrent loop update".to_string()],
        "UTC",
        "0 9 * * *",
        "atomicity-test",
    )
    .unwrap();
    let before = store.load_workflow(&created.workflow_id).unwrap();
    let task_id = before
        .tasks
        .iter()
        .find(|task| task.loop_control.is_some())
        .unwrap()
        .id
        .clone();
    let revision_count = before.revisions.len();
    let first_store = ForgeStore::open(&store_path).unwrap();
    let second_store = ForgeStore::open(&store_path).unwrap();
    let blocker = Connection::open(&store_path).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
    let barrier = Arc::new(Barrier::new(3));

    let first_barrier = Arc::clone(&barrier);
    let first_workflow_id = created.workflow_id.clone();
    let first_task_id = task_id.clone();
    let first = thread::spawn(move || {
        first_barrier.wait();
        update_loop_state(
            &first_store,
            &first_workflow_id,
            &first_task_id,
            "paused",
            "concurrent-paused",
        )
    });

    let second_barrier = Arc::clone(&barrier);
    let second_workflow_id = created.workflow_id.clone();
    let second_task_id = task_id.clone();
    let second = thread::spawn(move || {
        second_barrier.wait();
        update_loop_state(
            &second_store,
            &second_workflow_id,
            &second_task_id,
            "stopped",
            "concurrent-stopped",
        )
    });

    barrier.wait();
    thread::sleep(StdDuration::from_millis(75));
    blocker.execute_batch("COMMIT").unwrap();
    first.join().unwrap().unwrap();
    second.join().unwrap().unwrap();

    let persisted = store.load_workflow(&created.workflow_id).unwrap();
    let loop_state = &persisted
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .unwrap()
        .loop_control
        .as_ref()
        .unwrap()
        .state;
    assert!(matches!(loop_state.as_str(), "paused" | "stopped"));
    assert_eq!(persisted.revisions.len(), revision_count + 2);
}

#[test]
fn unsafe_goal_components_are_encoded_and_cannot_escape_artifact_root() {
    let temp = tempdir().unwrap();
    let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();
    let created = create_daily_goal_research_workflow(
        &store,
        vec![
            "../../../../sentinel".to_string(),
            "/absolute/goal".to_string(),
            "nested/goal".to_string(),
            r"..\windows\goal".to_string(),
        ],
        "UTC",
        "0 9 * * *",
        "path-safety-test",
    )
    .unwrap();
    let mut workflow = created.workflow;
    let sentinel = temp.path().join("sentinel-report.md");
    fs::write(&sentinel, b"external sentinel").unwrap();

    let report = run_daily_goal_research_smoke(&store, &mut workflow)
        .unwrap()
        .unwrap();

    assert_eq!(fs::read(&sentinel).unwrap(), b"external sentinel");
    let artifact_root = fs::canonicalize(temp.path().join("artifacts")).unwrap();
    for goal in &report.goals {
        for relative in [
            goal.markdown_path.as_str(),
            goal.pdf_path.as_str(),
            goal.telegram_delivery.markdown_path.as_str(),
            goal.telegram_delivery.pdf_path.as_str(),
        ] {
            let path = Path::new(relative);
            assert!(!path.is_absolute(), "{relative}");
            assert!(
                path.components()
                    .all(|component| matches!(component, Component::Normal(_))),
                "{relative}"
            );
            if goal.goal != "hackathon" {
                assert!(relative.contains("sha256-"), "{relative}");
            }
            let canonical = fs::canonicalize(temp.path().join(path)).unwrap();
            assert!(canonical.starts_with(&artifact_root), "{relative}");
        }
    }
}

#[cfg(unix)]
#[test]
fn atomic_replace_rejects_symlinked_workflow_directory_without_touching_external_files() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();
    let created = create_daily_goal_research_workflow(
        &store,
        vec!["safe-goal".to_string()],
        "UTC",
        "0 9 * * *",
        "path-safety-test",
    )
    .unwrap();
    let mut workflow = created.workflow;
    let external = temp.path().join("external");
    let artifact_root = temp.path().join("artifacts");
    fs::create_dir_all(&external).unwrap();
    fs::create_dir(&artifact_root).unwrap();
    let sentinel = external.join("sentinel");
    fs::write(&sentinel, b"external sentinel").unwrap();
    symlink(&external, artifact_root.join(&workflow.id)).unwrap();

    let error = run_daily_goal_research_smoke(&store, &mut workflow).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("artifact batch was not committed"),
        "{error:#}"
    );
    assert_eq!(fs::read(&sentinel).unwrap(), b"external sentinel");
    assert_eq!(files_below(&external), vec![sentinel]);
}

#[cfg(unix)]
#[test]
fn due_no_overwrite_rejects_symlinked_workflow_directory_without_touching_external_files() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();
    let created = create_daily_goal_research_workflow(
        &store,
        vec!["safe-goal".to_string()],
        "UTC",
        "0 9 * * *",
        "path-safety-test",
    )
    .unwrap();
    let mut workflow = store.load_workflow(&created.workflow_id).unwrap();
    for schedule in workflow
        .tasks
        .iter_mut()
        .filter_map(|task| task.schedule.as_mut())
    {
        schedule.next_run_at = Some(Utc::now() - Duration::seconds(1));
    }
    store.save_workflow(&workflow).unwrap();

    let external = temp.path().join("external");
    let artifact_root = temp.path().join("artifacts");
    fs::create_dir_all(&external).unwrap();
    fs::create_dir(&artifact_root).unwrap();
    let sentinel = external.join("sentinel");
    fs::write(&sentinel, b"external sentinel").unwrap();
    symlink(&external, artifact_root.join(&workflow.id)).unwrap();

    let error = run_due_workflow(&store, &workflow.id).unwrap_err();
    assert!(error.to_string().contains("symlink"), "{error:#}");
    assert_eq!(fs::read(&sentinel).unwrap(), b"external sentinel");
    assert_eq!(files_below(&external), vec![sentinel]);
}
