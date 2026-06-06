use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_yaml::Value as YamlValue;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const CREDENTIAL_VAULT_COMMAND_SCHEMA: &str = "forge.credential_vault.command.v1";

const DEFAULT_CREDENTIAL_VAULT_RELATIVE_BIN: &str =
    ".codex/skills/credential-vault/scripts/credential-vault";

#[derive(Debug, Clone, Serialize)]
pub struct CredentialVaultCommandReport {
    pub schema_version: String,
    pub status: String,
    pub action: String,
    pub credential_vault_bin: String,
    pub contract: Option<String>,
    pub data: Option<String>,
    pub record: Option<String>,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub secret_exposed: bool,
}

pub fn resolve_credential_vault_bin(explicit: Option<&Path>) -> PathBuf {
    if let Some(path) = explicit {
        return path.to_path_buf();
    }
    if let Ok(path) = env::var("FORGE_CREDENTIAL_VAULT_BIN") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    if let Ok(path) = env::var("CREDENTIAL_VAULT_BIN") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home).join(DEFAULT_CREDENTIAL_VAULT_RELATIVE_BIN);
    }
    PathBuf::from("credential-vault")
}

pub fn run_key_init(vault_bin: Option<&Path>) -> Result<CredentialVaultCommandReport> {
    run_capture(vault_bin, "key-init", None, None, None, Vec::new())
}

pub fn run_describe(
    vault_bin: Option<&Path>,
    contract: &Path,
    data: &Path,
) -> Result<CredentialVaultCommandReport> {
    run_capture(
        vault_bin,
        "describe",
        Some(contract),
        Some(data),
        None,
        Vec::new(),
    )
}

pub fn run_records(
    vault_bin: Option<&Path>,
    contract: &Path,
    data: &Path,
) -> Result<CredentialVaultCommandReport> {
    run_capture(
        vault_bin,
        "records",
        Some(contract),
        Some(data),
        None,
        Vec::new(),
    )
}

pub fn run_panel(
    vault_bin: Option<&Path>,
    contract: &Path,
    data: &Path,
    open: bool,
    timeout_seconds: Option<u64>,
    no_cli_fallback: bool,
) -> Result<CredentialVaultCommandReport> {
    let mut extra_args = Vec::new();
    if open {
        extra_args.push("--open".to_string());
    }
    if let Some(timeout_seconds) = timeout_seconds {
        extra_args.push("--timeout-seconds".to_string());
        extra_args.push(timeout_seconds.to_string());
    }
    if no_cli_fallback {
        extra_args.push("--no-cli-fallback".to_string());
    }
    run_capture(
        vault_bin,
        "panel",
        Some(contract),
        Some(data),
        None,
        extra_args,
    )
}

pub fn run_exec(
    vault_bin: Option<&Path>,
    contract: &Path,
    data: &Path,
    record: &str,
    manual_env_mappings: &[String],
    command: &[String],
) -> Result<i32> {
    if record.trim().is_empty() {
        bail!("credential-vault record cannot be empty");
    }
    if command.is_empty() {
        bail!("credential-vault exec requires a child command after --");
    }

    let mut env_mappings = terminal_env_mappings_from_contract(contract, record)?;
    for mapping in manual_env_mappings {
        validate_env_mapping(mapping)?;
        env_mappings.push(mapping.to_string());
    }
    if env_mappings.is_empty() {
        bail!(
            "no credential-vault env mappings found for record {record}; add usage.terminal_env in the contract or pass --env NAME=field.path"
        );
    }

    let bin = resolve_credential_vault_bin(vault_bin);
    let status = Command::new(&bin)
        .arg("exec")
        .arg("--contract")
        .arg(contract)
        .arg("--data")
        .arg(data)
        .arg("--record")
        .arg(record)
        .args(env_mappings.iter().flat_map(|mapping| ["--env", mapping]))
        .arg("--")
        .args(command)
        .status()
        .with_context(|| format!("failed to run credential-vault at {}", bin.display()))?;

    Ok(status.code().unwrap_or(1))
}

pub fn terminal_env_mappings_from_contract(contract: &Path, record: &str) -> Result<Vec<String>> {
    let text = fs::read_to_string(contract).with_context(|| {
        format!(
            "failed to read credential-vault contract {}",
            contract.display()
        )
    })?;
    let yaml: YamlValue = serde_yaml::from_str(&text).with_context(|| {
        format!(
            "failed to parse credential-vault contract {}",
            contract.display()
        )
    })?;
    let records = yaml_get(&yaml, "records")
        .and_then(YamlValue::as_mapping)
        .context("credential-vault contract is missing records mapping")?;
    let record_key = YamlValue::String(record.to_string());
    let record_yaml = records
        .get(&record_key)
        .with_context(|| format!("credential-vault record {record} was not found in contract"))?;
    let fields = yaml_get(record_yaml, "fields")
        .and_then(YamlValue::as_sequence)
        .with_context(|| format!("credential-vault record {record} is missing fields"))?;

    let mut mappings = Vec::new();
    for field in fields {
        let Some(path) = yaml_get(field, "path").and_then(YamlValue::as_str) else {
            continue;
        };
        let Some(usage) = yaml_get(field, "usage") else {
            continue;
        };
        let Some(terminal_env) = yaml_get(usage, "terminal_env").and_then(YamlValue::as_str) else {
            continue;
        };
        if terminal_env.trim().is_empty() || path.trim().is_empty() {
            continue;
        }
        let mapping = format!("{}={}", terminal_env.trim(), path.trim());
        validate_env_mapping(&mapping)?;
        mappings.push(mapping);
    }

    Ok(mappings)
}

fn run_capture(
    vault_bin: Option<&Path>,
    action: &str,
    contract: Option<&Path>,
    data: Option<&Path>,
    record: Option<&str>,
    extra_args: Vec<String>,
) -> Result<CredentialVaultCommandReport> {
    let bin = resolve_credential_vault_bin(vault_bin);
    let mut command = Command::new(&bin);
    command.arg(action);
    if let Some(contract) = contract {
        command.arg("--contract").arg(contract);
    }
    if let Some(data) = data {
        command.arg("--data").arg(data);
    }
    if let Some(record) = record {
        command.arg("--record").arg(record);
    }
    command.args(extra_args);

    let output = command
        .output()
        .with_context(|| format!("failed to run credential-vault at {}", bin.display()))?;
    let exit_code = output.status.code().unwrap_or(1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        bail!(
            "credential-vault {action} failed with exit code {exit_code}: {}",
            stderr.trim()
        );
    }

    Ok(CredentialVaultCommandReport {
        schema_version: CREDENTIAL_VAULT_COMMAND_SCHEMA.to_string(),
        status: "ok".to_string(),
        action: action.to_string(),
        credential_vault_bin: bin.display().to_string(),
        contract: contract.map(|path| path.display().to_string()),
        data: data.map(|path| path.display().to_string()),
        record: record.map(ToString::to_string),
        exit_code,
        stdout,
        stderr,
        secret_exposed: false,
    })
}

fn validate_env_mapping(mapping: &str) -> Result<()> {
    let Some((name, path)) = mapping.split_once('=') else {
        bail!("credential-vault env mapping must use NAME=field.path");
    };
    if name.trim().is_empty() || path.trim().is_empty() {
        bail!("credential-vault env mapping must use NAME=field.path");
    }
    Ok(())
}

fn yaml_get<'a>(value: &'a YamlValue, key: &str) -> Option<&'a YamlValue> {
    let mapping = value.as_mapping()?;
    let key = YamlValue::String(key.to_string());
    mapping.get(&key)
}
