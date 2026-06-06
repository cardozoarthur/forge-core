use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const AWS_OPS_COMMAND_SCHEMA: &str = "forge.aws_ops.command.v1";

const DEFAULT_AWS_OPS_RELATIVE_BIN: &str = "plugins/aws-ops/scripts/aws-ops";
const DEFAULT_AWS_OPS_RELATIVE_CONTRACT: &str =
    "plugins/aws-ops/assets/aws-api-account.contract.yaml";
const DEFAULT_AWS_OPS_RELATIVE_DATA: &str = ".codex/vaults/aws-api-account.data.yaml";

#[derive(Debug, Clone, Serialize)]
pub struct AwsOpsCommandReport {
    pub schema_version: String,
    pub status: String,
    pub action: String,
    pub aws_ops_bin: String,
    pub vault_contract: String,
    pub vault_data: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub secret_exposed: bool,
    pub secret_output_sanitized: bool,
}

pub fn resolve_aws_ops_bin(explicit: Option<&Path>) -> PathBuf {
    if let Some(path) = explicit {
        return path.to_path_buf();
    }
    if let Ok(path) = env::var("FORGE_AWS_OPS_BIN") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    if let Ok(path) = env::var("AWS_OPS_BIN") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home).join(DEFAULT_AWS_OPS_RELATIVE_BIN);
    }
    PathBuf::from("aws-ops")
}

pub fn default_aws_vault_contract() -> PathBuf {
    if let Ok(path) = env::var("FORGE_AWS_VAULT_CONTRACT") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home).join(DEFAULT_AWS_OPS_RELATIVE_CONTRACT);
    }
    PathBuf::from(DEFAULT_AWS_OPS_RELATIVE_CONTRACT)
}

pub fn default_aws_vault_data() -> PathBuf {
    if let Ok(path) = env::var("FORGE_AWS_VAULT_DATA") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    if let Ok(home) = env::var("HOME") {
        return PathBuf::from(home).join(DEFAULT_AWS_OPS_RELATIVE_DATA);
    }
    PathBuf::from(DEFAULT_AWS_OPS_RELATIVE_DATA)
}

pub fn run_check(
    aws_ops_bin: Option<&Path>,
    vault_contract: Option<&Path>,
    vault_data: Option<&Path>,
) -> Result<AwsOpsCommandReport> {
    run_capture(aws_ops_bin, vault_contract, vault_data, "check", Vec::new())
}

pub fn run_inventory(
    aws_ops_bin: Option<&Path>,
    vault_contract: Option<&Path>,
    vault_data: Option<&Path>,
    regions: Option<&str>,
    all_regions: bool,
    full: bool,
) -> Result<AwsOpsCommandReport> {
    let mut extra_args = Vec::new();
    if let Some(regions) = regions {
        if !regions.trim().is_empty() {
            extra_args.push("--regions".to_string());
            extra_args.push(regions.trim().to_string());
        }
    }
    if all_regions {
        extra_args.push("--all-regions".to_string());
    }
    if full {
        extra_args.push("--full".to_string());
    }
    run_capture(
        aws_ops_bin,
        vault_contract,
        vault_data,
        "inventory",
        extra_args,
    )
}

pub fn run_raw(
    aws_ops_bin: Option<&Path>,
    vault_contract: Option<&Path>,
    vault_data: Option<&Path>,
    allow_mutation: bool,
    reason: Option<&str>,
    aws_args: &[String],
) -> Result<AwsOpsCommandReport> {
    if aws_args.is_empty() {
        bail!("forge aws raw requires AWS CLI arguments after --");
    }
    let mut extra_args = Vec::new();
    if allow_mutation {
        extra_args.push("--allow-mutation".to_string());
        extra_args.push("--reason".to_string());
        extra_args.push(reason.unwrap_or("").to_string());
    }
    extra_args.push("--".to_string());
    extra_args.extend(aws_args.iter().cloned());
    run_capture(aws_ops_bin, vault_contract, vault_data, "raw", extra_args)
}

fn run_capture(
    aws_ops_bin: Option<&Path>,
    vault_contract: Option<&Path>,
    vault_data: Option<&Path>,
    action: &str,
    extra_args: Vec<String>,
) -> Result<AwsOpsCommandReport> {
    let bin = resolve_aws_ops_bin(aws_ops_bin);
    let contract = vault_contract
        .map(Path::to_path_buf)
        .unwrap_or_else(default_aws_vault_contract);
    let data = vault_data
        .map(Path::to_path_buf)
        .unwrap_or_else(default_aws_vault_data);

    let output = Command::new(&bin)
        .arg("--vault-contract")
        .arg(&contract)
        .arg("--vault-data")
        .arg(&data)
        .arg(action)
        .args(extra_args)
        .output()
        .with_context(|| format!("failed to run aws-ops at {}", bin.display()))?;
    let exit_code = output.status.code().unwrap_or(1);
    let raw_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let raw_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let (stdout, stdout_secret_exposed) = sanitize_aws_ops_stream(&raw_stdout);
    let (stderr, stderr_secret_exposed) = sanitize_aws_ops_stream(&raw_stderr);

    if !output.status.success() {
        bail!(
            "aws-ops {action} failed with exit code {exit_code}: {}",
            stderr.trim()
        );
    }

    Ok(AwsOpsCommandReport {
        schema_version: AWS_OPS_COMMAND_SCHEMA.to_string(),
        status: "ok".to_string(),
        action: action.to_string(),
        aws_ops_bin: bin.display().to_string(),
        vault_contract: contract.display().to_string(),
        vault_data: data.display().to_string(),
        exit_code,
        stdout,
        stderr,
        secret_exposed: false,
        secret_output_sanitized: stdout_secret_exposed || stderr_secret_exposed,
    })
}

fn sanitize_aws_ops_stream(input: &str) -> (String, bool) {
    let mut exposed = false;
    let mut lines = Vec::new();
    for line in input.lines() {
        let (line, line_exposed) = sanitize_sensitive_line(line);
        exposed |= line_exposed;
        lines.push(line);
    }
    let mut output = if input.ends_with('\n') {
        format!("{}\n", lines.join("\n"))
    } else {
        lines.join("\n")
    };
    let (masked_keys, key_exposed) = mask_aws_access_key_ids(&output);
    output = masked_keys;
    exposed |= key_exposed;
    (output, exposed)
}

fn sanitize_sensitive_line(line: &str) -> (String, bool) {
    let lower = line.to_lowercase();
    let sensitive_keys = [
        "aws_secret_access_key",
        "secretaccesskey",
        "sessiontoken",
        "aws_session_token",
        "x-amz-security-token",
        "authorization",
        "private_key",
        "credential_secret",
    ];
    let Some((key_start, key)) = sensitive_keys
        .iter()
        .filter_map(|key| lower.find(key).map(|index| (index, *key)))
        .min_by_key(|(index, _)| *index)
    else {
        return (line.to_string(), false);
    };

    let value_start = key_start + key.len();
    let Some(delimiter_offset) = line[value_start..]
        .char_indices()
        .find_map(|(index, ch)| matches!(ch, ':' | '=').then_some(index))
    else {
        return ("[REDACTED_SENSITIVE_LINE]".to_string(), true);
    };
    let delimiter_index = value_start + delimiter_offset;
    let value_prefix_end =
        delimiter_index + line[delimiter_index..].chars().next().unwrap().len_utf8();
    let mut prefix = line[..value_prefix_end].to_string();
    let mut rest = line[value_prefix_end..].chars().peekable();
    while matches!(rest.peek(), Some(ch) if ch.is_whitespace()) {
        prefix.push(rest.next().unwrap());
    }
    if matches!(rest.peek(), Some('"') | Some('\'')) {
        prefix.push(rest.next().unwrap());
    }
    prefix.push_str("[REDACTED]");
    (prefix, true)
}

fn mask_aws_access_key_ids(input: &str) -> (String, bool) {
    let mut output = String::with_capacity(input.len());
    let mut index = 0;
    let mut exposed = false;
    while index < input.len() {
        let remaining = &input[index..];
        if remaining.starts_with("AKIA") || remaining.starts_with("ASIA") {
            let candidate_len = remaining
                .chars()
                .take(20)
                .map(char::len_utf8)
                .sum::<usize>();
            if candidate_len > 0 {
                let candidate = &remaining[..candidate_len];
                if candidate.chars().count() == 20
                    && candidate.chars().all(|ch| ch.is_ascii_alphanumeric())
                {
                    output.push_str("[REDACTED_AWS_ACCESS_KEY_ID]");
                    index += candidate_len;
                    exposed = true;
                    continue;
                }
            }
        }
        let ch = remaining.chars().next().unwrap();
        output.push(ch);
        index += ch.len_utf8();
    }
    (output, exposed)
}

#[cfg(test)]
mod tests {
    use super::sanitize_aws_ops_stream;

    #[test]
    fn sanitizes_aws_secret_fields_before_reporting_output() {
        let input = "AccessKeyId=AKIAIOSFODNN7EXAMPLE\nSecretAccessKey = wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY\n{\"SessionToken\":\"token-value\",\"ok\":true}\n";
        let (sanitized, exposed) = sanitize_aws_ops_stream(input);

        assert!(exposed);
        assert!(!sanitized.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(!sanitized.contains("wJalrXUtnFEMI"));
        assert!(!sanitized.contains("token-value"));
        assert!(sanitized.contains("[REDACTED_AWS_ACCESS_KEY_ID]"));
        assert!(sanitized.contains("SecretAccessKey = [REDACTED]"));
    }

    #[test]
    fn leaves_non_secret_identity_output_readable() {
        let input =
            r#"{"Account":"123456789012","Arn":"arn:aws:iam::123456789012:user/AgentCode"}"#;
        let (sanitized, exposed) = sanitize_aws_ops_stream(input);

        assert!(!exposed);
        assert_eq!(sanitized, input);
    }
}
