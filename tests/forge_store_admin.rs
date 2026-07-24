use forge_core::store_admin::{backup_store, check_store, restore_store};
use rusqlite::Connection;
use tempfile::tempdir;

#[test]
fn backup_restore_round_trip_is_consistent_and_keeps_rollback() {
    let directory = tempdir().expect("tempdir");
    let store_path = directory.path().join("forge.sqlite");
    let backup_path = directory.path().join("forge.backup.sqlite");

    let connection = Connection::open(&store_path).expect("open source");
    connection
        .execute_batch(
            "
            PRAGMA journal_mode=WAL;
            PRAGMA user_version=3;
            CREATE TABLE evidence (id INTEGER PRIMARY KEY, value TEXT NOT NULL);
            INSERT INTO evidence (value) VALUES ('before');
            ",
        )
        .expect("seed source");
    drop(connection);

    let backup = backup_store(&store_path, &backup_path).expect("backup");
    assert_eq!(backup.status, "created");
    assert_eq!(backup.quick_check, vec!["ok"]);

    let connection = Connection::open(&store_path).expect("reopen source");
    connection
        .execute("UPDATE evidence SET value = 'after'", [])
        .expect("mutate source");
    drop(connection);

    let restored =
        restore_store(&store_path, &backup_path, "production-test", true).expect("restore");
    assert_eq!(restored.status, "restored");
    let rollback_path = restored
        .rollback_backup_path
        .as_ref()
        .expect("pre-restore rollback backup");
    assert!(std::path::Path::new(rollback_path).is_file());

    let connection = Connection::open(&store_path).expect("read restored");
    let value: String = connection
        .query_row("SELECT value FROM evidence", [], |row| row.get(0))
        .expect("restored value");
    assert_eq!(value, "before");

    let report = check_store(&store_path).expect("health report");
    assert!(report.healthy);
    assert_eq!(report.quick_check, vec!["ok"]);
}

#[test]
fn restore_requires_explicit_confirmation_and_operator() {
    let directory = tempdir().expect("tempdir");
    let store_path = directory.path().join("forge.sqlite");
    let backup_path = directory.path().join("backup.sqlite");
    Connection::open(&store_path).expect("source");
    backup_store(&store_path, &backup_path).expect("backup");

    let missing_confirmation =
        restore_store(&store_path, &backup_path, "operator", false).unwrap_err();
    assert!(missing_confirmation
        .to_string()
        .contains("--confirm-restore"));

    let missing_operator = restore_store(&store_path, &backup_path, "", true).unwrap_err();
    assert!(missing_operator.to_string().contains("--approved-by"));
}

#[cfg(unix)]
#[test]
fn backup_is_private_by_default() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempdir().expect("tempdir");
    let store_path = directory.path().join("forge.sqlite");
    let backup_directory = directory.path().join("backups");
    let backup_path = backup_directory.join("forge.sqlite");
    Connection::open(&store_path).expect("source");

    backup_store(&store_path, &backup_path).expect("backup");

    assert_eq!(
        std::fs::metadata(&backup_path)
            .expect("backup metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(&backup_directory)
            .expect("directory metadata")
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
}
