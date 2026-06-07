use crate::storage::ForgeStore;
use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_LIMIT: usize = 10;
const MAX_CHUNK_WORDS: usize = 400;
const CHUNK_OVERLAP_WORDS: usize = 48;

#[derive(Debug, Clone)]
pub struct MemorySearchOptions {
    pub query: String,
    pub scopes: Vec<String>,
    pub audience: String,
    pub visibility: Option<String>,
    pub run_id: Option<String>,
    pub limit: usize,
    pub global_root: Option<PathBuf>,
    pub project_root: Option<PathBuf>,
    pub processing_root: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryPolicyReport {
    pub schema_version: String,
    pub status: String,
    pub file_first: bool,
    pub hidden_state_disallowed: bool,
    pub search_policy: MemorySearchPolicy,
    pub scopes: Vec<MemoryScopePolicy>,
    pub visibility_levels: Vec<MemoryVisibilityPolicy>,
    pub shareability_levels: Vec<MemoryShareabilityPolicy>,
    pub interface_policy: Vec<MemoryInterfacePolicy>,
    pub business_operating_model: BusinessOperatingModel,
    pub source_influences: Vec<MemorySourceInfluence>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemorySearchPolicy {
    pub schema_version: String,
    pub retrieval_tool: String,
    pub precise_read_tool: String,
    pub indexing: String,
    pub chunk_target_tokens: usize,
    pub returns_full_file: bool,
    pub provider: String,
    pub future_embedding_boundary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryScopePolicy {
    pub scope: String,
    pub default_path: String,
    pub lifecycle: String,
    pub default_shareability: String,
    pub default_visibility: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryVisibilityPolicy {
    pub visibility: String,
    pub readable_by: String,
    pub write_policy: String,
    pub approval_required_for: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryShareabilityPolicy {
    pub shareability: String,
    pub meaning: String,
    pub allowed_scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryInterfacePolicy {
    pub scenario: String,
    pub default_scope: String,
    pub default_visibility: String,
    pub default_shareability: String,
    pub retention: String,
    pub governance: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BusinessOperatingModel {
    pub schema_version: String,
    pub default_departments: Vec<String>,
    pub required_decisions: Vec<String>,
    pub request_rule: String,
    pub sensitive_action_rule: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemorySourceInfluence {
    pub source: String,
    pub adopted_pattern: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemorySearchReport {
    pub schema_version: String,
    pub status: String,
    pub query: String,
    pub audience: String,
    pub requested_scopes: Vec<String>,
    pub searched_roots: Vec<MemorySearchRoot>,
    pub result_count: usize,
    pub results: Vec<MemorySearchResult>,
    pub governance: MemorySearchGovernance,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemorySearchRoot {
    pub scope: String,
    pub root: String,
    pub exists: bool,
    pub lifecycle: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemorySearchResult {
    pub scope: String,
    pub visibility: String,
    pub shareability: String,
    pub lifecycle: String,
    pub retention: String,
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub score: f64,
    pub provider: String,
    pub model: String,
    pub access_decision: String,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemorySearchGovernance {
    pub public_audience_rule: String,
    pub internal_audience_rule: String,
    pub private_audience_rule: String,
    pub denied_result_count: usize,
    pub temporary_memory_rule: String,
}

#[derive(Debug, Clone)]
struct MemoryRoot {
    scope: String,
    root: PathBuf,
    lifecycle: String,
}

#[derive(Debug, Clone, Default)]
struct MemoryMetadata {
    visibility: Option<String>,
    shareability: Option<String>,
    lifecycle: Option<String>,
    retention: Option<String>,
}

#[derive(Debug, Clone)]
struct MemoryChunk {
    metadata: MemoryMetadata,
    scope: String,
    lifecycle: String,
    path: PathBuf,
    start_line: usize,
    end_line: usize,
    text: String,
}

pub fn memory_policy_report(store: &ForgeStore) -> MemoryPolicyReport {
    let project_memory = store.base_dir().join("memory");
    MemoryPolicyReport {
        schema_version: "forge.memory_policy.v1".to_string(),
        status: "memory_policy_ready".to_string(),
        file_first: true,
        hidden_state_disallowed: true,
        search_policy: MemorySearchPolicy {
            schema_version: "forge.memory_search_policy.v1".to_string(),
            retrieval_tool: "forge memory search".to_string(),
            precise_read_tool: "read the returned path and line range only".to_string(),
            indexing: "markdown chunking plus deterministic lexical semantic scoring; embeddings can replace the scorer behind the same result contract".to_string(),
            chunk_target_tokens: MAX_CHUNK_WORDS,
            returns_full_file: false,
            provider: "forge_builtin_file_memory".to_string(),
            future_embedding_boundary: "provider/model are explicit on every result so vector backends can be introduced without changing governance semantics".to_string(),
        },
        scopes: vec![
            MemoryScopePolicy {
                scope: "global".to_string(),
                default_path: "~/.forge/memory".to_string(),
                lifecycle: "long_lived_cross_project".to_string(),
                default_shareability: "global_shared_after_classification".to_string(),
                default_visibility: "internal".to_string(),
                notes: "Curated operating knowledge, company decisions and stable preferences. Public writes require approval.".to_string(),
            },
            MemoryScopePolicy {
                scope: "project".to_string(),
                default_path: project_memory.display().to_string(),
                lifecycle: "project_lived".to_string(),
                default_shareability: "project_shared".to_string(),
                default_visibility: "internal".to_string(),
                notes: "Project-local facts, decisions, architecture notes and delivery history under the project's .forge directory.".to_string(),
            },
            MemoryScopePolicy {
                scope: "processing".to_string(),
                default_path: store.base_dir().join("runs/<run-id>/memory").display().to_string(),
                lifecycle: "run_lived_ephemeral".to_string(),
                default_shareability: "non_shareable".to_string(),
                default_visibility: "private".to_string(),
                notes: "Temporary run memory, lead conversations, scratch observations and intermediate context. It can be deleted after final packaging unless explicitly promoted.".to_string(),
            },
        ],
        visibility_levels: vec![
            MemoryVisibilityPolicy {
                visibility: "public".to_string(),
                readable_by: "external interfaces and public-facing agents".to_string(),
                write_policy: "only curated, non-sensitive facts".to_string(),
                approval_required_for: vec![
                    "public_memory_write".to_string(),
                    "public_post".to_string(),
                    "external_broadcast".to_string(),
                ],
            },
            MemoryVisibilityPolicy {
                visibility: "internal".to_string(),
                readable_by: "operators, managers and internal agents".to_string(),
                write_policy: "manager directives, product decisions and operational facts may default here".to_string(),
                approval_required_for: vec!["promotion_to_public".to_string()],
            },
            MemoryVisibilityPolicy {
                visibility: "private".to_string(),
                readable_by: "the bound customer, thread, run or authorized operator".to_string(),
                write_policy: "customer/lead statements, credentials, negotiations and personal data default here".to_string(),
                approval_required_for: vec![
                    "promotion_to_internal".to_string(),
                    "promotion_to_public".to_string(),
                    "cross_customer_share".to_string(),
                ],
            },
        ],
        shareability_levels: vec![
            MemoryShareabilityPolicy {
                shareability: "global_shared".to_string(),
                meaning: "safe to reuse across projects and interfaces after classification".to_string(),
                allowed_scopes: vec!["global".to_string()],
            },
            MemoryShareabilityPolicy {
                shareability: "project_shared".to_string(),
                meaning: "safe to reuse inside the current project or tenant".to_string(),
                allowed_scopes: vec!["project".to_string(), "global".to_string()],
            },
            MemoryShareabilityPolicy {
                shareability: "thread_private".to_string(),
                meaning: "only the originating customer, lead, thread or workflow run should see it".to_string(),
                allowed_scopes: vec!["processing".to_string(), "project".to_string()],
            },
            MemoryShareabilityPolicy {
                shareability: "manager_shared".to_string(),
                meaning: "a curated customer suggestion or operational note can be shared with a manager/product owner without becoming public or globally reusable".to_string(),
                allowed_scopes: vec!["processing".to_string(), "project".to_string()],
            },
            MemoryShareabilityPolicy {
                shareability: "non_shareable".to_string(),
                meaning: "scratch, sensitive, credential-like or transient content; never used outside its run unless promoted".to_string(),
                allowed_scopes: vec!["processing".to_string()],
            },
        ],
        interface_policy: vec![
            MemoryInterfacePolicy {
                scenario: "SDR customer or lead conversation".to_string(),
                default_scope: "processing".to_string(),
                default_visibility: "private".to_string(),
                default_shareability: "thread_private".to_string(),
                retention: "temporary until qualification is complete, then promote only curated non-sensitive summary".to_string(),
                governance: "do not write to public/shared memory without approval; CRM writes are sensitive external actions".to_string(),
            },
            MemoryInterfacePolicy {
                scenario: "customer suggestion that may help the manager or product team"
                    .to_string(),
                default_scope: "processing, then project after classification".to_string(),
                default_visibility: "private, promotable to internal".to_string(),
                default_shareability: "thread_private, promotable to manager_shared or project_shared".to_string(),
                retention: "keep the raw customer wording private; promote a curated suggestion summary when useful".to_string(),
                governance: "sharing with a manager is allowed after classifying/removing sensitive customer data; public/global reuse still requires explicit approval".to_string(),
            },
            MemoryInterfacePolicy {
                scenario: "manager/operator directive".to_string(),
                default_scope: "project or global".to_string(),
                default_visibility: "internal".to_string(),
                default_shareability: "project_shared, optionally global_shared after review".to_string(),
                retention: "persistent while relevant".to_string(),
                governance: "can influence future workflows, but public publication still needs explicit approval".to_string(),
            },
            MemoryInterfacePolicy {
                scenario: "public channel interaction".to_string(),
                default_scope: "project".to_string(),
                default_visibility: "public".to_string(),
                default_shareability: "project_shared".to_string(),
                retention: "persistent only for curated public facts".to_string(),
                governance: "private/internal memory is blocked from public context assembly".to_string(),
            },
        ],
        business_operating_model: BusinessOperatingModel {
            schema_version: "forge.company_request_model.v1".to_string(),
            default_departments: vec![
                "product".to_string(),
                "technical".to_string(),
                "financial".to_string(),
                "administrative".to_string(),
                "marketing".to_string(),
                "communication".to_string(),
                "delivery".to_string(),
            ],
            required_decisions: vec![
                "what_will_be_done".to_string(),
                "how_it_will_be_done".to_string(),
                "delivery_acceptance_and_evidence".to_string(),
                "how_the_delivery_will_be_communicated".to_string(),
                "cost_time_risk_owner".to_string(),
            ],
            request_rule: "Every customer request gets a product/business response before or alongside technical execution; small tasks may use a compact decision, large systems use full departmental review.".to_string(),
            sensitive_action_rule: "Public communication, shared memory writes, external broadcasts, financial commitments and customer-impacting actions require explicit governance.".to_string(),
        },
        source_influences: vec![
            MemorySourceInfluence {
                source: "Hermes/OpenClaw file memory".to_string(),
                adopted_pattern: "Markdown memory is the source of truth; search returns snippets and line ranges, not hidden state or full files.".to_string(),
            },
            MemorySourceInfluence {
                source: "OpenClaw async sessions".to_string(),
                adopted_pattern: "Interfaces, sessions and subagents have separate state and visibility; background work returns lineage instead of blocking one UI.".to_string(),
            },
            MemorySourceInfluence {
                source: "Paperclip company operating model".to_string(),
                adopted_pattern: "Requests are handled as company operations with product, technical, financial, administrative, marketing, communication and delivery concerns.".to_string(),
            },
        ],
    }
}

pub fn search_memory(
    store: &ForgeStore,
    options: MemorySearchOptions,
) -> Result<MemorySearchReport> {
    let scopes = normalize_scopes(&options.scopes);
    let roots = resolve_roots(store, &options, &scopes);
    let query_terms = tokenize(&options.query);
    let mut denied_result_count = 0usize;
    let mut results = Vec::new();

    for root in &roots {
        if !root.root.exists() {
            continue;
        }
        for file in markdown_files(&root.root)? {
            let chunks = chunks_for_file(root, &file)?;
            for chunk in chunks {
                let visibility = normalize_visibility(
                    chunk
                        .metadata
                        .visibility
                        .as_deref()
                        .unwrap_or_else(|| default_visibility(&chunk.scope)),
                );
                if let Some(filter) = options.visibility.as_deref() {
                    if normalize_visibility(filter) != visibility {
                        continue;
                    }
                }
                let shareability = normalize_shareability(
                    chunk
                        .metadata
                        .shareability
                        .as_deref()
                        .unwrap_or_else(|| default_shareability(&chunk.scope)),
                );
                let lifecycle = chunk
                    .metadata
                    .lifecycle
                    .clone()
                    .unwrap_or_else(|| chunk.lifecycle.clone());
                let retention = chunk.metadata.retention.clone().unwrap_or_else(|| {
                    if chunk.scope == "processing" {
                        "temporary".to_string()
                    } else {
                        "persistent".to_string()
                    }
                });
                let allowed = audience_can_read(&options.audience, &visibility, &shareability);
                if !allowed {
                    denied_result_count += 1;
                    continue;
                }
                let score = score_chunk(&options.query, &query_terms, &chunk.text);
                if score <= 0.0 {
                    continue;
                }
                results.push(MemorySearchResult {
                    scope: chunk.scope,
                    visibility,
                    shareability,
                    lifecycle,
                    retention,
                    path: chunk.path.display().to_string(),
                    start_line: chunk.start_line,
                    end_line: chunk.end_line,
                    score,
                    provider: "forge_builtin_file_memory".to_string(),
                    model: "hybrid_lexical_semantic_v1".to_string(),
                    access_decision: "allowed_by_audience_visibility_policy".to_string(),
                    snippet: compact_snippet(&chunk.text, 420),
                });
            }
        }
    }

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.start_line.cmp(&b.start_line))
    });
    let limit = if options.limit == 0 {
        DEFAULT_LIMIT
    } else {
        options.limit
    };
    results.truncate(limit);

    Ok(MemorySearchReport {
        schema_version: "forge.memory_search.v1".to_string(),
        status: "memory_search_complete".to_string(),
        query: options.query,
        audience: options.audience,
        requested_scopes: scopes,
        searched_roots: roots
            .iter()
            .map(|root| MemorySearchRoot {
                scope: root.scope.clone(),
                root: root.root.display().to_string(),
                exists: root.root.exists(),
                lifecycle: root.lifecycle.clone(),
            })
            .collect(),
        result_count: results.len(),
        results,
        governance: MemorySearchGovernance {
            public_audience_rule:
                "public audiences can only receive public memories marked global_shared or project_shared"
                    .to_string(),
            internal_audience_rule:
                "internal audiences can receive public/internal memory, but private customer/run memory stays isolated"
                    .to_string(),
            private_audience_rule:
                "private/operator audiences can inspect all local memory for debugging and governance"
                    .to_string(),
            denied_result_count,
            temporary_memory_rule:
                "processing memory is temporary by default and should be deleted or promoted explicitly during final packaging"
                    .to_string(),
        },
    })
}

fn normalize_scopes(scopes: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    if scopes.is_empty() {
        return vec![
            "global".to_string(),
            "project".to_string(),
            "processing".to_string(),
        ];
    }
    for scope in scopes {
        let scope = scope.trim().to_ascii_lowercase();
        if matches!(scope.as_str(), "global" | "project" | "processing")
            && !normalized.contains(&scope)
        {
            normalized.push(scope);
        }
    }
    if normalized.is_empty() {
        vec!["project".to_string()]
    } else {
        normalized
    }
}

fn resolve_roots(
    store: &ForgeStore,
    options: &MemorySearchOptions,
    scopes: &[String],
) -> Vec<MemoryRoot> {
    let mut roots = Vec::new();
    for scope in scopes {
        match scope.as_str() {
            "global" => roots.push(MemoryRoot {
                scope: scope.clone(),
                root: options
                    .global_root
                    .clone()
                    .unwrap_or_else(default_global_memory_root),
                lifecycle: "long_lived_cross_project".to_string(),
            }),
            "project" => roots.push(MemoryRoot {
                scope: scope.clone(),
                root: options
                    .project_root
                    .clone()
                    .unwrap_or_else(|| store.base_dir().join("memory")),
                lifecycle: "project_lived".to_string(),
            }),
            "processing" => roots.push(MemoryRoot {
                scope: scope.clone(),
                root: options.processing_root.clone().unwrap_or_else(|| {
                    let run = options
                        .run_id
                        .clone()
                        .unwrap_or_else(|| "current".to_string());
                    store.base_dir().join("runs").join(run).join("memory")
                }),
                lifecycle: "run_lived_ephemeral".to_string(),
            }),
            _ => {}
        }
    }
    roots
}

fn default_global_memory_root() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".forge")
        .join("memory")
}

fn markdown_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_markdown_files(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_markdown_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(path).with_context(|| format!("failed to read {}", path.display()))? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_markdown_files(&path, files)?;
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.eq_ignore_ascii_case("md"))
            .unwrap_or(false)
        {
            files.push(path);
        }
    }
    Ok(())
}

fn chunks_for_file(root: &MemoryRoot, path: &Path) -> Result<Vec<MemoryChunk>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read memory file {}", path.display()))?;
    let metadata = parse_metadata(&content);
    let mut words = Vec::new();
    for (line_index, line) in content.lines().enumerate() {
        if line.trim() == "---" && line_index == 0 {
            continue;
        }
        for word in line.split_whitespace() {
            words.push((word.to_string(), line_index + 1));
        }
    }
    if words.is_empty() {
        return Ok(Vec::new());
    }

    let mut chunks = Vec::new();
    let mut start = 0usize;
    while start < words.len() {
        let end = (start + MAX_CHUNK_WORDS).min(words.len());
        let text = words[start..end]
            .iter()
            .map(|(word, _)| word.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        chunks.push(MemoryChunk {
            metadata: metadata.clone(),
            scope: root.scope.clone(),
            lifecycle: root.lifecycle.clone(),
            path: path.to_path_buf(),
            start_line: words[start].1,
            end_line: words[end - 1].1,
            text,
        });
        if end == words.len() {
            break;
        }
        start = end.saturating_sub(CHUNK_OVERLAP_WORDS);
    }
    Ok(chunks)
}

fn parse_metadata(content: &str) -> MemoryMetadata {
    let mut metadata = MemoryMetadata::default();
    let mut lines = content.lines();
    if lines.next().map(str::trim) != Some("---") {
        return metadata;
    }
    for line in lines.take(80) {
        let line = line.trim();
        if line == "---" {
            break;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();
        match key {
            "visibility" | "access" => metadata.visibility = Some(value),
            "shareability" | "sharing" => metadata.shareability = Some(value),
            "lifecycle" => metadata.lifecycle = Some(value),
            "retention" => metadata.retention = Some(value),
            _ => {}
        }
    }
    metadata
}

fn default_visibility(scope: &str) -> &'static str {
    match scope {
        "processing" => "private",
        _ => "internal",
    }
}

fn default_shareability(scope: &str) -> &'static str {
    match scope {
        "global" => "global_shared",
        "project" => "project_shared",
        _ => "non_shareable",
    }
}

fn normalize_visibility(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "public" | "público" | "publica" | "pública" => "public".to_string(),
        "private" | "privado" | "privada" | "confidential" => "private".to_string(),
        _ => "internal".to_string(),
    }
}

fn normalize_shareability(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "global" | "global_shared" | "share_global" => "global_shared".to_string(),
        "project" | "project_shared" | "shared" => "project_shared".to_string(),
        "manager" | "manager_shared" | "gestor" | "gestor_shared" => "manager_shared".to_string(),
        "thread" | "thread_private" | "customer_private" | "lead_private" => {
            "thread_private".to_string()
        }
        _ => "non_shareable".to_string(),
    }
}

fn audience_can_read(audience: &str, visibility: &str, shareability: &str) -> bool {
    match audience.trim().to_ascii_lowercase().as_str() {
        "public" | "external" | "customer" | "cliente" => {
            visibility == "public" && matches!(shareability, "global_shared" | "project_shared")
        }
        "internal" | "manager" | "gestor" | "operator" => {
            visibility != "private" || shareability == "manager_shared"
        }
        _ => true,
    }
}

fn score_chunk(query: &str, query_terms: &[String], text: &str) -> f64 {
    if query_terms.is_empty() {
        return 0.0;
    }
    let text_lower = text.to_ascii_lowercase();
    let text_terms = tokenize(text);
    let text_set = text_terms.iter().cloned().collect::<BTreeSet<_>>();
    let unique_query = query_terms.iter().cloned().collect::<BTreeSet<_>>();
    let mut matched = 0usize;
    for term in &unique_query {
        if text_set.contains(term) {
            matched += 1;
        }
    }
    let coverage = matched as f64 / unique_query.len().max(1) as f64;
    let frequency = query_terms
        .iter()
        .filter(|term| text_terms.iter().any(|candidate| candidate == *term))
        .count() as f64
        / query_terms.len().max(1) as f64;
    let phrase_bonus =
        if !query.trim().is_empty() && text_lower.contains(&query.trim().to_ascii_lowercase()) {
            0.35
        } else {
            0.0
        };
    let score = coverage + (frequency * 0.4) + phrase_bonus;
    (score * 1000.0).round() / 1000.0
}

fn tokenize(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in value.chars() {
        if ch.is_alphanumeric() {
            current.extend(ch.to_lowercase());
        } else if !current.is_empty() {
            push_token(&mut tokens, &mut current);
        }
    }
    if !current.is_empty() {
        push_token(&mut tokens, &mut current);
    }
    tokens
}

fn push_token(tokens: &mut Vec<String>, current: &mut String) {
    if current.len() >= 2 && !is_stopword(current) {
        tokens.push(current.clone());
    }
    current.clear();
}

fn is_stopword(value: &str) -> bool {
    matches!(
        value,
        "a" | "o"
            | "e"
            | "de"
            | "da"
            | "do"
            | "das"
            | "dos"
            | "um"
            | "uma"
            | "the"
            | "and"
            | "or"
            | "to"
            | "of"
            | "for"
            | "com"
            | "para"
            | "por"
            | "que"
            | "em"
            | "no"
            | "na"
    )
}

fn compact_snippet(value: &str, max_chars: usize) -> String {
    let cleaned = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.chars().count() <= max_chars {
        return cleaned;
    }
    let mut output = String::new();
    for ch in cleaned.chars().take(max_chars.saturating_sub(1)) {
        output.push(ch);
    }
    output.push('…');
    output
}
