use forge_core::store_admin::{backup_store, check_store, restore_store};
use rusqlite::Connection;
use tempfile::tempdir;

#[cfg(target_os = "linux")]
use serde_json::Value;
#[cfg(target_os = "linux")]
use std::ffi::OsString;
#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "linux")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::process::{Command, Output};

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

#[cfg(target_os = "linux")]
#[test]
fn checkpointed_wal_check_and_backup_work_on_read_only_mount_with_encoded_path() {
    let directory = tempdir().expect("tempdir");
    let source_directory = directory.path().join("source ?#%");
    let backup_directory = directory.path().join("backups");
    fs::create_dir_all(&source_directory).expect("source directory");
    fs::create_dir_all(&backup_directory).expect("backup directory");
    let store_path = source_directory.join("forge ?#%.sqlite");
    let backup_path = backup_directory.join("forge.sqlite");
    seed_checkpointed_wal(&store_path, "checkpointed");

    for suffix in ["-wal", "-shm", "-journal"] {
        let sidecar = sqlite_sidecar(&store_path, suffix);
        match fs::remove_file(&sidecar) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => panic!("remove {}: {error}", sidecar.display()),
        }
        assert!(!sidecar.exists(), "{} must be absent", sidecar.display());
    }

    let write_probe = source_directory.join("read-only-probe");
    let probe = run_read_only_root(
        Path::new("/usr/bin/touch"),
        &[write_probe.as_os_str().to_owned()],
        &backup_directory,
    );
    assert!(
        !probe.status.success(),
        "read-only mount probe unexpectedly succeeded"
    );
    assert!(!write_probe.exists());

    let source_before = fs::read(&store_path).expect("source bytes");
    let check = run_forge_store_command(&store_path, &["check"], None, &backup_directory);
    assert_success(&check, "checkpointed read-only store check");
    let check_json: Value = serde_json::from_slice(&check.stdout).expect("check JSON");
    assert_eq!(check_json["healthy"], true);
    assert_eq!(check_json["journal_mode"], "wal");

    let backup = run_forge_store_command(
        &store_path,
        &["backup"],
        Some(&backup_path),
        &backup_directory,
    );
    assert_success(&backup, "checkpointed read-only store backup");
    let backup_json: Value = serde_json::from_slice(&backup.stdout).expect("backup JSON");
    assert_eq!(backup_json["status"], "created");
    assert_eq!(
        query_marker(&backup_path),
        "checkpointed",
        "immutable fallback omitted checkpointed content"
    );
    assert_eq!(
        fs::read(&store_path).expect("source bytes after backup"),
        source_before,
        "read-only backup changed the source database"
    );
    for suffix in ["-wal", "-shm", "-journal"] {
        assert!(
            !sqlite_sidecar(&store_path, suffix).exists(),
            "immutable fallback created a source sidecar"
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn active_wal_backup_on_read_only_mount_includes_uncheckpointed_commit() {
    let directory = tempdir().expect("tempdir");
    let source_directory = directory.path().join("source");
    let backup_directory = directory.path().join("backups");
    fs::create_dir_all(&source_directory).expect("source directory");
    fs::create_dir_all(&backup_directory).expect("backup directory");
    let store_path = source_directory.join("forge.sqlite");
    let backup_path = backup_directory.join("forge.sqlite");

    let writer = Connection::open(&store_path).expect("writer");
    writer
        .execute_batch(
            "
            PRAGMA journal_mode=WAL;
            PRAGMA wal_autocheckpoint=0;
            CREATE TABLE evidence (value TEXT NOT NULL);
            PRAGMA wal_checkpoint(TRUNCATE);
            INSERT INTO evidence (value) VALUES ('active-wal');
            ",
        )
        .expect("seed active WAL");
    assert!(sqlite_sidecar(&store_path, "-wal").is_file());
    assert!(sqlite_sidecar(&store_path, "-shm").is_file());

    let backup = run_forge_store_command(
        &store_path,
        &["backup"],
        Some(&backup_path),
        &backup_directory,
    );
    assert_success(&backup, "active WAL read-only store backup");
    assert_eq!(
        query_marker(&backup_path),
        "active-wal",
        "online backup omitted a transaction present only in WAL"
    );
    drop(writer);
}

#[cfg(target_os = "linux")]
#[test]
fn wal_without_shm_fails_closed_on_read_only_mount() {
    let directory = tempdir().expect("tempdir");
    let live_directory = directory.path().join("live");
    let fixture_directory = directory.path().join("fixture");
    let backup_directory = directory.path().join("backups");
    fs::create_dir_all(&live_directory).expect("live directory");
    fs::create_dir_all(&fixture_directory).expect("fixture directory");
    fs::create_dir_all(&backup_directory).expect("backup directory");
    let live_store = live_directory.join("forge.sqlite");
    let fixture_store = fixture_directory.join("forge.sqlite");
    let backup_path = backup_directory.join("must-not-exist.sqlite");

    let writer = Connection::open(&live_store).expect("writer");
    writer
        .execute_batch(
            "
            PRAGMA journal_mode=WAL;
            PRAGMA wal_autocheckpoint=0;
            CREATE TABLE evidence (value TEXT NOT NULL);
            PRAGMA wal_checkpoint(TRUNCATE);
            INSERT INTO evidence (value) VALUES ('uncheckpointed');
            ",
        )
        .expect("seed active WAL");
    fs::copy(&live_store, &fixture_store).expect("copy main database");
    fs::copy(
        sqlite_sidecar(&live_store, "-wal"),
        sqlite_sidecar(&fixture_store, "-wal"),
    )
    .expect("copy WAL");
    assert!(!sqlite_sidecar(&fixture_store, "-shm").exists());

    let check = run_forge_store_command(&fixture_store, &["check"], None, &backup_directory);
    assert!(
        !check.status.success(),
        "store check ignored a WAL without shared memory"
    );
    assert_output_contains(
        &check,
        "refusing immutable SQLite fallback because sidecar exists",
    );

    let backup = run_forge_store_command(
        &fixture_store,
        &["backup"],
        Some(&backup_path),
        &backup_directory,
    );
    assert!(
        !backup.status.success(),
        "store backup ignored a WAL without shared memory"
    );
    assert_output_contains(
        &backup,
        "refusing immutable SQLite fallback because sidecar exists",
    );
    assert!(
        !backup_path.exists(),
        "failed backup published a destination"
    );
    drop(writer);
}

#[cfg(target_os = "linux")]
fn seed_checkpointed_wal(path: &Path, marker: &str) {
    let connection = Connection::open(path).expect("checkpointed writer");
    connection
        .execute_batch(
            "
            PRAGMA journal_mode=WAL;
            PRAGMA wal_autocheckpoint=0;
            CREATE TABLE evidence (value TEXT NOT NULL);
            ",
        )
        .expect("create checkpointed WAL fixture");
    connection
        .execute("INSERT INTO evidence (value) VALUES (?1)", [marker])
        .expect("insert checkpointed marker");
    connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .expect("checkpoint fixture");
}

#[cfg(target_os = "linux")]
fn query_marker(path: &Path) -> String {
    Connection::open(path)
        .expect("open backup")
        .query_row("SELECT value FROM evidence LIMIT 1", [], |row| row.get(0))
        .expect("query backup marker")
}

#[cfg(target_os = "linux")]
fn sqlite_sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut sidecar = path.as_os_str().to_os_string();
    sidecar.push(suffix);
    PathBuf::from(sidecar)
}

#[cfg(target_os = "linux")]
fn run_forge_store_command(
    store: &Path,
    action: &[&str],
    destination: Option<&Path>,
    writable_directory: &Path,
) -> Output {
    let binary = assert_cmd::cargo::cargo_bin("forge");
    let mut arguments = vec![
        OsString::from("--store"),
        store.as_os_str().to_owned(),
        OsString::from("store"),
    ];
    arguments.extend(action.iter().map(OsString::from));
    if let Some(destination) = destination {
        arguments.push(OsString::from("--destination"));
        arguments.push(destination.as_os_str().to_owned());
    }
    arguments.push(OsString::from("--output"));
    arguments.push(OsString::from("json"));
    run_read_only_root(&binary, &arguments, writable_directory)
}

#[cfg(target_os = "linux")]
fn run_read_only_root(program: &Path, arguments: &[OsString], writable_directory: &Path) -> Output {
    Command::new("bwrap")
        .arg("--die-with-parent")
        .arg("--ro-bind")
        .arg("/")
        .arg("/")
        .arg("--bind")
        .arg(writable_directory)
        .arg(writable_directory)
        .arg("--setenv")
        .arg("TMPDIR")
        .arg(writable_directory)
        .arg("--")
        .arg(program)
        .args(arguments)
        .output()
        .expect("run command in read-only bubblewrap mount")
}

#[cfg(target_os = "linux")]
fn assert_success(output: &Output, operation: &str) {
    assert!(
        output.status.success(),
        "{operation} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(target_os = "linux")]
fn assert_output_contains(output: &Output, expected: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains(expected) || stderr.contains(expected),
        "missing `{expected}`\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
