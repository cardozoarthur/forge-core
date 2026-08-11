use foundry_core::graph::create_workflow;
use foundry_core::intent::parse_intent;
use foundry_core::storage::FoundryStore;
use rusqlite::Connection;
use serde_json::Value;
use tempfile::tempdir;

#[test]
fn v4_upgrade_preserves_data_and_installs_execution_reconciliation_guards() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("foundry-v4.sqlite");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            r#"
            CREATE TABLE workflows (
                id TEXT PRIMARY KEY,
                goal TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                data_json TEXT NOT NULL
            );
            CREATE TABLE events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                workflow_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                data_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            INSERT INTO workflows (id, goal, status, created_at, data_json)
            VALUES (
                'wf-preserved',
                'preserve migration data',
                'running',
                '2026-07-24T00:00:00Z',
                '{"sentinel":"workflow"}'
            );
            INSERT INTO events (workflow_id, kind, data_json, created_at)
            VALUES (
                'wf-preserved',
                'migration.sentinel',
                '{"sentinel":"event"}',
                '2026-07-24T00:00:00Z'
            );
            PRAGMA user_version = 4;
            "#,
        )
        .unwrap();
    drop(connection);

    let store = FoundryStore::open(&path).unwrap();
    let connection = Connection::open(store.path()).unwrap();
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 6);

    let workflow_sentinel: String = connection
        .query_row(
            "SELECT data_json FROM workflows WHERE id='wf-preserved'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(workflow_sentinel, r#"{"sentinel":"workflow"}"#);
    let event_sentinel: String = connection
        .query_row(
            "SELECT data_json FROM events WHERE kind='migration.sentinel'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(event_sentinel, r#"{"sentinel":"event"}"#);

    let mut columns = connection
        .prepare("PRAGMA table_info(mission_execution_receipts)")
        .unwrap();
    let columns = columns
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for required in [
        "mission_revision",
        "execution_started_at",
        "receipt_sha256",
        "receipt_json",
        "consumed_at",
        "consumed_by_submission",
    ] {
        assert!(
            columns.iter().any(|column| column == required),
            "v5 execution receipt schema must include {required}"
        );
    }

    let assignment_guard: String = connection
        .query_row(
            r#"
            SELECT sql
            FROM sqlite_master
            WHERE type='index'
              AND name='idx_mission_execution_receipts_assignment_guard'
            "#,
            [],
            |row| row.get(0),
        )
        .unwrap();
    let normalized = assignment_guard.to_ascii_lowercase();
    assert!(normalized.contains("create unique index"));
    for protected_state in ["failed", "timed_out", "indeterminate", "completed"] {
        assert!(
            normalized.contains(protected_state),
            "assignment guard must protect {protected_state}"
        );
    }

    let reconciliation_table_exists: bool = connection
        .query_row(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM sqlite_master
                WHERE type='table' AND name='mission_execution_reconciliations'
            )
            "#,
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(reconciliation_table_exists);
}

#[test]
fn v4_upgrade_rejects_inline_research_without_partial_schema_mutation() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("foundry-v4-inline-research.sqlite");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            r#"
            CREATE TABLE workflows (
                id TEXT PRIMARY KEY,
                goal TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                data_json TEXT NOT NULL
            );
            CREATE TABLE events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                workflow_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                data_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            INSERT INTO workflows (id, goal, status, created_at, data_json)
            VALUES (
                'wf-inline-research',
                'preserve unsupported research data',
                'running',
                '2026-07-24T00:00:00Z',
                '{"gate_decisions":[{"id":"unreleased-record"}]}'
            );
            PRAGMA user_version = 4;
            "#,
        )
        .unwrap();
    drop(connection);

    let error = match FoundryStore::open(&path) {
        Ok(_) => panic!("inline research telemetry must fail closed during migration"),
        Err(error) => error.to_string(),
    };
    assert!(error.contains("unsupported inline research telemetry"));

    let connection = Connection::open(&path).unwrap();
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 4);
    let research_table_exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workflow_research_records')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!research_table_exists);
}

#[test]
fn v5_upgrade_loads_a_real_research_free_workflow() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("foundry-v5-real-workflow.sqlite");
    let workflow = create_workflow(parse_intent("Preserve a real v5 workflow"));
    let workflow_id = workflow.id.clone();
    let store = FoundryStore::open(&path).unwrap();
    store.save_workflow(&workflow).unwrap();
    drop(store);

    let connection = Connection::open(&path).unwrap();
    let data_json: String = connection
        .query_row(
            "SELECT data_json FROM workflows WHERE id=?1",
            [&workflow_id],
            |row| row.get(0),
        )
        .unwrap();
    let mut snapshot: Value = serde_json::from_str(&data_json).unwrap();
    let snapshot = snapshot.as_object_mut().unwrap();
    for field in [
        "value_contract",
        "experiment",
        "gate_decisions",
        "outcomes",
        "research_revisions",
    ] {
        snapshot.remove(field);
    }
    connection
        .execute(
            "UPDATE workflows SET data_json=?1 WHERE id=?2",
            [
                serde_json::to_string(snapshot).unwrap(),
                workflow_id.clone(),
            ],
        )
        .unwrap();
    connection
        .execute_batch(
            r#"
            DROP TABLE workflow_research_records;
            DROP TABLE executor_runtime_dispatch_permits;
            PRAGMA user_version = 5;
            "#,
        )
        .unwrap();
    drop(connection);

    let store = FoundryStore::open(&path).unwrap();
    let operational = store.load_workflow(&workflow_id).unwrap();
    assert_eq!(operational.id, workflow_id);
    assert!(operational.value_contract.is_none());
    assert!(operational.experiment.is_none());
    assert!(operational.gate_decisions.is_empty());
    assert!(operational.outcomes.is_empty());
    assert!(operational.research_revisions.is_empty());

    let hydrated = store.load_workflow_with_research(&workflow_id).unwrap();
    assert_eq!(
        serde_json::to_value(&hydrated).unwrap(),
        serde_json::to_value(&operational).unwrap()
    );
}
