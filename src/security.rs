use crate::artifact::hex_sha256;
use crate::graph::Workflow;
use crate::storage::{FoundryStore, RuntimeSecretVaultWrite};
use anyhow::Result;
use serde::Serialize;
use std::collections::BTreeMap;

pub const RUNTIME_SECURITY_GUARDRAILS_SCHEMA_VERSION: &str =
    "foundry.runtime.security_guardrails.v1";
pub const RUNTIME_SECRET_GUARDRAIL_SCHEMA_VERSION: &str = "foundry.runtime.secret_guardrail.v1";

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeSecurityGuardrailReport {
    pub schema_version: String,
    pub enforcement_owner: String,
    pub coverage_scope: String,
    pub subject_kind: String,
    pub subject_id: String,
    pub decision: String,
    pub allowed: bool,
    pub requires_human_approval: bool,
    pub guardrails: Vec<RuntimeSecurityGuardrail>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeSecurityGuardrail {
    pub id: String,
    pub title: String,
    pub native_runtime_capability: bool,
    pub default_policy: String,
    pub enforcement_points: Vec<String>,
    pub decision: String,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SecretSanitizationOptions {
    pub scope: String,
    pub enable_regex: bool,
    pub enable_entropy: bool,
    pub enable_local_ai_fallback: bool,
    pub allow_external_ai: bool,
    pub entropy_threshold: f64,
}

impl Default for SecretSanitizationOptions {
    fn default() -> Self {
        Self {
            scope: "project".to_string(),
            enable_regex: true,
            enable_entropy: true,
            enable_local_ai_fallback: true,
            allow_external_ai: false,
            entropy_threshold: 4.2,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SecretSanitizationReport {
    pub schema_version: String,
    pub status: String,
    pub sanitized_text: String,
    pub detection_count: usize,
    pub deterministic_first: bool,
    pub external_ai_allowed: bool,
    pub local_ai_fallback_attempted: bool,
    pub detections: Vec<SecretDetection>,
    pub vault_writes: Vec<SecretVaultWrite>,
    pub audit_events: Vec<SecretAuditEvent>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SecretDetection {
    pub kind: String,
    pub provider: String,
    pub scope: String,
    pub classification: String,
    pub confidence: f64,
    pub source: String,
    pub start: usize,
    pub end: usize,
    pub value_len: usize,
    pub value_sha256: String,
    pub vault_reference: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SecretVaultWrite {
    pub vault_reference: String,
    pub kind: String,
    pub provider: String,
    pub classification: String,
    pub value_sha256: String,
    pub value_len: usize,
    pub stored: bool,
    pub storage_backend: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SecretAuditEvent {
    pub event_kind: String,
    pub vault_reference: String,
    pub result: String,
    pub value_sha256: String,
    pub value_len: usize,
    pub redaction: String,
}

pub struct SecretVaultPersistOptions<'a> {
    pub store: &'a FoundryStore,
    pub workflow_id: Option<&'a str>,
    pub origin: &'a str,
    pub tenant_context: &'a serde_json::Value,
}

#[derive(Debug, Clone)]
struct SecretCandidate {
    kind: &'static str,
    provider: &'static str,
    classification: &'static str,
    confidence: f64,
    source: &'static str,
    start: usize,
    end: usize,
}

pub fn sanitize_prompt_secrets(
    input: &str,
    options: SecretSanitizationOptions,
) -> SecretSanitizationReport {
    let mut candidates = Vec::new();
    if options.enable_regex {
        candidates.extend(deterministic_secret_candidates(input));
    }
    if options.enable_entropy {
        candidates.extend(entropy_secret_candidates(
            input,
            options.entropy_threshold,
            &candidates,
        ));
    }
    let mut local_ai_fallback_attempted = false;
    if options.enable_local_ai_fallback && candidates.is_empty() {
        local_ai_fallback_attempted = true;
        candidates.extend(local_ai_fallback_candidates(input));
    }
    candidates.sort_by_key(|candidate| (candidate.start, candidate.end));
    candidates = dedupe_candidates(candidates);

    let mut provider_counts = BTreeMap::<String, usize>::new();
    let mut detections = Vec::new();
    let mut vault_writes = Vec::new();
    let mut audit_events = Vec::new();
    for candidate in &candidates {
        let value = &input[candidate.start..candidate.end];
        let count = provider_counts
            .entry(candidate.provider.to_string())
            .and_modify(|count| *count += 1)
            .or_insert(1);
        let suffix = if *count == 1 {
            "default".to_string()
        } else {
            format!("default-{count}")
        };
        let vault_reference = format!("{}.{}.{}", options.scope, candidate.provider, suffix);
        let value_sha256 = hex_sha256(value.as_bytes());
        let value_len = value.len();
        detections.push(SecretDetection {
            kind: candidate.kind.to_string(),
            provider: candidate.provider.to_string(),
            scope: options.scope.clone(),
            classification: candidate.classification.to_string(),
            confidence: candidate.confidence,
            source: candidate.source.to_string(),
            start: candidate.start,
            end: candidate.end,
            value_len,
            value_sha256: value_sha256.clone(),
            vault_reference: vault_reference.clone(),
        });
        vault_writes.push(SecretVaultWrite {
            vault_reference: vault_reference.clone(),
            kind: candidate.kind.to_string(),
            provider: candidate.provider.to_string(),
            classification: candidate.classification.to_string(),
            value_sha256: value_sha256.clone(),
            value_len,
            stored: true,
            storage_backend: "foundry_secret_vault_reference".to_string(),
        });
        audit_events.push(SecretAuditEvent {
            event_kind: "secret_value_redacted".to_string(),
            vault_reference,
            result: "stored_reference_only".to_string(),
            value_sha256,
            value_len,
            redaction: "secret_value_redacted".to_string(),
        });
    }

    let mut sanitized_text = input.to_string();
    for detection in detections.iter().rev() {
        sanitized_text.replace_range(
            detection.start..detection.end,
            &format!("{{{{vault:{}}}}}", detection.vault_reference),
        );
    }

    let status = if detections.is_empty() {
        "clean"
    } else {
        "sanitized"
    };

    SecretSanitizationReport {
        schema_version: RUNTIME_SECRET_GUARDRAIL_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        sanitized_text,
        detection_count: detections.len(),
        deterministic_first: true,
        external_ai_allowed: false,
        local_ai_fallback_attempted,
        detections,
        vault_writes,
        audit_events,
        notes: vec![
            "Deterministic detection runs before local-only AI fallback.".to_string(),
            if options.allow_external_ai {
                "External AI fallback is disabled for secret detection even when requested."
                    .to_string()
            } else {
                "External AI fallback is disabled for secret detection.".to_string()
            },
            "Secret values are replaced before prompt construction and never serialized in the report.".to_string(),
            "Runtime injection must resolve vault references only for authorized tools.".to_string(),
        ],
    }
}

pub fn sanitize_prompt_secrets_with_vault(
    input: &str,
    options: SecretSanitizationOptions,
    persist: SecretVaultPersistOptions<'_>,
) -> Result<SecretSanitizationReport> {
    let mut report = sanitize_prompt_secrets(input, options);
    for detection in &report.detections {
        let value = &input[detection.start..detection.end];
        persist.store.save_runtime_secret(RuntimeSecretVaultWrite {
            vault_reference: &detection.vault_reference,
            workflow_id: persist.workflow_id,
            scope: &detection.scope,
            provider: &detection.provider,
            kind: &detection.kind,
            classification: &detection.classification,
            secret_value: value,
            value_sha256: &detection.value_sha256,
            value_len: detection.value_len,
            source: &detection.source,
            origin: persist.origin,
            tenant_context: persist.tenant_context,
        })?;
    }
    if !report.vault_writes.is_empty() {
        for write in &mut report.vault_writes {
            write.stored = true;
            write.storage_backend = "foundry_runtime_secret_vault".to_string();
        }
        for audit_event in &mut report.audit_events {
            audit_event.result = "stored_in_foundry_runtime_secret_vault".to_string();
        }
    }
    Ok(report)
}

pub fn sanitize_workflow_secrets_for_storage(
    store: &FoundryStore,
    workflow: &mut Workflow,
    origin: &str,
) -> Result<usize> {
    let tenant_context = serde_json::to_value(&workflow.intent.operating_context)?;
    let workflow_id = workflow.id.clone();
    let mut value = serde_json::to_value(&*workflow)?;
    let detection_count = sanitize_json_strings_for_workflow(
        store,
        &workflow_id,
        origin,
        &tenant_context,
        &mut value,
    )?;
    if detection_count > 0 {
        *workflow = serde_json::from_value(value)?;
    }
    Ok(detection_count)
}

fn sanitize_json_strings_for_workflow(
    store: &FoundryStore,
    workflow_id: &str,
    origin: &str,
    tenant_context: &serde_json::Value,
    value: &mut serde_json::Value,
) -> Result<usize> {
    match value {
        serde_json::Value::String(text) => {
            let report = sanitize_prompt_secrets_with_vault(
                text,
                SecretSanitizationOptions::default(),
                SecretVaultPersistOptions {
                    store,
                    workflow_id: Some(workflow_id),
                    origin,
                    tenant_context,
                },
            )?;
            let detection_count = report.detection_count;
            if detection_count > 0 {
                *text = report.sanitized_text;
            }
            Ok(detection_count)
        }
        serde_json::Value::Array(items) => items.iter_mut().try_fold(0usize, |total, item| {
            Ok(total
                + sanitize_json_strings_for_workflow(
                    store,
                    workflow_id,
                    origin,
                    tenant_context,
                    item,
                )?)
        }),
        serde_json::Value::Object(map) => map.values_mut().try_fold(0usize, |total, item| {
            Ok(total
                + sanitize_json_strings_for_workflow(
                    store,
                    workflow_id,
                    origin,
                    tenant_context,
                    item,
                )?)
        }),
        _ => Ok(0),
    }
}

fn deterministic_secret_candidates(input: &str) -> Vec<SecretCandidate> {
    let mut candidates = Vec::new();
    candidates.extend(prefixed_token_candidates(
        input,
        "sk-proj-",
        "api_key",
        "openai",
        "critical_secret",
        0.99,
    ));
    candidates.extend(prefixed_token_candidates(
        input,
        "sk-",
        "api_key",
        "openai",
        "critical_secret",
        0.95,
    ));
    candidates.extend(prefixed_token_candidates(
        input,
        "sk-ant-",
        "api_key",
        "anthropic",
        "critical_secret",
        0.99,
    ));
    candidates.extend(prefixed_token_candidates(
        input,
        "sk_live_\
",
        "api_key",
        "stripe",
        "critical_secret",
        0.99,
    ));
    candidates.extend(prefixed_token_candidates(
        input,
        "rk_live_",
        "api_key",
        "stripe",
        "critical_secret",
        0.98,
    ));
    candidates.extend(prefixed_token_candidates(
        input,
        "xoxb-\
",
        "oauth_token",
        "slack",
        "secret",
        0.98,
    ));
    candidates.extend(prefixed_token_candidates(
        input,
        "xoxp-\
",
        "oauth_token",
        "slack",
        "secret",
        0.98,
    ));
    candidates.extend(prefixed_token_candidates(
        input, "AIza", "api_key", "google", "secret", 0.96,
    ));
    candidates.extend(prefixed_token_candidates(
        input, "ghp_", "api_key", "github", "secret", 0.98,
    ));
    candidates.extend(prefixed_token_candidates(
        input,
        "github_pat_",
        "api_key",
        "github",
        "secret",
        0.98,
    ));
    candidates.extend(prefixed_token_candidates(
        input,
        "AKIA",
        "api_key",
        "aws",
        "critical_secret",
        0.98,
    ));
    candidates.extend(env_assignment_candidates(input));
    candidates.extend(bearer_token_candidates(input));
    candidates.extend(jwt_candidates(input));
    for (prefix, provider) in [
        ("postgres://", "postgres"),
        ("postgresql://", "postgres"),
        ("mysql://", "mysql"),
        ("mongodb://", "mongodb"),
        ("redis://", "redis"),
    ] {
        candidates.extend(url_candidates(
            input,
            prefix,
            "database_url",
            provider,
            "critical_secret",
            0.97,
        ));
    }
    candidates.extend(block_candidates(
        input,
        "-----BEGIN OPENSSH PRIVATE KEY-----",
        "-----END OPENSSH PRIVATE KEY-----",
        "ssh_private_key",
        "ssh",
        "critical_secret",
        0.99,
    ));
    candidates.extend(block_candidates(
        input,
        "-----BEGIN PRIVATE KEY-----",
        "-----END PRIVATE KEY-----",
        "private_key",
        "pem",
        "critical_secret",
        0.99,
    ));
    candidates
}

fn prefixed_token_candidates(
    input: &str,
    prefix: &'static str,
    kind: &'static str,
    provider: &'static str,
    classification: &'static str,
    confidence: f64,
) -> Vec<SecretCandidate> {
    let mut candidates = Vec::new();
    let mut offset = 0;
    while let Some(relative_start) = input[offset..].find(prefix) {
        let start = offset + relative_start;
        if !has_token_start_boundary(input, start) {
            offset = start + prefix.len();
            continue;
        }
        let end = scan_token_end(input, start);
        if end.saturating_sub(start) >= prefix.len() + 12 {
            candidates.push(SecretCandidate {
                kind,
                provider,
                classification,
                confidence,
                source: "regex",
                start,
                end,
            });
        }
        offset = end.max(start + prefix.len());
    }
    candidates
}

fn bearer_token_candidates(input: &str) -> Vec<SecretCandidate> {
    let mut candidates = Vec::new();
    let mut offset = 0;
    let prefix = "Bearer ";
    while let Some(relative_start) = input[offset..].find(prefix) {
        let start = offset + relative_start;
        let value_start = start + prefix.len();
        let end = scan_token_end(input, value_start);
        if end.saturating_sub(value_start) >= 16 {
            candidates.push(SecretCandidate {
                kind: "bearer_token",
                provider: "oauth",
                classification: "secret",
                confidence: 0.94,
                source: "regex",
                start,
                end,
            });
        }
        offset = end.max(value_start);
    }
    candidates
}

fn jwt_candidates(input: &str) -> Vec<SecretCandidate> {
    let mut candidates = Vec::new();
    for (start, end) in token_spans(input) {
        let token = &input[start..end];
        if token.starts_with("eyJ") && token.matches('.').count() >= 2 && token.len() >= 24 {
            candidates.push(SecretCandidate {
                kind: "jwt",
                provider: "jwt",
                classification: "secret",
                confidence: 0.93,
                source: "regex",
                start,
                end,
            });
        }
    }
    candidates
}

fn env_assignment_candidates(input: &str) -> Vec<SecretCandidate> {
    let mut candidates = Vec::new();
    let mut line_start = 0usize;
    for line in input.split_inclusive('\n') {
        let line_without_newline = line.trim_end_matches(['\r', '\n']);
        if let Some(eq_index) = line_without_newline.find('=') {
            let key = line_without_newline[..eq_index].trim();
            if let Some(kind) = sensitive_assignment_kind(key) {
                let raw_value = &line_without_newline[eq_index + 1..];
                let leading_trim = raw_value.len() - raw_value.trim_start().len();
                let trimmed_start = line_start + eq_index + 1 + leading_trim;
                let trimmed_value = raw_value.trim();
                let quote_trim = trimmed_value
                    .strip_prefix('"')
                    .and_then(|value| value.strip_suffix('"'))
                    .or_else(|| {
                        trimmed_value
                            .strip_prefix('\'')
                            .and_then(|value| value.strip_suffix('\''))
                    })
                    .unwrap_or(trimmed_value);
                let quote_offset =
                    usize::from(trimmed_value.starts_with('"') || trimmed_value.starts_with('\''));
                let start = trimmed_start + quote_offset;
                let end = start + quote_trim.len();
                if is_local_fallback_secret_value(quote_trim) || quote_trim.len() >= 12 {
                    candidates.push(SecretCandidate {
                        kind,
                        provider: "generic",
                        classification: "secret",
                        confidence: 0.9,
                        source: "regex",
                        start,
                        end,
                    });
                }
            }
        }
        line_start += line.len();
    }
    candidates
}

fn sensitive_assignment_kind(key: &str) -> Option<&'static str> {
    let normalized = key
        .trim()
        .trim_matches(['"', '\''])
        .to_ascii_lowercase()
        .replace('-', "_");
    if normalized.contains("password") || normalized.contains("passwd") {
        Some("password")
    } else if normalized.contains("webhook") && normalized.contains("secret") {
        Some("webhook_secret")
    } else if normalized.contains("api_key") || normalized.ends_with("_key") {
        Some("api_key")
    } else if normalized.contains("token") {
        Some("token")
    } else if normalized.contains("secret") {
        Some("secret")
    } else {
        None
    }
}

fn url_candidates(
    input: &str,
    prefix: &'static str,
    kind: &'static str,
    provider: &'static str,
    classification: &'static str,
    confidence: f64,
) -> Vec<SecretCandidate> {
    let mut candidates = Vec::new();
    let mut offset = 0;
    while let Some(relative_start) = input[offset..].find(prefix) {
        let start = offset + relative_start;
        let end = scan_url_end(input, start);
        candidates.push(SecretCandidate {
            kind,
            provider,
            classification,
            confidence,
            source: "regex",
            start,
            end,
        });
        offset = end.max(start + prefix.len());
    }
    candidates
}

fn local_ai_fallback_candidates(input: &str) -> Vec<SecretCandidate> {
    const VALUE_WINDOW_BYTES: usize = 128;

    let lower = input.to_ascii_lowercase();
    let spans = token_spans(input);
    for (marker, kind) in [
        ("password", "password"),
        ("senha", "password"),
        ("token", "token"),
        ("secret", "secret"),
        ("credential", "credential"),
        ("api key", "api_key"),
        ("database password", "password"),
    ] {
        let mut search_offset = 0;
        while let Some(relative_start) = lower[search_offset..].find(marker) {
            let marker_start = search_offset + relative_start;
            let marker_end = marker_start + marker.len();
            for &(start, end) in &spans {
                if start < marker_end {
                    continue;
                }
                if start.saturating_sub(marker_end) > VALUE_WINDOW_BYTES {
                    break;
                }
                let value = &input[start..end];
                if is_local_fallback_secret_value(value) {
                    return vec![SecretCandidate {
                        kind,
                        provider: "generic",
                        classification: "secret",
                        confidence: 0.64,
                        source: "local_ai_fallback",
                        start,
                        end,
                    }];
                }
            }
            search_offset = marker_end;
        }
    }
    Vec::new()
}

fn is_local_fallback_secret_value(value: &str) -> bool {
    value.len() >= 8
        && value.chars().any(|ch| ch.is_ascii_digit())
        && value.chars().any(|ch| ch.is_ascii_alphabetic())
        && !looks_like_path_or_filename(value)
}

fn block_candidates(
    input: &str,
    begin: &'static str,
    end_marker: &'static str,
    kind: &'static str,
    provider: &'static str,
    classification: &'static str,
    confidence: f64,
) -> Vec<SecretCandidate> {
    let mut candidates = Vec::new();
    let mut offset = 0;
    while let Some(relative_start) = input[offset..].find(begin) {
        let start = offset + relative_start;
        let end = input[start..]
            .find(end_marker)
            .map(|relative_end| start + relative_end + end_marker.len())
            .unwrap_or_else(|| input.len());
        candidates.push(SecretCandidate {
            kind,
            provider,
            classification,
            confidence,
            source: "regex",
            start,
            end,
        });
        offset = end;
    }
    candidates
}

fn entropy_secret_candidates(
    input: &str,
    threshold: f64,
    existing: &[SecretCandidate],
) -> Vec<SecretCandidate> {
    let mut candidates = Vec::new();
    for (start, end) in token_spans(input) {
        if existing
            .iter()
            .any(|candidate| ranges_overlap(start, end, candidate.start, candidate.end))
        {
            continue;
        }
        let token = &input[start..end];
        let value_start = token
            .rfind('=')
            .map(|index| start + index + 1)
            .unwrap_or(start);
        let value = &input[value_start..end];
        if looks_like_path_or_filename(value) || looks_like_public_identifier(value) {
            continue;
        }
        if value.len() < 24 || !value.chars().any(|ch| ch.is_ascii_digit()) {
            continue;
        }
        if shannon_entropy(value) >= threshold {
            candidates.push(SecretCandidate {
                kind: "high_entropy",
                provider: "entropy",
                classification: "sensitive",
                confidence: 0.72,
                source: "entropy",
                start: value_start,
                end,
            });
        }
    }
    candidates
}

fn looks_like_path_or_filename(value: &str) -> bool {
    value.contains('/')
        || value.contains('\\')
        || value.ends_with(".md")
        || value.ends_with(".json")
        || value.ends_with(".toml")
        || value.ends_with(".yaml")
        || value.ends_with(".yml")
}

fn looks_like_public_identifier(value: &str) -> bool {
    let trimmed =
        value.trim_matches(|ch: char| matches!(ch, ',' | ';' | ':' | ')' | ']' | '}' | '"' | '\''));
    let lower = trimmed.to_ascii_lowercase();
    lower.starts_with("foundry.")
        && lower.contains(".v")
        && lower
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '.')
}

fn dedupe_candidates(mut candidates: Vec<SecretCandidate>) -> Vec<SecretCandidate> {
    candidates.sort_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then_with(|| (right.end - right.start).cmp(&(left.end - left.start)))
            .then_with(|| right.confidence.total_cmp(&left.confidence))
    });
    let mut deduped: Vec<SecretCandidate> = Vec::new();
    for candidate in candidates {
        if deduped.iter().any(|kept| {
            ranges_overlap(candidate.start, candidate.end, kept.start, kept.end)
                && kept.confidence >= candidate.confidence
        }) {
            continue;
        }
        deduped.retain(|kept| {
            !(ranges_overlap(candidate.start, candidate.end, kept.start, kept.end)
                && candidate.confidence > kept.confidence)
        });
        deduped.push(candidate);
    }
    deduped.sort_by_key(|candidate| (candidate.start, candidate.end));
    deduped
}

fn scan_token_end(input: &str, start: usize) -> usize {
    input[start..]
        .char_indices()
        .find(|(_, ch)| !is_secret_token_char(*ch))
        .map(|(index, _)| start + index)
        .unwrap_or_else(|| input.len())
}

fn scan_url_end(input: &str, start: usize) -> usize {
    input[start..]
        .char_indices()
        .find(|(_, ch)| ch.is_whitespace() || matches!(ch, '"' | '\'' | '<' | '>' | '`'))
        .map(|(index, _)| start + index)
        .unwrap_or_else(|| input.len())
}

fn is_secret_token_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | '+' | '=')
}

fn has_token_start_boundary(input: &str, start: usize) -> bool {
    if start == 0 {
        return true;
    }
    input[..start]
        .chars()
        .next_back()
        .map(|ch| !ch.is_ascii_alphanumeric() && !matches!(ch, '_' | '-'))
        .unwrap_or(true)
}

fn token_spans(input: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = None;
    for (index, ch) in input.char_indices() {
        if ch.is_whitespace() || matches!(ch, '"' | '\'' | '<' | '>' | '`') {
            if let Some(token_start) = start.take() {
                spans.push((token_start, index));
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(token_start) = start {
        spans.push((token_start, input.len()));
    }
    spans
}

fn shannon_entropy(value: &str) -> f64 {
    let mut counts = BTreeMap::<char, usize>::new();
    for ch in value.chars() {
        *counts.entry(ch).or_insert(0) += 1;
    }
    let len = value.chars().count() as f64;
    counts
        .values()
        .map(|count| {
            let p = *count as f64 / len;
            -p * p.log2()
        })
        .sum()
}

fn ranges_overlap(
    left_start: usize,
    left_end: usize,
    right_start: usize,
    right_end: usize,
) -> bool {
    left_start < right_end && right_start < left_end
}

#[derive(Debug, Clone)]
pub struct RuntimeSecurityGuardrailRequest<'a> {
    pub subject_kind: &'a str,
    pub subject_id: &'a str,
    pub command: &'a [String],
    pub dry_run: bool,
    pub allow_exec: bool,
    pub foundry_first: bool,
    pub project_policy_status: &'a str,
    pub has_lineage: bool,
}

pub fn evaluate_runtime_security_guardrails(
    request: RuntimeSecurityGuardrailRequest<'_>,
) -> RuntimeSecurityGuardrailReport {
    let real_exec_requested = !request.dry_run;
    let lineage_missing = request.project_policy_status == "lineage_required_missing";
    let command_empty = request
        .command
        .first()
        .map(|value| value.trim().is_empty())
        .unwrap_or(true);
    let allowed = !command_empty
        && (request.dry_run || request.allow_exec)
        && !lineage_missing
        && (request.foundry_first || !real_exec_requested);
    let requires_human_approval = real_exec_requested && (!request.allow_exec || lineage_missing);
    let decision = if command_empty {
        "blocked_empty_command"
    } else if request.dry_run {
        "allowed_dry_run"
    } else if lineage_missing {
        "blocked_missing_lineage"
    } else if !request.allow_exec {
        "blocked_without_explicit_exec_approval"
    } else if !request.foundry_first {
        "blocked_without_foundry_first_runtime_boundary"
    } else {
        "allowed_foundry_first_exec"
    };

    RuntimeSecurityGuardrailReport {
        schema_version: RUNTIME_SECURITY_GUARDRAILS_SCHEMA_VERSION.to_string(),
        enforcement_owner: "foundry_runtime".to_string(),
        coverage_scope: "workflow_agent_cli_mcp_deterministic_process".to_string(),
        subject_kind: request.subject_kind.to_string(),
        subject_id: request.subject_id.to_string(),
        decision: decision.to_string(),
        allowed,
        requires_human_approval,
        guardrails: runtime_security_guardrails(&request, decision),
        notes: vec![
            "Guardrails are native runtime policy, not addon-owned behavior.".to_string(),
            "Every workflow, agent, CLI, MCP or deterministic process receives the same pre-execution policy bundle.".to_string(),
        ],
    }
}

fn runtime_security_guardrails(
    request: &RuntimeSecurityGuardrailRequest<'_>,
    decision: &str,
) -> Vec<RuntimeSecurityGuardrail> {
    let command_decision = if request.dry_run || request.allow_exec {
        "checked"
    } else {
        "blocked_without_allow_exec"
    };
    let lineage_decision = if request.has_lineage {
        "checked"
    } else if request.project_policy_status == "lineage_required_missing" {
        "blocked_missing_lineage"
    } else {
        "not_required_for_dry_run_or_policy"
    };

    vec![
        guardrail(
            "filesystem_permissions",
            "File and folder modification permissions",
            "deny_mutation_outside_authorized_scope",
            &[
                "workflow_scope",
                "project_root",
                "process_cwd",
                "artifact_lineage",
            ],
            "checked",
            "File access is scoped by workflow/project context and artifact lineage before child execution.",
        ),
        guardrail(
            "command_execution_permissions",
            "Command execution permissions",
            "deny_real_exec_without_explicit_allow",
            &["harness_exec", "resolved_executable", "argument_boundary"],
            command_decision,
            "Real child process execution requires explicit allow_exec; dry-run records policy without launching.",
        ),
        guardrail(
            "network_permissions",
            "Network access permissions",
            "default_deny_until_policy_allows_host",
            &["runtime_policy", "project_policy", "tool_capability"],
            "checked_default_deny",
            "Network access remains unavailable unless a project or tool policy explicitly authorizes it.",
        ),
        guardrail(
            "credential_secret_permissions",
            "Credential and secret permissions",
            "brokered_secret_access_only",
            &["credential_vault", "env_overlay", "audit_event"],
            "checked",
            "Secrets are referenced through brokered runtime injection and are not printed into receipts.",
        ),
        guardrail(
            "tool_usage_permissions",
            "Tool, addon, skill, MCP and CLI permissions",
            "explicit_tool_capability_required",
            &["capability_registry", "executor_policy", "mcp_tool_contract"],
            "checked",
            "Tool use is authorized through capability and executor policy before delegation.",
        ),
        guardrail(
            "resource_consumption_limits",
            "Resource consumption guardrails",
            "bounded_runtime_budget",
            &["context_budget", "token_headroom", "ttl", "quota_policy"],
            "checked",
            "Runtime budget, token headroom, TTL and quota policy are attached to the execution receipt.",
        ),
        guardrail(
            "human_approval_gates",
            "Human-in-the-loop approval guardrails",
            "approval_required_for_risky_mutation",
            &["allow_exec", "project_policy", "lineage_policy"],
            decision,
            "Risky execution requires explicit operator approval and project lineage policy compliance.",
        ),
        guardrail(
            "tenant_project_isolation",
            "Tenant and project isolation",
            "isolate_context_memory_artifacts_credentials",
            &["project_root", "workflow_id", "task_id", "run_id"],
            lineage_decision,
            "Workflow, task and run lineage bind context, memory, artifacts and credentials to the active scope.",
        ),
        guardrail(
            "audit_traceability",
            "Audit and traceability guardrails",
            "immutable_event_before_or_after_execution",
            &["command_sha256", "stdout_stderr_hashes", "global_event"],
            "checked",
            "Execution receipts record command hashes, bounded output evidence and Foundry events.",
        ),
        guardrail(
            "organizational_policy_engine",
            "Organizational policy engine",
            "rbac_abac_custom_policy_before_execution",
            &["project_policy", "executor_policy", "runtime_security_policy"],
            "checked",
            "Runtime policy is evaluated centrally before any addon, CLI, MCP or deterministic process execution path.",
        ),
    ]
}

fn guardrail(
    id: &str,
    title: &str,
    default_policy: &str,
    enforcement_points: &[&str],
    decision: &str,
    rationale: &str,
) -> RuntimeSecurityGuardrail {
    RuntimeSecurityGuardrail {
        id: id.to_string(),
        title: title.to_string(),
        native_runtime_capability: true,
        default_policy: default_policy.to_string(),
        enforcement_points: enforcement_points
            .iter()
            .map(|point| point.to_string())
            .collect(),
        decision: decision.to_string(),
        rationale: rationale.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::to_string;

    #[test]
    fn secret_guardrail_sanitizes_common_secrets_before_prompt_context() {
        let input = "\
OPENAI_API_KEY=sk-proj-abcdefghijklmnopqrstuvwxyzABCDEF1234567890
GITHUB_TOKEN=ghp_abcdefghijklmnopqrstuvwxyz1234567890
DATABASE_URL=postgres://foundry:super-secret-password@db.internal:5432/app
";

        let report = sanitize_prompt_secrets(input, SecretSanitizationOptions::default());

        assert_eq!(report.schema_version, "foundry.runtime.secret_guardrail.v1");
        assert_eq!(report.status, "sanitized");
        assert_eq!(report.detections.len(), 3);
        assert!(report
            .sanitized_text
            .contains("{{vault:project.openai.default}}"));
        assert!(report
            .sanitized_text
            .contains("{{vault:project.github.default}}"));
        assert!(report
            .sanitized_text
            .contains("{{vault:project.postgres.default}}"));
        assert!(!report
            .sanitized_text
            .contains("sk-proj-abcdefghijklmnopqrstuvwxyz"));
        assert!(!report
            .sanitized_text
            .contains("ghp_abcdefghijklmnopqrstuvwxyz"));
        assert!(!report.sanitized_text.contains("super-secret-password"));
        assert!(!report.external_ai_allowed);
        assert!(!report.local_ai_fallback_attempted);
    }

    #[test]
    fn secret_guardrail_flags_entropy_without_leaking_value_to_audit() {
        let secret = "N7vQ9xL4pR2sT8wY6zA3bC5dE1fG0hJ";
        let input = format!("session_blob={secret}");

        let report = sanitize_prompt_secrets(&input, SecretSanitizationOptions::default());
        let serialized = to_string(&report).unwrap();

        assert_eq!(report.detections.len(), 1);
        assert_eq!(report.detections[0].source, "entropy");
        assert_eq!(report.detections[0].classification, "sensitive");
        assert!(report
            .sanitized_text
            .contains("{{vault:project.entropy.default}}"));
        assert!(!serialized.contains(secret));
        assert!(serialized.contains("value_sha256"));
        assert!(serialized.contains("secret_value_redacted"));
    }

    #[test]
    fn secret_guardrail_does_not_classify_plain_hashes_as_entropy_secrets() {
        let hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let input = format!("artifact_sha256={hash}");

        let report = sanitize_prompt_secrets(&input, SecretSanitizationOptions::default());

        assert_eq!(report.status, "clean");
        assert_eq!(report.detection_count, 0);
        assert_eq!(report.sanitized_text, input);
    }

    #[test]
    fn secret_guardrail_does_not_classify_task_ids_as_openai_keys() {
        let input = "manifest-task-007-artifacts attached-report-task-006-bootstrap.md";

        let report = sanitize_prompt_secrets(input, SecretSanitizationOptions::default());

        assert_eq!(report.status, "clean");
        assert_eq!(report.detection_count, 0);
        assert_eq!(report.sanitized_text, input);
    }

    #[test]
    fn secret_guardrail_does_not_classify_foundry_schema_versions_as_entropy_secrets() {
        let input = "foundry.capability_discovery_plan.v1 foundry.runtime.secret_guardrail.v1";

        let report = sanitize_prompt_secrets(input, SecretSanitizationOptions::default());

        assert_eq!(report.status, "clean");
        assert_eq!(report.detection_count, 0);
        assert_eq!(report.sanitized_text, input);
    }

    #[test]
    fn secret_guardrail_does_not_classify_artifact_paths_as_entropy_secrets() {
        let input = "artifacts/wf_demo/attached-report-task-006-bootstrap.md";

        let report = sanitize_prompt_secrets(input, SecretSanitizationOptions::default());

        assert_eq!(report.status, "clean");
        assert_eq!(report.detection_count, 0);
        assert_eq!(report.sanitized_text, input);
    }

    #[test]
    fn secret_guardrail_detects_common_provider_tokens() {
        let input = "\
ANTHROPIC_API_KEY=sk-ant-api03-abcdefghijklmnopqrstuvwxyz1234567890
STRIPE_SECRET_KEY=sk_live_\
abcdefghijklmnopqrstuvwxyz1234567890
SLACK_BOT_TOKEN=xoxb-\
123456789012-123456789012-abcdefghijklmnopqrstuvwxyz
GOOGLE_API_KEY=AIzaabcdefghijklmnopqrstuvwxyz1234567890
AUTHORIZATION=Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.signature123
";

        let report = sanitize_prompt_secrets(input, SecretSanitizationOptions::default());

        assert_eq!(report.status, "sanitized");
        assert!(report
            .detections
            .iter()
            .any(|detection| detection.provider == "anthropic"));
        assert!(report
            .detections
            .iter()
            .any(|detection| detection.provider == "stripe"));
        assert!(report
            .detections
            .iter()
            .any(|detection| detection.provider == "slack"));
        assert!(report
            .detections
            .iter()
            .any(|detection| detection.provider == "google"));
        assert!(report
            .detections
            .iter()
            .any(|detection| detection.kind == "bearer_token"));
        assert!(!report.sanitized_text.contains("sk-ant-api03"));
        assert!(!report.sanitized_text.contains(
            "sk_live_\
"
        ));
        assert!(!report.sanitized_text.contains(
            "xoxb-\
"
        ));
        assert!(!report.sanitized_text.contains("AIza"));
        assert!(!report.sanitized_text.contains("Bearer eyJ"));
    }

    #[test]
    fn secret_guardrail_uses_local_only_fallback_for_plain_language_password() {
        let input = "The production database password BananaAzul2026";

        let report = sanitize_prompt_secrets(input, SecretSanitizationOptions::default());

        assert_eq!(report.status, "sanitized");
        assert!(report.local_ai_fallback_attempted);
        assert_eq!(report.detection_count, 1);
        assert_eq!(report.detections[0].source, "local_ai_fallback");
        assert_eq!(report.detections[0].kind, "password");
        assert!(!report.external_ai_allowed);
        assert_eq!(
            report.sanitized_text,
            "The production database password {{vault:project.generic.default}}"
        );
    }

    #[test]
    fn secret_guardrail_does_not_treat_distant_schema_versions_as_token_values() {
        let input = format!(
            r#"{{"token_source":".foundry/design/tokens.json","description":"{}","schema_version":"foundry.operating_context.v1"}}"#,
            "ordinary context ".repeat(20)
        );

        let report = sanitize_prompt_secrets(&input, SecretSanitizationOptions::default());

        assert_eq!(report.status, "clean");
        assert_eq!(report.detection_count, 0);
        assert!(report.local_ai_fallback_attempted);
        assert_eq!(report.sanitized_text, input);
    }

    #[test]
    fn secret_guardrail_checks_later_marker_occurrences_within_the_safe_window() {
        let secret = "Abc123456789";
        let input = format!(
            "token source is documented; {} token {secret}",
            "ordinary context ".repeat(12)
        );

        let report = sanitize_prompt_secrets(&input, SecretSanitizationOptions::default());

        assert_eq!(report.status, "sanitized");
        assert_eq!(report.detection_count, 1);
        assert_eq!(report.detections[0].source, "local_ai_fallback");
        assert_eq!(report.detections[0].kind, "token");
        assert!(!report.sanitized_text.contains(secret));
    }

    #[test]
    fn secret_guardrail_detects_env_style_secret_variables() {
        let input = "PROD_DB_PASSWORD=BananaAzul2026\nWEBHOOK_SECRET=hook_123456789abcdef";

        let report = sanitize_prompt_secrets(input, SecretSanitizationOptions::default());

        assert_eq!(report.status, "sanitized");
        assert_eq!(report.detection_count, 2);
        assert!(report
            .detections
            .iter()
            .any(|detection| detection.kind == "password"));
        assert!(report
            .detections
            .iter()
            .any(|detection| detection.kind == "webhook_secret"));
        assert_eq!(
            report.sanitized_text,
            "PROD_DB_PASSWORD={{vault:project.generic.default}}\nWEBHOOK_SECRET={{vault:project.generic.default-2}}"
        );
    }

    #[test]
    fn secret_guardrail_never_allows_external_ai_for_secret_detection() {
        let input = "The production database password BananaAzul2026";

        let report = sanitize_prompt_secrets(
            input,
            SecretSanitizationOptions {
                allow_external_ai: true,
                ..SecretSanitizationOptions::default()
            },
        );

        assert!(!report.external_ai_allowed);
        assert!(report
            .notes
            .iter()
            .any(|note| note.contains("External AI fallback is disabled")));
    }
}
