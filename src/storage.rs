use crate::checkpoint::TaskCheckpoint;
use crate::event::{build_event_observability, categorize_event, infer_severity};
use crate::graph::Workflow;
use crate::intent::OperatingContextSpec;
use anyhow::{Context, Result};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use rusqlite::{
    params, params_from_iter, Connection, Error as SqliteError, ErrorCode, OptionalExtension, Row,
    Transaction, TransactionBehavior,
};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(5);
const SQLITE_RETRY_DELAY: Duration = Duration::from_millis(25);
const STORE_SCHEMA_VERSION: i64 = 4;
const EVENT_OBSERVABILITY_RECONCILIATION_BATCH_SIZE: usize = 64;
const EVENT_OBSERVABILITY_RECONCILIATION_CURSOR: &str = "event_observability_schema_v3_rebuild";
const GLOBAL_EVENTS_OBSERVABILITY_QUEUE_TRIGGER: &str = "trg_global_events_observability_queue";
const EVENT_OBSERVABILITY_DELETE_QUEUE_TRIGGER: &str = "trg_event_observability_delete_queue";
const RUNTIME_SECRET_ENCRYPTED_INSERT_TRIGGER: &str = "trg_runtime_secret_vault_encrypted_insert";
const RUNTIME_SECRET_ENCRYPTED_UPDATE_TRIGGER: &str = "trg_runtime_secret_vault_encrypted_update";
const RUNTIME_SECRET_ENVELOPE_PREFIX: &str = "forge:vault:v1";
const RUNTIME_SECRET_KEY_FILE_PREFIX: &str = "forge-secret-vault-key-v1:";
const RUNTIME_SECRET_KEY_ENV: &str = "FORGE_SECRET_VAULT_KEY";
const RUNTIME_SECRET_KEY_FILE_ENV: &str = "FORGE_SECRET_VAULT_KEY_FILE";
const RUNTIME_SECRET_PREVIOUS_KEYS_ENV: &str = "FORGE_SECRET_VAULT_PREVIOUS_KEYS";
const RUNTIME_SECRET_PREVIOUS_KEY_FILES_ENV: &str = "FORGE_SECRET_VAULT_PREVIOUS_KEY_FILES";
const RUNTIME_SECRET_SCRUB_CURSOR: &str = "runtime_secret_vault_encryption_v1_scrub";
const RUNTIME_SECRET_NONCE_BYTES: usize = 24;
const RUNTIME_SECRET_KEY_BYTES: usize = 32;
const RUNTIME_SECRET_KEY_FILE_MAX_BYTES: u64 = 16 * 1024;
const RUNTIME_SECRET_MAX_BYTES: usize = 1024 * 1024;
const GLOBAL_EVENTS_OBSERVABILITY_QUEUE_TRIGGER_SQL: &str = r#"
CREATE TRIGGER trg_global_events_observability_queue
AFTER INSERT ON global_events
BEGIN
    INSERT OR IGNORE INTO event_observability_reconciliation_queue (global_event_id)
    VALUES (NEW.id);
END;
"#;
const EVENT_OBSERVABILITY_DELETE_QUEUE_TRIGGER_SQL: &str = r#"
CREATE TRIGGER trg_event_observability_delete_queue
AFTER DELETE ON event_observability_index
WHEN EXISTS (
    SELECT 1 FROM global_events WHERE id = OLD.global_event_id
)
BEGIN
    INSERT OR IGNORE INTO event_observability_reconciliation_queue (global_event_id)
    VALUES (OLD.global_event_id);
END;
"#;
const RUNTIME_SECRET_ENCRYPTED_INSERT_TRIGGER_SQL: &str = r#"
CREATE TRIGGER trg_runtime_secret_vault_encrypted_insert
BEFORE INSERT ON runtime_secret_vault
WHEN NEW.secret_value NOT GLOB 'forge:vault:v1:*'
BEGIN
    SELECT RAISE(ABORT, 'runtime secret vault requires an encrypted v1 envelope');
END;
"#;
const RUNTIME_SECRET_ENCRYPTED_UPDATE_TRIGGER_SQL: &str = r#"
CREATE TRIGGER trg_runtime_secret_vault_encrypted_update
BEFORE UPDATE OF secret_value ON runtime_secret_vault
WHEN NEW.secret_value NOT GLOB 'forge:vault:v1:*'
BEGIN
    SELECT RAISE(ABORT, 'runtime secret vault requires an encrypted v1 envelope');
END;
"#;

type InboundEventRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    String,
    String,
    String,
    String,
    String,
    String,
);
type RuntimeSecretStoredRecord = (String, Zeroizing<String>, String, i64);

pub struct ForgeStore {
    path: PathBuf,
    connection: Connection,
    runtime_secret_cipher: RuntimeSecretCipher,
}

struct RuntimeSecretCipher {
    current: RuntimeSecretKey,
    previous: Vec<RuntimeSecretKey>,
}

struct RuntimeSecretKey {
    id: String,
    bytes: Zeroizing<[u8; RUNTIME_SECRET_KEY_BYTES]>,
}

impl RuntimeSecretCipher {
    fn load(store_path: &Path, encrypted_records_exist: bool) -> Result<Self> {
        let configured_key = std::env::var_os(RUNTIME_SECRET_KEY_ENV);
        let configured_key_file = std::env::var_os(RUNTIME_SECRET_KEY_FILE_ENV);
        if configured_key.is_some() && configured_key_file.is_some() {
            anyhow::bail!(
                "{RUNTIME_SECRET_KEY_ENV} and {RUNTIME_SECRET_KEY_FILE_ENV} are mutually exclusive"
            );
        }
        let external_key_configured = configured_key.is_some() || configured_key_file.is_some();

        let fallback_path = runtime_secret_fallback_key_path(store_path);
        let mut keyring = if let Some(encoded) = configured_key {
            let encoded = Zeroizing::new(encoded.into_string().map_err(|_| {
                anyhow::anyhow!("{RUNTIME_SECRET_KEY_ENV} must contain valid UTF-8")
            })?);
            vec![RuntimeSecretKey::parse(
                encoded.trim(),
                RUNTIME_SECRET_KEY_ENV,
            )?]
        } else if let Some(path) = configured_key_file {
            let path = PathBuf::from(path);
            if path.as_os_str().is_empty() {
                anyhow::bail!("{RUNTIME_SECRET_KEY_FILE_ENV} must not be empty");
            }
            read_runtime_secret_keyring(&path, false)?
        } else if is_sqlite_memory_path(store_path) {
            vec![ephemeral_runtime_secret_key()?]
        } else if fallback_path.exists() {
            read_runtime_secret_keyring(&fallback_path, true)?
        } else if encrypted_records_exist {
            anyhow::bail!(
                "runtime secret vault encryption key is unavailable; restore the configured key before opening this store"
            );
        } else {
            vec![create_runtime_secret_fallback_key(&fallback_path)?]
        };

        if keyring.is_empty() {
            anyhow::bail!("runtime secret vault keyring must contain a current key");
        }
        let current = keyring.remove(0);
        let mut previous = keyring;

        if external_key_configured && fallback_path.exists() && !is_sqlite_memory_path(store_path) {
            previous.extend(read_runtime_secret_keyring(&fallback_path, true)?);
        }

        if let Some(encoded_previous) = std::env::var_os(RUNTIME_SECRET_PREVIOUS_KEYS_ENV) {
            let encoded_previous =
                Zeroizing::new(encoded_previous.into_string().map_err(|_| {
                    anyhow::anyhow!("{RUNTIME_SECRET_PREVIOUS_KEYS_ENV} must contain valid UTF-8")
                })?);
            for encoded in encoded_previous
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                previous.push(RuntimeSecretKey::parse(
                    encoded,
                    RUNTIME_SECRET_PREVIOUS_KEYS_ENV,
                )?);
            }
        }

        if let Some(previous_files) = std::env::var_os(RUNTIME_SECRET_PREVIOUS_KEY_FILES_ENV) {
            for path in std::env::split_paths(&previous_files) {
                previous.extend(read_runtime_secret_keyring(&path, false)?);
            }
        }

        let mut seen = std::collections::BTreeSet::new();
        seen.insert(current.id.clone());
        previous.retain(|key| seen.insert(key.id.clone()));
        if previous.len() > 32 {
            anyhow::bail!("runtime secret vault keyring exceeds the 32-key safety limit");
        }

        Ok(Self { current, previous })
    }

    fn encrypt(
        &self,
        vault_key: &str,
        value_sha256: &str,
        value_len: usize,
        plaintext: &[u8],
    ) -> Result<String> {
        validate_runtime_secret_metadata(plaintext, value_sha256, value_len)?;
        let mut nonce = [0u8; RUNTIME_SECRET_NONCE_BYTES];
        getrandom::fill(&mut nonce)
            .map_err(|_| anyhow::anyhow!("failed to obtain OS randomness for secret encryption"))?;
        let cipher = XChaCha20Poly1305::new_from_slice(self.current.bytes.as_ref())
            .map_err(|_| anyhow::anyhow!("failed to initialize runtime secret vault cipher"))?;
        let aad = runtime_secret_aad(vault_key, value_sha256, value_len);
        let ciphertext = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| anyhow::anyhow!("failed to encrypt runtime secret vault value"))?;
        Ok(format!(
            "{RUNTIME_SECRET_ENVELOPE_PREFIX}:{}:{}:{}",
            self.current.id,
            hex_encode(&nonce),
            hex_encode(&ciphertext)
        ))
    }

    fn decrypt(
        &self,
        vault_key: &str,
        value_sha256: &str,
        value_len: usize,
        envelope: &str,
    ) -> Result<(Zeroizing<Vec<u8>>, bool)> {
        let components = envelope.split(':').collect::<Vec<_>>();
        if components.len() != 6
            || components[0] != "forge"
            || components[1] != "vault"
            || components[2] != "v1"
        {
            anyhow::bail!("runtime secret vault contains an unsupported encrypted envelope");
        }
        let key_id = components[3];
        let key = std::iter::once(&self.current)
            .chain(self.previous.iter())
            .find(|key| key.id == key_id)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "runtime secret vault key `{key_id}` is unavailable; provide it as a previous rotation key"
                )
            })?;
        if components[4].len() != RUNTIME_SECRET_NONCE_BYTES * 2 {
            anyhow::bail!("runtime secret vault contains an invalid nonce");
        }
        let nonce = hex_decode(components[4], "runtime secret nonce")?;
        if value_len > RUNTIME_SECRET_MAX_BYTES {
            anyhow::bail!("runtime secret vault values are limited to 1 MiB");
        }
        let expected_ciphertext_bytes = value_len
            .checked_add(16)
            .context("runtime secret vault value length overflow")?;
        if components[5].len() != expected_ciphertext_bytes * 2 {
            anyhow::bail!("runtime secret vault encrypted value length is inconsistent");
        }
        let ciphertext = hex_decode(components[5], "runtime secret ciphertext")?;
        let cipher = XChaCha20Poly1305::new_from_slice(key.bytes.as_ref())
            .map_err(|_| anyhow::anyhow!("failed to initialize runtime secret vault cipher"))?;
        let aad = runtime_secret_aad(vault_key, value_sha256, value_len);
        let plaintext = cipher
            .decrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| {
                anyhow::anyhow!(
                    "runtime secret vault authentication failed; the key or encrypted record is invalid"
                )
            })?;
        validate_runtime_secret_metadata(&plaintext, value_sha256, value_len)?;
        Ok((Zeroizing::new(plaintext), key.id != self.current.id))
    }
}

impl RuntimeSecretKey {
    fn parse(encoded: &str, source: &str) -> Result<Self> {
        let encoded = encoded
            .strip_prefix(RUNTIME_SECRET_KEY_FILE_PREFIX)
            .unwrap_or(encoded);
        if encoded.len() != RUNTIME_SECRET_KEY_BYTES * 2 {
            anyhow::bail!(
                "{source} must contain a 32-byte key encoded as exactly 64 hexadecimal characters"
            );
        }
        let decoded = hex_decode(encoded, source)?;
        let mut bytes = Zeroizing::new([0u8; RUNTIME_SECRET_KEY_BYTES]);
        bytes.copy_from_slice(&decoded);
        if bytes.iter().all(|byte| *byte == 0) {
            anyhow::bail!("{source} must not contain an all-zero key");
        }
        let digest = crate::artifact::hex_sha256(bytes.as_ref());
        Ok(Self {
            id: digest[..32].to_string(),
            bytes,
        })
    }

    fn encoded(&self) -> Zeroizing<String> {
        Zeroizing::new(format!(
            "{RUNTIME_SECRET_KEY_FILE_PREFIX}{}",
            hex_encode(self.bytes.as_ref())
        ))
    }
}

fn runtime_secret_aad(vault_key: &str, value_sha256: &str, value_len: usize) -> Vec<u8> {
    let mut aad = b"forge.runtime.secret-vault.aead.v1".to_vec();
    append_len_prefixed(&mut aad, vault_key.as_bytes());
    append_len_prefixed(&mut aad, value_sha256.as_bytes());
    aad.extend_from_slice(&(value_len as u64).to_be_bytes());
    aad
}

fn append_len_prefixed(output: &mut Vec<u8>, value: &[u8]) {
    output.extend_from_slice(&(value.len() as u64).to_be_bytes());
    output.extend_from_slice(value);
}

fn validate_runtime_secret_metadata(
    plaintext: &[u8],
    value_sha256: &str,
    value_len: usize,
) -> Result<()> {
    if value_len > RUNTIME_SECRET_MAX_BYTES || plaintext.len() > RUNTIME_SECRET_MAX_BYTES {
        anyhow::bail!("runtime secret vault values are limited to 1 MiB");
    }
    if plaintext.len() != value_len || crate::artifact::hex_sha256(plaintext) != value_sha256 {
        anyhow::bail!("runtime secret vault value metadata validation failed");
    }
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn hex_decode(encoded: &str, subject: &str) -> Result<Vec<u8>> {
    if !encoded.len().is_multiple_of(2) {
        anyhow::bail!("{subject} must be hexadecimal");
    }
    let mut decoded = Vec::with_capacity(encoded.len() / 2);
    for pair in encoded.as_bytes().chunks_exact(2) {
        let high =
            hex_nibble(pair[0]).ok_or_else(|| anyhow::anyhow!("{subject} must be hexadecimal"))?;
        let low =
            hex_nibble(pair[1]).ok_or_else(|| anyhow::anyhow!("{subject} must be hexadecimal"))?;
        decoded.push((high << 4) | low);
    }
    Ok(decoded)
}

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn runtime_secret_fallback_key_path(store_path: &Path) -> PathBuf {
    let mut path = OsString::from(store_path.as_os_str());
    path.push(".secret.key");
    PathBuf::from(path)
}

fn read_runtime_secret_keyring(
    path: &Path,
    locally_managed: bool,
) -> Result<Vec<RuntimeSecretKey>> {
    let metadata = if locally_managed {
        secure_existing_private_file(path, 0o600)?;
        std::fs::symlink_metadata(path)
    } else {
        std::fs::metadata(path)
    }
    .with_context(|| {
        format!(
            "failed to inspect runtime secret vault key file {}",
            path.display()
        )
    })?;
    if !metadata.is_file() {
        anyhow::bail!(
            "runtime secret vault key path {} is not a regular file",
            path.display()
        );
    }
    if metadata.len() > RUNTIME_SECRET_KEY_FILE_MAX_BYTES {
        anyhow::bail!(
            "runtime secret vault key file {} exceeds 16 KiB",
            path.display()
        );
    }
    require_private_key_file_mode(path, &metadata)?;

    let file = open_runtime_secret_key_file(path, locally_managed)?;
    let mut contents = Zeroizing::new(String::new());
    file.take(RUNTIME_SECRET_KEY_FILE_MAX_BYTES + 1)
        .read_to_string(&mut contents)
        .with_context(|| {
            format!(
                "failed to read runtime secret vault key file {}",
                path.display()
            )
        })?;
    if contents.len() as u64 > RUNTIME_SECRET_KEY_FILE_MAX_BYTES {
        anyhow::bail!(
            "runtime secret vault key file {} exceeds 16 KiB",
            path.display()
        );
    }
    let mut keys = Vec::new();
    for encoded in contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        keys.push(RuntimeSecretKey::parse(
            encoded,
            &format!("runtime secret vault key file {}", path.display()),
        )?);
    }
    if keys.is_empty() {
        anyhow::bail!("runtime secret vault key file {} is empty", path.display());
    }
    Ok(keys)
}

fn create_runtime_secret_fallback_key(path: &Path) -> Result<RuntimeSecretKey> {
    let mut bytes = Zeroizing::new([0u8; RUNTIME_SECRET_KEY_BYTES]);
    getrandom::fill(bytes.as_mut())
        .map_err(|_| anyhow::anyhow!("failed to obtain OS randomness for secret vault key"))?;
    let digest = crate::artifact::hex_sha256(bytes.as_ref());
    let key = RuntimeSecretKey {
        id: digest[..32].to_string(),
        bytes,
    };
    let encoded = key.encoded();
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);
    }
    match options.open(path) {
        Ok(mut file) => {
            if let Err(error) = file
                .write_all(encoded.as_bytes())
                .and_then(|_| file.write_all(b"\n"))
                .and_then(|_| file.sync_all())
            {
                let _ = std::fs::remove_file(path);
                return Err(error).with_context(|| {
                    format!(
                        "failed to persist runtime secret vault key {}",
                        path.display()
                    )
                });
            }
            secure_existing_private_file(path, 0o600)?;
            sync_parent_directory(path)?;
            Ok(key)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            read_runtime_secret_keyring(path, true).and_then(|mut keys| {
                if keys.is_empty() {
                    anyhow::bail!("runtime secret vault key file {} is empty", path.display());
                }
                Ok(keys.remove(0))
            })
        }
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed to create runtime secret vault key {}",
                path.display()
            )
        }),
    }
}

fn ephemeral_runtime_secret_key() -> Result<RuntimeSecretKey> {
    static KEY: OnceLock<[u8; RUNTIME_SECRET_KEY_BYTES]> = OnceLock::new();
    if KEY.get().is_none() {
        let mut generated = Zeroizing::new([0u8; RUNTIME_SECRET_KEY_BYTES]);
        getrandom::fill(generated.as_mut())
            .map_err(|_| anyhow::anyhow!("failed to obtain OS randomness for in-memory vault"))?;
        let _ = KEY.set(*generated);
    }
    let bytes = KEY
        .get()
        .context("failed to initialize the in-memory runtime secret vault key")?;
    let bytes = Zeroizing::new(*bytes);
    let digest = crate::artifact::hex_sha256(bytes.as_ref());
    Ok(RuntimeSecretKey {
        id: digest[..32].to_string(),
        bytes,
    })
}

fn is_sqlite_memory_path(path: &Path) -> bool {
    let value = path.to_string_lossy();
    value == ":memory:" || (value.starts_with("file:") && value.contains("mode=memory"))
}

fn runtime_secret_encrypted_records_exist(connection: &Connection) -> Result<bool> {
    let table_exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='runtime_secret_vault')",
        [],
        |row| row.get(0),
    )?;
    if !table_exists {
        return Ok(false);
    }
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM runtime_secret_vault WHERE secret_value LIKE 'forge:vault:%')",
            [],
            |row| row.get(0),
        )
        .context("failed to inspect runtime secret vault encryption state")
}

fn open_runtime_secret_key_file(path: &Path, no_follow: bool) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let flags = libc::O_CLOEXEC | if no_follow { libc::O_NOFOLLOW } else { 0 };
        options.custom_flags(flags);
    }
    options.open(path).with_context(|| {
        format!(
            "failed to open runtime secret vault key file {}",
            path.display()
        )
    })
}

#[cfg(unix)]
fn require_private_key_file_mode(path: &Path, metadata: &std::fs::Metadata) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if metadata.permissions().mode() & 0o077 != 0 {
        anyhow::bail!(
            "runtime secret vault key file {} must not be accessible by group or other users",
            path.display()
        );
    }
    Ok(())
}

#[cfg(not(unix))]
fn require_private_key_file_mode(_path: &Path, _metadata: &std::fs::Metadata) -> Result<()> {
    Ok(())
}

fn prepare_private_store_path(path: &Path) -> Result<()> {
    if is_sqlite_memory_path(path) {
        return Ok(());
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        let parent_existed = parent.exists();
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create store directory {}", parent.display()))?;
        if !parent_existed || parent.file_name().is_some_and(|name| name == ".forge") {
            secure_private_directory(parent)?;
        }
    }

    let mut options = OpenOptions::new();
    options.read(true).write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);
    }
    let created = match options.open(path) {
        Ok(file) => {
            file.sync_all()
                .with_context(|| format!("failed to initialize SQLite store {}", path.display()))?;
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to initialize SQLite store {}", path.display()));
        }
    };
    secure_existing_private_file(path, 0o600)?;
    if created {
        sync_parent_directory(path)?;
    }
    Ok(())
}

fn secure_sqlite_files(path: &Path) -> Result<()> {
    if is_sqlite_memory_path(path) {
        return Ok(());
    }
    secure_existing_private_file(path, 0o600)?;
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut sidecar = OsString::from(path.as_os_str());
        sidecar.push(suffix);
        let sidecar = PathBuf::from(sidecar);
        match std::fs::symlink_metadata(&sidecar) {
            Ok(_) => secure_existing_private_file(&sidecar, 0o600)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to inspect SQLite sidecar {}", sidecar.display())
                });
            }
        }
    }
    Ok(())
}

fn secure_private_directory(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect store directory {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        anyhow::bail!(
            "store directory {} must be a real directory, not a symlink",
            path.display()
        );
    }
    set_private_permissions(path, 0o700)
}

fn secure_existing_private_file(path: &Path, mode: u32) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect private file {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        anyhow::bail!(
            "private file {} must be a regular file, not a symlink",
            path.display()
        );
    }
    set_private_permissions(path, mode)
}

#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .with_context(|| {
            format!(
                "failed to synchronize private file directory {}",
                parent.display()
            )
        })
}

#[cfg(not(unix))]
fn sync_parent_directory(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_private_permissions(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .with_context(|| format!("failed to secure permissions on {}", path.display()))
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

pub struct TaskLeaseWrite<'a> {
    pub workflow_id: &'a str,
    pub task_id: &'a str,
    pub lease_id: &'a str,
    pub executor: &'a str,
    pub acquired_at: &'a str,
    pub expires_at: &'a str,
    pub data: &'a serde_json::Value,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MemoryPromotionQuery<'a> {
    pub from_scope: Option<&'a str>,
    pub to_scope: Option<&'a str>,
    pub approved_by: Option<&'a str>,
    pub workflow_id: Option<&'a str>,
    pub organization_id: Option<&'a str>,
    pub brand_id: Option<&'a str>,
    pub product_id: Option<&'a str>,
}

#[derive(Debug, Clone, Copy)]
pub struct GlobalEventWrite<'a> {
    pub source: &'a str,
    pub source_id: &'a str,
    pub workflow_id: Option<&'a str>,
    pub kind: &'a str,
    pub origin: &'a str,
    pub status: &'a str,
    pub data: &'a serde_json::Value,
    pub tenant_context: &'a serde_json::Value,
}

pub struct RuntimeSecretVaultWrite<'a> {
    pub vault_reference: &'a str,
    pub workflow_id: Option<&'a str>,
    pub scope: &'a str,
    pub provider: &'a str,
    pub kind: &'a str,
    pub classification: &'a str,
    pub secret_value: &'a str,
    pub value_sha256: &'a str,
    pub value_len: usize,
    pub source: &'a str,
    pub origin: &'a str,
    pub tenant_context: &'a serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct RuntimeSecretVaultResolve {
    pub vault_reference: String,
    pub workflow_id: Option<String>,
    pub secret_value: String,
    pub value_sha256: String,
    pub value_len: usize,
    pub audit_event_id: i64,
}

pub struct RuntimeSecretVaultAccess<'a> {
    pub vault_reference: &'a str,
    pub workflow_id: Option<&'a str>,
    pub requester: &'a str,
    pub allowed: bool,
    pub origin: &'a str,
    pub tenant_context: &'a serde_json::Value,
}

struct EventObservabilityIndexWrite<'a> {
    global_event_id: i64,
    workflow_id: Option<&'a str>,
    kind: &'a str,
    origin: &'a str,
    source: &'a str,
    organization_id: &'a str,
    brand_id: &'a str,
    product_id: &'a str,
    data: &'a serde_json::Value,
    created_at: &'a str,
}

pub struct AddonPermissionAuthorizationWrite<'a> {
    pub addon_id: &'a str,
    pub permission_id: &'a str,
    pub status: &'a str,
    pub risk: &'a str,
    pub approved_by: &'a str,
    pub source: &'a str,
    pub data: &'a serde_json::Value,
}

pub struct IdentityMembershipWrite<'a> {
    pub subject_scope: &'a str,
    pub subject_id: &'a str,
    pub organization_id: &'a str,
    pub brand_id: &'a str,
    pub product_id: &'a str,
    pub role: &'a str,
    pub status: &'a str,
    pub source: &'a str,
    pub data: &'a serde_json::Value,
}

pub struct IdentityLinkWrite<'a> {
    pub id: &'a str,
    pub left_scope: &'a str,
    pub left_id: &'a str,
    pub right_scope: &'a str,
    pub right_id: &'a str,
    pub link_type: &'a str,
    pub status: &'a str,
    pub source: &'a str,
    pub data: &'a serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct StoreEvent {
    pub id: i64,
    pub workflow_id: String,
    pub kind: String,
    pub data: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct StoredGlobalEventRecord {
    pub id: i64,
    pub source: String,
    pub source_id: String,
    pub workflow_id: Option<String>,
    pub kind: String,
    pub origin: String,
    pub status: String,
    pub organization_id: String,
    pub brand_id: String,
    pub product_id: String,
    pub user_id: String,
    pub channel_id: String,
    pub tenant_context: serde_json::Value,
    pub data: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct StoredEventObservabilityRecord {
    pub global_event_id: i64,
    pub workflow_id: String,
    pub kind: String,
    pub category: String,
    pub severity: String,
    pub origin: String,
    pub source: String,
    pub organization_id: String,
    pub brand_id: String,
    pub product_id: String,
    pub node_ref: Option<String>,
    pub addon_id: Option<String>,
    pub duration_ms: Option<i64>,
    pub retry_count: Option<i64>,
    pub wait_state: Option<String>,
    pub wait_seconds: Option<i64>,
    pub context_budget_bytes: Option<i64>,
    pub selected_context_bytes: Option<i64>,
    pub context_remaining_bytes: Option<i64>,
    pub context_pressure_bps: Option<i64>,
    pub context_pressure_state: Option<String>,
    pub memory_level: Option<String>,
    pub memory_scope: Option<String>,
    pub data: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct CostLedgerIndexWrite {
    pub row_key: String,
    pub source_kind: String,
    pub workflow_id: String,
    pub task_id: Option<String>,
    pub event_id: Option<i64>,
    pub organization_id: String,
    pub brand_id: String,
    pub product_id: String,
    pub addon_id: Option<String>,
    pub executor: Option<String>,
    pub model_call_required: bool,
    pub model_call_avoided: bool,
    pub estimated_task_cost_usd: f64,
    pub observed_event_cost_usd: f64,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CostLedgerIndexQuery<'a> {
    pub workflow_id: Option<&'a str>,
    pub organization_id: Option<&'a str>,
    pub brand_id: Option<&'a str>,
    pub product_id: Option<&'a str>,
    pub source_kind: Option<&'a str>,
    pub addon_id: Option<&'a str>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
pub struct CostLedgerRetentionQuery<'a> {
    pub index: CostLedgerIndexQuery<'a>,
    pub updated_before: &'a str,
}

#[derive(Debug, Clone)]
pub struct StoredCostLedgerIndexRecord {
    pub row_key: String,
    pub source_kind: String,
    pub workflow_id: String,
    pub task_id: Option<String>,
    pub event_id: Option<i64>,
    pub organization_id: String,
    pub brand_id: String,
    pub product_id: String,
    pub addon_id: Option<String>,
    pub executor: Option<String>,
    pub model_call_required: bool,
    pub model_call_avoided: bool,
    pub estimated_task_cost_usd: f64,
    pub observed_event_cost_usd: f64,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub data: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct HeadroomBlobWrite {
    pub source: String,
    pub content_kind: String,
    pub strategy: String,
    pub reversible: bool,
    pub original_sha256: String,
    pub original_bytes: i64,
    pub compressed_sha256: String,
    pub compressed_bytes: i64,
    pub estimated_original_tokens: i64,
    pub estimated_compressed_tokens: i64,
    pub estimated_saved_tokens: i64,
    pub budget_tokens: i64,
    pub budget_status: String,
    pub routing: serde_json::Value,
    pub original_content: String,
    pub compressed_content: String,
}

#[derive(Debug, Clone)]
pub struct StoredHeadroomBlobRecord {
    pub source: String,
    pub content_kind: String,
    pub strategy: String,
    pub reversible: bool,
    pub original_sha256: String,
    pub original_bytes: i64,
    pub compressed_sha256: String,
    pub compressed_bytes: i64,
    pub estimated_original_tokens: i64,
    pub estimated_compressed_tokens: i64,
    pub estimated_saved_tokens: i64,
    pub budget_tokens: i64,
    pub budget_status: String,
    pub routing: serde_json::Value,
    pub original_content: String,
    pub compressed_content: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct InboundEventRecord {
    pub id: String,
    pub origin: String,
    pub action: String,
    pub status: String,
    pub data: serde_json::Value,
    pub created_at: String,
    pub processed_at: Option<String>,
    pub organization_id: String,
    pub brand_id: String,
    pub product_id: String,
    pub user_id: String,
    pub channel_id: String,
    pub tenant_context: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct StoredAddonRecord {
    pub id: String,
    pub status: String,
    pub source: String,
    pub manifest: serde_json::Value,
    pub installed_at: String,
    pub updated_at: String,
}

pub struct StoredAddonCapabilityWrite {
    pub capability_id: String,
    pub title: String,
    pub source: String,
    pub addon_version: String,
    pub domains: serde_json::Value,
    pub keywords: serde_json::Value,
    pub workflow_extensions: serde_json::Value,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct StoredAddonCapabilityRecord {
    pub addon_id: String,
    pub capability_id: String,
    pub status: String,
    pub source: String,
    pub addon_version: String,
    pub title: String,
    pub domains: serde_json::Value,
    pub keywords: serde_json::Value,
    pub workflow_extensions: serde_json::Value,
    pub data: serde_json::Value,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct StoredAddonPermissionAuthorizationRecord {
    pub addon_id: String,
    pub permission_id: String,
    pub status: String,
    pub risk: String,
    pub approved_by: String,
    pub source: String,
    pub data: serde_json::Value,
    pub granted_at: String,
    pub updated_at: String,
}

pub struct RuntimeContractDispatchWrite<'a> {
    pub id: &'a str,
    pub addon_id: &'a str,
    pub contract_id: &'a str,
    pub contract_type: &'a str,
    pub capability_id: &'a str,
    pub runtime: &'a str,
    pub entrypoint: &'a str,
    pub status: &'a str,
    pub source: &'a str,
    pub input: &'a serde_json::Value,
    pub policy: &'a serde_json::Value,
    pub data: &'a serde_json::Value,
}

pub struct RuntimeWorkerWrite<'a> {
    pub id: &'a str,
    pub runtime: &'a str,
    pub status: &'a str,
    pub trust_level: &'a str,
    pub source: &'a str,
    pub data: &'a serde_json::Value,
}

pub struct EventServiceWrite<'a> {
    pub id: &'a str,
    pub service_kind: &'a str,
    pub status: &'a str,
    pub tenant_context: &'a serde_json::Value,
    pub lease_owner: &'a str,
    pub lease_id: &'a str,
    pub lease_acquired_at: &'a str,
    pub lease_expires_at: &'a str,
    pub last_heartbeat_at: &'a str,
    pub heartbeat_ttl_seconds: u64,
    pub data: &'a serde_json::Value,
}

pub struct AddonMarketplacePackageWrite<'a> {
    pub package_id: &'a str,
    pub addon_id: &'a str,
    pub addon_version: &'a str,
    pub repository: &'a str,
    pub channel: &'a str,
    pub manifest_sha256: &'a str,
    pub package_sha256: &'a str,
    pub status: &'a str,
    pub signature_status: &'a str,
    pub verification_status: &'a str,
    pub source: &'a str,
    pub package: &'a serde_json::Value,
}

pub struct AddonTrustKeyWrite<'a> {
    pub key_id: &'a str,
    pub repository: &'a str,
    pub channel: &'a str,
    pub public_key: &'a str,
    pub status: &'a str,
    pub trust_level: &'a str,
    pub approved_by: &'a str,
    pub source: &'a str,
    pub data: &'a serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct StoredRuntimeContractDispatchRecord {
    pub id: String,
    pub addon_id: String,
    pub contract_id: String,
    pub contract_type: String,
    pub capability_id: String,
    pub runtime: String,
    pub entrypoint: String,
    pub status: String,
    pub source: String,
    pub input: serde_json::Value,
    pub policy: serde_json::Value,
    pub data: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct StoredRuntimeWorkerRecord {
    pub id: String,
    pub runtime: String,
    pub status: String,
    pub trust_level: String,
    pub source: String,
    pub data: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct StoredEventServiceRecord {
    pub id: String,
    pub service_kind: String,
    pub status: String,
    pub organization_id: String,
    pub brand_id: String,
    pub product_id: String,
    pub user_id: String,
    pub channel_id: String,
    pub tenant_context: serde_json::Value,
    pub lease_owner: String,
    pub lease_id: String,
    pub lease_acquired_at: String,
    pub lease_expires_at: String,
    pub last_heartbeat_at: String,
    pub heartbeat_ttl_seconds: u64,
    pub data: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct StoredAddonMarketplacePackageRecord {
    pub package_id: String,
    pub addon_id: String,
    pub addon_version: String,
    pub repository: String,
    pub channel: String,
    pub manifest_sha256: String,
    pub package_sha256: String,
    pub status: String,
    pub signature_status: String,
    pub verification_status: String,
    pub source: String,
    pub package: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct StoredAddonTrustKeyRecord {
    pub key_id: String,
    pub repository: String,
    pub channel: String,
    pub public_key: String,
    pub status: String,
    pub trust_level: String,
    pub approved_by: String,
    pub source: String,
    pub data: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct StoredIdentityRecord {
    pub scope: String,
    pub id: String,
    pub label: String,
    pub source: String,
    pub data: serde_json::Value,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct StoredIdentityMembershipRecord {
    pub subject_scope: String,
    pub subject_id: String,
    pub organization_id: String,
    pub brand_id: String,
    pub product_id: String,
    pub role: String,
    pub status: String,
    pub source: String,
    pub data: serde_json::Value,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct StoredIdentityLinkRecord {
    pub id: String,
    pub left_scope: String,
    pub left_id: String,
    pub right_scope: String,
    pub right_id: String,
    pub link_type: String,
    pub status: String,
    pub source: String,
    pub data: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct TenantIndexRecord {
    pub resource_type: String,
    pub resource_id: String,
    pub workflow_id: String,
    pub organization_id: String,
    pub brand_id: String,
    pub product_id: String,
    pub user_id: String,
    pub channel_id: String,
    pub memory_scope: String,
    pub personality_scope: String,
    pub source: String,
    pub data: serde_json::Value,
    pub updated_at: String,
}

pub struct MemoryPromotionWrite<'a> {
    pub id: &'a str,
    pub workflow_id: &'a str,
    pub organization_id: &'a str,
    pub brand_id: &'a str,
    pub product_id: &'a str,
    pub user_id: &'a str,
    pub channel_id: &'a str,
    pub from_scope: &'a str,
    pub to_scope: &'a str,
    pub source_path: &'a str,
    pub target_path: &'a str,
    pub visibility: &'a str,
    pub shareability: &'a str,
    pub approved_by: &'a str,
    pub reason: &'a str,
    pub summary_sha256: &'a str,
    pub promoted_memory_sha256: &'a str,
    pub data: &'a serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct StoredMemoryPromotionRecord {
    pub id: String,
    pub workflow_id: String,
    pub organization_id: String,
    pub brand_id: String,
    pub product_id: String,
    pub user_id: String,
    pub channel_id: String,
    pub from_scope: String,
    pub to_scope: String,
    pub source_path: String,
    pub target_path: String,
    pub visibility: String,
    pub shareability: String,
    pub approved_by: String,
    pub reason: String,
    pub summary_sha256: String,
    pub promoted_memory_sha256: String,
    pub data: serde_json::Value,
    pub created_at: String,
}

pub(crate) fn open_configured_connection(path: &Path) -> Result<Connection> {
    prepare_private_store_path(path)?;
    let connection = Connection::open(path)
        .with_context(|| format!("failed to open SQLite store {}", path.display()))?;
    connection.busy_timeout(Duration::ZERO).with_context(|| {
        format!(
            "failed to configure SQLite probe timeout for {}",
            path.display()
        )
    })?;
    let journal_mode =
        connection.pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0));
    let is_memory_database = if journal_mode
        .as_ref()
        .is_ok_and(|mode| mode.eq_ignore_ascii_case("memory"))
    {
        connection
            .query_row(
                "SELECT file FROM pragma_database_list WHERE name = 'main'",
                [],
                |row| row.get::<_, String>(0),
            )
            .with_context(|| {
                format!(
                    "failed to inspect SQLite database path for {}",
                    path.display()
                )
            })?
            .is_empty()
    } else {
        false
    };
    connection
        .busy_timeout(SQLITE_BUSY_TIMEOUT)
        .with_context(|| format!("failed to configure SQLite timeout for {}", path.display()))?;
    match journal_mode {
        Ok(mode) if mode.eq_ignore_ascii_case("wal") || is_memory_database => {}
        Ok(_) => ensure_wal(&connection, path, SQLITE_BUSY_TIMEOUT)?,
        Err(error) if sqlite_is_contention(&error) => {
            // A transient lock can hide either WAL or a rollback journal. Retry until the
            // persistent mode is confirmed instead of returning an ambiguously configured
            // connection.
            ensure_wal(&connection, path, SQLITE_BUSY_TIMEOUT)?;
        }
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "failed to inspect SQLite journal mode for {}",
                    path.display()
                )
            });
        }
    }
    connection
        .pragma_update(None, "synchronous", "FULL")
        .with_context(|| {
            format!(
                "failed to configure SQLite durability for {}",
                path.display()
            )
        })?;
    connection
        .pragma_update(None, "secure_delete", "ON")
        .with_context(|| {
            format!(
                "failed to configure SQLite secure deletion for {}",
                path.display()
            )
        })?;
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .with_context(|| {
            format!(
                "failed to enable SQLite foreign key enforcement for {}",
                path.display()
            )
        })?;
    let foreign_keys_enabled: i64 = connection
        .pragma_query_value(None, "foreign_keys", |row| row.get(0))
        .with_context(|| {
            format!(
                "failed to verify SQLite foreign key enforcement for {}",
                path.display()
            )
        })?;
    if foreign_keys_enabled != 1 {
        anyhow::bail!(
            "SQLite foreign key enforcement could not be enabled for {}",
            path.display()
        );
    }
    secure_sqlite_files(path)?;
    Ok(connection)
}

fn ensure_wal(connection: &Connection, path: &Path, timeout: Duration) -> Result<()> {
    let started = Instant::now();
    loop {
        match connection.pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0)) {
            Ok(mode) if mode.eq_ignore_ascii_case("wal") => return Ok(()),
            Ok(_) => {}
            Err(error) if sqlite_is_contention(&error) => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to inspect SQLite journal mode for {}",
                        path.display()
                    )
                });
            }
        }

        if started.elapsed() >= timeout {
            anyhow::bail!(
                "timed out after {} ms enabling SQLite WAL for {} because the store remained busy or locked",
                timeout.as_millis(),
                path.display()
            );
        }

        match connection
            .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get::<_, String>(0))
        {
            Ok(mode) if mode.eq_ignore_ascii_case("wal") => return Ok(()),
            Ok(_) => {}
            Err(error) if sqlite_is_contention(&error) => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("failed to enable SQLite WAL for {}", path.display())
                });
            }
        }

        if started.elapsed() >= timeout {
            anyhow::bail!(
                "timed out after {} ms enabling SQLite WAL for {} because the store remained busy or locked",
                timeout.as_millis(),
                path.display()
            );
        }
        thread::sleep(SQLITE_RETRY_DELAY.min(timeout.saturating_sub(started.elapsed())));
    }
}

fn sqlite_is_contention(error: &SqliteError) -> bool {
    matches!(
        error.sqlite_error_code(),
        Some(ErrorCode::DatabaseBusy) | Some(ErrorCode::DatabaseLocked)
    )
}

#[cfg(test)]
fn event_observability_trigger_count(connection: &Connection) -> Result<i64> {
    connection
        .query_row(
            r#"
            SELECT count(*)
            FROM sqlite_master
            WHERE type = 'trigger'
              AND name IN (?1, ?2)
            "#,
            params![
                GLOBAL_EVENTS_OBSERVABILITY_QUEUE_TRIGGER,
                EVENT_OBSERVABILITY_DELETE_QUEUE_TRIGGER
            ],
            |row| row.get(0),
        )
        .context("failed to inspect event observability reconciliation triggers")
}

fn canonical_trigger_sql(sql: &str) -> String {
    let compact = sql
        .chars()
        .filter(|character| !character.is_whitespace() && *character != ';')
        .collect::<String>()
        .to_ascii_lowercase();
    compact.replacen("createtriggerifnotexists", "createtrigger", 1)
}

fn event_observability_triggers_are_valid(connection: &Connection) -> Result<bool> {
    for (name, table, expected_sql) in [
        (
            GLOBAL_EVENTS_OBSERVABILITY_QUEUE_TRIGGER,
            "global_events",
            GLOBAL_EVENTS_OBSERVABILITY_QUEUE_TRIGGER_SQL,
        ),
        (
            EVENT_OBSERVABILITY_DELETE_QUEUE_TRIGGER,
            "event_observability_index",
            EVENT_OBSERVABILITY_DELETE_QUEUE_TRIGGER_SQL,
        ),
    ] {
        let trigger = connection
            .query_row(
                r#"
                SELECT tbl_name, sql
                FROM sqlite_master
                WHERE type = 'trigger' AND name = ?1
                "#,
                params![name],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()
            .with_context(|| format!("failed to inspect event observability trigger {name}"))?;
        let Some((actual_table, Some(actual_sql))) = trigger else {
            return Ok(false);
        };
        if actual_table != table
            || canonical_trigger_sql(&actual_sql) != canonical_trigger_sql(expected_sql)
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn runtime_secret_vault_encryption_triggers_are_valid(connection: &Connection) -> Result<bool> {
    for (name, expected_sql) in [
        (
            RUNTIME_SECRET_ENCRYPTED_INSERT_TRIGGER,
            RUNTIME_SECRET_ENCRYPTED_INSERT_TRIGGER_SQL,
        ),
        (
            RUNTIME_SECRET_ENCRYPTED_UPDATE_TRIGGER,
            RUNTIME_SECRET_ENCRYPTED_UPDATE_TRIGGER_SQL,
        ),
    ] {
        let trigger_sql = connection
            .query_row(
                r#"
                SELECT sql
                FROM sqlite_master
                WHERE type = 'trigger'
                  AND name = ?1
                  AND tbl_name = 'runtime_secret_vault'
                "#,
                params![name],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .with_context(|| format!("failed to inspect runtime secret vault trigger {name}"))?
            .flatten();
        let Some(trigger_sql) = trigger_sql else {
            return Ok(false);
        };
        if canonical_trigger_sql(&trigger_sql) != canonical_trigger_sql(expected_sql) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn load_runtime_secret_vault_records(
    connection: &Connection,
) -> Result<Vec<RuntimeSecretStoredRecord>> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT vault_key, secret_value, value_sha256, value_len
            FROM runtime_secret_vault
            ORDER BY vault_key
            "#,
        )
        .context("failed to prepare runtime secret vault migration")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            Zeroizing::new(row.get::<_, String>(1)?),
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
        ))
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .context("failed to inspect runtime secret vault migration state")
}

impl ForgeStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let connection = open_configured_connection(&path)?;

        let table_count: i64 = connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
                [],
                |row| row.get(0),
            )
            .with_context(|| {
                format!("failed to inspect SQLite schema in {}", path.display())
            })?;

        if table_count > 0 {
            let events_exists: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='events')",
                    [],
                    |row| row.get(0),
                )
                .with_context(|| {
                    format!("failed to inspect events table in {}", path.display())
                })?;
            let workflows_exists: bool = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workflows')",
                    [],
                    |row| row.get(0),
                )
                .with_context(|| {
                    format!("failed to inspect workflows table in {}", path.display())
                })?;
            if events_exists && !workflows_exists {
                anyhow::bail!("Database is corrupted: table 'workflows' is missing.");
            }
        }

        let encrypted_records_exist = runtime_secret_encrypted_records_exist(&connection)?;
        let runtime_secret_cipher = RuntimeSecretCipher::load(&path, encrypted_records_exist)?;
        let store = Self {
            path,
            connection,
            runtime_secret_cipher,
        };
        store.migrate_if_needed()?;
        store.ensure_runtime_secret_vault_encryption_triggers()?;
        store.migrate_runtime_secret_vault_encryption()?;
        store.repair_event_observability_triggers_if_needed()?;
        store.reconcile_derived_state_if_needed()?;
        secure_sqlite_files(&store.path)?;
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn base_dir(&self) -> PathBuf {
        self.path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    pub fn with_transaction<T>(&self, operation: impl FnOnce() -> Result<T>) -> Result<T> {
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        match operation() {
            Ok(value) => {
                transaction.commit()?;
                Ok(value)
            }
            Err(error) => {
                drop(transaction);
                Err(error)
            }
        }
    }

    fn migrate_if_needed(&self) -> Result<()> {
        let version = self.store_schema_version()?;
        if version > STORE_SCHEMA_VERSION {
            anyhow::bail!(
                "SQLite store schema version {version} is newer than supported version {STORE_SCHEMA_VERSION}"
            );
        }
        if version == STORE_SCHEMA_VERSION {
            return Ok(());
        }

        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .context("failed to acquire SQLite store migration lock")?;
        let locked_version: i64 =
            transaction.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if locked_version > STORE_SCHEMA_VERSION {
            anyhow::bail!(
                "SQLite store schema version {locked_version} is newer than supported version {STORE_SCHEMA_VERSION}"
            );
        }
        if locked_version < STORE_SCHEMA_VERSION {
            self.migrate()?;
            self.initialize_event_observability_reconciliation_cursor()?;
            transaction.pragma_update(None, "user_version", STORE_SCHEMA_VERSION)?;
        }
        transaction
            .commit()
            .context("failed to commit SQLite store migration")?;
        Ok(())
    }

    fn ensure_runtime_secret_vault_encryption_triggers(&self) -> Result<()> {
        if runtime_secret_vault_encryption_triggers_are_valid(&self.connection)? {
            return Ok(());
        }
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .context("failed to acquire runtime secret vault trigger repair lock")?;
        if runtime_secret_vault_encryption_triggers_are_valid(&transaction)? {
            transaction.commit()?;
            return Ok(());
        }
        transaction.execute_batch(&format!(
            r#"
            DROP TRIGGER IF EXISTS {RUNTIME_SECRET_ENCRYPTED_INSERT_TRIGGER};
            DROP TRIGGER IF EXISTS {RUNTIME_SECRET_ENCRYPTED_UPDATE_TRIGGER};
            {RUNTIME_SECRET_ENCRYPTED_INSERT_TRIGGER_SQL}
            {RUNTIME_SECRET_ENCRYPTED_UPDATE_TRIGGER_SQL}
            "#
        ))?;
        transaction
            .commit()
            .context("failed to enforce encrypted runtime secret vault writes")
    }

    fn migrate_runtime_secret_vault_encryption(&self) -> Result<usize> {
        let mut migration_required = false;
        for (vault_key, stored_value, value_sha256, raw_value_len) in
            load_runtime_secret_vault_records(&self.connection)?
        {
            let (_plaintext, _value_len, requires_reencryption) = self
                .decode_runtime_secret_record(
                    &vault_key,
                    &stored_value,
                    &value_sha256,
                    raw_value_len,
                )?;
            migration_required |= requires_reencryption;
        }
        let mut scrub_pending = self.runtime_secret_vault_scrub_pending()?;
        let mut migrated = 0usize;
        if migration_required {
            let transaction =
                Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                    .context("failed to acquire runtime secret vault migration lock")?;
            for (vault_key, stored_value, value_sha256, raw_value_len) in
                load_runtime_secret_vault_records(&transaction)?
            {
                let (plaintext, value_len, requires_reencryption) = self
                    .decode_runtime_secret_record(
                        &vault_key,
                        &stored_value,
                        &value_sha256,
                        raw_value_len,
                    )?;
                if !requires_reencryption {
                    continue;
                }
                let envelope = self.runtime_secret_cipher.encrypt(
                    &vault_key,
                    &value_sha256,
                    value_len,
                    &plaintext,
                )?;
                transaction
                    .execute(
                        "UPDATE runtime_secret_vault SET secret_value = ?1, updated_at = CURRENT_TIMESTAMP WHERE vault_key = ?2",
                        params![envelope, vault_key],
                    )
                    .context("failed to re-encrypt runtime secret vault record")?;
                migrated += 1;
            }
            if migrated > 0 {
                transaction
                    .execute(
                        r#"
                        INSERT INTO store_reconciliation_cursors (
                            cursor_key, last_global_event_id, upper_bound_global_event_id,
                            status, updated_at
                        )
                        VALUES (?1, 0, 0, 'pending', CURRENT_TIMESTAMP)
                        ON CONFLICT(cursor_key) DO UPDATE SET
                            status='pending',
                            updated_at=CURRENT_TIMESTAMP
                        "#,
                        params![RUNTIME_SECRET_SCRUB_CURSOR],
                    )
                    .context("failed to record runtime secret vault scrub requirement")?;
            }
            transaction
                .commit()
                .context("failed to commit runtime secret vault migration")?;
            scrub_pending |= migrated > 0;
        }

        if scrub_pending {
            if is_sqlite_memory_path(&self.path) {
                self.connection.execute(
                    "DELETE FROM store_reconciliation_cursors WHERE cursor_key = ?1",
                    params![RUNTIME_SECRET_SCRUB_CURSOR],
                )?;
            } else {
                self.scrub_runtime_secret_vault_migration()?;
            }
        }
        Ok(migrated)
    }

    fn decode_runtime_secret_record(
        &self,
        vault_key: &str,
        stored_value: &str,
        value_sha256: &str,
        raw_value_len: i64,
    ) -> Result<(Zeroizing<Vec<u8>>, usize, bool)> {
        let value_len = usize::try_from(raw_value_len)
            .context("runtime secret vault contains an invalid value length")?;
        if stored_value.starts_with("forge:vault:") {
            let (plaintext, requires_reencryption) = self.runtime_secret_cipher.decrypt(
                vault_key,
                value_sha256,
                value_len,
                stored_value,
            )?;
            Ok((plaintext, value_len, requires_reencryption))
        } else {
            let plaintext = Zeroizing::new(stored_value.as_bytes().to_vec());
            validate_runtime_secret_metadata(&plaintext, value_sha256, value_len)?;
            Ok((plaintext, value_len, true))
        }
    }

    fn runtime_secret_vault_scrub_pending(&self) -> Result<bool> {
        self.connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM store_reconciliation_cursors WHERE cursor_key = ?1 AND status = 'pending')",
                params![RUNTIME_SECRET_SCRUB_CURSOR],
                |row| row.get(0),
            )
            .context("failed to inspect runtime secret vault scrub state")
    }

    fn scrub_runtime_secret_vault_migration(&self) -> Result<()> {
        self.checkpoint_runtime_secret_vault_migration()?;
        self.connection
            .execute_batch("VACUUM")
            .context("failed to vacuum SQLite after runtime secret vault migration")?;
        self.checkpoint_runtime_secret_vault_migration()?;
        self.connection
            .execute(
                "DELETE FROM store_reconciliation_cursors WHERE cursor_key = ?1",
                params![RUNTIME_SECRET_SCRUB_CURSOR],
            )
            .context("failed to complete runtime secret vault scrub marker")?;
        self.checkpoint_runtime_secret_vault_migration()?;
        secure_sqlite_files(&self.path)
    }

    fn checkpoint_runtime_secret_vault_migration(&self) -> Result<()> {
        let (busy, _log_frames, _checkpointed_frames): (i64, i64, i64) = self
            .connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .context("failed to checkpoint runtime secret vault migration")?;
        if busy != 0 {
            anyhow::bail!(
                "runtime secret vault migration checkpoint is busy; retry after other store users exit"
            );
        }
        Ok(())
    }

    fn store_schema_version(&self) -> Result<i64> {
        self.connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .context("failed to read SQLite store schema version")
    }

    fn initialize_event_observability_reconciliation_cursor(&self) -> Result<()> {
        self.connection.execute(
            "DELETE FROM store_reconciliation_cursors WHERE cursor_key = ?1",
            params![EVENT_OBSERVABILITY_RECONCILIATION_CURSOR],
        )?;
        self.connection.execute(
            r#"
            INSERT INTO store_reconciliation_cursors (
                cursor_key,
                last_global_event_id,
                upper_bound_global_event_id,
                status,
                updated_at
            )
            SELECT
                ?1,
                0,
                COALESCE(MAX(id), 0),
                CASE WHEN COALESCE(MAX(id), 0) = 0 THEN 'completed' ELSE 'pending' END,
                CURRENT_TIMESTAMP
            FROM global_events
            "#,
            params![EVENT_OBSERVABILITY_RECONCILIATION_CURSOR],
        )?;
        Ok(())
    }

    fn repair_event_observability_triggers_if_needed(&self) -> Result<()> {
        if event_observability_triggers_are_valid(&self.connection)? {
            return Ok(());
        }

        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .context("failed to acquire event observability trigger repair lock")?;
        if event_observability_triggers_are_valid(&transaction)? {
            transaction
                .commit()
                .context("failed to release event observability trigger repair lock")?;
            return Ok(());
        }

        transaction.execute_batch(&format!(
            r#"
            DROP TRIGGER IF EXISTS trg_global_events_observability_queue;
            DROP TRIGGER IF EXISTS trg_event_observability_delete_queue;
            {GLOBAL_EVENTS_OBSERVABILITY_QUEUE_TRIGGER_SQL}
            {EVENT_OBSERVABILITY_DELETE_QUEUE_TRIGGER_SQL}
            "#
        ))?;
        transaction
            .commit()
            .context("failed to commit event observability trigger repair")?;
        Ok(())
    }

    fn migrate(&self) -> Result<()> {
        self.connection.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS workflows (
                id TEXT PRIMARY KEY,
                goal TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                data_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS artifacts (
                id TEXT PRIMARY KEY,
                workflow_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                path TEXT NOT NULL,
                sha256 TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                workflow_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                data_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS global_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source TEXT NOT NULL,
                source_id TEXT NOT NULL,
                workflow_id TEXT,
                kind TEXT NOT NULL,
                origin TEXT NOT NULL,
                status TEXT NOT NULL,
                organization_id TEXT NOT NULL,
                brand_id TEXT NOT NULL,
                product_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                channel_id TEXT NOT NULL,
                tenant_context_json TEXT NOT NULL,
                data_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_global_events_workflow
                ON global_events (workflow_id, id);
            CREATE INDEX IF NOT EXISTS idx_global_events_tenant
                ON global_events (organization_id, brand_id, product_id, id);
            CREATE INDEX IF NOT EXISTS idx_global_events_kind
            ON global_events (kind, id);
            CREATE TABLE IF NOT EXISTS runtime_secret_vault (
                vault_key TEXT PRIMARY KEY,
                vault_reference TEXT NOT NULL,
                workflow_id TEXT NOT NULL DEFAULT '',
                scope TEXT NOT NULL,
                provider TEXT NOT NULL,
                kind TEXT NOT NULL,
                classification TEXT NOT NULL,
                secret_value TEXT NOT NULL,
                value_sha256 TEXT NOT NULL,
                value_len INTEGER NOT NULL,
                source TEXT NOT NULL,
                organization_id TEXT NOT NULL,
                brand_id TEXT NOT NULL,
                product_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                channel_id TEXT NOT NULL,
                tenant_context_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_runtime_secret_vault_reference
            ON runtime_secret_vault (vault_reference, workflow_id);
            CREATE INDEX IF NOT EXISTS idx_runtime_secret_vault_tenant
            ON runtime_secret_vault (organization_id, brand_id, product_id, vault_reference);
            CREATE TRIGGER IF NOT EXISTS trg_runtime_secret_vault_encrypted_insert
            BEFORE INSERT ON runtime_secret_vault
            WHEN NEW.secret_value NOT GLOB 'forge:vault:v1:*'
            BEGIN
                SELECT RAISE(ABORT, 'runtime secret vault requires an encrypted v1 envelope');
            END;
            CREATE TRIGGER IF NOT EXISTS trg_runtime_secret_vault_encrypted_update
            BEFORE UPDATE OF secret_value ON runtime_secret_vault
            WHEN NEW.secret_value NOT GLOB 'forge:vault:v1:*'
            BEGIN
                SELECT RAISE(ABORT, 'runtime secret vault requires an encrypted v1 envelope');
            END;
            CREATE TABLE IF NOT EXISTS event_observability_index (
                global_event_id INTEGER PRIMARY KEY,
                workflow_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                category TEXT NOT NULL,
                severity TEXT NOT NULL,
                origin TEXT NOT NULL,
                source TEXT NOT NULL,
                organization_id TEXT NOT NULL,
                brand_id TEXT NOT NULL,
                product_id TEXT NOT NULL,
                node_ref TEXT,
                addon_id TEXT,
                duration_ms INTEGER,
                retry_count INTEGER,
                wait_state TEXT,
                wait_seconds INTEGER,
                context_budget_bytes INTEGER,
                selected_context_bytes INTEGER,
                context_remaining_bytes INTEGER,
                context_pressure_bps INTEGER,
                context_pressure_state TEXT,
                memory_level TEXT,
                memory_scope TEXT,
                data_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_event_observability_workflow
                ON event_observability_index (workflow_id, global_event_id);
            CREATE INDEX IF NOT EXISTS idx_event_observability_tenant
                ON event_observability_index (organization_id, brand_id, product_id, global_event_id);
            CREATE INDEX IF NOT EXISTS idx_event_observability_node
                ON event_observability_index (node_ref, global_event_id);
            CREATE INDEX IF NOT EXISTS idx_event_observability_addon
                ON event_observability_index (addon_id, global_event_id);
            CREATE TABLE IF NOT EXISTS event_observability_reconciliation_queue (
                global_event_id INTEGER PRIMARY KEY,
                enqueued_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS store_reconciliation_cursors (
                cursor_key TEXT PRIMARY KEY,
                last_global_event_id INTEGER NOT NULL,
                upper_bound_global_event_id INTEGER NOT NULL,
                status TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TRIGGER IF NOT EXISTS trg_global_events_observability_queue
            AFTER INSERT ON global_events
            BEGIN
                INSERT OR IGNORE INTO event_observability_reconciliation_queue (global_event_id)
                VALUES (NEW.id);
            END;
            CREATE TRIGGER IF NOT EXISTS trg_event_observability_delete_queue
            AFTER DELETE ON event_observability_index
            WHEN EXISTS (
                SELECT 1 FROM global_events WHERE id = OLD.global_event_id
            )
            BEGIN
                INSERT OR IGNORE INTO event_observability_reconciliation_queue (global_event_id)
                VALUES (OLD.global_event_id);
            END;
            CREATE TABLE IF NOT EXISTS cost_ledger_index (
                row_key TEXT PRIMARY KEY,
                source_kind TEXT NOT NULL,
                workflow_id TEXT NOT NULL,
                task_id TEXT,
                event_id INTEGER,
                organization_id TEXT NOT NULL,
                brand_id TEXT NOT NULL,
                product_id TEXT NOT NULL,
                addon_id TEXT,
                executor TEXT,
                model_call_required INTEGER NOT NULL,
                model_call_avoided INTEGER NOT NULL,
                estimated_task_cost_usd REAL NOT NULL,
                observed_event_cost_usd REAL NOT NULL,
                tokens_in INTEGER NOT NULL,
                tokens_out INTEGER NOT NULL,
                data_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_cost_ledger_workflow
                ON cost_ledger_index (workflow_id, source_kind);
            CREATE INDEX IF NOT EXISTS idx_cost_ledger_tenant
                ON cost_ledger_index (organization_id, brand_id, product_id, source_kind);
            CREATE INDEX IF NOT EXISTS idx_cost_ledger_addon
                ON cost_ledger_index (addon_id, source_kind);
            CREATE TABLE IF NOT EXISTS harness_headroom_blobs (
                original_sha256 TEXT PRIMARY KEY,
                source TEXT NOT NULL,
                content_kind TEXT NOT NULL,
                strategy TEXT NOT NULL,
                reversible INTEGER NOT NULL,
                original_bytes INTEGER NOT NULL,
                compressed_sha256 TEXT NOT NULL,
                compressed_bytes INTEGER NOT NULL,
                estimated_original_tokens INTEGER NOT NULL,
                estimated_compressed_tokens INTEGER NOT NULL,
                estimated_saved_tokens INTEGER NOT NULL,
                budget_tokens INTEGER NOT NULL,
                budget_status TEXT NOT NULL,
                routing_json TEXT NOT NULL,
                original_content TEXT NOT NULL,
                compressed_content TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_harness_headroom_source
                ON harness_headroom_blobs (source, updated_at);
            CREATE INDEX IF NOT EXISTS idx_harness_headroom_kind
                ON harness_headroom_blobs (content_kind, updated_at);
            CREATE TABLE IF NOT EXISTS event_inbox (
                id TEXT PRIMARY KEY,
                origin TEXT NOT NULL,
                action TEXT NOT NULL,
                status TEXT NOT NULL,
                organization_id TEXT NOT NULL DEFAULT '',
                brand_id TEXT NOT NULL DEFAULT '',
                product_id TEXT NOT NULL DEFAULT '',
                user_id TEXT NOT NULL DEFAULT '',
                channel_id TEXT NOT NULL DEFAULT '',
                tenant_context_json TEXT NOT NULL DEFAULT '{}',
                data_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                processed_at TEXT
            );
            CREATE TABLE IF NOT EXISTS event_services (
                id TEXT PRIMARY KEY,
                service_kind TEXT NOT NULL,
                status TEXT NOT NULL,
                organization_id TEXT NOT NULL DEFAULT '',
                brand_id TEXT NOT NULL DEFAULT '',
                product_id TEXT NOT NULL DEFAULT '',
                user_id TEXT NOT NULL DEFAULT '',
                channel_id TEXT NOT NULL DEFAULT '',
                tenant_context_json TEXT NOT NULL DEFAULT '{}',
                lease_owner TEXT NOT NULL,
                lease_id TEXT NOT NULL,
                lease_acquired_at TEXT NOT NULL,
                lease_expires_at TEXT NOT NULL,
                last_heartbeat_at TEXT NOT NULL,
                heartbeat_ttl_seconds INTEGER NOT NULL,
                data_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_event_services_kind
                ON event_services (service_kind);
            CREATE INDEX IF NOT EXISTS idx_event_services_status
                ON event_services (status);
            CREATE INDEX IF NOT EXISTS idx_event_services_lease
                ON event_services (lease_expires_at);
            CREATE TABLE IF NOT EXISTS installed_addons (
                id TEXT PRIMARY KEY,
                status TEXT NOT NULL,
                source TEXT NOT NULL,
                manifest_json TEXT NOT NULL,
                installed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS addon_capabilities (
                addon_id TEXT NOT NULL,
                capability_id TEXT NOT NULL,
                status TEXT NOT NULL,
                source TEXT NOT NULL,
                addon_version TEXT NOT NULL,
                title TEXT NOT NULL,
                domains_json TEXT NOT NULL,
                keywords_json TEXT NOT NULL,
                workflow_extensions_json TEXT NOT NULL,
                data_json TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (addon_id, capability_id)
            );
            CREATE INDEX IF NOT EXISTS idx_addon_capabilities_capability
                ON addon_capabilities (capability_id);
            CREATE INDEX IF NOT EXISTS idx_addon_capabilities_status
                ON addon_capabilities (status);
            CREATE TABLE IF NOT EXISTS addon_permission_authorizations (
                addon_id TEXT NOT NULL,
                permission_id TEXT NOT NULL,
                status TEXT NOT NULL,
                risk TEXT NOT NULL,
                approved_by TEXT NOT NULL,
                source TEXT NOT NULL,
                data_json TEXT NOT NULL,
                granted_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (addon_id, permission_id)
            );
            CREATE INDEX IF NOT EXISTS idx_addon_permission_authorizations_status
                ON addon_permission_authorizations (status);
            CREATE TABLE IF NOT EXISTS runtime_contract_dispatches (
                id TEXT PRIMARY KEY,
                addon_id TEXT NOT NULL,
                contract_id TEXT NOT NULL,
                contract_type TEXT NOT NULL,
                capability_id TEXT NOT NULL,
                runtime TEXT NOT NULL,
                entrypoint TEXT NOT NULL,
                status TEXT NOT NULL,
                source TEXT NOT NULL,
                input_json TEXT NOT NULL,
                policy_json TEXT NOT NULL,
                data_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_runtime_contract_dispatches_contract
                ON runtime_contract_dispatches (addon_id, contract_id);
            CREATE INDEX IF NOT EXISTS idx_runtime_contract_dispatches_status
                ON runtime_contract_dispatches (status);
            CREATE TABLE IF NOT EXISTS runtime_workers (
                id TEXT PRIMARY KEY,
                runtime TEXT NOT NULL,
                status TEXT NOT NULL,
                trust_level TEXT NOT NULL,
                source TEXT NOT NULL,
                data_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_runtime_workers_runtime
                ON runtime_workers (runtime);
            CREATE INDEX IF NOT EXISTS idx_runtime_workers_status
                ON runtime_workers (status);
            CREATE TABLE IF NOT EXISTS addon_marketplace_packages (
                package_id TEXT PRIMARY KEY,
                addon_id TEXT NOT NULL,
                addon_version TEXT NOT NULL,
                repository TEXT NOT NULL,
                channel TEXT NOT NULL,
                manifest_sha256 TEXT NOT NULL,
                package_sha256 TEXT NOT NULL,
                status TEXT NOT NULL,
                signature_status TEXT NOT NULL,
                verification_status TEXT NOT NULL,
                source TEXT NOT NULL,
                package_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_addon_marketplace_packages_addon
                ON addon_marketplace_packages (addon_id);
            CREATE INDEX IF NOT EXISTS idx_addon_marketplace_packages_repo_channel
                ON addon_marketplace_packages (repository, channel);
            CREATE INDEX IF NOT EXISTS idx_addon_marketplace_packages_status
                ON addon_marketplace_packages (status);
            CREATE TABLE IF NOT EXISTS addon_trust_keys (
                key_id TEXT PRIMARY KEY,
                repository TEXT NOT NULL,
                channel TEXT NOT NULL,
                public_key TEXT NOT NULL,
                status TEXT NOT NULL,
                trust_level TEXT NOT NULL,
                approved_by TEXT NOT NULL,
                source TEXT NOT NULL,
                data_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_addon_trust_keys_repo_channel
                ON addon_trust_keys (repository, channel);
            CREATE INDEX IF NOT EXISTS idx_addon_trust_keys_status
                ON addon_trust_keys (status);
            CREATE TABLE IF NOT EXISTS identity_registry (
                scope TEXT NOT NULL,
                id TEXT NOT NULL,
                label TEXT NOT NULL,
                source TEXT NOT NULL,
                data_json TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (scope, id)
            );
            CREATE INDEX IF NOT EXISTS idx_identity_registry_scope
                ON identity_registry (scope);
            CREATE TABLE IF NOT EXISTS identity_memberships (
                subject_scope TEXT NOT NULL,
                subject_id TEXT NOT NULL,
                organization_id TEXT NOT NULL,
                brand_id TEXT NOT NULL,
                product_id TEXT NOT NULL,
                role TEXT NOT NULL,
                status TEXT NOT NULL,
                source TEXT NOT NULL,
                data_json TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (subject_scope, subject_id, organization_id, brand_id, product_id)
            );
            CREATE INDEX IF NOT EXISTS idx_identity_memberships_subject
                ON identity_memberships (subject_scope, subject_id);
            CREATE INDEX IF NOT EXISTS idx_identity_memberships_tenant
                ON identity_memberships (organization_id, brand_id, product_id);
            CREATE INDEX IF NOT EXISTS idx_identity_memberships_status
                ON identity_memberships (status);
            CREATE TABLE IF NOT EXISTS identity_links (
                id TEXT PRIMARY KEY,
                left_scope TEXT NOT NULL,
                left_id TEXT NOT NULL,
                right_scope TEXT NOT NULL,
                right_id TEXT NOT NULL,
                link_type TEXT NOT NULL,
                status TEXT NOT NULL,
                source TEXT NOT NULL,
                data_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_identity_links_left
                ON identity_links (left_scope, left_id);
            CREATE INDEX IF NOT EXISTS idx_identity_links_right
                ON identity_links (right_scope, right_id);
            CREATE INDEX IF NOT EXISTS idx_identity_links_status
                ON identity_links (status);
            CREATE TABLE IF NOT EXISTS tenant_index (
                resource_type TEXT NOT NULL,
                resource_id TEXT NOT NULL,
                workflow_id TEXT NOT NULL,
                organization_id TEXT NOT NULL,
                brand_id TEXT NOT NULL,
                product_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                channel_id TEXT NOT NULL,
                memory_scope TEXT NOT NULL,
                personality_scope TEXT NOT NULL,
                source TEXT NOT NULL,
                data_json TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (resource_type, resource_id)
            );
            CREATE INDEX IF NOT EXISTS idx_tenant_index_workflow
                ON tenant_index (workflow_id);
            CREATE INDEX IF NOT EXISTS idx_tenant_index_org_brand_product
                ON tenant_index (organization_id, brand_id, product_id);
            CREATE INDEX IF NOT EXISTS idx_tenant_index_resource
                ON tenant_index (resource_type);
            CREATE TABLE IF NOT EXISTS memory_promotions (
                id TEXT PRIMARY KEY,
                workflow_id TEXT NOT NULL DEFAULT '',
                organization_id TEXT NOT NULL DEFAULT '',
                brand_id TEXT NOT NULL DEFAULT '',
                product_id TEXT NOT NULL DEFAULT '',
                user_id TEXT NOT NULL DEFAULT '',
                channel_id TEXT NOT NULL DEFAULT '',
                from_scope TEXT NOT NULL,
                to_scope TEXT NOT NULL,
                source_path TEXT NOT NULL,
                target_path TEXT NOT NULL,
                visibility TEXT NOT NULL,
                shareability TEXT NOT NULL,
                approved_by TEXT NOT NULL,
                reason TEXT NOT NULL,
                summary_sha256 TEXT NOT NULL,
                promoted_memory_sha256 TEXT NOT NULL,
                data_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_memory_promotions_scopes
                ON memory_promotions (from_scope, to_scope);
            CREATE INDEX IF NOT EXISTS idx_memory_promotions_approved_by
                ON memory_promotions (approved_by);
            CREATE TABLE IF NOT EXISTS executor_policy (
                id TEXT PRIMARY KEY,
                data_json TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS runtime_policy (
                id TEXT PRIMARY KEY,
                data_json TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS worktree_states (
                id TEXT PRIMARY KEY,
                repository_root TEXT NOT NULL,
                worktree_root TEXT NOT NULL UNIQUE,
                branch TEXT,
                head TEXT NOT NULL,
                data_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_worktree_states_repository
                ON worktree_states (repository_root, worktree_root);
            CREATE INDEX IF NOT EXISTS idx_worktree_states_branch
                ON worktree_states (branch, updated_at);
            CREATE TABLE IF NOT EXISTS worktree_sandbox_states (
                id TEXT PRIMARY KEY,
                worktree_id TEXT NOT NULL,
                status TEXT NOT NULL,
                data_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_worktree_sandbox_states_worktree
                ON worktree_sandbox_states (worktree_id, status, updated_at);
            CREATE TABLE IF NOT EXISTS runs (
                id TEXT PRIMARY KEY,
                workflow_id TEXT NOT NULL,
                organization_id TEXT NOT NULL DEFAULT '',
                brand_id TEXT NOT NULL DEFAULT '',
                product_id TEXT NOT NULL DEFAULT '',
                user_id TEXT NOT NULL DEFAULT '',
                channel_id TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL,
                data_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS task_leases (
                workflow_id TEXT NOT NULL,
                task_id TEXT NOT NULL,
                lease_id TEXT NOT NULL,
                executor TEXT NOT NULL,
                organization_id TEXT NOT NULL DEFAULT '',
                brand_id TEXT NOT NULL DEFAULT '',
                product_id TEXT NOT NULL DEFAULT '',
                user_id TEXT NOT NULL DEFAULT '',
                channel_id TEXT NOT NULL DEFAULT '',
                acquired_at TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                data_json TEXT NOT NULL,
                PRIMARY KEY (workflow_id, task_id)
            );
            CREATE TABLE IF NOT EXISTS task_checkpoints (
                id TEXT PRIMARY KEY,
                workflow_id TEXT NOT NULL,
                task_id TEXT NOT NULL,
                executor TEXT NOT NULL,
                organization_id TEXT NOT NULL DEFAULT '',
                brand_id TEXT NOT NULL DEFAULT '',
                product_id TEXT NOT NULL DEFAULT '',
                user_id TEXT NOT NULL DEFAULT '',
                channel_id TEXT NOT NULL DEFAULT '',
                state TEXT NOT NULL,
                created_at TEXT NOT NULL,
                data_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS cluster_nodes (
                id TEXT PRIMARY KEY,
                data_json TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS executor_quotas (
                executor TEXT NOT NULL,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                data_json TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (executor, provider, model)
            );
            CREATE TABLE IF NOT EXISTS web_benchmark_cache (
                brain_id TEXT PRIMARY KEY,
                lmsys_score INTEGER NOT NULL,
                mmlu_score REAL NOT NULL,
                human_eval_score REAL NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
        )?;
        self.ensure_event_observability_context_columns()?;
        self.ensure_event_inbox_tenant_columns()?;
        self.ensure_event_services_tenant_columns()?;
        self.ensure_memory_promotion_tenant_columns()?;
        self.ensure_operational_tenant_columns()?;
        Ok(())
    }

    fn reconcile_derived_state_if_needed(&self) -> Result<()> {
        self.backfill_missing_event_observability_index()?;
        self.backfill_event_inbox_tenant_context()?;
        self.backfill_event_services_tenant_context()?;
        self.backfill_operational_tenant_columns()?;
        Ok(())
    }

    fn ensure_event_observability_context_columns(&self) -> Result<()> {
        for (column, sql_type) in [
            ("context_budget_bytes", "INTEGER"),
            ("selected_context_bytes", "INTEGER"),
            ("context_remaining_bytes", "INTEGER"),
            ("context_pressure_bps", "INTEGER"),
            ("context_pressure_state", "TEXT"),
            ("memory_level", "TEXT"),
            ("memory_scope", "TEXT"),
        ] {
            if !self.table_has_column("event_observability_index", column)? {
                let result = self.connection.execute(
                    &format!(
                        "ALTER TABLE event_observability_index ADD COLUMN {column} {sql_type}"
                    ),
                    [],
                );
                if let Err(error) = result {
                    if !error.to_string().contains("duplicate column name") {
                        return Err(error.into());
                    }
                }
            }
        }
        self.connection.execute_batch(
            r#"
            CREATE INDEX IF NOT EXISTS idx_event_observability_context_pressure
                ON event_observability_index (context_pressure_bps, global_event_id);
            "#,
        )?;
        Ok(())
    }

    fn ensure_memory_promotion_tenant_columns(&self) -> Result<()> {
        for column in [
            "workflow_id",
            "organization_id",
            "brand_id",
            "product_id",
            "user_id",
            "channel_id",
        ] {
            if !self.table_has_column("memory_promotions", column)? {
                let result = self.connection.execute(
                    &format!(
                        "ALTER TABLE memory_promotions ADD COLUMN {column} TEXT NOT NULL DEFAULT ''"
                    ),
                    [],
                );
                if let Err(error) = result {
                    if !error.to_string().contains("duplicate column name") {
                        return Err(error.into());
                    }
                }
            }
        }
        self.connection.execute_batch(
            r#"
            CREATE INDEX IF NOT EXISTS idx_memory_promotions_workflow
                ON memory_promotions (workflow_id);
            CREATE INDEX IF NOT EXISTS idx_memory_promotions_tenant
                ON memory_promotions (organization_id, brand_id, product_id);
            "#,
        )?;
        Ok(())
    }

    fn ensure_event_inbox_tenant_columns(&self) -> Result<()> {
        for column in [
            "organization_id",
            "brand_id",
            "product_id",
            "user_id",
            "channel_id",
        ] {
            if !self.table_has_column("event_inbox", column)? {
                let result = self.connection.execute(
                    &format!(
                        "ALTER TABLE event_inbox ADD COLUMN {column} TEXT NOT NULL DEFAULT ''"
                    ),
                    [],
                );
                if let Err(error) = result {
                    if !error.to_string().contains("duplicate column name") {
                        return Err(error.into());
                    }
                }
            }
        }
        if !self.table_has_column("event_inbox", "tenant_context_json")? {
            let result = self.connection.execute(
                "ALTER TABLE event_inbox ADD COLUMN tenant_context_json TEXT NOT NULL DEFAULT '{}'",
                [],
            );
            if let Err(error) = result {
                if !error.to_string().contains("duplicate column name") {
                    return Err(error.into());
                }
            }
        }
        self.connection.execute_batch(
            r#"
            CREATE INDEX IF NOT EXISTS idx_event_inbox_tenant
                ON event_inbox (organization_id, brand_id, product_id, status, created_at);
            CREATE INDEX IF NOT EXISTS idx_event_inbox_missing_tenant
                ON event_inbox (id)
                WHERE organization_id = ''
                   OR brand_id = ''
                   OR product_id = ''
                   OR user_id = ''
                   OR channel_id = ''
                   OR tenant_context_json = '{}';
            "#,
        )?;
        self.backfill_event_inbox_tenant_context()?;
        Ok(())
    }

    fn backfill_event_inbox_tenant_context(&self) -> Result<()> {
        let missing: bool = self.connection.query_row(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM event_inbox
                WHERE organization_id = ''
                   OR brand_id = ''
                   OR product_id = ''
                   OR user_id = ''
                   OR channel_id = ''
                   OR tenant_context_json = '{}'
            )
            "#,
            [],
            |row| row.get(0),
        )?;
        if !missing {
            return Ok(());
        }

        let default_context = default_operating_context_json();
        self.connection.execute(
            r#"
            UPDATE event_inbox
            SET organization_id = ?1,
                brand_id = ?2,
                product_id = ?3,
                user_id = ?4,
                channel_id = ?5,
                tenant_context_json = ?6
            WHERE organization_id = ''
               OR brand_id = ''
               OR product_id = ''
               OR user_id = ''
               OR channel_id = ''
               OR tenant_context_json = '{}'
            "#,
            params![
                tenant_context_identity_id(&default_context, "organization"),
                tenant_context_identity_id(&default_context, "brand"),
                tenant_context_identity_id(&default_context, "product"),
                tenant_context_identity_id(&default_context, "user"),
                tenant_context_identity_id(&default_context, "channel"),
                serde_json::to_string(&default_context)?,
            ],
        )?;
        Ok(())
    }

    fn ensure_event_services_tenant_columns(&self) -> Result<()> {
        for column in [
            "organization_id",
            "brand_id",
            "product_id",
            "user_id",
            "channel_id",
        ] {
            if !self.table_has_column("event_services", column)? {
                let result = self.connection.execute(
                    &format!(
                        "ALTER TABLE event_services ADD COLUMN {column} TEXT NOT NULL DEFAULT ''"
                    ),
                    [],
                );
                if let Err(error) = result {
                    if !error.to_string().contains("duplicate column name") {
                        return Err(error.into());
                    }
                }
            }
        }
        if !self.table_has_column("event_services", "tenant_context_json")? {
            let result = self.connection.execute(
                "ALTER TABLE event_services ADD COLUMN tenant_context_json TEXT NOT NULL DEFAULT '{}'",
                [],
            );
            if let Err(error) = result {
                if !error.to_string().contains("duplicate column name") {
                    return Err(error.into());
                }
            }
        }
        self.connection.execute_batch(
            r#"
            CREATE INDEX IF NOT EXISTS idx_event_services_tenant
                ON event_services (organization_id, brand_id, product_id, service_kind, status, updated_at);
            CREATE INDEX IF NOT EXISTS idx_event_services_missing_tenant
                ON event_services (id)
                WHERE organization_id = ''
                   OR brand_id = ''
                   OR product_id = ''
                   OR user_id = ''
                   OR channel_id = ''
                   OR tenant_context_json = '{}';
            "#,
        )?;
        self.backfill_event_services_tenant_context()?;
        Ok(())
    }

    fn backfill_event_services_tenant_context(&self) -> Result<()> {
        let missing: bool = self.connection.query_row(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM event_services
                WHERE organization_id = ''
                   OR brand_id = ''
                   OR product_id = ''
                   OR user_id = ''
                   OR channel_id = ''
                   OR tenant_context_json = '{}'
            )
            "#,
            [],
            |row| row.get(0),
        )?;
        if !missing {
            return Ok(());
        }

        let default_context = default_operating_context_json();
        self.connection.execute(
            r#"
            UPDATE event_services
            SET organization_id = ?1,
                brand_id = ?2,
                product_id = ?3,
                user_id = ?4,
                channel_id = ?5,
                tenant_context_json = ?6
            WHERE organization_id = ''
               OR brand_id = ''
               OR product_id = ''
               OR user_id = ''
               OR channel_id = ''
               OR tenant_context_json = '{}'
            "#,
            params![
                tenant_context_identity_id(&default_context, "organization"),
                tenant_context_identity_id(&default_context, "brand"),
                tenant_context_identity_id(&default_context, "product"),
                tenant_context_identity_id(&default_context, "user"),
                tenant_context_identity_id(&default_context, "channel"),
                serde_json::to_string(&default_context)?,
            ],
        )?;
        Ok(())
    }

    fn ensure_operational_tenant_columns(&self) -> Result<()> {
        for table in ["runs", "task_leases", "task_checkpoints"] {
            for column in [
                "organization_id",
                "brand_id",
                "product_id",
                "user_id",
                "channel_id",
            ] {
                if !self.table_has_column(table, column)? {
                    let result = self.connection.execute(
                        &format!(
                            "ALTER TABLE {table} ADD COLUMN {column} TEXT NOT NULL DEFAULT ''"
                        ),
                        [],
                    );
                    if let Err(error) = result {
                        if !error.to_string().contains("duplicate column name") {
                            return Err(error.into());
                        }
                    }
                }
            }
        }
        self.connection.execute_batch(
            r#"
            CREATE INDEX IF NOT EXISTS idx_runs_tenant
                ON runs (organization_id, brand_id, product_id, status);
            CREATE INDEX IF NOT EXISTS idx_task_leases_tenant
                ON task_leases (organization_id, brand_id, product_id, expires_at);
            CREATE INDEX IF NOT EXISTS idx_task_checkpoints_tenant
                ON task_checkpoints (organization_id, brand_id, product_id, created_at);
            CREATE INDEX IF NOT EXISTS idx_runs_missing_tenant
                ON runs (id)
                WHERE organization_id = ''
                   OR brand_id = ''
                   OR product_id = ''
                   OR user_id = ''
                   OR channel_id = '';
            CREATE INDEX IF NOT EXISTS idx_task_leases_missing_tenant
                ON task_leases (workflow_id, task_id)
                WHERE organization_id = ''
                   OR brand_id = ''
                   OR product_id = ''
                   OR user_id = ''
                   OR channel_id = '';
            CREATE INDEX IF NOT EXISTS idx_task_checkpoints_missing_tenant
                ON task_checkpoints (id)
                WHERE organization_id = ''
                   OR brand_id = ''
                   OR product_id = ''
                   OR user_id = ''
                   OR channel_id = '';
            "#,
        )?;
        self.backfill_operational_tenant_columns()?;
        Ok(())
    }

    fn backfill_operational_tenant_columns(&self) -> Result<()> {
        let mut tables_with_missing_tenant = Vec::new();
        for table in ["runs", "task_leases", "task_checkpoints"] {
            let missing: bool = self.connection.query_row(
                &format!(
                    r#"
                    SELECT EXISTS(
                        SELECT 1
                        FROM {table}
                        WHERE organization_id = ''
                           OR brand_id = ''
                           OR product_id = ''
                           OR user_id = ''
                           OR channel_id = ''
                    )
                    "#
                ),
                [],
                |row| row.get(0),
            )?;
            if missing {
                tables_with_missing_tenant.push(table);
            }
        }
        if tables_with_missing_tenant.is_empty() {
            return Ok(());
        }

        for workflow in self.load_workflows()? {
            let tenant = operational_tenant_columns(Some(&workflow));
            for table in &tables_with_missing_tenant {
                self.connection.execute(
                    &format!(
                        r#"
                        UPDATE {table}
                        SET organization_id = ?2,
                            brand_id = ?3,
                            product_id = ?4,
                            user_id = ?5,
                            channel_id = ?6
                        WHERE workflow_id = ?1
                          AND (
                            organization_id = ''
                            OR brand_id = ''
                            OR product_id = ''
                            OR user_id = ''
                            OR channel_id = ''
                          )
                        "#
                    ),
                    params![
                        workflow.id,
                        tenant.organization_id,
                        tenant.brand_id,
                        tenant.product_id,
                        tenant.user_id,
                        tenant.channel_id,
                    ],
                )?;
            }
        }

        let default_tenant = operational_tenant_columns(None);
        for table in tables_with_missing_tenant {
            self.connection.execute(
                &format!(
                    r#"
                    UPDATE {table}
                    SET organization_id = ?1,
                        brand_id = ?2,
                        product_id = ?3,
                        user_id = ?4,
                        channel_id = ?5
                    WHERE organization_id = ''
                       OR brand_id = ''
                       OR product_id = ''
                       OR user_id = ''
                       OR channel_id = ''
                    "#
                ),
                params![
                    default_tenant.organization_id,
                    default_tenant.brand_id,
                    default_tenant.product_id,
                    default_tenant.user_id,
                    default_tenant.channel_id,
                ],
            )?;
        }
        Ok(())
    }

    fn table_has_column(&self, table: &str, column: &str) -> Result<bool> {
        let mut statement = self
            .connection
            .prepare(&format!("PRAGMA table_info({table})"))?;
        let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
        for row in rows {
            if row? == column {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn save_workflow(&self, workflow: &Workflow) -> Result<()> {
        let data_json = serde_json::to_string(workflow)?;
        self.connection.execute(
            r#"
            INSERT INTO workflows (id, goal, status, created_at, data_json)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(id) DO UPDATE SET
                goal=excluded.goal,
                status=excluded.status,
                data_json=excluded.data_json
            "#,
            params![
                workflow.id,
                workflow.goal,
                workflow.status,
                workflow.created_at.to_rfc3339(),
                data_json
            ],
        )?;
        self.replace_workflow_tenant_projection(workflow)?;
        Ok(())
    }

    pub fn load_workflow(&self, id: &str) -> Result<Workflow> {
        let data_json: Option<String> = self
            .connection
            .query_row(
                "SELECT data_json FROM workflows WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        let data_json = data_json.with_context(|| format!("workflow not found: {id}"))?;
        Ok(serde_json::from_str(&data_json)?)
    }

    pub fn load_workflows(&self) -> Result<Vec<Workflow>> {
        let mut statement = self
            .connection
            .prepare("SELECT data_json FROM workflows ORDER BY created_at ASC, id ASC")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut workflows = Vec::new();
        for row in rows {
            workflows.push(serde_json::from_str(&row?)?);
        }
        Ok(workflows)
    }

    pub fn load_recent_workflows(&self, limit: usize) -> Result<Vec<Workflow>> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut statement = self.connection.prepare(
            "SELECT data_json FROM workflows ORDER BY created_at DESC, id DESC LIMIT ?1",
        )?;
        let rows = statement.query_map(params![limit], |row| row.get::<_, String>(0))?;
        let mut workflows = Vec::new();
        for row in rows {
            workflows.push(serde_json::from_str(&row?)?);
        }
        Ok(workflows)
    }

    pub fn count_rows(&self, table: &str) -> Result<usize> {
        let table = checked_count_table(table)?;
        let count: i64 =
            self.connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })?;
        Ok(count.max(0) as usize)
    }

    pub fn count_rows_where_in(&self, table: &str, column: &str, values: &[&str]) -> Result<usize> {
        let table = checked_count_table(table)?;
        let column = checked_count_column(table, column)?;
        if values.is_empty() {
            return self.count_rows(table);
        }
        let placeholders = std::iter::repeat_n("?", values.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("SELECT COUNT(*) FROM {table} WHERE {column} IN ({placeholders})");
        let count: i64 =
            self.connection
                .query_row(&sql, params_from_iter(values.iter().copied()), |row| {
                    row.get(0)
                })?;
        Ok(count.max(0) as usize)
    }

    pub fn row_counts_by_value(
        &self,
        table: &str,
        column: &str,
    ) -> Result<BTreeMap<String, usize>> {
        let table = checked_count_table(table)?;
        let column = checked_count_column(table, column)?;
        let mut statement = self.connection.prepare(&format!(
            "SELECT {column}, COUNT(*) FROM {table} GROUP BY {column}"
        ))?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut counts = BTreeMap::new();
        for row in rows {
            let (value, count) = row?;
            counts.insert(value, count.max(0) as usize);
        }
        Ok(counts)
    }

    fn replace_workflow_tenant_projection(&self, workflow: &Workflow) -> Result<()> {
        self.save_tenant_index_record(
            "workflow",
            &workflow.id,
            &workflow.id,
            workflow,
            "workflows",
            &serde_json::json!({
                "workflow_id": workflow.id,
                "status": workflow.status,
                "goal": workflow.goal,
                "workflow_mode": workflow.intent.workflow_mode.kind,
            }),
        )?;
        self.connection.execute(
            "DELETE FROM tenant_index WHERE workflow_id = ?1 AND resource_type = 'artifact'",
            params![workflow.id],
        )?;
        for artifact in &workflow.artifacts {
            self.save_tenant_index_record(
                "artifact",
                &artifact.id,
                &workflow.id,
                workflow,
                "workflow.artifacts",
                &serde_json::json!({
                    "artifact_id": artifact.id,
                    "kind": artifact.kind,
                    "path": artifact.path,
                    "sha256": artifact.sha256,
                    "created_at": artifact.created_at,
                }),
            )?;
        }
        Ok(())
    }

    fn save_tenant_index_record(
        &self,
        resource_type: &str,
        resource_id: &str,
        workflow_id: &str,
        workflow: &Workflow,
        source: &str,
        data: &serde_json::Value,
    ) -> Result<()> {
        let context = &workflow.intent.operating_context;
        self.connection.execute(
            r#"
            INSERT INTO tenant_index (
                resource_type,
                resource_id,
                workflow_id,
                organization_id,
                brand_id,
                product_id,
                user_id,
                channel_id,
                memory_scope,
                personality_scope,
                source,
                data_json,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, CURRENT_TIMESTAMP)
            ON CONFLICT(resource_type, resource_id) DO UPDATE SET
                workflow_id=excluded.workflow_id,
                organization_id=excluded.organization_id,
                brand_id=excluded.brand_id,
                product_id=excluded.product_id,
                user_id=excluded.user_id,
                channel_id=excluded.channel_id,
                memory_scope=excluded.memory_scope,
                personality_scope=excluded.personality_scope,
                source=excluded.source,
                data_json=excluded.data_json,
                updated_at=CURRENT_TIMESTAMP
            "#,
            params![
                resource_type,
                resource_id,
                workflow_id,
                context.organization.id,
                context.brand.id,
                context.product.id,
                context.user.id,
                context.channel.id,
                context.memory_scope,
                context.personality_scope,
                source,
                serde_json::to_string(data)?,
            ],
        )?;
        Ok(())
    }

    pub fn list_tenant_index(
        &self,
        resource_type: Option<&str>,
        organization_id: Option<&str>,
        brand_id: Option<&str>,
        product_id: Option<&str>,
        workflow_id: Option<&str>,
    ) -> Result<Vec<TenantIndexRecord>> {
        let mut statement = self.connection.prepare(
            r#"
            SELECT
                resource_type,
                resource_id,
                workflow_id,
                organization_id,
                brand_id,
                product_id,
                user_id,
                channel_id,
                memory_scope,
                personality_scope,
                source,
                data_json,
                updated_at
            FROM tenant_index
            ORDER BY organization_id ASC, brand_id ASC, product_id ASC, resource_type ASC, resource_id ASC
            "#,
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, String>(12)?,
            ))
        })?;
        let mut records = Vec::new();
        for row in rows {
            let (
                resource_type_value,
                resource_id,
                workflow_id_value,
                organization_id_value,
                brand_id_value,
                product_id_value,
                user_id,
                channel_id,
                memory_scope,
                personality_scope,
                source,
                data_json,
                updated_at,
            ) = row?;
            if resource_type.is_some_and(|filter| filter != resource_type_value.as_str()) {
                continue;
            }
            if organization_id.is_some_and(|filter| filter != organization_id_value.as_str()) {
                continue;
            }
            if brand_id.is_some_and(|filter| filter != brand_id_value.as_str()) {
                continue;
            }
            if product_id.is_some_and(|filter| filter != product_id_value.as_str()) {
                continue;
            }
            if workflow_id.is_some_and(|filter| filter != workflow_id_value.as_str()) {
                continue;
            }
            records.push(TenantIndexRecord {
                resource_type: resource_type_value,
                resource_id,
                workflow_id: workflow_id_value,
                organization_id: organization_id_value,
                brand_id: brand_id_value,
                product_id: product_id_value,
                user_id,
                channel_id,
                memory_scope,
                personality_scope,
                source,
                data: serde_json::from_str(&data_json)?,
                updated_at,
            });
        }
        Ok(records)
    }

    pub fn save_memory_promotion(&self, record: MemoryPromotionWrite<'_>) -> Result<()> {
        self.connection.execute(
            r#"
            INSERT INTO memory_promotions (
                id,
                workflow_id,
                organization_id,
                brand_id,
                product_id,
                user_id,
                channel_id,
                from_scope,
                to_scope,
                source_path,
                target_path,
                visibility,
                shareability,
                approved_by,
                reason,
                summary_sha256,
                promoted_memory_sha256,
                data_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
            ON CONFLICT(id) DO UPDATE SET
                workflow_id=excluded.workflow_id,
                organization_id=excluded.organization_id,
                brand_id=excluded.brand_id,
                product_id=excluded.product_id,
                user_id=excluded.user_id,
                channel_id=excluded.channel_id,
                from_scope=excluded.from_scope,
                to_scope=excluded.to_scope,
                source_path=excluded.source_path,
                target_path=excluded.target_path,
                visibility=excluded.visibility,
                shareability=excluded.shareability,
                approved_by=excluded.approved_by,
                reason=excluded.reason,
                summary_sha256=excluded.summary_sha256,
                promoted_memory_sha256=excluded.promoted_memory_sha256,
                data_json=excluded.data_json
            "#,
            params![
                record.id,
                record.workflow_id,
                record.organization_id,
                record.brand_id,
                record.product_id,
                record.user_id,
                record.channel_id,
                record.from_scope,
                record.to_scope,
                record.source_path,
                record.target_path,
                record.visibility,
                record.shareability,
                record.approved_by,
                record.reason,
                record.summary_sha256,
                record.promoted_memory_sha256,
                serde_json::to_string(record.data)?,
            ],
        )?;
        Ok(())
    }

    pub fn list_memory_promotions(
        &self,
        query: MemoryPromotionQuery<'_>,
    ) -> Result<Vec<StoredMemoryPromotionRecord>> {
        let mut statement = self.connection.prepare(
            r#"
            SELECT
                id,
                workflow_id,
                organization_id,
                brand_id,
                product_id,
                user_id,
                channel_id,
                from_scope,
                to_scope,
                source_path,
                target_path,
                visibility,
                shareability,
                approved_by,
                reason,
                summary_sha256,
                promoted_memory_sha256,
                data_json,
                created_at
            FROM memory_promotions
            ORDER BY created_at DESC, id ASC
            "#,
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, String>(12)?,
                row.get::<_, String>(13)?,
                row.get::<_, String>(14)?,
                row.get::<_, String>(15)?,
                row.get::<_, String>(16)?,
                row.get::<_, String>(17)?,
                row.get::<_, String>(18)?,
            ))
        })?;
        let mut records = Vec::new();
        for row in rows {
            let (
                id,
                workflow_id_value,
                organization_id_value,
                brand_id_value,
                product_id_value,
                user_id_value,
                channel_id_value,
                from_scope_value,
                to_scope_value,
                source_path,
                target_path,
                visibility,
                shareability,
                approved_by_value,
                reason,
                summary_sha256,
                promoted_memory_sha256,
                data_json,
                created_at,
            ) = row?;
            if query
                .workflow_id
                .is_some_and(|filter| filter != workflow_id_value.as_str())
            {
                continue;
            }
            if query
                .organization_id
                .is_some_and(|filter| filter != organization_id_value.as_str())
            {
                continue;
            }
            if query
                .brand_id
                .is_some_and(|filter| filter != brand_id_value.as_str())
            {
                continue;
            }
            if query
                .product_id
                .is_some_and(|filter| filter != product_id_value.as_str())
            {
                continue;
            }
            if query
                .from_scope
                .is_some_and(|filter| filter != from_scope_value.as_str())
            {
                continue;
            }
            if query
                .to_scope
                .is_some_and(|filter| filter != to_scope_value.as_str())
            {
                continue;
            }
            if query
                .approved_by
                .is_some_and(|filter| filter != approved_by_value.as_str())
            {
                continue;
            }
            records.push(StoredMemoryPromotionRecord {
                id,
                workflow_id: workflow_id_value,
                organization_id: organization_id_value,
                brand_id: brand_id_value,
                product_id: product_id_value,
                user_id: user_id_value,
                channel_id: channel_id_value,
                from_scope: from_scope_value,
                to_scope: to_scope_value,
                source_path,
                target_path,
                visibility,
                shareability,
                approved_by: approved_by_value,
                reason,
                summary_sha256,
                promoted_memory_sha256,
                data: serde_json::from_str(&data_json)?,
                created_at,
            });
        }
        Ok(records)
    }

    pub fn record_event(
        &self,
        workflow_id: &str,
        kind: &str,
        data: &serde_json::Value,
    ) -> Result<()> {
        self.connection.execute(
            "INSERT INTO events (workflow_id, kind, data_json) VALUES (?1, ?2, ?3)",
            params![workflow_id, kind, serde_json::to_string(data)?],
        )?;
        let event_id = self.connection.last_insert_rowid().to_string();
        let workflow = self.load_workflow(workflow_id).ok();
        let tenant_context = workflow
            .as_ref()
            .map(|workflow| serde_json::to_value(&workflow.intent.operating_context))
            .transpose()?
            .unwrap_or_else(default_operating_context_json);
        self.insert_global_event(GlobalEventWrite {
            source: "workflow_event",
            source_id: &event_id,
            workflow_id: Some(workflow_id),
            kind,
            origin: &extract_event_origin(data),
            status: "recorded",
            data,
            tenant_context: &tenant_context,
        })?;
        if let Some(workflow) = workflow {
            self.save_tenant_index_record(
                "event",
                &event_id,
                workflow_id,
                &workflow,
                "events",
                &serde_json::json!({
                    "event_id": event_id,
                    "kind": kind,
                    "data": data,
                }),
            )?;
        }
        Ok(())
    }

    pub fn save_runtime_secret(&self, write: RuntimeSecretVaultWrite<'_>) -> Result<i64> {
        let workflow_key = write.workflow_id.unwrap_or("");
        let vault_key = runtime_secret_vault_key(workflow_key, write.vault_reference);
        let encrypted_value = self.runtime_secret_cipher.encrypt(
            &vault_key,
            write.value_sha256,
            write.value_len,
            write.secret_value.as_bytes(),
        )?;
        let organization_id = tenant_context_identity_id(write.tenant_context, "organization");
        let brand_id = tenant_context_identity_id(write.tenant_context, "brand");
        let product_id = tenant_context_identity_id(write.tenant_context, "product");
        let user_id = tenant_context_identity_id(write.tenant_context, "user");
        let channel_id = tenant_context_identity_id(write.tenant_context, "channel");
        self.connection.execute(
            r#"
            INSERT INTO runtime_secret_vault (
                vault_key, vault_reference, workflow_id, scope, provider, kind,
                classification, secret_value, value_sha256, value_len, source,
                organization_id, brand_id, product_id, user_id, channel_id,
                tenant_context_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
            ON CONFLICT(vault_key) DO UPDATE SET
                scope=excluded.scope,
                provider=excluded.provider,
                kind=excluded.kind,
                classification=excluded.classification,
                secret_value=excluded.secret_value,
                value_sha256=excluded.value_sha256,
                value_len=excluded.value_len,
                source=excluded.source,
                organization_id=excluded.organization_id,
                brand_id=excluded.brand_id,
                product_id=excluded.product_id,
                user_id=excluded.user_id,
                channel_id=excluded.channel_id,
                tenant_context_json=excluded.tenant_context_json,
                updated_at=CURRENT_TIMESTAMP
            "#,
            params![
                vault_key,
                write.vault_reference,
                workflow_key,
                write.scope,
                write.provider,
                write.kind,
                write.classification,
                encrypted_value,
                write.value_sha256,
                write.value_len as i64,
                write.source,
                organization_id,
                brand_id,
                product_id,
                user_id,
                channel_id,
                serde_json::to_string(write.tenant_context)?,
            ],
        )?;
        let data = serde_json::json!({
            "schema_version": "forge.runtime.secret_vault.audit.v1",
            "action": "write",
            "vault_reference": write.vault_reference,
            "workflow_id": write.workflow_id,
            "scope": write.scope,
            "provider": write.provider,
            "kind": write.kind,
            "classification": write.classification,
            "source": write.source,
            "value_sha256": write.value_sha256,
            "value_len": write.value_len,
            "redaction": "secret_value_redacted",
        });
        self.insert_global_event(GlobalEventWrite {
            source: "runtime_secret_vault",
            source_id: &vault_key,
            workflow_id: write.workflow_id,
            kind: "runtime_secret_vault_write",
            origin: write.origin,
            status: "stored",
            data: &data,
            tenant_context: write.tenant_context,
        })
    }

    pub fn resolve_runtime_secret(
        &self,
        access: RuntimeSecretVaultAccess<'_>,
    ) -> Result<RuntimeSecretVaultResolve> {
        let workflow_key = access.workflow_id.unwrap_or("");
        let organization_id = tenant_context_identity_id(access.tenant_context, "organization");
        let brand_id = tenant_context_identity_id(access.tenant_context, "brand");
        let product_id = tenant_context_identity_id(access.tenant_context, "product");
        let record = self
            .connection
            .query_row(
                r#"
                SELECT vault_key, workflow_id, secret_value, value_sha256, value_len
                FROM runtime_secret_vault
                WHERE vault_reference = ?1
                  AND workflow_id IN (?2, '')
                  AND organization_id = ?3
                  AND brand_id = ?4
                  AND product_id = ?5
                ORDER BY CASE WHEN workflow_id = ?2 THEN 0 ELSE 1 END
                LIMIT 1
                "#,
                params![
                    access.vault_reference,
                    workflow_key,
                    organization_id,
                    brand_id,
                    product_id,
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .optional()?;
        let status = if record.is_some() {
            if access.allowed {
                "allowed"
            } else {
                "denied"
            }
        } else {
            "missing"
        };
        let (_resolved_workflow_id, value_sha256, value_len) = record
            .as_ref()
            .map(
                |(_vault_key, workflow_id, _secret_value, value_sha256, value_len)| {
                    (
                        workflow_id.clone(),
                        value_sha256.clone(),
                        (*value_len).max(0) as usize,
                    )
                },
            )
            .unwrap_or_else(|| (workflow_key.to_string(), String::new(), 0));
        let data = serde_json::json!({
            "schema_version": "forge.runtime.secret_vault.audit.v1",
            "action": "resolve",
            "vault_reference": access.vault_reference,
            "workflow_id": access.workflow_id,
            "requester": access.requester,
            "result": status,
            "value_sha256": value_sha256,
            "value_len": value_len,
            "redaction": "secret_value_redacted",
        });
        let audit_event_id = self.insert_global_event(GlobalEventWrite {
            source: "runtime_secret_vault",
            source_id: access.vault_reference,
            workflow_id: access.workflow_id,
            kind: "runtime_secret_vault_access",
            origin: access.origin,
            status,
            data: &data,
            tenant_context: access.tenant_context,
        })?;
        let Some((vault_key, resolved_workflow_id, encrypted_value, value_sha256, value_len)) =
            record
        else {
            anyhow::bail!(
                "runtime secret vault reference `{}` was not found for current tenant",
                access.vault_reference
            );
        };
        if !access.allowed {
            anyhow::bail!(
                "runtime secret vault access denied for `{}`",
                access.vault_reference
            );
        }
        let value_len = usize::try_from(value_len)
            .context("runtime secret vault contains an invalid value length")?;
        let (plaintext, _requires_rotation) = self.runtime_secret_cipher.decrypt(
            &vault_key,
            &value_sha256,
            value_len,
            &encrypted_value,
        )?;
        let secret_value = String::from_utf8(plaintext.to_vec())
            .context("runtime secret vault decrypted value is not valid UTF-8")?;
        Ok(RuntimeSecretVaultResolve {
            vault_reference: access.vault_reference.to_string(),
            workflow_id: if resolved_workflow_id.is_empty() {
                None
            } else {
                Some(resolved_workflow_id)
            },
            secret_value,
            value_sha256,
            value_len,
            audit_event_id,
        })
    }

    pub fn load_global_events(&self) -> Result<Vec<StoredGlobalEventRecord>> {
        let mut statement = self.connection.prepare(
            r#"
            SELECT id, source, source_id, workflow_id, kind, origin, status,
                   organization_id, brand_id, product_id, user_id, channel_id,
                   tenant_context_json, data_json, created_at
            FROM global_events
            ORDER BY id ASC
            "#,
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, String>(12)?,
                row.get::<_, String>(13)?,
                row.get::<_, String>(14)?,
            ))
        })?;
        let mut events = Vec::new();
        for row in rows {
            let (
                id,
                source,
                source_id,
                workflow_id,
                kind,
                origin,
                status,
                organization_id,
                brand_id,
                product_id,
                user_id,
                channel_id,
                tenant_context_json,
                data_json,
                created_at,
            ) = row?;
            events.push(StoredGlobalEventRecord {
                id,
                source,
                source_id,
                workflow_id,
                kind,
                origin,
                status,
                organization_id,
                brand_id,
                product_id,
                user_id,
                channel_id,
                tenant_context: serde_json::from_str(&tenant_context_json)?,
                data: serde_json::from_str(&data_json)?,
                created_at,
            });
        }
        Ok(events)
    }

    pub fn load_event_observability_index(
        &self,
        workflow_id: Option<&str>,
        organization_id: Option<&str>,
        brand_id: Option<&str>,
        product_id: Option<&str>,
        node_ref: Option<&str>,
        addon_id: Option<&str>,
    ) -> Result<Vec<StoredEventObservabilityRecord>> {
        let workflow_filter = normalize_optional_filter(workflow_id);
        let organization_filter = normalize_optional_filter(organization_id);
        let brand_filter = normalize_optional_filter(brand_id);
        let product_filter = normalize_optional_filter(product_id);
        let node_filter = normalize_optional_filter(node_ref);
        let addon_filter = normalize_optional_filter(addon_id);
        let mut statement = self.connection.prepare(
            r#"
            SELECT global_event_id, workflow_id, kind, category, severity, origin, source,
                   organization_id, brand_id, product_id, node_ref, addon_id,
                   duration_ms, retry_count, wait_state, wait_seconds,
                   context_budget_bytes, selected_context_bytes, context_remaining_bytes,
                   context_pressure_bps, context_pressure_state, memory_level, memory_scope,
                   data_json, created_at
            FROM event_observability_index
            WHERE (?1 IS NULL OR workflow_id = ?1)
              AND (?2 IS NULL OR organization_id = ?2)
              AND (?3 IS NULL OR brand_id = ?3)
              AND (?4 IS NULL OR product_id = ?4)
              AND (?5 IS NULL OR node_ref = ?5)
              AND (?6 IS NULL OR addon_id = ?6)
            ORDER BY global_event_id ASC
            "#,
        )?;
        let rows = statement.query_map(
            params![
                workflow_filter,
                organization_filter,
                brand_filter,
                product_filter,
                node_filter,
                addon_filter
            ],
            stored_event_observability_from_row,
        )?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

    pub fn replace_cost_ledger_index_records(
        &self,
        workflow_ids: &[String],
        records: &[CostLedgerIndexWrite],
    ) -> Result<usize> {
        for workflow_id in workflow_ids {
            self.connection.execute(
                "DELETE FROM cost_ledger_index WHERE workflow_id = ?1",
                params![workflow_id],
            )?;
        }
        for record in records {
            self.connection.execute(
                r#"
                INSERT INTO cost_ledger_index (
                    row_key,
                    source_kind,
                    workflow_id,
                    task_id,
                    event_id,
                    organization_id,
                    brand_id,
                    product_id,
                    addon_id,
                    executor,
                    model_call_required,
                    model_call_avoided,
                    estimated_task_cost_usd,
                    observed_event_cost_usd,
                    tokens_in,
                    tokens_out,
                    data_json,
                    updated_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, CURRENT_TIMESTAMP)
                ON CONFLICT(row_key) DO UPDATE SET
                    source_kind=excluded.source_kind,
                    workflow_id=excluded.workflow_id,
                    task_id=excluded.task_id,
                    event_id=excluded.event_id,
                    organization_id=excluded.organization_id,
                    brand_id=excluded.brand_id,
                    product_id=excluded.product_id,
                    addon_id=excluded.addon_id,
                    executor=excluded.executor,
                    model_call_required=excluded.model_call_required,
                    model_call_avoided=excluded.model_call_avoided,
                    estimated_task_cost_usd=excluded.estimated_task_cost_usd,
                    observed_event_cost_usd=excluded.observed_event_cost_usd,
                    tokens_in=excluded.tokens_in,
                    tokens_out=excluded.tokens_out,
                    data_json=excluded.data_json,
                    updated_at=CURRENT_TIMESTAMP
                "#,
                params![
                    record.row_key,
                    record.source_kind,
                    record.workflow_id,
                    record.task_id,
                    record.event_id,
                    record.organization_id,
                    record.brand_id,
                    record.product_id,
                    record.addon_id,
                    record.executor,
                    record.model_call_required,
                    record.model_call_avoided,
                    record.estimated_task_cost_usd,
                    record.observed_event_cost_usd,
                    record.tokens_in,
                    record.tokens_out,
                    serde_json::to_string(&record.data)?,
                ],
            )?;
        }
        Ok(records.len())
    }

    pub fn load_cost_ledger_index(
        &self,
        query: CostLedgerIndexQuery<'_>,
    ) -> Result<Vec<StoredCostLedgerIndexRecord>> {
        let workflow_filter = normalize_optional_filter(query.workflow_id);
        let organization_filter = normalize_optional_filter(query.organization_id);
        let brand_filter = normalize_optional_filter(query.brand_id);
        let product_filter = normalize_optional_filter(query.product_id);
        let source_kind_filter = normalize_optional_filter(query.source_kind);
        let addon_filter = normalize_optional_filter(query.addon_id);
        let limit = query.limit.filter(|limit| *limit > 0).unwrap_or(500);
        let mut statement = self.connection.prepare(
            r#"
            SELECT row_key, source_kind, workflow_id, task_id, event_id,
                   organization_id, brand_id, product_id, addon_id, executor,
                   model_call_required, model_call_avoided,
                   estimated_task_cost_usd, observed_event_cost_usd,
                   tokens_in, tokens_out, data_json, created_at, updated_at
            FROM cost_ledger_index
            WHERE (?1 IS NULL OR workflow_id = ?1)
              AND (?2 IS NULL OR organization_id = ?2)
              AND (?3 IS NULL OR brand_id = ?3)
              AND (?4 IS NULL OR product_id = ?4)
              AND (?5 IS NULL OR source_kind = ?5)
              AND (?6 IS NULL OR addon_id = ?6)
            ORDER BY updated_at DESC, row_key ASC
            LIMIT ?7
            "#,
        )?;
        let rows = statement.query_map(
            params![
                workflow_filter,
                organization_filter,
                brand_filter,
                product_filter,
                source_kind_filter,
                addon_filter,
                i64::try_from(limit).unwrap_or(i64::MAX)
            ],
            stored_cost_ledger_index_from_row,
        )?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

    pub fn load_cost_ledger_retention_candidates(
        &self,
        query: CostLedgerRetentionQuery<'_>,
    ) -> Result<Vec<StoredCostLedgerIndexRecord>> {
        let workflow_filter = normalize_optional_filter(query.index.workflow_id);
        let organization_filter = normalize_optional_filter(query.index.organization_id);
        let brand_filter = normalize_optional_filter(query.index.brand_id);
        let product_filter = normalize_optional_filter(query.index.product_id);
        let source_kind_filter = normalize_optional_filter(query.index.source_kind);
        let addon_filter = normalize_optional_filter(query.index.addon_id);
        let limit = query.index.limit.filter(|limit| *limit > 0).unwrap_or(500);
        let mut statement = self.connection.prepare(
            r#"
            SELECT row_key, source_kind, workflow_id, task_id, event_id,
                   organization_id, brand_id, product_id, addon_id, executor,
                   model_call_required, model_call_avoided,
                   estimated_task_cost_usd, observed_event_cost_usd,
                   tokens_in, tokens_out, data_json, created_at, updated_at
            FROM cost_ledger_index
            WHERE datetime(updated_at) < datetime(?1)
              AND (?2 IS NULL OR workflow_id = ?2)
              AND (?3 IS NULL OR organization_id = ?3)
              AND (?4 IS NULL OR brand_id = ?4)
              AND (?5 IS NULL OR product_id = ?5)
              AND (?6 IS NULL OR source_kind = ?6)
              AND (?7 IS NULL OR addon_id = ?7)
            ORDER BY updated_at ASC, row_key ASC
            LIMIT ?8
            "#,
        )?;
        let rows = statement.query_map(
            params![
                query.updated_before,
                workflow_filter,
                organization_filter,
                brand_filter,
                product_filter,
                source_kind_filter,
                addon_filter,
                i64::try_from(limit).unwrap_or(i64::MAX)
            ],
            stored_cost_ledger_index_from_row,
        )?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }

    pub fn delete_cost_ledger_index_rows(&self, row_keys: &[String]) -> Result<usize> {
        let mut deleted = 0;
        for row_key in row_keys {
            deleted += self.connection.execute(
                "DELETE FROM cost_ledger_index WHERE row_key = ?1",
                params![row_key],
            )?;
        }
        Ok(deleted)
    }

    pub fn save_headroom_blob(
        &self,
        record: &HeadroomBlobWrite,
    ) -> Result<StoredHeadroomBlobRecord> {
        self.connection.execute(
            r#"
            INSERT INTO harness_headroom_blobs (
                original_sha256,
                source,
                content_kind,
                strategy,
                reversible,
                original_bytes,
                compressed_sha256,
                compressed_bytes,
                estimated_original_tokens,
                estimated_compressed_tokens,
                estimated_saved_tokens,
                budget_tokens,
                budget_status,
                routing_json,
                original_content,
                compressed_content,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, CURRENT_TIMESTAMP)
            ON CONFLICT(original_sha256) DO UPDATE SET
                source=excluded.source,
                content_kind=excluded.content_kind,
                strategy=excluded.strategy,
                reversible=excluded.reversible,
                original_bytes=excluded.original_bytes,
                compressed_sha256=excluded.compressed_sha256,
                compressed_bytes=excluded.compressed_bytes,
                estimated_original_tokens=excluded.estimated_original_tokens,
                estimated_compressed_tokens=excluded.estimated_compressed_tokens,
                estimated_saved_tokens=excluded.estimated_saved_tokens,
                budget_tokens=excluded.budget_tokens,
                budget_status=excluded.budget_status,
                routing_json=excluded.routing_json,
                original_content=excluded.original_content,
                compressed_content=excluded.compressed_content,
                updated_at=CURRENT_TIMESTAMP
            "#,
            params![
                record.original_sha256,
                record.source,
                record.content_kind,
                record.strategy,
                record.reversible,
                record.original_bytes,
                record.compressed_sha256,
                record.compressed_bytes,
                record.estimated_original_tokens,
                record.estimated_compressed_tokens,
                record.estimated_saved_tokens,
                record.budget_tokens,
                record.budget_status,
                serde_json::to_string(&record.routing)?,
                record.original_content,
                record.compressed_content,
            ],
        )?;
        self.load_headroom_blob_by_sha(&record.original_sha256)?
            .with_context(|| "persisted headroom blob was not readable after save")
    }

    pub fn load_headroom_blob_by_sha(
        &self,
        original_sha256: &str,
    ) -> Result<Option<StoredHeadroomBlobRecord>> {
        let sha = original_sha256.trim();
        if sha.is_empty() {
            return Ok(None);
        }
        self.connection
            .query_row(
                r#"
                SELECT source, content_kind, strategy, reversible, original_sha256,
                       original_bytes, compressed_sha256, compressed_bytes,
                       estimated_original_tokens, estimated_compressed_tokens,
                       estimated_saved_tokens, budget_tokens, budget_status,
                       routing_json, original_content, compressed_content,
                       created_at, updated_at
                FROM harness_headroom_blobs
                WHERE original_sha256 = ?1
                "#,
                params![sha],
                stored_headroom_blob_from_row,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn load_headroom_blobs(
        &self,
        source: Option<&str>,
        content_kind: Option<&str>,
    ) -> Result<Vec<StoredHeadroomBlobRecord>> {
        let source = normalize_optional_filter(source);
        let content_kind = normalize_optional_filter(content_kind);
        let mut statement = self.connection.prepare(
            r#"
            SELECT source, content_kind, strategy, reversible, original_sha256,
                   original_bytes, compressed_sha256, compressed_bytes,
                   estimated_original_tokens, estimated_compressed_tokens,
                   estimated_saved_tokens, budget_tokens, budget_status,
                   routing_json, original_content, compressed_content,
                   created_at, updated_at
            FROM harness_headroom_blobs
            WHERE (?1 IS NULL OR source = ?1)
              AND (?2 IS NULL OR content_kind = ?2)
            ORDER BY updated_at DESC, estimated_saved_tokens DESC
            "#,
        )?;
        let rows = statement.query_map(
            params![source.as_deref(), content_kind.as_deref()],
            stored_headroom_blob_from_row,
        )?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    fn backfill_missing_event_observability_index(&self) -> Result<()> {
        let migration_batch_enqueued =
            self.enqueue_event_observability_reconciliation_cursor_batch()?;
        if !migration_batch_enqueued {
            self.enqueue_missing_event_observability_records()?;
        }
        self.backfill_queued_event_observability_records()
    }

    fn enqueue_event_observability_reconciliation_cursor_batch(&self) -> Result<bool> {
        let cursor_pending: bool = self.connection.query_row(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM store_reconciliation_cursors
                WHERE cursor_key = ?1
                  AND status = 'pending'
            )
            "#,
            params![EVENT_OBSERVABILITY_RECONCILIATION_CURSOR],
            |row| row.get(0),
        )?;
        if !cursor_pending {
            return Ok(false);
        }
        if self.connection.is_autocommit() {
            return self.with_transaction(|| {
                self.enqueue_event_observability_reconciliation_cursor_batch_in_transaction()
            });
        }
        self.enqueue_event_observability_reconciliation_cursor_batch_in_transaction()
    }

    fn enqueue_event_observability_reconciliation_cursor_batch_in_transaction(
        &self,
    ) -> Result<bool> {
        let cursor = self
            .connection
            .query_row(
                r#"
                SELECT last_global_event_id, upper_bound_global_event_id, status
                FROM store_reconciliation_cursors
                WHERE cursor_key = ?1
                "#,
                params![EVENT_OBSERVABILITY_RECONCILIATION_CURSOR],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((last_global_event_id, upper_bound_global_event_id, status)) = cursor else {
            return Ok(false);
        };
        if status != "pending" {
            return Ok(false);
        }

        let candidate_ids = {
            let mut statement = self.connection.prepare(
                r#"
                SELECT id
                FROM global_events
                WHERE id > ?1
                  AND id <= ?2
                ORDER BY id ASC
                LIMIT ?3
                "#,
            )?;
            let rows = statement.query_map(
                params![
                    last_global_event_id,
                    upper_bound_global_event_id,
                    EVENT_OBSERVABILITY_RECONCILIATION_BATCH_SIZE as i64
                ],
                |row| row.get::<_, i64>(0),
            )?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };

        let Some(last_enqueued_id) = candidate_ids.last().copied() else {
            self.connection.execute(
                r#"
                UPDATE store_reconciliation_cursors
                SET status = 'completed',
                    updated_at = CURRENT_TIMESTAMP
                WHERE cursor_key = ?1
                "#,
                params![EVENT_OBSERVABILITY_RECONCILIATION_CURSOR],
            )?;
            return Ok(false);
        };
        for global_event_id in candidate_ids {
            self.connection.execute(
                r#"
                INSERT OR IGNORE INTO event_observability_reconciliation_queue (global_event_id)
                VALUES (?1)
                "#,
                params![global_event_id],
            )?;
        }
        let next_status = if last_enqueued_id >= upper_bound_global_event_id {
            "completed"
        } else {
            "pending"
        };
        self.connection.execute(
            r#"
            UPDATE store_reconciliation_cursors
            SET last_global_event_id = ?2,
                status = ?3,
                updated_at = CURRENT_TIMESTAMP
            WHERE cursor_key = ?1
            "#,
            params![
                EVENT_OBSERVABILITY_RECONCILIATION_CURSOR,
                last_enqueued_id,
                next_status
            ],
        )?;
        Ok(true)
    }

    fn enqueue_missing_event_observability_records(&self) -> Result<()> {
        let missing: bool = self.connection.query_row(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM global_events g
                LEFT JOIN event_observability_index o
                  ON o.global_event_id = g.id
                LEFT JOIN event_observability_reconciliation_queue q
                  ON q.global_event_id = g.id
                WHERE o.global_event_id IS NULL
                  AND q.global_event_id IS NULL
                LIMIT 1
            )
            "#,
            [],
            |row| row.get(0),
        )?;
        if !missing {
            return Ok(());
        }
        self.connection.execute(
            r#"
            INSERT OR IGNORE INTO event_observability_reconciliation_queue (global_event_id)
            SELECT g.id
            FROM global_events g
            LEFT JOIN event_observability_index o
              ON o.global_event_id = g.id
            LEFT JOIN event_observability_reconciliation_queue q
              ON q.global_event_id = g.id
            WHERE o.global_event_id IS NULL
              AND q.global_event_id IS NULL
            ORDER BY g.id ASC
            LIMIT ?1
            "#,
            params![EVENT_OBSERVABILITY_RECONCILIATION_BATCH_SIZE as i64],
        )?;
        Ok(())
    }

    fn backfill_queued_event_observability_records(&self) -> Result<()> {
        let upper_bound: Option<i64> = self.connection.query_row(
            "SELECT max(global_event_id) FROM event_observability_reconciliation_queue",
            [],
            |row| row.get(0),
        )?;
        let Some(upper_bound) = upper_bound else {
            return Ok(());
        };

        for _ in 0..EVENT_OBSERVABILITY_RECONCILIATION_BATCH_SIZE {
            let candidate_id = self
                .connection
                .query_row(
                    r#"
                    SELECT global_event_id
                    FROM event_observability_reconciliation_queue
                    WHERE global_event_id <= ?1
                    ORDER BY global_event_id ASC
                    LIMIT 1
                    "#,
                    params![upper_bound],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;
            let Some(candidate_id) = candidate_id else {
                break;
            };
            self.reconcile_queued_event_observability_record(candidate_id)?;
        }

        Ok(())
    }

    fn reconcile_queued_event_observability_record(&self, global_event_id: i64) -> Result<bool> {
        if self.connection.is_autocommit() {
            return self.with_transaction(|| {
                self.reconcile_queued_event_observability_record_in_transaction(global_event_id)
            });
        }
        self.reconcile_queued_event_observability_record_in_transaction(global_event_id)
    }

    fn reconcile_queued_event_observability_record_in_transaction(
        &self,
        global_event_id: i64,
    ) -> Result<bool> {
        let record = self
            .connection
            .query_row(
                r#"
                SELECT g.id, g.source, g.source_id, g.workflow_id, g.kind, g.origin, g.status,
                       g.organization_id, g.brand_id, g.product_id, g.user_id, g.channel_id,
                       g.tenant_context_json, g.data_json, g.created_at
                FROM event_observability_reconciliation_queue q
                JOIN global_events g ON g.id = q.global_event_id
                WHERE q.global_event_id = ?1
                "#,
                params![global_event_id],
                stored_global_event_from_row,
            )
            .optional()?;
        let Some(record) = record else {
            self.complete_event_observability_reconciliation(global_event_id)?;
            return Ok(false);
        };

        self.upsert_event_observability_index_record(EventObservabilityIndexWrite {
            global_event_id: record.id,
            workflow_id: record.workflow_id.as_deref(),
            kind: &record.kind,
            origin: &record.origin,
            source: &record.source,
            organization_id: &record.organization_id,
            brand_id: &record.brand_id,
            product_id: &record.product_id,
            data: &record.data,
            created_at: &record.created_at,
        })?;
        self.complete_event_observability_reconciliation(global_event_id)?;
        Ok(true)
    }

    fn reconcile_event_observability_record(
        &self,
        write: EventObservabilityIndexWrite<'_>,
    ) -> Result<()> {
        if self.connection.is_autocommit() {
            return self.with_transaction(|| {
                self.reconcile_event_observability_record_in_transaction(write)
            });
        }
        self.reconcile_event_observability_record_in_transaction(write)
    }

    fn reconcile_event_observability_record_in_transaction(
        &self,
        write: EventObservabilityIndexWrite<'_>,
    ) -> Result<()> {
        let global_event_id = write.global_event_id;
        let queued: bool = self.connection.query_row(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM event_observability_reconciliation_queue
                WHERE global_event_id = ?1
            )
            "#,
            params![global_event_id],
            |row| row.get(0),
        )?;
        if !queued {
            return Ok(());
        }
        self.upsert_event_observability_index_record(write)?;
        self.complete_event_observability_reconciliation(global_event_id)
    }

    fn complete_event_observability_reconciliation(&self, global_event_id: i64) -> Result<()> {
        self.connection.execute(
            "DELETE FROM event_observability_reconciliation_queue WHERE global_event_id = ?1",
            params![global_event_id],
        )?;
        Ok(())
    }

    fn global_event_has_durable_observability_state(&self, global_event_id: i64) -> Result<bool> {
        self.connection
            .query_row(
                r#"
                SELECT EXISTS(
                    SELECT 1
                    FROM global_events g
                    WHERE g.id = ?1
                      AND (
                          EXISTS (
                              SELECT 1
                              FROM event_observability_reconciliation_queue q
                              WHERE q.global_event_id = g.id
                          )
                          OR EXISTS (
                              SELECT 1
                              FROM event_observability_index o
                              WHERE o.global_event_id = g.id
                          )
                      )
                )
                "#,
                params![global_event_id],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub fn try_save_event_service(&self, service: EventServiceWrite<'_>) -> Result<bool> {
        let organization_id = tenant_context_identity_id(service.tenant_context, "organization");
        let brand_id = tenant_context_identity_id(service.tenant_context, "brand");
        let product_id = tenant_context_identity_id(service.tenant_context, "product");
        let user_id = tenant_context_identity_id(service.tenant_context, "user");
        let channel_id = tenant_context_identity_id(service.tenant_context, "channel");
        let changed = self.connection.execute(
            r#"
            INSERT INTO event_services (
                id,
                service_kind,
                status,
                organization_id,
                brand_id,
                product_id,
                user_id,
                channel_id,
                tenant_context_json,
                lease_owner,
                lease_id,
                lease_acquired_at,
                lease_expires_at,
                last_heartbeat_at,
                heartbeat_ttl_seconds,
                data_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            ON CONFLICT(id) DO UPDATE SET
                service_kind=excluded.service_kind,
                status=excluded.status,
                organization_id=excluded.organization_id,
                brand_id=excluded.brand_id,
                product_id=excluded.product_id,
                user_id=excluded.user_id,
                channel_id=excluded.channel_id,
                tenant_context_json=excluded.tenant_context_json,
                lease_owner=excluded.lease_owner,
                lease_id=excluded.lease_id,
                lease_acquired_at=excluded.lease_acquired_at,
                lease_expires_at=excluded.lease_expires_at,
                last_heartbeat_at=excluded.last_heartbeat_at,
                heartbeat_ttl_seconds=excluded.heartbeat_ttl_seconds,
                data_json=excluded.data_json,
                updated_at=CURRENT_TIMESTAMP
            WHERE event_services.lease_expires_at <= ?17
               OR event_services.status IN ('completed', 'completed_with_failures', 'failed', 'stopped')
            "#,
            params![
                service.id,
                service.service_kind,
                service.status,
                organization_id,
                brand_id,
                product_id,
                user_id,
                channel_id,
                serde_json::to_string(service.tenant_context)?,
                service.lease_owner,
                service.lease_id,
                service.lease_acquired_at,
                service.lease_expires_at,
                service.last_heartbeat_at,
                i64::try_from(service.heartbeat_ttl_seconds).unwrap_or(i64::MAX),
                serde_json::to_string(service.data)?,
                service.lease_acquired_at,
            ],
        )?;
        Ok(changed == 1)
    }

    pub fn save_event_service(&self, service: EventServiceWrite<'_>) -> Result<()> {
        let organization_id = tenant_context_identity_id(service.tenant_context, "organization");
        let brand_id = tenant_context_identity_id(service.tenant_context, "brand");
        let product_id = tenant_context_identity_id(service.tenant_context, "product");
        let user_id = tenant_context_identity_id(service.tenant_context, "user");
        let channel_id = tenant_context_identity_id(service.tenant_context, "channel");
        self.connection.execute(
            r#"
            INSERT INTO event_services (
                id,
                service_kind,
                status,
                organization_id,
                brand_id,
                product_id,
                user_id,
                channel_id,
                tenant_context_json,
                lease_owner,
                lease_id,
                lease_acquired_at,
                lease_expires_at,
                last_heartbeat_at,
                heartbeat_ttl_seconds,
                data_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            ON CONFLICT(id) DO UPDATE SET
                service_kind=excluded.service_kind,
                status=excluded.status,
                organization_id=excluded.organization_id,
                brand_id=excluded.brand_id,
                product_id=excluded.product_id,
                user_id=excluded.user_id,
                channel_id=excluded.channel_id,
                tenant_context_json=excluded.tenant_context_json,
                lease_owner=excluded.lease_owner,
                lease_id=excluded.lease_id,
                lease_acquired_at=excluded.lease_acquired_at,
                lease_expires_at=excluded.lease_expires_at,
                last_heartbeat_at=excluded.last_heartbeat_at,
                heartbeat_ttl_seconds=excluded.heartbeat_ttl_seconds,
                data_json=excluded.data_json,
                updated_at=CURRENT_TIMESTAMP
            "#,
            params![
                service.id,
                service.service_kind,
                service.status,
                organization_id,
                brand_id,
                product_id,
                user_id,
                channel_id,
                serde_json::to_string(service.tenant_context)?,
                service.lease_owner,
                service.lease_id,
                service.lease_acquired_at,
                service.lease_expires_at,
                service.last_heartbeat_at,
                i64::try_from(service.heartbeat_ttl_seconds).unwrap_or(i64::MAX),
                serde_json::to_string(service.data)?,
            ],
        )?;
        Ok(())
    }

    pub fn load_event_service(&self, id: &str) -> Result<Option<StoredEventServiceRecord>> {
        let mut statement = self.connection.prepare(
            r#"
            SELECT id, service_kind, status, lease_owner, lease_id, lease_acquired_at,
                   lease_expires_at, last_heartbeat_at, heartbeat_ttl_seconds,
                   data_json, created_at, updated_at,
                   organization_id, brand_id, product_id, user_id, channel_id, tenant_context_json
            FROM event_services
            WHERE id = ?1
            "#,
        )?;
        statement
            .query_row(params![id], stored_event_service_from_row)
            .optional()
            .map_err(Into::into)
    }

    pub fn list_event_services(
        &self,
        service_kind: Option<&str>,
        status: Option<&str>,
        limit: usize,
        organization_id: Option<&str>,
        brand_id: Option<&str>,
        product_id: Option<&str>,
    ) -> Result<Vec<StoredEventServiceRecord>> {
        let limit = limit.max(1);
        let kind_filter = service_kind
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let status_filter = status.map(str::trim).filter(|value| !value.is_empty());
        let organization_filter = normalize_optional_filter(organization_id);
        let brand_filter = normalize_optional_filter(brand_id);
        let product_filter = normalize_optional_filter(product_id);
        let mut statement = self.connection.prepare(
            r#"
            SELECT id, service_kind, status, lease_owner, lease_id, lease_acquired_at,
                   lease_expires_at, last_heartbeat_at, heartbeat_ttl_seconds,
                   data_json, created_at, updated_at,
                   organization_id, brand_id, product_id, user_id, channel_id, tenant_context_json
            FROM event_services
            WHERE (?1 IS NULL OR service_kind = ?1)
              AND (?2 IS NULL OR status = ?2)
              AND (?3 IS NULL OR organization_id = ?3)
              AND (?4 IS NULL OR brand_id = ?4)
              AND (?5 IS NULL OR product_id = ?5)
            ORDER BY updated_at DESC, created_at DESC, id ASC
            LIMIT ?6
            "#,
        )?;
        let rows = statement.query_map(
            params![
                kind_filter,
                status_filter,
                organization_filter,
                brand_filter,
                product_filter,
                i64::try_from(limit).unwrap_or(i64::MAX)
            ],
            stored_event_service_from_row,
        )?;
        let mut services = Vec::new();
        for row in rows {
            services.push(row?);
        }
        Ok(services)
    }

    fn upsert_event_observability_index_record(
        &self,
        write: EventObservabilityIndexWrite<'_>,
    ) -> Result<()> {
        let observability = build_event_observability(write.kind, write.data);
        self.connection.execute(
            r#"
            INSERT INTO event_observability_index (
                global_event_id,
                workflow_id,
                kind,
                category,
                severity,
                origin,
                source,
                organization_id,
                brand_id,
                product_id,
                node_ref,
                addon_id,
                duration_ms,
                retry_count,
                wait_state,
                wait_seconds,
                context_budget_bytes,
                selected_context_bytes,
                context_remaining_bytes,
                context_pressure_bps,
                context_pressure_state,
                memory_level,
                memory_scope,
                data_json,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, CURRENT_TIMESTAMP)
            ON CONFLICT(global_event_id) DO UPDATE SET
                workflow_id=excluded.workflow_id,
                kind=excluded.kind,
                category=excluded.category,
                severity=excluded.severity,
                origin=excluded.origin,
                source=excluded.source,
                organization_id=excluded.organization_id,
                brand_id=excluded.brand_id,
                product_id=excluded.product_id,
                node_ref=excluded.node_ref,
                addon_id=excluded.addon_id,
                duration_ms=excluded.duration_ms,
                retry_count=excluded.retry_count,
                wait_state=excluded.wait_state,
                wait_seconds=excluded.wait_seconds,
                context_budget_bytes=excluded.context_budget_bytes,
                selected_context_bytes=excluded.selected_context_bytes,
                context_remaining_bytes=excluded.context_remaining_bytes,
                context_pressure_bps=excluded.context_pressure_bps,
                context_pressure_state=excluded.context_pressure_state,
                memory_level=excluded.memory_level,
                memory_scope=excluded.memory_scope,
                data_json=excluded.data_json,
                created_at=excluded.created_at,
                updated_at=CURRENT_TIMESTAMP
            "#,
            params![
                write.global_event_id,
                write.workflow_id.unwrap_or("_global"),
                write.kind,
                categorize_event(write.kind),
                infer_severity(write.kind, write.data),
                write.origin,
                write.source,
                write.organization_id,
                write.brand_id,
                write.product_id,
                observability.node_ref,
                observability.addon_id,
                observability.duration_ms,
                observability.retry_count,
                observability.wait_state,
                observability.wait_seconds,
                observability.context_budget_bytes,
                observability.selected_context_bytes,
                observability.context_remaining_bytes,
                observability.context_pressure_bps,
                observability.context_pressure_state,
                observability.memory_level,
                observability.memory_scope,
                serde_json::to_string(write.data)?,
                write.created_at,
            ],
        )?;
        Ok(())
    }

    fn insert_global_event(&self, write: GlobalEventWrite<'_>) -> Result<i64> {
        let can_defer_observability_reconciliation = self.connection.is_autocommit();
        let organization_id = tenant_context_identity_id(write.tenant_context, "organization");
        let brand_id = tenant_context_identity_id(write.tenant_context, "brand");
        let product_id = tenant_context_identity_id(write.tenant_context, "product");
        let user_id = tenant_context_identity_id(write.tenant_context, "user");
        let channel_id = tenant_context_identity_id(write.tenant_context, "channel");
        self.connection.execute(
            r#"
            INSERT INTO global_events (
                source, source_id, workflow_id, kind, origin, status,
                organization_id, brand_id, product_id, user_id, channel_id,
                tenant_context_json, data_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
            params![
                write.source,
                write.source_id,
                write.workflow_id,
                write.kind,
                write.origin,
                write.status,
                organization_id,
                brand_id,
                product_id,
                user_id,
                channel_id,
                serde_json::to_string(write.tenant_context)?,
                serde_json::to_string(write.data)?,
            ],
        )?;
        let global_event_id = self.connection.last_insert_rowid();
        let created_at: String = self.connection.query_row(
            "SELECT created_at FROM global_events WHERE id = ?1",
            params![global_event_id],
            |row| row.get(0),
        )?;
        let reconciliation =
            self.reconcile_event_observability_record(EventObservabilityIndexWrite {
                global_event_id,
                workflow_id: write.workflow_id,
                kind: write.kind,
                origin: write.origin,
                source: write.source,
                organization_id: &organization_id,
                brand_id: &brand_id,
                product_id: &product_id,
                data: write.data,
                created_at: &created_at,
            });
        if let Err(error) = reconciliation {
            if can_defer_observability_reconciliation
                && self
                    .global_event_has_durable_observability_state(global_event_id)
                    .is_ok_and(|durable| durable)
            {
                return Ok(global_event_id);
            }
            return Err(error);
        }
        Ok(global_event_id)
    }

    pub fn record_global_event(&self, write: GlobalEventWrite<'_>) -> Result<i64> {
        self.insert_global_event(write)
    }

    pub fn load_workflow_events(&self, workflow_id: &str) -> Result<Vec<StoreEvent>> {
        let mut statement = self.connection.prepare(
            r#"
            SELECT id, workflow_id, kind, data_json, created_at
            FROM events
            WHERE workflow_id = ?1
            ORDER BY id ASC
            "#,
        )?;
        let rows = statement.query_map(params![workflow_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        let mut events = Vec::new();
        for row in rows {
            let (id, workflow_id, kind, data_json, created_at) = row?;
            events.push(StoreEvent {
                id,
                workflow_id,
                kind,
                data: serde_json::from_str(&data_json)?,
                created_at,
            });
        }
        Ok(events)
    }

    pub fn save_installed_addon(
        &self,
        id: &str,
        status: &str,
        source: &str,
        manifest: &serde_json::Value,
    ) -> Result<()> {
        self.connection.execute(
            r#"
            INSERT INTO installed_addons (id, status, source, manifest_json, updated_at)
            VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP)
            ON CONFLICT(id) DO UPDATE SET
                status=excluded.status,
                source=excluded.source,
                manifest_json=excluded.manifest_json,
                updated_at=CURRENT_TIMESTAMP
            "#,
            params![id, status, source, serde_json::to_string(manifest)?],
        )?;
        Ok(())
    }

    pub fn load_installed_addon(&self, id: &str) -> Result<StoredAddonRecord> {
        let row: Option<(String, String, String, String, String, String)> = self
            .connection
            .query_row(
                r#"
                SELECT id, status, source, manifest_json, installed_at, updated_at
                FROM installed_addons
                WHERE id = ?1
                "#,
                params![id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .optional()?;
        let (id, status, source, manifest_json, installed_at, updated_at) =
            row.with_context(|| format!("installed addon not found: {id}"))?;
        Ok(StoredAddonRecord {
            id,
            status,
            source,
            manifest: serde_json::from_str(&manifest_json)?,
            installed_at,
            updated_at,
        })
    }

    pub fn list_installed_addons(&self) -> Result<Vec<StoredAddonRecord>> {
        let mut statement = self.connection.prepare(
            r#"
            SELECT id, status, source, manifest_json, installed_at, updated_at
            FROM installed_addons
            ORDER BY id ASC
            "#,
        )?;
        let rows = statement.query_map([], |row| {
            let manifest_json: String = row.get(3)?;
            let manifest = serde_json::from_str(&manifest_json).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    3,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            Ok(StoredAddonRecord {
                id: row.get(0)?,
                status: row.get(1)?,
                source: row.get(2)?,
                manifest,
                installed_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?;
        let mut addons = Vec::new();
        for row in rows {
            addons.push(row?);
        }
        Ok(addons)
    }

    pub fn update_installed_addon_status(&self, id: &str, status: &str) -> Result<()> {
        let changed = self.connection.execute(
            r#"
            UPDATE installed_addons
            SET status = ?2,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?1
            "#,
            params![id, status],
        )?;
        if changed == 0 {
            anyhow::bail!("installed addon not found: {id}");
        }
        Ok(())
    }

    pub fn delete_installed_addon(&self, id: &str) -> Result<()> {
        let changed = self
            .connection
            .execute("DELETE FROM installed_addons WHERE id = ?1", params![id])?;
        if changed == 0 {
            anyhow::bail!("installed addon not found: {id}");
        }
        Ok(())
    }

    pub fn replace_addon_capabilities(
        &self,
        addon_id: &str,
        status: &str,
        capabilities: &[StoredAddonCapabilityWrite],
    ) -> Result<()> {
        self.connection.execute(
            "DELETE FROM addon_capabilities WHERE addon_id = ?1",
            params![addon_id],
        )?;
        for capability in capabilities {
            self.connection.execute(
                r#"
                INSERT INTO addon_capabilities (
                    addon_id,
                    capability_id,
                    status,
                    source,
                    addon_version,
                    title,
                    domains_json,
                    keywords_json,
                    workflow_extensions_json,
                    data_json,
                    updated_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, CURRENT_TIMESTAMP)
                "#,
                params![
                    addon_id,
                    capability.capability_id,
                    status,
                    capability.source,
                    capability.addon_version,
                    capability.title,
                    serde_json::to_string(&capability.domains)?,
                    serde_json::to_string(&capability.keywords)?,
                    serde_json::to_string(&capability.workflow_extensions)?,
                    serde_json::to_string(&capability.data)?,
                ],
            )?;
        }
        Ok(())
    }

    pub fn update_addon_capabilities_status(&self, addon_id: &str, status: &str) -> Result<()> {
        self.connection.execute(
            r#"
            UPDATE addon_capabilities
            SET status = ?2,
                updated_at = CURRENT_TIMESTAMP
            WHERE addon_id = ?1
            "#,
            params![addon_id, status],
        )?;
        Ok(())
    }

    pub fn delete_addon_capabilities(&self, addon_id: &str) -> Result<()> {
        self.connection.execute(
            "DELETE FROM addon_capabilities WHERE addon_id = ?1",
            params![addon_id],
        )?;
        Ok(())
    }

    pub fn list_addon_capabilities(
        &self,
        addon_id: Option<&str>,
        capability_id: Option<&str>,
        status: Option<&str>,
    ) -> Result<Vec<StoredAddonCapabilityRecord>> {
        let mut statement = self.connection.prepare(
            r#"
            SELECT
                addon_id,
                capability_id,
                status,
                source,
                addon_version,
                title,
                domains_json,
                keywords_json,
                workflow_extensions_json,
                data_json,
                updated_at
            FROM addon_capabilities
            ORDER BY addon_id ASC, capability_id ASC
            "#,
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
            ))
        })?;
        let mut capabilities = Vec::new();
        for row in rows {
            let (
                addon_id_value,
                capability_id_value,
                status_value,
                source,
                addon_version,
                title,
                domains_json,
                keywords_json,
                workflow_extensions_json,
                data_json,
                updated_at,
            ) = row?;
            if addon_id.is_some_and(|filter| filter != addon_id_value.as_str()) {
                continue;
            }
            if capability_id.is_some_and(|filter| filter != capability_id_value.as_str()) {
                continue;
            }
            if status.is_some_and(|filter| filter != status_value.as_str()) {
                continue;
            }
            capabilities.push(StoredAddonCapabilityRecord {
                addon_id: addon_id_value,
                capability_id: capability_id_value,
                status: status_value,
                source,
                addon_version,
                title,
                domains: serde_json::from_str(&domains_json)?,
                keywords: serde_json::from_str(&keywords_json)?,
                workflow_extensions: serde_json::from_str(&workflow_extensions_json)?,
                data: serde_json::from_str(&data_json)?,
                updated_at,
            });
        }
        Ok(capabilities)
    }

    pub fn save_addon_permission_authorization(
        &self,
        write: AddonPermissionAuthorizationWrite<'_>,
    ) -> Result<()> {
        self.connection.execute(
            r#"
            INSERT INTO addon_permission_authorizations (
                addon_id,
                permission_id,
                status,
                risk,
                approved_by,
                source,
                data_json,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, CURRENT_TIMESTAMP)
            ON CONFLICT(addon_id, permission_id)
            DO UPDATE SET
                status=excluded.status,
                risk=excluded.risk,
                approved_by=excluded.approved_by,
                source=excluded.source,
                data_json=excluded.data_json,
                updated_at=CURRENT_TIMESTAMP
            "#,
            params![
                write.addon_id,
                write.permission_id,
                write.status,
                write.risk,
                write.approved_by,
                write.source,
                serde_json::to_string(write.data)?,
            ],
        )?;
        Ok(())
    }

    pub fn list_addon_permission_authorizations(
        &self,
        addon_id: Option<&str>,
        permission_id: Option<&str>,
        status: Option<&str>,
    ) -> Result<Vec<StoredAddonPermissionAuthorizationRecord>> {
        let mut statement = self.connection.prepare(
            r#"
            SELECT
                addon_id,
                permission_id,
                status,
                risk,
                approved_by,
                source,
                data_json,
                granted_at,
                updated_at
            FROM addon_permission_authorizations
            ORDER BY addon_id ASC, permission_id ASC
            "#,
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
            ))
        })?;
        let mut authorizations = Vec::new();
        for row in rows {
            let (
                addon_id_value,
                permission_id_value,
                status_value,
                risk,
                approved_by,
                source,
                data_json,
                granted_at,
                updated_at,
            ) = row?;
            if addon_id.is_some_and(|filter| filter != addon_id_value.as_str()) {
                continue;
            }
            if permission_id.is_some_and(|filter| filter != permission_id_value.as_str()) {
                continue;
            }
            if status.is_some_and(|filter| filter != status_value.as_str()) {
                continue;
            }
            authorizations.push(StoredAddonPermissionAuthorizationRecord {
                addon_id: addon_id_value,
                permission_id: permission_id_value,
                status: status_value,
                risk,
                approved_by,
                source,
                data: serde_json::from_str(&data_json)?,
                granted_at,
                updated_at,
            });
        }
        Ok(authorizations)
    }

    pub fn save_runtime_contract_dispatch(
        &self,
        dispatch: RuntimeContractDispatchWrite<'_>,
    ) -> Result<()> {
        self.connection.execute(
            r#"
            INSERT INTO runtime_contract_dispatches (
                id,
                addon_id,
                contract_id,
                contract_type,
                capability_id,
                runtime,
                entrypoint,
                status,
                source,
                input_json,
                policy_json,
                data_json,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, CURRENT_TIMESTAMP)
            ON CONFLICT(id)
            DO UPDATE SET
                status=excluded.status,
                input_json=excluded.input_json,
                policy_json=excluded.policy_json,
                data_json=excluded.data_json,
                updated_at=CURRENT_TIMESTAMP
            "#,
            params![
                dispatch.id,
                dispatch.addon_id,
                dispatch.contract_id,
                dispatch.contract_type,
                dispatch.capability_id,
                dispatch.runtime,
                dispatch.entrypoint,
                dispatch.status,
                dispatch.source,
                serde_json::to_string(dispatch.input)?,
                serde_json::to_string(dispatch.policy)?,
                serde_json::to_string(dispatch.data)?,
            ],
        )?;
        Ok(())
    }

    pub fn load_runtime_contract_dispatch(
        &self,
        id: &str,
    ) -> Result<Option<StoredRuntimeContractDispatchRecord>> {
        let record = self
            .connection
            .query_row(
                r#"
                SELECT
                    id,
                    addon_id,
                    contract_id,
                    contract_type,
                    capability_id,
                    runtime,
                    entrypoint,
                    status,
                    source,
                    input_json,
                    policy_json,
                    data_json,
                    created_at,
                    updated_at
                FROM runtime_contract_dispatches
                WHERE id = ?1
                "#,
                params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, String>(9)?,
                        row.get::<_, String>(10)?,
                        row.get::<_, String>(11)?,
                        row.get::<_, String>(12)?,
                        row.get::<_, String>(13)?,
                    ))
                },
            )
            .optional()?;
        record
            .map(
                |(
                    id,
                    addon_id,
                    contract_id,
                    contract_type,
                    capability_id,
                    runtime,
                    entrypoint,
                    status,
                    source,
                    input_json,
                    policy_json,
                    data_json,
                    created_at,
                    updated_at,
                )|
                 -> Result<StoredRuntimeContractDispatchRecord> {
                    Ok(StoredRuntimeContractDispatchRecord {
                        id,
                        addon_id,
                        contract_id,
                        contract_type,
                        capability_id,
                        runtime,
                        entrypoint,
                        status,
                        source,
                        input: serde_json::from_str(&input_json)?,
                        policy: serde_json::from_str(&policy_json)?,
                        data: serde_json::from_str(&data_json)?,
                        created_at,
                        updated_at,
                    })
                },
            )
            .transpose()
    }

    pub fn update_runtime_contract_dispatch_state(
        &self,
        id: &str,
        status: &str,
        policy: &serde_json::Value,
        data: &serde_json::Value,
    ) -> Result<()> {
        self.connection.execute(
            r#"
            UPDATE runtime_contract_dispatches
            SET
                status = ?2,
                policy_json = ?3,
                data_json = ?4,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?1
            "#,
            params![
                id,
                status,
                serde_json::to_string(policy)?,
                serde_json::to_string(data)?,
            ],
        )?;
        Ok(())
    }

    pub fn list_runtime_contract_dispatches(
        &self,
        addon_id: Option<&str>,
        contract_id: Option<&str>,
        status: Option<&str>,
        limit: usize,
    ) -> Result<Vec<StoredRuntimeContractDispatchRecord>> {
        let limit = limit.max(1) as i64;
        let mut statement = self.connection.prepare(
            r#"
            SELECT
                id,
                addon_id,
                contract_id,
                contract_type,
                capability_id,
                runtime,
                entrypoint,
                status,
                source,
                input_json,
                policy_json,
                data_json,
                created_at,
                updated_at
            FROM runtime_contract_dispatches
            WHERE (?1 IS NULL OR addon_id = ?1)
              AND (?2 IS NULL OR contract_id = ?2)
              AND (?3 IS NULL OR status = ?3)
            ORDER BY created_at DESC, id DESC
            LIMIT ?4
            "#,
        )?;
        let rows = statement.query_map(params![addon_id, contract_id, status, limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, String>(12)?,
                row.get::<_, String>(13)?,
            ))
        })?;
        let mut dispatches = Vec::new();
        for row in rows {
            let (
                id,
                addon_id_value,
                contract_id_value,
                contract_type,
                capability_id,
                runtime,
                entrypoint,
                status_value,
                source,
                input_json,
                policy_json,
                data_json,
                created_at,
                updated_at,
            ) = row?;
            dispatches.push(StoredRuntimeContractDispatchRecord {
                id,
                addon_id: addon_id_value,
                contract_id: contract_id_value,
                contract_type,
                capability_id,
                runtime,
                entrypoint,
                status: status_value,
                source,
                input: serde_json::from_str(&input_json)?,
                policy: serde_json::from_str(&policy_json)?,
                data: serde_json::from_str(&data_json)?,
                created_at,
                updated_at,
            });
        }
        Ok(dispatches)
    }

    pub fn save_runtime_worker(&self, worker: RuntimeWorkerWrite<'_>) -> Result<()> {
        self.connection.execute(
            r#"
            INSERT INTO runtime_workers (
                id,
                runtime,
                status,
                trust_level,
                source,
                data_json,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, CURRENT_TIMESTAMP)
            ON CONFLICT(id)
            DO UPDATE SET
                runtime=excluded.runtime,
                status=excluded.status,
                trust_level=excluded.trust_level,
                source=excluded.source,
                data_json=excluded.data_json,
                updated_at=CURRENT_TIMESTAMP
            "#,
            params![
                worker.id,
                worker.runtime,
                worker.status,
                worker.trust_level,
                worker.source,
                serde_json::to_string(worker.data)?,
            ],
        )?;
        Ok(())
    }

    pub fn load_runtime_worker(&self, id: &str) -> Result<Option<StoredRuntimeWorkerRecord>> {
        let record = self
            .connection
            .query_row(
                r#"
                SELECT
                    id,
                    runtime,
                    status,
                    trust_level,
                    source,
                    data_json,
                    created_at,
                    updated_at
                FROM runtime_workers
                WHERE id = ?1
                "#,
                params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                    ))
                },
            )
            .optional()?;
        record
            .map(
                |(
                    id,
                    runtime,
                    status,
                    trust_level,
                    source,
                    data_json,
                    created_at,
                    updated_at,
                )|
                 -> Result<StoredRuntimeWorkerRecord> {
                    Ok(StoredRuntimeWorkerRecord {
                        id,
                        runtime,
                        status,
                        trust_level,
                        source,
                        data: serde_json::from_str(&data_json)?,
                        created_at,
                        updated_at,
                    })
                },
            )
            .transpose()
    }

    pub fn list_runtime_workers(
        &self,
        runtime: Option<&str>,
        status: Option<&str>,
        trust_level: Option<&str>,
        limit: usize,
    ) -> Result<Vec<StoredRuntimeWorkerRecord>> {
        let limit = limit.max(1) as i64;
        let mut statement = self.connection.prepare(
            r#"
            SELECT
                id,
                runtime,
                status,
                trust_level,
                source,
                data_json,
                created_at,
                updated_at
            FROM runtime_workers
            ORDER BY updated_at DESC, id ASC
            LIMIT ?1
            "#,
        )?;
        let rows = statement.query_map(params![limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
            ))
        })?;
        let mut workers = Vec::new();
        for row in rows {
            let (
                id,
                runtime_value,
                status_value,
                trust_level_value,
                source,
                data_json,
                created_at,
                updated_at,
            ) = row?;
            if runtime.is_some_and(|filter| filter != runtime_value.as_str()) {
                continue;
            }
            if status.is_some_and(|filter| filter != status_value.as_str()) {
                continue;
            }
            if trust_level.is_some_and(|filter| filter != trust_level_value.as_str()) {
                continue;
            }
            workers.push(StoredRuntimeWorkerRecord {
                id,
                runtime: runtime_value,
                status: status_value,
                trust_level: trust_level_value,
                source,
                data: serde_json::from_str(&data_json)?,
                created_at,
                updated_at,
            });
        }
        Ok(workers)
    }

    pub fn save_addon_marketplace_package(
        &self,
        package: AddonMarketplacePackageWrite<'_>,
    ) -> Result<()> {
        self.connection.execute(
            r#"
            INSERT INTO addon_marketplace_packages (
                package_id,
                addon_id,
                addon_version,
                repository,
                channel,
                manifest_sha256,
                package_sha256,
                status,
                signature_status,
                verification_status,
                source,
                package_json,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, CURRENT_TIMESTAMP)
            ON CONFLICT(package_id)
            DO UPDATE SET
                addon_id=excluded.addon_id,
                addon_version=excluded.addon_version,
                repository=excluded.repository,
                channel=excluded.channel,
                manifest_sha256=excluded.manifest_sha256,
                package_sha256=excluded.package_sha256,
                status=excluded.status,
                signature_status=excluded.signature_status,
                verification_status=excluded.verification_status,
                source=excluded.source,
                package_json=excluded.package_json,
                updated_at=CURRENT_TIMESTAMP
            "#,
            params![
                package.package_id,
                package.addon_id,
                package.addon_version,
                package.repository,
                package.channel,
                package.manifest_sha256,
                package.package_sha256,
                package.status,
                package.signature_status,
                package.verification_status,
                package.source,
                serde_json::to_string(package.package)?,
            ],
        )?;
        Ok(())
    }

    pub fn list_addon_marketplace_packages(
        &self,
        repository: Option<&str>,
        channel: Option<&str>,
        addon_id: Option<&str>,
        status: Option<&str>,
        limit: usize,
    ) -> Result<Vec<StoredAddonMarketplacePackageRecord>> {
        let limit = limit.max(1) as i64;
        let mut statement = self.connection.prepare(
            r#"
            SELECT
                package_id,
                addon_id,
                addon_version,
                repository,
                channel,
                manifest_sha256,
                package_sha256,
                status,
                signature_status,
                verification_status,
                source,
                package_json,
                created_at,
                updated_at
            FROM addon_marketplace_packages
            ORDER BY updated_at DESC, package_id ASC
            LIMIT ?1
            "#,
        )?;
        let rows = statement.query_map(params![limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, String>(12)?,
                row.get::<_, String>(13)?,
            ))
        })?;
        let mut packages = Vec::new();
        for row in rows {
            let (
                package_id,
                addon_id_value,
                addon_version,
                repository_value,
                channel_value,
                manifest_sha256,
                package_sha256,
                status_value,
                signature_status,
                verification_status,
                source,
                package_json,
                created_at,
                updated_at,
            ) = row?;
            if repository.is_some_and(|filter| filter != repository_value.as_str()) {
                continue;
            }
            if channel.is_some_and(|filter| filter != channel_value.as_str()) {
                continue;
            }
            if addon_id.is_some_and(|filter| filter != addon_id_value.as_str()) {
                continue;
            }
            if status.is_some_and(|filter| filter != status_value.as_str()) {
                continue;
            }
            packages.push(StoredAddonMarketplacePackageRecord {
                package_id,
                addon_id: addon_id_value,
                addon_version,
                repository: repository_value,
                channel: channel_value,
                manifest_sha256,
                package_sha256,
                status: status_value,
                signature_status,
                verification_status,
                source,
                package: serde_json::from_str(&package_json)?,
                created_at,
                updated_at,
            });
        }
        Ok(packages)
    }

    pub fn save_addon_trust_key(&self, key: AddonTrustKeyWrite<'_>) -> Result<()> {
        self.connection.execute(
            r#"
            INSERT INTO addon_trust_keys (
                key_id,
                repository,
                channel,
                public_key,
                status,
                trust_level,
                approved_by,
                source,
                data_json,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, CURRENT_TIMESTAMP)
            ON CONFLICT(key_id)
            DO UPDATE SET
                repository=excluded.repository,
                channel=excluded.channel,
                public_key=excluded.public_key,
                status=excluded.status,
                trust_level=excluded.trust_level,
                approved_by=excluded.approved_by,
                source=excluded.source,
                data_json=excluded.data_json,
                updated_at=CURRENT_TIMESTAMP
            "#,
            params![
                key.key_id,
                key.repository,
                key.channel,
                key.public_key,
                key.status,
                key.trust_level,
                key.approved_by,
                key.source,
                serde_json::to_string(key.data)?,
            ],
        )?;
        Ok(())
    }

    pub fn list_addon_trust_keys(
        &self,
        repository: Option<&str>,
        channel: Option<&str>,
        public_key: Option<&str>,
        status: Option<&str>,
        limit: usize,
    ) -> Result<Vec<StoredAddonTrustKeyRecord>> {
        let limit = limit.max(1) as i64;
        let mut statement = self.connection.prepare(
            r#"
            SELECT
                key_id,
                repository,
                channel,
                public_key,
                status,
                trust_level,
                approved_by,
                source,
                data_json,
                created_at,
                updated_at
            FROM addon_trust_keys
            ORDER BY updated_at DESC, key_id ASC
            LIMIT ?1
            "#,
        )?;
        let rows = statement.query_map(params![limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
            ))
        })?;
        let mut keys = Vec::new();
        for row in rows {
            let (
                key_id,
                repository_value,
                channel_value,
                public_key_value,
                status_value,
                trust_level,
                approved_by,
                source,
                data_json,
                created_at,
                updated_at,
            ) = row?;
            if repository.is_some_and(|filter| filter != repository_value.as_str()) {
                continue;
            }
            if let Some(filter) = channel {
                if filter != channel_value.as_str() && channel_value != "*" {
                    continue;
                }
            }
            if public_key.is_some_and(|filter| filter != public_key_value.as_str()) {
                continue;
            }
            if status.is_some_and(|filter| filter != status_value.as_str()) {
                continue;
            }
            keys.push(StoredAddonTrustKeyRecord {
                key_id,
                repository: repository_value,
                channel: channel_value,
                public_key: public_key_value,
                status: status_value,
                trust_level,
                approved_by,
                source,
                data: serde_json::from_str(&data_json)?,
                created_at,
                updated_at,
            });
        }
        Ok(keys)
    }

    pub fn save_identity_record(
        &self,
        scope: &str,
        id: &str,
        label: &str,
        source: &str,
        data: &serde_json::Value,
    ) -> Result<()> {
        self.connection.execute(
            r#"
            INSERT INTO identity_registry (scope, id, label, source, data_json, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, CURRENT_TIMESTAMP)
            ON CONFLICT(scope, id) DO UPDATE SET
                label=excluded.label,
                source=excluded.source,
                data_json=excluded.data_json,
                updated_at=CURRENT_TIMESTAMP
            "#,
            params![scope, id, label, source, serde_json::to_string(data)?],
        )?;
        Ok(())
    }

    pub fn list_identity_records(
        &self,
        scope: Option<&str>,
        id: Option<&str>,
    ) -> Result<Vec<StoredIdentityRecord>> {
        let mut statement = self.connection.prepare(
            r#"
            SELECT scope, id, label, source, data_json, updated_at
            FROM identity_registry
            ORDER BY scope ASC, id ASC
            "#,
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        let mut records = Vec::new();
        for row in rows {
            let (scope_value, id_value, label, source, data_json, updated_at) = row?;
            if scope.is_some_and(|filter| filter != scope_value.as_str()) {
                continue;
            }
            if id.is_some_and(|filter| filter != id_value.as_str()) {
                continue;
            }
            records.push(StoredIdentityRecord {
                scope: scope_value,
                id: id_value,
                label,
                source,
                data: serde_json::from_str(&data_json)?,
                updated_at,
            });
        }
        Ok(records)
    }

    pub fn save_identity_membership(&self, write: IdentityMembershipWrite<'_>) -> Result<()> {
        self.connection.execute(
            r#"
            INSERT INTO identity_memberships (
                subject_scope,
                subject_id,
                organization_id,
                brand_id,
                product_id,
                role,
                status,
                source,
                data_json,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, CURRENT_TIMESTAMP)
            ON CONFLICT(subject_scope, subject_id, organization_id, brand_id, product_id)
            DO UPDATE SET
                role=excluded.role,
                status=excluded.status,
                source=excluded.source,
                data_json=excluded.data_json,
                updated_at=CURRENT_TIMESTAMP
            "#,
            params![
                write.subject_scope,
                write.subject_id,
                write.organization_id,
                write.brand_id,
                write.product_id,
                write.role,
                write.status,
                write.source,
                serde_json::to_string(write.data)?,
            ],
        )?;
        Ok(())
    }

    pub fn list_identity_memberships(
        &self,
        subject_scope: Option<&str>,
        subject_id: Option<&str>,
        organization_id: Option<&str>,
        brand_id: Option<&str>,
        product_id: Option<&str>,
        status: Option<&str>,
    ) -> Result<Vec<StoredIdentityMembershipRecord>> {
        let mut statement = self.connection.prepare(
            r#"
            SELECT
                subject_scope,
                subject_id,
                organization_id,
                brand_id,
                product_id,
                role,
                status,
                source,
                data_json,
                updated_at
            FROM identity_memberships
            ORDER BY organization_id ASC, brand_id ASC, product_id ASC, subject_scope ASC, subject_id ASC
            "#,
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
            ))
        })?;
        let mut records = Vec::new();
        for row in rows {
            let (
                subject_scope_value,
                subject_id_value,
                organization_id_value,
                brand_id_value,
                product_id_value,
                role,
                status_value,
                source,
                data_json,
                updated_at,
            ) = row?;
            if subject_scope.is_some_and(|filter| filter != subject_scope_value.as_str()) {
                continue;
            }
            if subject_id.is_some_and(|filter| filter != subject_id_value.as_str()) {
                continue;
            }
            if organization_id.is_some_and(|filter| filter != organization_id_value.as_str()) {
                continue;
            }
            if brand_id.is_some_and(|filter| filter != brand_id_value.as_str()) {
                continue;
            }
            if product_id.is_some_and(|filter| filter != product_id_value.as_str()) {
                continue;
            }
            if status.is_some_and(|filter| filter != status_value.as_str()) {
                continue;
            }
            records.push(StoredIdentityMembershipRecord {
                subject_scope: subject_scope_value,
                subject_id: subject_id_value,
                organization_id: organization_id_value,
                brand_id: brand_id_value,
                product_id: product_id_value,
                role,
                status: status_value,
                source,
                data: serde_json::from_str(&data_json)?,
                updated_at,
            });
        }
        Ok(records)
    }

    pub fn save_identity_link(&self, write: IdentityLinkWrite<'_>) -> Result<()> {
        self.connection.execute(
            r#"
            INSERT INTO identity_links (
                id,
                left_scope,
                left_id,
                right_scope,
                right_id,
                link_type,
                status,
                source,
                data_json,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            ON CONFLICT(id) DO UPDATE SET
                left_scope=excluded.left_scope,
                left_id=excluded.left_id,
                right_scope=excluded.right_scope,
                right_id=excluded.right_id,
                link_type=excluded.link_type,
                status=excluded.status,
                source=excluded.source,
                data_json=excluded.data_json,
                updated_at=CURRENT_TIMESTAMP
            "#,
            params![
                write.id,
                write.left_scope,
                write.left_id,
                write.right_scope,
                write.right_id,
                write.link_type,
                write.status,
                write.source,
                serde_json::to_string(write.data)?,
            ],
        )?;
        Ok(())
    }

    pub fn update_identity_link_status(
        &self,
        id: &str,
        status: &str,
        source: &str,
        data: &serde_json::Value,
    ) -> Result<bool> {
        let updated = self.connection.execute(
            r#"
            UPDATE identity_links
            SET status = ?2,
                source = ?3,
                data_json = ?4,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?1
            "#,
            params![id, status, source, serde_json::to_string(data)?],
        )?;
        Ok(updated > 0)
    }

    pub fn list_identity_links(
        &self,
        scope: Option<&str>,
        id: Option<&str>,
        status: Option<&str>,
    ) -> Result<Vec<StoredIdentityLinkRecord>> {
        let mut statement = self.connection.prepare(
            r#"
            SELECT
                id,
                left_scope,
                left_id,
                right_scope,
                right_id,
                link_type,
                status,
                source,
                data_json,
                created_at,
                updated_at
            FROM identity_links
            ORDER BY left_scope ASC, left_id ASC, right_scope ASC, right_id ASC
            "#,
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
            ))
        })?;
        let mut records = Vec::new();
        for row in rows {
            let (
                link_id,
                left_scope,
                left_id,
                right_scope,
                right_id,
                link_type,
                status_value,
                source,
                data_json,
                created_at,
                updated_at,
            ) = row?;
            let identity_matches = match (scope, id) {
                (Some(scope_filter), Some(id_filter)) => {
                    (left_scope == scope_filter && left_id == id_filter)
                        || (right_scope == scope_filter && right_id == id_filter)
                }
                (Some(scope_filter), None) => {
                    left_scope == scope_filter || right_scope == scope_filter
                }
                (None, Some(id_filter)) => left_id == id_filter || right_id == id_filter,
                (None, None) => true,
            };
            if !identity_matches {
                continue;
            }
            if status.is_some_and(|filter| filter != status_value.as_str()) {
                continue;
            }
            records.push(StoredIdentityLinkRecord {
                id: link_id,
                left_scope,
                left_id,
                right_scope,
                right_id,
                link_type,
                status: status_value,
                source,
                data: serde_json::from_str(&data_json)?,
                created_at,
                updated_at,
            });
        }
        Ok(records)
    }

    pub fn save_inbound_event(
        &self,
        id: &str,
        origin: &str,
        action: &str,
        status: &str,
        data: &serde_json::Value,
        tenant_context: &serde_json::Value,
    ) -> Result<()> {
        let organization_id = tenant_context_identity_id(tenant_context, "organization");
        let brand_id = tenant_context_identity_id(tenant_context, "brand");
        let product_id = tenant_context_identity_id(tenant_context, "product");
        let user_id = tenant_context_identity_id(tenant_context, "user");
        let channel_id = tenant_context_identity_id(tenant_context, "channel");
        self.connection.execute(
            r#"
            INSERT INTO event_inbox (
                id, origin, action, status,
                organization_id, brand_id, product_id, user_id, channel_id,
                tenant_context_json, data_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
            params![
                id,
                origin,
                action,
                status,
                organization_id,
                brand_id,
                product_id,
                user_id,
                channel_id,
                serde_json::to_string(tenant_context)?,
                serde_json::to_string(data)?
            ],
        )?;
        self.insert_global_event(GlobalEventWrite {
            source: "event_inbox",
            source_id: id,
            workflow_id: None,
            kind: "inbound_event_ingested",
            origin,
            status,
            data: &serde_json::json!({
                "event_id": id,
                "origin": origin,
                "action": action,
                "status": status,
                "data": data,
            }),
            tenant_context,
        })?;
        Ok(())
    }

    pub fn update_inbound_event_status(
        &self,
        id: &str,
        status: &str,
        data: &serde_json::Value,
    ) -> Result<()> {
        self.connection.execute(
            r#"
            UPDATE event_inbox
            SET status = ?2,
                data_json = ?3,
                processed_at = CURRENT_TIMESTAMP
            WHERE id = ?1
            "#,
            params![id, status, serde_json::to_string(data)?],
        )?;
        if let Ok(event) = self.load_inbound_event(id) {
            let workflow_id = event
                .data
                .get("workflow_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            self.insert_global_event(GlobalEventWrite {
                source: "event_inbox",
                source_id: id,
                workflow_id: workflow_id.as_deref(),
                kind: "inbound_event_status_updated",
                origin: &event.origin,
                status,
                data: &serde_json::json!({
                    "event_id": event.id.clone(),
                    "origin": event.origin.clone(),
                    "action": event.action.clone(),
                    "status": status,
                    "data": data,
                }),
                tenant_context: &event.tenant_context,
            })?;
        }
        Ok(())
    }

    pub fn load_inbound_event(&self, id: &str) -> Result<InboundEventRecord> {
        let row: Option<InboundEventRow> = self
            .connection
            .query_row(
                r#"
                SELECT id, origin, action, status, data_json, created_at, processed_at,
                       organization_id, brand_id, product_id, user_id, channel_id, tenant_context_json
                FROM event_inbox
                WHERE id = ?1
                "#,
                params![id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                        row.get(9)?,
                        row.get(10)?,
                        row.get(11)?,
                        row.get(12)?,
                    ))
                },
            )
            .optional()?;
        let (
            id,
            origin,
            action,
            status,
            data_json,
            created_at,
            processed_at,
            organization_id,
            brand_id,
            product_id,
            user_id,
            channel_id,
            tenant_context_json,
        ) = row.with_context(|| format!("inbound event not found: {id}"))?;
        Ok(InboundEventRecord {
            id,
            origin,
            action,
            status,
            data: serde_json::from_str(&data_json)?,
            created_at,
            processed_at,
            organization_id,
            brand_id,
            product_id,
            user_id,
            channel_id,
            tenant_context: serde_json::from_str(&tenant_context_json)?,
        })
    }

    pub fn list_inbound_events(
        &self,
        status: Option<&str>,
        limit: usize,
        organization_id: Option<&str>,
        brand_id: Option<&str>,
        product_id: Option<&str>,
    ) -> Result<Vec<InboundEventRecord>> {
        let limit = limit.max(1) as i64;
        let status_filter = normalize_optional_filter(status);
        let organization_filter = normalize_optional_filter(organization_id);
        let brand_filter = normalize_optional_filter(brand_id);
        let product_filter = normalize_optional_filter(product_id);
        let mut statement = self.connection.prepare(
            r#"
            SELECT id, origin, action, status, data_json, created_at, processed_at,
                   organization_id, brand_id, product_id, user_id, channel_id, tenant_context_json
            FROM event_inbox
            WHERE (?1 IS NULL OR status = ?1)
              AND (?2 IS NULL OR organization_id = ?2)
              AND (?3 IS NULL OR brand_id = ?3)
              AND (?4 IS NULL OR product_id = ?4)
            ORDER BY created_at DESC, id DESC
            LIMIT ?5
            "#,
        )?;
        let map_row = |row: &rusqlite::Row<'_>| {
            Ok(InboundEventRecord {
                id: row.get(0)?,
                origin: row.get(1)?,
                action: row.get(2)?,
                status: row.get(3)?,
                data: serde_json::from_str(&row.get::<_, String>(4)?).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        4,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?,
                created_at: row.get(5)?,
                processed_at: row.get(6)?,
                organization_id: row.get(7)?,
                brand_id: row.get(8)?,
                product_id: row.get(9)?,
                user_id: row.get(10)?,
                channel_id: row.get(11)?,
                tenant_context: serde_json::from_str(&row.get::<_, String>(12)?).map_err(
                    |error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            12,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    },
                )?,
            })
        };
        let rows = statement.query_map(
            params![
                status_filter,
                organization_filter,
                brand_filter,
                product_filter,
                limit
            ],
            map_row,
        )?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    pub fn save_executor_state(&self, id: &str, data: &serde_json::Value) -> Result<()> {
        self.connection.execute(
            r#"
            INSERT INTO executor_policy (id, data_json, updated_at)
            VALUES (?1, ?2, CURRENT_TIMESTAMP)
            ON CONFLICT(id) DO UPDATE SET
                data_json=excluded.data_json,
                updated_at=CURRENT_TIMESTAMP
            "#,
            params![id, serde_json::to_string(data)?],
        )?;
        Ok(())
    }

    pub fn load_executor_states(&self) -> Result<Vec<serde_json::Value>> {
        let mut statement = self
            .connection
            .prepare("SELECT data_json FROM executor_policy ORDER BY id")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut states = Vec::new();
        for row in rows {
            states.push(serde_json::from_str(&row?)?);
        }
        Ok(states)
    }

    pub fn save_runtime_state(&self, id: &str, data: &serde_json::Value) -> Result<()> {
        self.connection.execute(
            r#"
            INSERT INTO runtime_policy (id, data_json, updated_at)
            VALUES (?1, ?2, CURRENT_TIMESTAMP)
            ON CONFLICT(id) DO UPDATE SET
                data_json=excluded.data_json,
                updated_at=CURRENT_TIMESTAMP
            "#,
            params![id, serde_json::to_string(data)?],
        )?;
        Ok(())
    }

    pub fn load_runtime_states(&self) -> Result<Vec<serde_json::Value>> {
        let mut statement = self
            .connection
            .prepare("SELECT data_json FROM runtime_policy ORDER BY id")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut states = Vec::new();
        for row in rows {
            states.push(serde_json::from_str(&row?)?);
        }
        Ok(states)
    }

    pub fn save_worktree_state(
        &self,
        id: &str,
        repository_root: &str,
        worktree_root: &str,
        branch: Option<&str>,
        head: &str,
        data: &serde_json::Value,
    ) -> Result<()> {
        self.connection.execute(
            r#"
            INSERT INTO worktree_states (
                id,
                repository_root,
                worktree_root,
                branch,
                head,
                data_json,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            ON CONFLICT(id) DO UPDATE SET
                repository_root=excluded.repository_root,
                worktree_root=excluded.worktree_root,
                branch=excluded.branch,
                head=excluded.head,
                data_json=excluded.data_json,
                updated_at=CURRENT_TIMESTAMP
            "#,
            params![
                id,
                repository_root,
                worktree_root,
                branch,
                head,
                serde_json::to_string(data)?
            ],
        )?;
        Ok(())
    }

    pub fn load_worktree_states(&self) -> Result<Vec<serde_json::Value>> {
        let mut statement = self
            .connection
            .prepare("SELECT data_json FROM worktree_states ORDER BY worktree_root")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut states = Vec::new();
        for row in rows {
            states.push(serde_json::from_str(&row?)?);
        }
        Ok(states)
    }

    pub fn save_worktree_sandbox_state(
        &self,
        id: &str,
        worktree_id: &str,
        status: &str,
        data: &serde_json::Value,
    ) -> Result<()> {
        self.connection.execute(
            r#"
            INSERT INTO worktree_sandbox_states (
                id,
                worktree_id,
                status,
                data_json,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            ON CONFLICT(id) DO UPDATE SET
                worktree_id=excluded.worktree_id,
                status=excluded.status,
                data_json=excluded.data_json,
                updated_at=CURRENT_TIMESTAMP
            "#,
            params![id, worktree_id, status, serde_json::to_string(data)?],
        )?;
        Ok(())
    }

    pub fn load_worktree_sandbox_state(&self, id: &str) -> Result<Option<serde_json::Value>> {
        let data = self
            .connection
            .query_row(
                "SELECT data_json FROM worktree_sandbox_states WHERE id = ?1",
                params![id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        data.map(|data| serde_json::from_str(&data).map_err(Into::into))
            .transpose()
    }

    pub fn load_worktree_sandbox_states(&self) -> Result<Vec<serde_json::Value>> {
        let mut statement = self
            .connection
            .prepare("SELECT data_json FROM worktree_sandbox_states ORDER BY created_at, id")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut states = Vec::new();
        for row in rows {
            states.push(serde_json::from_str(&row?)?);
        }
        Ok(states)
    }

    pub fn save_run(
        &self,
        id: &str,
        workflow_id: &str,
        status: &str,
        data: &serde_json::Value,
    ) -> Result<()> {
        let workflow = self.load_workflow(workflow_id).ok();
        let tenant = operational_tenant_columns(workflow.as_ref());
        self.connection.execute(
            r#"
            INSERT INTO runs (
                id,
                workflow_id,
                organization_id,
                brand_id,
                product_id,
                user_id,
                channel_id,
                status,
                data_json,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, CURRENT_TIMESTAMP)
            ON CONFLICT(id) DO UPDATE SET
                workflow_id=excluded.workflow_id,
                organization_id=excluded.organization_id,
                brand_id=excluded.brand_id,
                product_id=excluded.product_id,
                user_id=excluded.user_id,
                channel_id=excluded.channel_id,
                status=excluded.status,
                data_json=excluded.data_json,
                updated_at=CURRENT_TIMESTAMP
            "#,
            params![
                id,
                workflow_id,
                tenant.organization_id,
                tenant.brand_id,
                tenant.product_id,
                tenant.user_id,
                tenant.channel_id,
                status,
                serde_json::to_string(data)?
            ],
        )?;
        if let Some(workflow) = workflow {
            self.save_tenant_index_record(
                "run",
                id,
                workflow_id,
                &workflow,
                "runs",
                &serde_json::json!({
                    "run_id": id,
                    "status": status,
                }),
            )?;
        }
        Ok(())
    }

    pub fn load_run(&self, id: &str) -> Result<serde_json::Value> {
        let data_json: Option<String> = self
            .connection
            .query_row(
                "SELECT data_json FROM runs WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        let data_json = data_json.with_context(|| format!("run not found: {id}"))?;
        Ok(serde_json::from_str(&data_json)?)
    }

    pub fn load_runs(&self) -> Result<Vec<serde_json::Value>> {
        let mut statement = self
            .connection
            .prepare("SELECT data_json FROM runs ORDER BY created_at ASC, id ASC")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut runs = Vec::new();
        for row in rows {
            runs.push(serde_json::from_str(&row?)?);
        }
        Ok(runs)
    }

    pub fn load_recent_runs(&self, limit: usize) -> Result<Vec<serde_json::Value>> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut statement = self
            .connection
            .prepare("SELECT data_json FROM runs ORDER BY created_at DESC, id DESC LIMIT ?1")?;
        let rows = statement.query_map(params![limit], |row| row.get::<_, String>(0))?;
        let mut runs = Vec::new();
        for row in rows {
            runs.push(serde_json::from_str(&row?)?);
        }
        Ok(runs)
    }

    pub fn try_save_task_lease(&self, lease: TaskLeaseWrite<'_>) -> Result<bool> {
        let workflow = self.load_workflow(lease.workflow_id).ok();
        let tenant = operational_tenant_columns(workflow.as_ref());
        let changed = self.connection.execute(
            r#"
            INSERT INTO task_leases (
                workflow_id,
                task_id,
                lease_id,
                executor,
                organization_id,
                brand_id,
                product_id,
                user_id,
                channel_id,
                acquired_at,
                expires_at,
                data_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            ON CONFLICT(workflow_id, task_id) DO UPDATE SET
                lease_id=excluded.lease_id,
                executor=excluded.executor,
                organization_id=excluded.organization_id,
                brand_id=excluded.brand_id,
                product_id=excluded.product_id,
                user_id=excluded.user_id,
                channel_id=excluded.channel_id,
                acquired_at=excluded.acquired_at,
                expires_at=excluded.expires_at,
                data_json=excluded.data_json
            WHERE task_leases.expires_at <= ?13
            "#,
            params![
                lease.workflow_id,
                lease.task_id,
                lease.lease_id,
                lease.executor,
                tenant.organization_id,
                tenant.brand_id,
                tenant.product_id,
                tenant.user_id,
                tenant.channel_id,
                lease.acquired_at,
                lease.expires_at,
                serde_json::to_string(lease.data)?,
                lease.acquired_at
            ],
        )?;
        Ok(changed == 1)
    }

    pub fn load_task_lease(
        &self,
        workflow_id: &str,
        task_id: &str,
    ) -> Result<Option<serde_json::Value>> {
        let data_json: Option<String> = self
            .connection
            .query_row(
                "SELECT data_json FROM task_leases WHERE workflow_id = ?1 AND task_id = ?2",
                params![workflow_id, task_id],
                |row| row.get(0),
            )
            .optional()?;
        data_json
            .map(|value| serde_json::from_str(&value).map_err(Into::into))
            .transpose()
    }

    pub fn load_task_leases(&self) -> Result<Vec<serde_json::Value>> {
        let mut statement = self
            .connection
            .prepare("SELECT data_json FROM task_leases ORDER BY workflow_id ASC, task_id ASC")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut leases = Vec::new();
        for row in rows {
            leases.push(serde_json::from_str(&row?)?);
        }
        Ok(leases)
    }

    pub fn delete_task_lease(
        &self,
        workflow_id: &str,
        task_id: &str,
        lease_id: &str,
    ) -> Result<bool> {
        let changed = self.connection.execute(
            r#"
            DELETE FROM task_leases
            WHERE workflow_id = ?1 AND task_id = ?2 AND lease_id = ?3
            "#,
            params![workflow_id, task_id, lease_id],
        )?;
        Ok(changed == 1)
    }

    pub fn save_task_checkpoint(&self, checkpoint: &TaskCheckpoint) -> Result<()> {
        let workflow = self.load_workflow(&checkpoint.workflow_id).ok();
        let tenant = operational_tenant_columns(workflow.as_ref());
        self.connection.execute(
            r#"
            INSERT INTO task_checkpoints (
                id,
                workflow_id,
                task_id,
                executor,
                organization_id,
                brand_id,
                product_id,
                user_id,
                channel_id,
                state,
                created_at,
                data_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
            params![
                &checkpoint.checkpoint_id,
                &checkpoint.workflow_id,
                &checkpoint.task_id,
                &checkpoint.executor,
                tenant.organization_id,
                tenant.brand_id,
                tenant.product_id,
                tenant.user_id,
                tenant.channel_id,
                &checkpoint.state,
                checkpoint.created_at.to_rfc3339(),
                serde_json::to_string(checkpoint)?
            ],
        )?;
        Ok(())
    }

    pub fn load_task_checkpoints(
        &self,
        workflow_id: &str,
        task_id: Option<&str>,
    ) -> Result<Vec<serde_json::Value>> {
        let sql = if task_id.is_some() {
            r#"
            SELECT data_json FROM task_checkpoints
            WHERE workflow_id = ?1 AND task_id = ?2
            ORDER BY created_at ASC, id ASC
            "#
        } else {
            r#"
            SELECT data_json FROM task_checkpoints
            WHERE workflow_id = ?1
            ORDER BY created_at ASC, id ASC
            "#
        };
        let mut statement = self.connection.prepare(sql)?;
        let mut checkpoints = Vec::new();
        if let Some(task_id) = task_id {
            let rows = statement
                .query_map(params![workflow_id, task_id], |row| row.get::<_, String>(0))?;
            for row in rows {
                checkpoints.push(serde_json::from_str(&row?)?);
            }
        } else {
            let rows = statement.query_map(params![workflow_id], |row| row.get::<_, String>(0))?;
            for row in rows {
                checkpoints.push(serde_json::from_str(&row?)?);
            }
        }
        Ok(checkpoints)
    }

    pub fn save_cluster_node(&self, id: &str, data: &serde_json::Value) -> Result<()> {
        self.connection.execute(
            r#"
            INSERT INTO cluster_nodes (id, data_json, updated_at)
            VALUES (?1, ?2, CURRENT_TIMESTAMP)
            ON CONFLICT(id) DO UPDATE SET
                data_json=excluded.data_json,
                updated_at=CURRENT_TIMESTAMP
            "#,
            params![id, serde_json::to_string(data)?],
        )?;
        Ok(())
    }

    pub fn load_cluster_node(&self, id: &str) -> Result<Option<serde_json::Value>> {
        let data_json: Option<String> = self
            .connection
            .query_row(
                "SELECT data_json FROM cluster_nodes WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        data_json
            .map(|value| serde_json::from_str(&value).map_err(Into::into))
            .transpose()
    }

    pub fn load_cluster_nodes(&self) -> Result<Vec<serde_json::Value>> {
        let mut statement = self
            .connection
            .prepare("SELECT data_json FROM cluster_nodes ORDER BY id ASC")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut nodes = Vec::new();
        for row in rows {
            nodes.push(serde_json::from_str(&row?)?);
        }
        Ok(nodes)
    }

    pub fn save_executor_quota(
        &self,
        executor: &str,
        provider: &str,
        model: &str,
        data: &serde_json::Value,
    ) -> Result<()> {
        self.connection.execute(
            r#"
            INSERT INTO executor_quotas (executor, provider, model, data_json, updated_at)
            VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP)
            ON CONFLICT(executor, provider, model) DO UPDATE SET
                data_json=excluded.data_json,
                updated_at=CURRENT_TIMESTAMP
            "#,
            params![executor, provider, model, serde_json::to_string(data)?],
        )?;
        Ok(())
    }

    pub fn load_executor_quotas(&self) -> Result<Vec<serde_json::Value>> {
        let mut statement = self
            .connection
            .prepare("SELECT data_json FROM executor_quotas ORDER BY updated_at DESC")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut quotas = Vec::new();
        for row in rows {
            quotas.push(serde_json::from_str(&row?)?);
        }
        Ok(quotas)
    }
}

fn default_operating_context_json() -> serde_json::Value {
    serde_json::to_value(OperatingContextSpec::default()).unwrap_or_else(|_| serde_json::json!({}))
}

fn runtime_secret_vault_key(workflow_id: &str, vault_reference: &str) -> String {
    format!("{workflow_id}:{vault_reference}")
}

struct OperationalTenantColumns {
    organization_id: String,
    brand_id: String,
    product_id: String,
    user_id: String,
    channel_id: String,
}

fn operational_tenant_columns(workflow: Option<&Workflow>) -> OperationalTenantColumns {
    let default_context;
    let context = if let Some(workflow) = workflow {
        &workflow.intent.operating_context
    } else {
        default_context = OperatingContextSpec::default();
        &default_context
    };
    OperationalTenantColumns {
        organization_id: context.organization.id.clone(),
        brand_id: context.brand.id.clone(),
        product_id: context.product.id.clone(),
        user_id: context.user.id.clone(),
        channel_id: context.channel.id.clone(),
    }
}

fn stored_event_service_from_row(row: &Row<'_>) -> rusqlite::Result<StoredEventServiceRecord> {
    let heartbeat_ttl_seconds = row.get::<_, i64>(8)?.max(0) as u64;
    let data_json = row.get::<_, String>(9)?;
    let tenant_context_json = row.get::<_, String>(17)?;
    Ok(StoredEventServiceRecord {
        id: row.get(0)?,
        service_kind: row.get(1)?,
        status: row.get(2)?,
        organization_id: row.get(12)?,
        brand_id: row.get(13)?,
        product_id: row.get(14)?,
        user_id: row.get(15)?,
        channel_id: row.get(16)?,
        tenant_context: serde_json::from_str(&tenant_context_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                17,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        lease_owner: row.get(3)?,
        lease_id: row.get(4)?,
        lease_acquired_at: row.get(5)?,
        lease_expires_at: row.get(6)?,
        last_heartbeat_at: row.get(7)?,
        heartbeat_ttl_seconds,
        data: serde_json::from_str(&data_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                9,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn stored_global_event_from_row(row: &Row<'_>) -> rusqlite::Result<StoredGlobalEventRecord> {
    let tenant_context_json = row.get::<_, String>(12)?;
    let data_json = row.get::<_, String>(13)?;
    Ok(StoredGlobalEventRecord {
        id: row.get(0)?,
        source: row.get(1)?,
        source_id: row.get(2)?,
        workflow_id: row.get(3)?,
        kind: row.get(4)?,
        origin: row.get(5)?,
        status: row.get(6)?,
        organization_id: row.get(7)?,
        brand_id: row.get(8)?,
        product_id: row.get(9)?,
        user_id: row.get(10)?,
        channel_id: row.get(11)?,
        tenant_context: serde_json::from_str(&tenant_context_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                12,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        data: serde_json::from_str(&data_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                13,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        created_at: row.get(14)?,
    })
}

fn stored_event_observability_from_row(
    row: &Row<'_>,
) -> rusqlite::Result<StoredEventObservabilityRecord> {
    let data_json = row.get::<_, String>(23)?;
    Ok(StoredEventObservabilityRecord {
        global_event_id: row.get(0)?,
        workflow_id: row.get(1)?,
        kind: row.get(2)?,
        category: row.get(3)?,
        severity: row.get(4)?,
        origin: row.get(5)?,
        source: row.get(6)?,
        organization_id: row.get(7)?,
        brand_id: row.get(8)?,
        product_id: row.get(9)?,
        node_ref: row.get(10)?,
        addon_id: row.get(11)?,
        duration_ms: row.get(12)?,
        retry_count: row.get(13)?,
        wait_state: row.get(14)?,
        wait_seconds: row.get(15)?,
        context_budget_bytes: row.get(16)?,
        selected_context_bytes: row.get(17)?,
        context_remaining_bytes: row.get(18)?,
        context_pressure_bps: row.get(19)?,
        context_pressure_state: row.get(20)?,
        memory_level: row.get(21)?,
        memory_scope: row.get(22)?,
        data: serde_json::from_str(&data_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                23,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        created_at: row.get(24)?,
    })
}

fn stored_cost_ledger_index_from_row(
    row: &Row<'_>,
) -> rusqlite::Result<StoredCostLedgerIndexRecord> {
    let data_json = row.get::<_, String>(16)?;
    Ok(StoredCostLedgerIndexRecord {
        row_key: row.get(0)?,
        source_kind: row.get(1)?,
        workflow_id: row.get(2)?,
        task_id: row.get(3)?,
        event_id: row.get(4)?,
        organization_id: row.get(5)?,
        brand_id: row.get(6)?,
        product_id: row.get(7)?,
        addon_id: row.get(8)?,
        executor: row.get(9)?,
        model_call_required: row.get(10)?,
        model_call_avoided: row.get(11)?,
        estimated_task_cost_usd: row.get(12)?,
        observed_event_cost_usd: row.get(13)?,
        tokens_in: row.get(14)?,
        tokens_out: row.get(15)?,
        data: serde_json::from_str(&data_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                16,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        created_at: row.get(17)?,
        updated_at: row.get(18)?,
    })
}

fn stored_headroom_blob_from_row(row: &Row<'_>) -> rusqlite::Result<StoredHeadroomBlobRecord> {
    let routing_json = row.get::<_, String>(13)?;
    Ok(StoredHeadroomBlobRecord {
        source: row.get(0)?,
        content_kind: row.get(1)?,
        strategy: row.get(2)?,
        reversible: row.get(3)?,
        original_sha256: row.get(4)?,
        original_bytes: row.get(5)?,
        compressed_sha256: row.get(6)?,
        compressed_bytes: row.get(7)?,
        estimated_original_tokens: row.get(8)?,
        estimated_compressed_tokens: row.get(9)?,
        estimated_saved_tokens: row.get(10)?,
        budget_tokens: row.get(11)?,
        budget_status: row.get(12)?,
        routing: serde_json::from_str(&routing_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                13,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        original_content: row.get(14)?,
        compressed_content: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
    })
}

fn checked_count_table(table: &str) -> Result<&'static str> {
    match table {
        "workflows" => Ok("workflows"),
        "runs" => Ok("runs"),
        "events" => Ok("events"),
        "global_events" => Ok("global_events"),
        "artifacts" => Ok("artifacts"),
        "cost_ledger_index" => Ok("cost_ledger_index"),
        "task_checkpoints" => Ok("task_checkpoints"),
        "harness_headroom_blobs" => Ok("harness_headroom_blobs"),
        _ => anyhow::bail!("unsupported count table: {table}"),
    }
}

fn checked_count_column(table: &str, column: &str) -> Result<&'static str> {
    match (table, column) {
        ("workflows", "status") => Ok("status"),
        ("runs", "status") => Ok("status"),
        ("global_events", "status") => Ok("status"),
        ("global_events", "kind") => Ok("kind"),
        ("events", "kind") => Ok("kind"),
        _ => anyhow::bail!("unsupported count column: {table}.{column}"),
    }
}

fn normalize_optional_filter(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn tenant_context_identity_id(tenant_context: &serde_json::Value, key: &str) -> String {
    tenant_context
        .get(key)
        .and_then(|value| value.get("id"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("default")
        .to_string()
}

fn extract_event_origin(data: &serde_json::Value) -> String {
    for key in ["origin", "actor", "executor", "source"] {
        if let Some(value) = data.get(key).and_then(serde_json::Value::as_str) {
            let value = value.trim();
            if !value.is_empty() {
                return value.to_string();
            }
        }
    }
    "forge".to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_wal, event_observability_trigger_count, event_observability_triggers_are_valid,
        open_configured_connection, operational_tenant_columns, runtime_secret_fallback_key_path,
        ForgeStore, GlobalEventWrite, RuntimeSecretKey, RuntimeSecretVaultAccess,
        RuntimeSecretVaultWrite, EVENT_OBSERVABILITY_RECONCILIATION_BATCH_SIZE,
        RUNTIME_SECRET_ENVELOPE_PREFIX, STORE_SCHEMA_VERSION,
    };
    use rusqlite::{params, Connection};
    use std::path::{Path, PathBuf};
    use std::sync::{mpsc, Arc, Barrier};
    use std::thread;
    use std::time::Duration;

    fn save_test_runtime_secret<'a>(
        store: &ForgeStore,
        secret: &'a str,
        tenant_context: &'a serde_json::Value,
    ) {
        let value_sha256 = crate::artifact::hex_sha256(secret.as_bytes());
        store
            .save_runtime_secret(RuntimeSecretVaultWrite {
                vault_reference: "project.test.default",
                workflow_id: Some("wf-secret-test"),
                scope: "project",
                provider: "test",
                kind: "api_key",
                classification: "secret",
                secret_value: secret,
                value_sha256: &value_sha256,
                value_len: secret.len(),
                source: "test",
                origin: "storage_test",
                tenant_context,
            })
            .unwrap();
    }

    fn resolve_test_runtime_secret(
        store: &ForgeStore,
        tenant_context: &serde_json::Value,
    ) -> anyhow::Result<super::RuntimeSecretVaultResolve> {
        store.resolve_runtime_secret(RuntimeSecretVaultAccess {
            vault_reference: "project.test.default",
            workflow_id: Some("wf-secret-test"),
            requester: "storage_test",
            allowed: true,
            origin: "storage_test",
            tenant_context,
        })
    }

    #[test]
    fn runtime_secret_vault_persists_only_authenticated_envelopes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("forge.sqlite");
        let tenant_context = serde_json::json!({});
        let secret = "runtime-secret-value-123456";
        let store = ForgeStore::open(&path).unwrap();

        save_test_runtime_secret(&store, secret, &tenant_context);

        let stored: String = store
            .connection
            .query_row(
                "SELECT secret_value FROM runtime_secret_vault WHERE vault_reference = 'project.test.default'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(stored.starts_with(RUNTIME_SECRET_ENVELOPE_PREFIX));
        assert!(!stored.contains(secret));
        let resolved = resolve_test_runtime_secret(&store, &tenant_context).unwrap();
        assert_eq!(resolved.secret_value, secret);

        let plaintext_update = store.connection.execute(
            "UPDATE runtime_secret_vault SET secret_value = 'plaintext-forbidden'",
            [],
        );
        assert!(plaintext_update.is_err());
    }

    #[test]
    fn runtime_secret_vault_migrates_legacy_plaintext_without_serializing_it() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("forge.sqlite");
        let tenant_context = serde_json::json!({});
        let secret = "legacy-runtime-secret-123456";
        let store = ForgeStore::open(&path).unwrap();
        save_test_runtime_secret(&store, secret, &tenant_context);
        drop(store);

        let legacy = Connection::open(&path).unwrap();
        legacy
            .execute_batch(
                r#"
                DROP TRIGGER trg_runtime_secret_vault_encrypted_insert;
                DROP TRIGGER trg_runtime_secret_vault_encrypted_update;
                "#,
            )
            .unwrap();
        legacy
            .execute(
                "UPDATE runtime_secret_vault SET secret_value = ?1",
                params![secret],
            )
            .unwrap();
        drop(legacy);

        let migrated = ForgeStore::open(&path).unwrap();
        let stored: String = migrated
            .connection
            .query_row(
                "SELECT secret_value FROM runtime_secret_vault WHERE vault_reference = 'project.test.default'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(stored.starts_with(RUNTIME_SECRET_ENVELOPE_PREFIX));
        assert!(!stored.contains(secret));
        assert_eq!(
            resolve_test_runtime_secret(&migrated, &tenant_context)
                .unwrap()
                .secret_value,
            secret
        );
    }

    #[test]
    fn runtime_secret_vault_rotates_from_a_previous_key_on_open() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("forge.sqlite");
        let key_path = runtime_secret_fallback_key_path(&path);
        let tenant_context = serde_json::json!({});
        let secret = "rotated-runtime-secret-123456";
        let store = ForgeStore::open(&path).unwrap();
        save_test_runtime_secret(&store, secret, &tenant_context);
        drop(store);

        let previous_key = zeroize::Zeroizing::new(std::fs::read_to_string(&key_path).unwrap());
        let current_key = RuntimeSecretKey::parse(&"11".repeat(32), "rotation test key").unwrap();
        let current_key_id = current_key.id.clone();
        let encoded_current = current_key.encoded();
        std::fs::write(
            &key_path,
            format!("{}\n{}", encoded_current.as_str(), previous_key.trim()),
        )
        .unwrap();

        let rotated = ForgeStore::open(&path).unwrap();
        let stored: String = rotated
            .connection
            .query_row(
                "SELECT secret_value FROM runtime_secret_vault WHERE vault_reference = 'project.test.default'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(stored.contains(&format!(":{current_key_id}:")));
        assert!(!stored.contains(secret));
        assert_eq!(
            resolve_test_runtime_secret(&rotated, &tenant_context)
                .unwrap()
                .secret_value,
            secret
        );
    }

    #[test]
    fn runtime_secret_vault_fails_closed_for_tampered_ciphertext() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("forge.sqlite");
        let tenant_context = serde_json::json!({});
        let secret = "tamper-detection-secret-123456";
        let store = ForgeStore::open(&path).unwrap();
        save_test_runtime_secret(&store, secret, &tenant_context);
        let mut stored: String = store
            .connection
            .query_row(
                "SELECT secret_value FROM runtime_secret_vault WHERE vault_reference = 'project.test.default'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let replacement = if stored.ends_with('0') { "1" } else { "0" };
        stored.replace_range(stored.len() - 1.., replacement);
        store
            .connection
            .execute(
                "UPDATE runtime_secret_vault SET secret_value = ?1",
                params![stored],
            )
            .unwrap();

        let error = resolve_test_runtime_secret(&store, &tenant_context)
            .unwrap_err()
            .to_string();
        assert!(error.contains("authentication failed"));
        assert!(!error.contains(secret));
    }

    #[test]
    fn runtime_secret_vault_fails_closed_when_local_key_is_missing() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("forge.sqlite");
        let key_path = runtime_secret_fallback_key_path(&path);
        let tenant_context = serde_json::json!({});
        let store = ForgeStore::open(&path).unwrap();
        save_test_runtime_secret(&store, "missing-key-secret-123456", &tenant_context);
        drop(store);
        std::fs::remove_file(key_path).unwrap();

        let error = ForgeStore::open(&path).err().unwrap().to_string();
        assert!(error.contains("encryption key is unavailable"));
    }

    #[cfg(unix)]
    #[test]
    fn forge_store_secures_directory_database_sidecars_and_key_material() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let store_dir = temp.path().join(".forge");
        std::fs::create_dir(&store_dir).unwrap();
        std::fs::set_permissions(&store_dir, std::fs::Permissions::from_mode(0o777)).unwrap();
        let path = store_dir.join("forge.sqlite");
        std::fs::write(&path, []).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).unwrap();

        let _store = ForgeStore::open(&path).unwrap();

        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&store_dir), 0o700);
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(&runtime_secret_fallback_key_path(&path)), 0o600);
        for suffix in ["-wal", "-shm"] {
            let sidecar = PathBuf::from(format!("{}{suffix}", path.display()));
            assert!(sidecar.exists());
            assert_eq!(mode(&sidecar), 0o600);
        }
    }

    #[test]
    fn forge_store_open_accepts_sqlite_memory_journal() {
        let store = ForgeStore::open(":memory:").unwrap();

        let mode: String = store
            .connection
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        assert_eq!(mode, "memory");
    }

    #[test]
    fn forge_store_open_accepts_sqlite_shared_memory_uri() {
        let store = ForgeStore::open("file:forge_store_memory?mode=memory&cache=shared").unwrap();

        let mode: String = store
            .connection
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        let database_path: String = store
            .connection
            .query_row(
                "SELECT file FROM pragma_database_list WHERE name = 'main'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(mode, "memory");
        assert!(database_path.is_empty());
    }

    #[test]
    fn configured_connection_enforces_sqlite_security_pragmas() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("sqlite-security-pragmas.sqlite");
        let store = ForgeStore::open(&path).unwrap();

        let foreign_keys: i64 = store
            .connection
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();
        let secure_delete: i64 = store
            .connection
            .pragma_query_value(None, "secure_delete", |row| row.get(0))
            .unwrap();

        assert_eq!(foreign_keys, 1);
        assert_eq!(secure_delete, 1);
    }

    #[test]
    fn raw_global_event_insert_is_reconciled_from_persistent_queue_on_reopen() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("raw-event-reconciliation.sqlite");
        let store = ForgeStore::open(&path).unwrap();
        assert_eq!(store.store_schema_version().unwrap(), STORE_SCHEMA_VERSION);
        drop(store);

        let connection = Connection::open(&path).unwrap();
        connection
            .execute(
                r#"
                INSERT INTO global_events (
                    source, source_id, workflow_id, kind, origin, status,
                    organization_id, brand_id, product_id, user_id, channel_id,
                    tenant_context_json, data_json
                )
                VALUES (
                    'raw_test', 'raw-event-001', NULL, 'raw_event_recorded', 'test', 'recorded',
                    'org-test', 'brand-test', 'product-test', 'user-test', 'channel-test',
                    '{}', '{"node_id":"raw-node"}'
                )
                "#,
                [],
            )
            .unwrap();
        let global_event_id = connection.last_insert_rowid();
        let queued: i64 = connection
            .query_row(
                "SELECT count(*) FROM event_observability_reconciliation_queue WHERE global_event_id = ?1",
                params![global_event_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(queued, 1);
        drop(connection);

        let reopened = ForgeStore::open(&path).unwrap();
        let indexed: i64 = reopened
            .connection
            .query_row(
                "SELECT count(*) FROM event_observability_index WHERE global_event_id = ?1",
                params![global_event_id],
                |row| row.get(0),
            )
            .unwrap();
        let queued: i64 = reopened
            .connection
            .query_row(
                "SELECT count(*) FROM event_observability_reconciliation_queue WHERE global_event_id = ?1",
                params![global_event_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(indexed, 1);
        assert_eq!(queued, 0);
    }

    #[test]
    fn deleted_observability_index_row_is_reconciled_on_reopen() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("deleted-observability-index.sqlite");
        let store = ForgeStore::open(&path).unwrap();
        let tenant_context = serde_json::json!({
            "organization": {"id": "org-test"},
            "brand": {"id": "brand-test"},
            "product": {"id": "product-test"},
            "user": {"id": "user-test"},
            "channel": {"id": "channel-test"}
        });
        let event_data = serde_json::json!({"node_id": "deleted-index-node"});
        let global_event_id = store
            .record_global_event(GlobalEventWrite {
                source: "deleted_index_test",
                source_id: "deleted-index-event-001",
                workflow_id: None,
                kind: "deleted_index_event_recorded",
                origin: "test",
                status: "recorded",
                data: &event_data,
                tenant_context: &tenant_context,
            })
            .unwrap();
        store
            .connection
            .execute(
                "DELETE FROM event_observability_index WHERE global_event_id = ?1",
                params![global_event_id],
            )
            .unwrap();
        let queued: i64 = store
            .connection
            .query_row(
                "SELECT count(*) FROM event_observability_reconciliation_queue WHERE global_event_id = ?1",
                params![global_event_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(queued, 1);
        drop(store);

        let reopened = ForgeStore::open(&path).unwrap();
        let indexed: i64 = reopened
            .connection
            .query_row(
                "SELECT count(*) FROM event_observability_index WHERE global_event_id = ?1",
                params![global_event_id],
                |row| row.get(0),
            )
            .unwrap();
        let queued: i64 = reopened
            .connection
            .query_row(
                "SELECT count(*) FROM event_observability_reconciliation_queue WHERE global_event_id = ?1",
                params![global_event_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(indexed, 1);
        assert_eq!(queued, 0);
    }

    #[test]
    fn observability_reconciliation_failure_is_deferred_after_durable_enqueue() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp
            .path()
            .join("atomic-observability-reconciliation.sqlite");
        let store = ForgeStore::open(&path).unwrap();
        store
            .connection
            .execute_batch(
                r#"
                CREATE TRIGGER reject_observability_queue_ack
                BEFORE DELETE ON event_observability_reconciliation_queue
                BEGIN
                    SELECT RAISE(ABORT, 'queue ack blocked for atomicity test');
                END;
                "#,
            )
            .unwrap();
        let tenant_context = serde_json::json!({
            "organization": {"id": "org-test"},
            "brand": {"id": "brand-test"},
            "product": {"id": "product-test"},
            "user": {"id": "user-test"},
            "channel": {"id": "channel-test"}
        });
        let event_data = serde_json::json!({"node_id": "atomic-reconciliation-node"});

        let global_event_id = store
            .record_global_event(GlobalEventWrite {
                source: "atomic_reconciliation_test",
                source_id: "atomic-reconciliation-event-001",
                workflow_id: None,
                kind: "atomic_reconciliation_event_recorded",
                origin: "test",
                status: "recorded",
                data: &event_data,
                tenant_context: &tenant_context,
            })
            .unwrap();
        let global_events: i64 = store
            .connection
            .query_row(
                "SELECT count(*) FROM global_events WHERE source_id = 'atomic-reconciliation-event-001'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let indexed: i64 = store
            .connection
            .query_row(
                "SELECT count(*) FROM event_observability_index WHERE global_event_id = ?1",
                params![global_event_id],
                |row| row.get(0),
            )
            .unwrap();
        let queued: i64 = store
            .connection
            .query_row(
                "SELECT count(*) FROM event_observability_reconciliation_queue WHERE global_event_id = ?1",
                params![global_event_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(global_events, 1, "the durable event must be recorded once");
        assert_eq!(indexed, 0, "the upsert must roll back when its ack fails");
        assert_eq!(queued, 1, "the durable queue entry must remain retryable");

        store
            .connection
            .execute_batch("DROP TRIGGER reject_observability_queue_ack")
            .unwrap();
        drop(store);

        let reopened = ForgeStore::open(&path).unwrap();
        let global_events: i64 = reopened
            .connection
            .query_row(
                "SELECT count(*) FROM global_events WHERE source_id = 'atomic-reconciliation-event-001'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let indexed: i64 = reopened
            .connection
            .query_row(
                "SELECT count(*) FROM event_observability_index WHERE global_event_id = ?1",
                params![global_event_id],
                |row| row.get(0),
            )
            .unwrap();
        let queued: i64 = reopened
            .connection
            .query_row(
                "SELECT count(*) FROM event_observability_reconciliation_queue WHERE global_event_id = ?1",
                params![global_event_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(global_events, 1);
        assert_eq!(indexed, 1);
        assert_eq!(queued, 0);
    }

    #[test]
    fn concurrent_openers_claim_each_observability_queue_entry_once() {
        const EVENT_COUNT: i64 = 32;

        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("concurrent-observability-openers.sqlite");
        let store = ForgeStore::open(&path).unwrap();
        store
            .connection
            .execute_batch(
                r#"
                CREATE TABLE observability_reconciliation_audit (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    global_event_id INTEGER NOT NULL,
                    operation TEXT NOT NULL
                );
                CREATE TRIGGER audit_observability_insert
                AFTER INSERT ON event_observability_index
                BEGIN
                    INSERT INTO observability_reconciliation_audit (global_event_id, operation)
                    VALUES (NEW.global_event_id, 'insert');
                END;
                CREATE TRIGGER audit_observability_update
                AFTER UPDATE ON event_observability_index
                BEGIN
                    INSERT INTO observability_reconciliation_audit (global_event_id, operation)
                    VALUES (NEW.global_event_id, 'update');
                END;
                "#,
            )
            .unwrap();
        let tenant_context = serde_json::json!({
            "organization": {"id": "org-test"},
            "brand": {"id": "brand-test"},
            "product": {"id": "product-test"},
            "user": {"id": "user-test"},
            "channel": {"id": "channel-test"}
        });
        let tenant_context_json = serde_json::to_string(&tenant_context).unwrap();
        let payload = "x".repeat(64 * 1024);
        store
            .with_transaction(|| {
                for index in 0..EVENT_COUNT {
                    let source_id = format!("concurrent-queued-event-{index:03}");
                    let data_json = serde_json::to_string(&serde_json::json!({
                        "index": index,
                        "payload": &payload
                    }))?;
                    store.connection.execute(
                        r#"
                        INSERT INTO global_events (
                            source, source_id, workflow_id, kind, origin, status,
                            organization_id, brand_id, product_id, user_id, channel_id,
                            tenant_context_json, data_json
                        )
                        VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                        "#,
                        params![
                            "concurrent_opener_test",
                            source_id,
                            "concurrent_opener_event_recorded",
                            "test",
                            "recorded",
                            "org-test",
                            "brand-test",
                            "product-test",
                            "user-test",
                            "channel-test",
                            &tenant_context_json,
                            data_json,
                        ],
                    )?;
                }
                Ok(())
            })
            .unwrap();
        let queued: i64 = store
            .connection
            .query_row(
                "SELECT count(*) FROM event_observability_reconciliation_queue",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(queued, EVENT_COUNT);
        drop(store);

        let barrier = Arc::new(Barrier::new(3));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let worker_barrier = Arc::clone(&barrier);
            let worker_path = path.clone();
            handles.push(thread::spawn(move || {
                worker_barrier.wait();
                ForgeStore::open(worker_path).map(drop)
            }));
        }
        barrier.wait();
        for handle in handles {
            handle.join().unwrap().unwrap();
        }

        let connection = Connection::open(&path).unwrap();
        let global_events: i64 = connection
            .query_row("SELECT count(*) FROM global_events", [], |row| row.get(0))
            .unwrap();
        let indexed: i64 = connection
            .query_row(
                "SELECT count(*) FROM event_observability_index",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let queued: i64 = connection
            .query_row(
                "SELECT count(*) FROM event_observability_reconciliation_queue",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let materializations: i64 = connection
            .query_row(
                "SELECT count(*) FROM observability_reconciliation_audit",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(global_events, EVENT_COUNT);
        assert_eq!(indexed, EVENT_COUNT);
        assert_eq!(queued, 0);
        assert_eq!(
            materializations, EVENT_COUNT,
            "each queued event must be materialized exactly once"
        );
    }

    #[test]
    fn store_open_materializes_at_most_one_observability_batch_per_invocation() {
        const EXTRA_EVENTS: usize = 3;
        let event_count = EVENT_OBSERVABILITY_RECONCILIATION_BATCH_SIZE + EXTRA_EVENTS;

        let temp = tempfile::tempdir().unwrap();
        let path = temp
            .path()
            .join("bounded-observability-reconciliation.sqlite");
        let store = ForgeStore::open(&path).unwrap();
        store
            .with_transaction(|| {
                for index in 0..event_count {
                    store.connection.execute(
                        r#"
                        INSERT INTO global_events (
                            source, source_id, workflow_id, kind, origin, status,
                            organization_id, brand_id, product_id, user_id, channel_id,
                            tenant_context_json, data_json
                        )
                        VALUES (?1, ?2, NULL, ?3, ?4, ?5, '', '', '', '', '', '{}', ?6)
                        "#,
                        params![
                            "bounded_reconciliation_test",
                            format!("bounded-reconciliation-event-{index:03}"),
                            "bounded_reconciliation_event_recorded",
                            "test",
                            "recorded",
                            serde_json::json!({"index": index}).to_string(),
                        ],
                    )?;
                }
                Ok(())
            })
            .unwrap();
        drop(store);

        let reopened = ForgeStore::open(&path).unwrap();
        let indexed: usize = reopened
            .connection
            .query_row(
                "SELECT count(*) FROM event_observability_index",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let queued: usize = reopened
            .connection
            .query_row(
                "SELECT count(*) FROM event_observability_reconciliation_queue",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(indexed, EVENT_OBSERVABILITY_RECONCILIATION_BATCH_SIZE);
        assert_eq!(queued, EXTRA_EVENTS);
        drop(reopened);

        let fully_reconciled = ForgeStore::open(&path).unwrap();
        let indexed: usize = fully_reconciled
            .connection
            .query_row(
                "SELECT count(*) FROM event_observability_index",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let queued: usize = fully_reconciled
            .connection
            .query_row(
                "SELECT count(*) FROM event_observability_reconciliation_queue",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(indexed, event_count);
        assert_eq!(queued, 0);
    }

    #[test]
    fn legacy_migration_enqueues_and_reconciles_observability_in_bounded_batches() {
        const EXTRA_EVENTS: usize = 3;
        let event_count = EVENT_OBSERVABILITY_RECONCILIATION_BATCH_SIZE + EXTRA_EVENTS;

        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("bounded-observability-migration.sqlite");
        let store = ForgeStore::open(&path).unwrap();
        store
            .connection
            .execute_batch(
                r#"
                DROP TRIGGER trg_global_events_observability_queue;
                DROP TRIGGER trg_event_observability_delete_queue;
                PRAGMA user_version = 1;
                "#,
            )
            .unwrap();
        store
            .with_transaction(|| {
                for index in 0..event_count {
                    store.connection.execute(
                        r#"
                        INSERT INTO global_events (
                            source, source_id, workflow_id, kind, origin, status,
                            organization_id, brand_id, product_id, user_id, channel_id,
                            tenant_context_json, data_json
                        )
                        VALUES (?1, ?2, NULL, ?3, ?4, ?5, '', '', '', '', '', '{}', ?6)
                        "#,
                        params![
                            "bounded_migration_test",
                            format!("bounded-migration-event-{index:03}"),
                            "bounded_migration_event_recorded",
                            "test",
                            "recorded",
                            serde_json::json!({"index": index}).to_string(),
                        ],
                    )?;
                }
                Ok(())
            })
            .unwrap();
        let queued: usize = store
            .connection
            .query_row(
                "SELECT count(*) FROM event_observability_reconciliation_queue",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(queued, 0);
        drop(store);

        let reopened = ForgeStore::open(&path).unwrap();
        let indexed: usize = reopened
            .connection
            .query_row(
                "SELECT count(*) FROM event_observability_index",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let missing: usize = reopened
            .connection
            .query_row(
                r#"
                SELECT count(*)
                FROM global_events g
                LEFT JOIN event_observability_index o
                  ON o.global_event_id = g.id
                WHERE o.global_event_id IS NULL
                "#,
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(indexed, EVENT_OBSERVABILITY_RECONCILIATION_BATCH_SIZE);
        assert_eq!(missing, EXTRA_EVENTS);
        assert_eq!(
            event_observability_trigger_count(&reopened.connection).unwrap(),
            2
        );
        drop(reopened);

        let fully_reconciled = ForgeStore::open(&path).unwrap();
        let indexed: usize = fully_reconciled
            .connection
            .query_row(
                "SELECT count(*) FROM event_observability_index",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let queued: usize = fully_reconciled
            .connection
            .query_row(
                "SELECT count(*) FROM event_observability_reconciliation_queue",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(indexed, event_count);
        assert_eq!(queued, 0);
    }

    #[test]
    fn missing_observability_trigger_is_recreated_and_gap_is_reconciled() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("missing-observability-trigger.sqlite");
        let store = ForgeStore::open(&path).unwrap();
        store
            .connection
            .execute_batch("DROP TRIGGER trg_global_events_observability_queue")
            .unwrap();
        store
            .connection
            .execute(
                r#"
                INSERT INTO global_events (
                    source, source_id, workflow_id, kind, origin, status,
                    organization_id, brand_id, product_id, user_id, channel_id,
                    tenant_context_json, data_json
                )
                VALUES (
                    'trigger_repair_test', 'trigger-gap-001', NULL,
                    'trigger_gap_recorded', 'test', 'recorded',
                    'org-test', 'brand-test', 'product-test', 'user-test', 'channel-test',
                    '{}', '{"node_id":"trigger-repair-node"}'
                )
                "#,
                [],
            )
            .unwrap();
        let global_event_id = store.connection.last_insert_rowid();
        assert_eq!(
            event_observability_trigger_count(&store.connection).unwrap(),
            1
        );
        let queued: i64 = store
            .connection
            .query_row(
                "SELECT count(*) FROM event_observability_reconciliation_queue WHERE global_event_id = ?1",
                params![global_event_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(queued, 0);
        drop(store);

        let reopened = ForgeStore::open(&path).unwrap();
        assert_eq!(
            event_observability_trigger_count(&reopened.connection).unwrap(),
            2
        );
        let queued: i64 = reopened
            .connection
            .query_row(
                "SELECT count(*) FROM event_observability_reconciliation_queue WHERE global_event_id = ?1",
                params![global_event_id],
                |row| row.get(0),
            )
            .unwrap();
        let indexed: i64 = reopened
            .connection
            .query_row(
                "SELECT count(*) FROM event_observability_index WHERE global_event_id = ?1",
                params![global_event_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(queued, 0);
        assert_eq!(indexed, 1);
    }

    #[test]
    fn malformed_observability_trigger_is_replaced_and_gap_is_reconciled() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("malformed-observability-trigger.sqlite");
        let store = ForgeStore::open(&path).unwrap();
        assert!(event_observability_triggers_are_valid(&store.connection).unwrap());
        store
            .connection
            .execute_batch(
                r#"
                DROP TRIGGER trg_global_events_observability_queue;
                CREATE TRIGGER trg_global_events_observability_queue
                AFTER INSERT ON global_events
                BEGIN
                    SELECT 1;
                END;
                "#,
            )
            .unwrap();
        assert_eq!(
            event_observability_trigger_count(&store.connection).unwrap(),
            2
        );
        assert!(!event_observability_triggers_are_valid(&store.connection).unwrap());
        store
            .connection
            .execute(
                r#"
                INSERT INTO global_events (
                    source, source_id, workflow_id, kind, origin, status,
                    organization_id, brand_id, product_id, user_id, channel_id,
                    tenant_context_json, data_json
                )
                VALUES (
                    'malformed_trigger_test', 'malformed-trigger-gap-001', NULL,
                    'malformed_trigger_gap_recorded', 'test', 'recorded',
                    'org-test', 'brand-test', 'product-test', 'user-test', 'channel-test',
                    '{}', '{"node_id":"malformed-trigger-repair-node"}'
                )
                "#,
                [],
            )
            .unwrap();
        let global_event_id = store.connection.last_insert_rowid();
        drop(store);

        let reopened = ForgeStore::open(&path).unwrap();
        assert!(event_observability_triggers_are_valid(&reopened.connection).unwrap());
        let indexed: i64 = reopened
            .connection
            .query_row(
                "SELECT count(*) FROM event_observability_index WHERE global_event_id = ?1",
                params![global_event_id],
                |row| row.get(0),
            )
            .unwrap();
        let queued: i64 = reopened
            .connection
            .query_row(
                "SELECT count(*) FROM event_observability_reconciliation_queue WHERE global_event_id = ?1",
                params![global_event_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(indexed, 1);
        assert_eq!(queued, 0);
    }

    #[test]
    fn reopen_with_empty_observability_queue_does_not_rewrite_index() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("empty-observability-queue.sqlite");
        let store = ForgeStore::open(&path).unwrap();
        let tenant_context = serde_json::json!({
            "organization": {"id": "org-test"},
            "brand": {"id": "brand-test"},
            "product": {"id": "product-test"},
            "user": {"id": "user-test"},
            "channel": {"id": "channel-test"}
        });
        let event_data = serde_json::json!({"node_id": "steady-state-node"});
        let global_event_id = store
            .record_global_event(GlobalEventWrite {
                source: "steady_state_test",
                source_id: "steady-event-001",
                workflow_id: None,
                kind: "steady_state_event_recorded",
                origin: "test",
                status: "recorded",
                data: &event_data,
                tenant_context: &tenant_context,
            })
            .unwrap();
        let queued: i64 = store
            .connection
            .query_row(
                "SELECT count(*) FROM event_observability_reconciliation_queue",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(queued, 0);
        store
            .connection
            .execute_batch(
                r#"
                CREATE TRIGGER reject_steady_state_observability_rewrite
                BEFORE UPDATE ON event_observability_index
                BEGIN
                    SELECT RAISE(ABORT, 'steady-state reopen rewrote observability index');
                END;
                "#,
            )
            .unwrap();
        drop(store);

        let reopened = ForgeStore::open(&path).unwrap();
        let indexed: i64 = reopened
            .connection
            .query_row(
                "SELECT count(*) FROM event_observability_index WHERE global_event_id = ?1",
                params![global_event_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(indexed, 1);
    }

    #[test]
    fn orphaned_operational_tenant_rows_are_repaired_once_then_reopen_read_only() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("orphaned-operational-tenant.sqlite");
        let store = ForgeStore::open(&path).unwrap();
        store
            .connection
            .execute_batch(
                r#"
                INSERT INTO runs (id, workflow_id, status, data_json)
                VALUES ('run-orphan', 'wf-missing', 'accepted', '{}');
                INSERT INTO task_leases (
                    workflow_id, task_id, lease_id, executor,
                    acquired_at, expires_at, data_json
                )
                VALUES (
                    'wf-missing', 'task-orphan', 'lease-orphan', 'audit',
                    '2026-07-23T00:00:00Z', '2026-07-23T00:05:00Z', '{}'
                );
                INSERT INTO task_checkpoints (
                    id, workflow_id, task_id, executor, state, created_at, data_json
                )
                VALUES (
                    'checkpoint-orphan', 'wf-missing', 'task-orphan', 'audit',
                    'saved', '2026-07-23T00:00:00Z', '{}'
                );
                "#,
            )
            .unwrap();
        drop(store);

        let repaired = ForgeStore::open(&path).unwrap();
        let default_tenant = operational_tenant_columns(None);
        for table in ["runs", "task_leases", "task_checkpoints"] {
            let tenant = repaired
                .connection
                .query_row(
                    &format!(
                        "SELECT organization_id, brand_id, product_id, user_id, channel_id FROM {table}"
                    ),
                    [],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                        ))
                    },
                )
                .unwrap();
            assert_eq!(tenant.0, default_tenant.organization_id);
            assert_eq!(tenant.1, default_tenant.brand_id);
            assert_eq!(tenant.2, default_tenant.product_id);
            assert_eq!(tenant.3, default_tenant.user_id);
            assert_eq!(tenant.4, default_tenant.channel_id);
        }
        drop(repaired);

        let blocker = Connection::open(&path).unwrap();
        let mode: String = blocker
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        assert_eq!(mode.to_ascii_lowercase(), "wal");
        blocker.execute_batch("BEGIN IMMEDIATE").unwrap();

        let (sender, receiver) = mpsc::channel();
        let worker_path = path.clone();
        let handle = thread::spawn(move || {
            sender
                .send(ForgeStore::open(worker_path).map(|_| ()))
                .unwrap();
        });
        let reopened = match receiver.recv_timeout(Duration::from_secs(1)) {
            Ok(result) => result,
            Err(error) => {
                blocker.execute_batch("ROLLBACK").unwrap();
                handle.join().unwrap();
                panic!(
                    "steady-state reopen attempted a write while WAL writer was active: {error}"
                );
            }
        };
        blocker.execute_batch("ROLLBACK").unwrap();
        handle.join().unwrap();
        reopened.unwrap();
    }

    #[test]
    fn ensure_wal_retries_a_transient_exclusive_lock() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("wal-retry.sqlite");
        let bootstrap = Connection::open(&path).unwrap();
        bootstrap
            .execute_batch("CREATE TABLE marker(id INTEGER); PRAGMA journal_mode=DELETE;")
            .unwrap();
        drop(bootstrap);

        let blocker = Connection::open(&path).unwrap();
        blocker.execute_batch("BEGIN EXCLUSIVE").unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let (sender, receiver) = mpsc::channel();
        let worker_barrier = Arc::clone(&barrier);
        let worker_path = path.clone();
        let handle = thread::spawn(move || {
            let connection = Connection::open(&worker_path).unwrap();
            connection.busy_timeout(Duration::ZERO).unwrap();
            worker_barrier.wait();
            let result =
                ensure_wal(&connection, &worker_path, Duration::from_secs(1)).and_then(|_| {
                    connection
                        .pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))
                        .map_err(Into::into)
                });
            sender.send(result).unwrap();
        });

        barrier.wait();
        assert!(
            matches!(
                receiver.recv_timeout(Duration::from_millis(150)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ),
            "ensure_wal returned before the transient lock was released"
        );
        blocker.execute_batch("ROLLBACK").unwrap();
        let mode = receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        assert_eq!(mode.to_ascii_lowercase(), "wal");
        handle.join().unwrap();
    }

    #[test]
    fn configured_connection_confirms_wal_after_a_transient_exclusive_lock() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("configured-wal-retry.sqlite");
        let bootstrap = Connection::open(&path).unwrap();
        bootstrap
            .execute_batch("CREATE TABLE marker(id INTEGER); PRAGMA journal_mode=DELETE;")
            .unwrap();
        drop(bootstrap);

        let blocker = Connection::open(&path).unwrap();
        blocker.execute_batch("BEGIN EXCLUSIVE").unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let (sender, receiver) = mpsc::channel();
        let worker_barrier = Arc::clone(&barrier);
        let worker_path = path.clone();
        let handle = thread::spawn(move || {
            worker_barrier.wait();
            let result = open_configured_connection(&worker_path).and_then(|connection| {
                connection
                    .pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))
                    .map_err(Into::into)
            });
            sender.send(result).unwrap();
        });

        barrier.wait();
        assert!(
            matches!(
                receiver.recv_timeout(Duration::from_millis(150)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ),
            "configured connection returned before the transient lock was released"
        );
        blocker.execute_batch("ROLLBACK").unwrap();
        let mode = receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        assert_eq!(mode.to_ascii_lowercase(), "wal");
        handle.join().unwrap();
    }
}
