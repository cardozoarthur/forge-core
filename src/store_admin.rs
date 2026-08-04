use anyhow::{bail, Context, Result};
use chrono::Utc;
use rusqlite::backup::Backup;
use rusqlite::{Connection, Error as RusqliteError, ErrorCode, OpenFlags};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;

const STORE_CHECK_SCHEMA: &str = "forge.store.check.v1";
const STORE_BACKUP_SCHEMA: &str = "forge.store.backup.v1";
const STORE_RESTORE_SCHEMA: &str = "forge.store.restore.v1";

#[derive(Debug, Clone, Serialize)]
pub struct StoreCheckReport {
    pub schema_version: &'static str,
    pub status: &'static str,
    pub healthy: bool,
    pub store_path: String,
    pub bytes: u64,
    pub sha256: String,
    pub sqlite_user_version: i64,
    pub journal_mode: String,
    pub page_count: i64,
    pub freelist_count: i64,
    pub quick_check: Vec<String>,
    pub unix_mode: Option<u32>,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoreBackupReport {
    pub schema_version: &'static str,
    pub status: &'static str,
    pub source_path: String,
    pub backup_path: String,
    pub bytes: u64,
    pub sha256: String,
    pub sqlite_user_version: i64,
    pub quick_check: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoreRestoreReport {
    pub schema_version: &'static str,
    pub status: &'static str,
    pub store_path: String,
    pub source_path: String,
    pub rollback_backup_path: Option<String>,
    pub bytes: u64,
    pub sha256: String,
    pub sqlite_user_version: i64,
    pub approved_by: String,
    pub restored_at: String,
}

pub fn check_store(path: impl AsRef<Path>) -> Result<StoreCheckReport> {
    let path = path.as_ref();
    reject_symlink(path)?;
    let metadata = fs::metadata(path)
        .with_context(|| format!("failed to inspect SQLite store {}", path.display()))?;
    if !metadata.is_file() {
        bail!("SQLite store is not a regular file: {}", path.display());
    }

    let connection = open_read_only(path)?;
    let quick_check = quick_check(&connection)?;
    let healthy = quick_check.len() == 1 && quick_check[0] == "ok";
    let status = if healthy { "healthy" } else { "corrupt" };
    let sqlite_user_version = pragma_i64(&connection, "user_version")?;
    let page_count = pragma_i64(&connection, "page_count")?;
    let freelist_count = pragma_i64(&connection, "freelist_count")?;
    let journal_mode = journal_mode(&connection, path)?;

    Ok(StoreCheckReport {
        schema_version: STORE_CHECK_SCHEMA,
        status,
        healthy,
        store_path: absolute_display(path)?,
        bytes: metadata.len(),
        sha256: sha256_file(path)?,
        sqlite_user_version,
        journal_mode,
        page_count,
        freelist_count,
        quick_check,
        unix_mode: unix_mode(&metadata),
        checked_at: Utc::now().to_rfc3339(),
    })
}

pub fn backup_store(
    source_path: impl AsRef<Path>,
    backup_path: impl AsRef<Path>,
) -> Result<StoreBackupReport> {
    let source_path = source_path.as_ref();
    let backup_path = backup_path.as_ref();
    reject_symlink(source_path)?;
    if backup_path.exists() {
        bail!(
            "backup destination already exists; choose a new path: {}",
            backup_path.display()
        );
    }
    reject_same_path(source_path, backup_path)?;
    prepare_private_parent(backup_path)?;

    let temporary_path = temporary_sibling(backup_path, "backup");
    let copy_result = copy_database(source_path, &temporary_path);
    if let Err(error) = copy_result {
        remove_temporary_file(&temporary_path);
        return Err(error);
    }

    set_private_file_permissions(&temporary_path)?;
    sync_file(&temporary_path)?;
    let report = check_store(&temporary_path)?;
    if !report.healthy {
        remove_temporary_file(&temporary_path);
        bail!("new SQLite backup failed quick_check");
    }
    fs::rename(&temporary_path, backup_path)
        .with_context(|| format!("failed to publish SQLite backup {}", backup_path.display()))?;
    sync_parent(backup_path)?;

    Ok(StoreBackupReport {
        schema_version: STORE_BACKUP_SCHEMA,
        status: "created",
        source_path: absolute_display(source_path)?,
        backup_path: absolute_display(backup_path)?,
        bytes: fs::metadata(backup_path)?.len(),
        sha256: sha256_file(backup_path)?,
        sqlite_user_version: report.sqlite_user_version,
        quick_check: report.quick_check,
        created_at: Utc::now().to_rfc3339(),
    })
}

pub fn restore_store(
    store_path: impl AsRef<Path>,
    source_path: impl AsRef<Path>,
    approved_by: &str,
    confirm_restore: bool,
) -> Result<StoreRestoreReport> {
    let store_path = store_path.as_ref();
    let source_path = source_path.as_ref();
    if approved_by.trim().is_empty() {
        bail!("--approved-by must identify the operator authorizing restore");
    }
    if !confirm_restore {
        bail!("restore requires --confirm-restore");
    }
    reject_symlink(source_path)?;
    if store_path.exists() {
        reject_symlink(store_path)?;
    }
    reject_same_path(store_path, source_path)?;

    let source_check = check_store(source_path)?;
    if !source_check.healthy {
        bail!("restore source failed SQLite quick_check");
    }
    prepare_private_parent(store_path)?;

    let rollback_backup_path = if store_path.exists() {
        let path = rollback_sibling(store_path);
        backup_store(store_path, &path)?;
        Some(path)
    } else {
        None
    };

    if let Err(error) = copy_database(source_path, store_path) {
        if let Some(rollback_path) = rollback_backup_path.as_ref() {
            let _ = copy_database(rollback_path, store_path);
        }
        return Err(error).context("failed to restore SQLite store");
    }
    set_private_file_permissions(store_path)?;
    sync_file(store_path)?;
    sync_parent(store_path)?;

    let restored = check_store(store_path)?;
    if !restored.healthy {
        if let Some(rollback_path) = rollback_backup_path.as_ref() {
            copy_database(rollback_path, store_path)
                .context("restore failed and rollback backup could not be reapplied")?;
        }
        bail!("restored SQLite store failed quick_check; rollback reapplied");
    }

    Ok(StoreRestoreReport {
        schema_version: STORE_RESTORE_SCHEMA,
        status: "restored",
        store_path: absolute_display(store_path)?,
        source_path: absolute_display(source_path)?,
        rollback_backup_path: rollback_backup_path
            .as_deref()
            .map(absolute_display)
            .transpose()?,
        bytes: restored.bytes,
        sha256: restored.sha256,
        sqlite_user_version: restored.sqlite_user_version,
        approved_by: approved_by.trim().to_owned(),
        restored_at: Utc::now().to_rfc3339(),
    })
}

fn copy_database(source_path: &Path, destination_path: &Path) -> Result<()> {
    let source = open_read_only(source_path)?;
    create_private_file_if_missing(destination_path)?;
    let mut destination = Connection::open_with_flags(
        destination_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| {
        format!(
            "failed to open SQLite backup destination {}",
            destination_path.display()
        )
    })?;
    destination.busy_timeout(Duration::from_secs(30))?;
    destination.pragma_update(None, "synchronous", "FULL")?;
    {
        let backup = Backup::new(&source, &mut destination)
            .context("failed to initialize SQLite online backup")?;
        backup
            .run_to_completion(256, Duration::from_millis(10), None)
            .context("SQLite online backup did not complete")?;
    }
    let quick_check = quick_check(&destination)?;
    if quick_check.len() != 1 || quick_check[0] != "ok" {
        bail!("copied SQLite database failed quick_check");
    }
    destination
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .context("failed to checkpoint copied SQLite database")?;
    Ok(())
}

fn open_read_only(path: &Path) -> Result<Connection> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("failed to open SQLite store {}", path.display()))?;
    connection.busy_timeout(Duration::from_secs(30))?;
    match initialize_read_only_connection(&connection) {
        Ok(()) => Ok(connection),
        Err(error) if is_read_only_wal_initialization_error(&error) => {
            drop(connection);
            if let Some(sidecar) = first_existing_sqlite_sidecar(path)? {
                return Err(error).with_context(|| {
                    format!(
                        "refusing immutable SQLite fallback because sidecar exists: {}",
                        sidecar.display()
                    )
                });
            }
            open_immutable_read_only(path)
        }
        Err(error) => Err(error)
            .with_context(|| format!("failed to initialize SQLite store {}", path.display())),
    }
}

fn initialize_read_only_connection(connection: &Connection) -> rusqlite::Result<()> {
    connection
        .pragma_query_value(None, "schema_version", |row| row.get::<_, i64>(0))
        .map(|_| ())
}

fn is_read_only_wal_initialization_error(error: &RusqliteError) -> bool {
    matches!(
        error,
        RusqliteError::SqliteFailure(details, _)
            if details.code == ErrorCode::CannotOpen
                || details.extended_code == rusqlite::ffi::SQLITE_READONLY_CANTINIT
    )
}

fn first_existing_sqlite_sidecar(path: &Path) -> Result<Option<PathBuf>> {
    let absolute = absolute_candidate(path)?;
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut candidate = absolute.as_os_str().to_os_string();
        candidate.push(suffix);
        let candidate = PathBuf::from(candidate);
        match fs::symlink_metadata(&candidate) {
            Ok(_) => return Ok(Some(candidate)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to inspect SQLite sidecar {}", candidate.display())
                });
            }
        }
    }
    Ok(None)
}

fn open_immutable_read_only(path: &Path) -> Result<Connection> {
    let uri = immutable_sqlite_uri(path)?;
    let connection = Connection::open_with_flags(
        uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_URI,
    )
    .with_context(|| format!("failed to open immutable SQLite store {}", path.display()))?;
    connection.busy_timeout(Duration::from_secs(30))?;
    initialize_read_only_connection(&connection).with_context(|| {
        format!(
            "failed to initialize immutable SQLite store {}",
            path.display()
        )
    })?;
    if let Some(sidecar) = first_existing_sqlite_sidecar(path)? {
        bail!(
            "SQLite sidecar appeared during immutable fallback: {}",
            sidecar.display()
        );
    }
    Ok(connection)
}

fn immutable_sqlite_uri(path: &Path) -> Result<String> {
    let absolute = absolute_candidate(path)?;
    let raw = absolute
        .to_str()
        .context("SQLite immutable fallback requires a UTF-8 path")?;
    let normalized = raw.replace('\\', "/");
    let normalized = if cfg!(windows)
        && normalized.as_bytes().get(1) == Some(&b':')
        && !normalized.starts_with('/')
    {
        format!("/{normalized}")
    } else {
        normalized
    };

    let mut uri = String::with_capacity(normalized.len() + 32);
    uri.push_str("file:");
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in normalized.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b':' | b'-' | b'.' | b'_' | b'~') {
            uri.push(char::from(byte));
        } else {
            uri.push('%');
            uri.push(char::from(HEX[usize::from(byte >> 4)]));
            uri.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    uri.push_str("?mode=ro&immutable=1");
    Ok(uri)
}

fn journal_mode(connection: &Connection, path: &Path) -> Result<String> {
    let effective_mode = connection
        .pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))
        .context("failed to read SQLite journal mode")?;
    if sqlite_header_uses_wal(path)? {
        Ok("wal".to_string())
    } else {
        Ok(effective_mode)
    }
}

fn sqlite_header_uses_wal(path: &Path) -> Result<bool> {
    let mut file = File::open(path)
        .with_context(|| format!("failed to inspect SQLite header {}", path.display()))?;
    let mut header = [0_u8; 20];
    match file.read_exact(&mut header) {
        Ok(()) => Ok(&header[..16] == b"SQLite format 3\0" && header[18] == 2 && header[19] == 2),
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
        Err(error) => {
            Err(error).with_context(|| format!("failed to read SQLite header {}", path.display()))
        }
    }
}

fn quick_check(connection: &Connection) -> Result<Vec<String>> {
    let mut statement = connection
        .prepare("PRAGMA quick_check")
        .context("failed to prepare SQLite quick_check")?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn pragma_i64(connection: &Connection, pragma: &str) -> Result<i64> {
    connection
        .pragma_query_value(None, pragma, |row| row.get(0))
        .with_context(|| format!("failed to read SQLite PRAGMA {pragma}"))
}

fn sha256_file(path: &Path) -> Result<String> {
    let file = File::open(path)
        .with_context(|| format!("failed to hash SQLite file {}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn reject_symlink(path: &Path) -> Result<()> {
    if path
        .symlink_metadata()
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        bail!("refusing SQLite store symlink: {}", path.display());
    }
    Ok(())
}

fn reject_same_path(left: &Path, right: &Path) -> Result<()> {
    let left = absolute_candidate(left)?;
    let right = absolute_candidate(right)?;
    if left == right {
        bail!("source and destination SQLite paths must differ");
    }
    Ok(())
}

fn absolute_candidate(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return fs::canonicalize(path)
            .with_context(|| format!("failed to canonicalize {}", path.display()));
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut cursor = absolute.as_path();
    let mut missing_components = Vec::new();
    while !cursor.exists() {
        let file_name = cursor
            .file_name()
            .context("SQLite path must include a file name")?;
        missing_components.push(file_name.to_os_string());
        cursor = cursor
            .parent()
            .context("SQLite path has no existing ancestor")?;
    }
    let mut resolved = fs::canonicalize(cursor)
        .with_context(|| format!("failed to canonicalize {}", cursor.display()))?;
    for component in missing_components.into_iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

fn absolute_display(path: &Path) -> Result<String> {
    Ok(absolute_candidate(path)?.display().to_string())
}

fn temporary_sibling(path: &Path, purpose: &str) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("forge.sqlite");
    path.with_file_name(format!(".{file_name}.{purpose}.{}.tmp", Uuid::new_v4()))
}

fn rollback_sibling(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("forge.sqlite");
    path.with_file_name(format!(
        "{file_name}.pre-restore-{}-{}.sqlite",
        Utc::now().format("%Y%m%dT%H%M%SZ"),
        Uuid::new_v4()
    ))
}

fn prepare_private_parent(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .filter(|candidate| !candidate.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create directory {}", parent.display()))?;
    set_private_directory_permissions(parent)
}

fn sync_file(path: &Path) -> Result<()> {
    File::open(path)
        .with_context(|| format!("failed to open {} for fsync", path.display()))?
        .sync_all()
        .with_context(|| format!("failed to fsync {}", path.display()))
}

fn sync_parent(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .filter(|candidate| !candidate.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    File::open(parent)
        .with_context(|| format!("failed to open {} for fsync", parent.display()))?
        .sync_all()
        .with_context(|| format!("failed to fsync {}", parent.display()))
}

fn remove_temporary_file(path: &Path) {
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

#[cfg(unix)]
fn create_private_file_if_missing(path: &Path) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    if path.exists() {
        return Ok(());
    }
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("failed to create private SQLite file {}", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn create_private_file_if_missing(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .with_context(|| format!("failed to create SQLite file {}", path.display()))?;
    Ok(())
}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("failed to set 0600 on {}", path.display()))
}

#[cfg(not(unix))]
fn set_private_file_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("failed to set 0700 on {}", path.display()))
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn unix_mode(metadata: &fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(metadata.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
fn unix_mode(_metadata: &fs::Metadata) -> Option<u32> {
    None
}
