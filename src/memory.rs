use crate::artifact::hex_sha256;
use crate::identity::ensure_workflow_policy;
use crate::storage::{
    ForgeStore, MemoryPromotionQuery, MemoryPromotionWrite, StoredMemoryPromotionRecord,
};
use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const DEFAULT_LIMIT: usize = 10;
const MAX_CHUNK_WORDS: usize = 400;
const CHUNK_OVERLAP_WORDS: usize = 48;
const MEMORY_GOVERNANCE_SCHEMA_VERSION: &str = "forge.memory_governance_config.v1";

#[derive(Debug, Clone)]
pub struct MemorySearchOptions {
    pub query: String,
    pub workflow_id: Option<String>,
    pub scopes: Vec<String>,
    pub audience: Option<String>,
    pub visibility: Option<String>,
    pub memory_level: Option<String>,
    pub run_id: Option<String>,
    pub organization_id: Option<String>,
    pub limit: usize,
    pub global_root: Option<PathBuf>,
    pub organization_root: Option<PathBuf>,
    pub project_root: Option<PathBuf>,
    pub processing_root: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct MemoryPromotionOptions {
    pub workflow_id: Option<String>,
    pub from_scope: String,
    pub to_scope: String,
    pub source_path: PathBuf,
    pub source_start_line: Option<usize>,
    pub source_end_line: Option<usize>,
    pub summary: String,
    pub approved_by: String,
    pub reason: String,
    pub visibility: String,
    pub shareability: Option<String>,
    pub organization_id: Option<String>,
    pub global_root: Option<PathBuf>,
    pub organization_root: Option<PathBuf>,
    pub project_root: Option<PathBuf>,
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub struct MemoryRetentionOptions {
    pub workflow_id: Option<String>,
    pub scopes: Vec<String>,
    pub run_id: Option<String>,
    pub organization_id: Option<String>,
    pub global_root: Option<PathBuf>,
    pub organization_root: Option<PathBuf>,
    pub project_root: Option<PathBuf>,
    pub processing_root: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct MemoryCleanupOptions {
    pub workflow_id: Option<String>,
    pub scopes: Vec<String>,
    pub run_id: Option<String>,
    pub organization_id: Option<String>,
    pub global_root: Option<PathBuf>,
    pub organization_root: Option<PathBuf>,
    pub project_root: Option<PathBuf>,
    pub processing_root: Option<PathBuf>,
    pub mode: String,
    pub archive_root: Option<PathBuf>,
    pub approved_by: Option<String>,
    pub reason: Option<String>,
    pub dry_run: bool,
    pub confirm: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryPromotionReport {
    pub schema_version: String,
    pub status: String,
    pub promotion_id: String,
    pub from_scope: String,
    pub to_scope: String,
    pub source_path: String,
    pub source_start_line: Option<usize>,
    pub source_end_line: Option<usize>,
    pub target_path: String,
    pub target_written: bool,
    pub visibility: String,
    pub shareability: String,
    pub approved_by: String,
    pub approved_at: String,
    pub reason: String,
    pub summary_sha256: String,
    pub promoted_memory_sha256: String,
    pub governance: MemoryPromotionGovernance,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryPromotionIndexReport {
    pub schema_version: String,
    pub status: String,
    pub filters: MemoryPromotionIndexFilters,
    pub promotion_count: usize,
    pub promotions: Vec<MemoryPromotionIndexEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryPromotionIndexFilters {
    pub workflow_id: Option<String>,
    pub organization_id: Option<String>,
    pub brand_id: Option<String>,
    pub product_id: Option<String>,
    pub from_scope: Option<String>,
    pub to_scope: Option<String>,
    pub approved_by: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryPromotionIndexEntry {
    pub promotion_id: String,
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
    pub created_at: String,
    pub report: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryRetentionReport {
    pub schema_version: String,
    pub status: String,
    pub requested_scopes: Vec<String>,
    pub searched_roots: Vec<MemorySearchRoot>,
    pub item_count: usize,
    pub keep_count: usize,
    pub promote_or_delete_count: usize,
    pub delete_candidate_count: usize,
    pub items: Vec<MemoryRetentionItem>,
    pub governance: MemoryRetentionGovernance,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryCleanupReport {
    pub schema_version: String,
    pub status: String,
    pub mode: String,
    pub dry_run: bool,
    pub approved_by: Option<String>,
    pub reason: Option<String>,
    pub requested_scopes: Vec<String>,
    pub item_count: usize,
    pub archived_count: usize,
    pub deleted_count: usize,
    pub skipped_count: usize,
    pub items: Vec<MemoryCleanupItem>,
    pub governance: MemoryCleanupGovernance,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryCleanupItem {
    pub scope: String,
    pub path: String,
    pub retention_action: String,
    pub cleanup_action: String,
    pub target_path: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryCleanupGovernance {
    pub approval_required: bool,
    pub approval_recorded: bool,
    pub confirm_required: bool,
    pub confirm_recorded: bool,
    pub destructive_actions_performed: bool,
    pub retention_source_schema: String,
    pub workflow_binding: Option<MemoryWorkflowBinding>,
    pub rule: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryRetentionItem {
    pub scope: String,
    pub path: String,
    pub visibility: String,
    pub shareability: String,
    pub lifecycle: String,
    pub retention: String,
    pub action: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryRetentionGovernance {
    pub destructive_actions_performed: bool,
    pub deletion_requires_explicit_future_command: bool,
    pub workflow_binding: Option<MemoryWorkflowBinding>,
    pub processing_rule: String,
    pub promoted_rule: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryPromotionGovernance {
    pub raw_source_copied: bool,
    pub approval_required: bool,
    pub approval_recorded: bool,
    pub classification_required: bool,
    pub classification_recorded: bool,
    pub allowed_targets: Vec<String>,
    pub workflow_binding: Option<MemoryWorkflowBinding>,
    pub rule: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryPolicyReport {
    pub schema_version: String,
    pub status: String,
    pub file_first: bool,
    pub hidden_state_disallowed: bool,
    pub project_governance: ProjectMemoryGovernanceReport,
    pub effective_defaults: MemoryEffectiveDefaults,
    pub search_policy: MemorySearchPolicy,
    pub memory_levels: Vec<MemoryLevelPolicy>,
    pub scopes: Vec<MemoryScopePolicy>,
    pub visibility_levels: Vec<MemoryVisibilityPolicy>,
    pub shareability_levels: Vec<MemoryShareabilityPolicy>,
    pub interface_policy: Vec<MemoryInterfacePolicy>,
    pub business_operating_model: BusinessOperatingModel,
    pub source_influences: Vec<MemorySourceInfluence>,
}

#[derive(Debug, Clone)]
pub struct MemoryGovernanceConfigOptions {
    pub project_root: PathBuf,
    pub memory_level: String,
    pub default_scopes: Vec<String>,
    pub default_audience: String,
    pub privacy_mode: String,
    pub retention_mode: String,
    pub approved_by: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryGovernanceApproval {
    pub approved_by: String,
    pub approved_at: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryGovernanceConfigReport {
    pub schema_version: String,
    pub status: String,
    pub project_root: String,
    pub config_path: String,
    pub memory_level: String,
    pub default_scopes: Vec<String>,
    pub default_audience: String,
    pub privacy_mode: String,
    pub retention_mode: String,
    pub approval: MemoryGovernanceApproval,
    pub governance_rule: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryEffectiveDefaults {
    pub memory_level: String,
    pub default_scopes: Vec<String>,
    pub default_audience: String,
    pub privacy_mode: String,
    pub retention_mode: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectMemoryGovernanceReport {
    pub schema_version: String,
    pub status: String,
    pub project_root: String,
    pub config_path: String,
    pub memory_level: String,
    pub default_scopes: Vec<String>,
    pub default_audience: String,
    pub privacy_mode: String,
    pub retention_mode: String,
    pub approval: Option<MemoryGovernanceApproval>,
    pub issues: Vec<String>,
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
pub struct MemoryLevelPolicy {
    pub level: String,
    pub allowed_scopes: Vec<String>,
    pub default_audience: String,
    pub can_read_private: bool,
    pub lifecycle: String,
    pub notes: String,
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
    pub memory_level: String,
    pub requested_scopes: Vec<String>,
    pub effective_scopes: Vec<String>,
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
    pub memory_level_rule: String,
    pub project_governance_status: String,
    pub project_governance_config_path: String,
    pub memory_level_source: String,
    pub requested_scopes_source: String,
    pub audience_source: String,
    pub denied_result_count: usize,
    pub workflow_binding: Option<MemoryWorkflowBinding>,
    pub temporary_memory_rule: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryWorkflowBinding {
    pub workflow_id: String,
    pub organization_id: String,
    pub brand_id: String,
    pub product_id: String,
    pub user_id: String,
    pub channel_id: String,
    pub tenant_policy_mode: String,
    pub memory_scope: String,
    pub allowed_scopes: Vec<String>,
    pub enforced: bool,
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

pub fn configure_memory_governance(
    options: MemoryGovernanceConfigOptions,
) -> Result<MemoryGovernanceConfigReport> {
    let project_root = options.project_root;
    let config_path = memory_governance_config_path(&project_root);
    let memory_level = normalize_memory_level(Some(&options.memory_level));
    let default_scopes =
        apply_memory_level(&normalize_scopes(&options.default_scopes), &memory_level);
    let default_scopes = if default_scopes.is_empty() {
        default_scopes_for_level(&memory_level)
    } else {
        default_scopes
    };
    let default_audience = normalize_default_audience(&options.default_audience)?;
    let privacy_mode = normalize_governance_mode(&options.privacy_mode, "private_by_default");
    let retention_mode = normalize_governance_mode(&options.retention_mode, "governed_retention");
    let approved_by = options.approved_by.trim();
    if approved_by.is_empty() {
        bail!("memory governance approved_by is required");
    }
    let reason = options.reason.trim();
    if reason.is_empty() {
        bail!("memory governance reason is required");
    }

    let report = MemoryGovernanceConfigReport {
        schema_version: MEMORY_GOVERNANCE_SCHEMA_VERSION.to_string(),
        status: "memory_governance_configured".to_string(),
        project_root: project_root.display().to_string(),
        config_path: config_path.display().to_string(),
        memory_level,
        default_scopes,
        default_audience,
        privacy_mode,
        retention_mode,
        approval: MemoryGovernanceApproval {
            approved_by: approved_by.to_string(),
            approved_at: Utc::now().to_rfc3339(),
            reason: reason.to_string(),
        },
        governance_rule: "project .forge/memory-governance.json controls the default memory level, scopes, audience, privacy and retention posture for Forge-owned project operations".to_string(),
    };

    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let bytes = serde_json::to_vec_pretty(&report)?;
    fs::write(&config_path, bytes)
        .with_context(|| format!("failed to write {}", config_path.display()))?;

    Ok(report)
}

pub fn memory_policy_report(store: &ForgeStore) -> MemoryPolicyReport {
    memory_policy_report_for_project(store, None)
}

pub fn project_memory_governance_report(
    project_root: Option<&Path>,
) -> ProjectMemoryGovernanceReport {
    load_project_memory_governance(project_root)
}

pub fn memory_policy_report_for_project(
    store: &ForgeStore,
    project_root: Option<&Path>,
) -> MemoryPolicyReport {
    let project_memory = store.base_dir().join("memory");
    let project_governance = load_project_memory_governance(project_root);
    let effective_defaults = MemoryEffectiveDefaults {
        memory_level: project_governance.memory_level.clone(),
        default_scopes: project_governance.default_scopes.clone(),
        default_audience: project_governance.default_audience.clone(),
        privacy_mode: project_governance.privacy_mode.clone(),
        retention_mode: project_governance.retention_mode.clone(),
    };
    MemoryPolicyReport {
        schema_version: "forge.memory_policy.v1".to_string(),
        status: "memory_policy_ready".to_string(),
        file_first: true,
        hidden_state_disallowed: true,
        project_governance,
        effective_defaults,
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
        memory_levels: memory_level_policies(),
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
                scope: "organization".to_string(),
                default_path: store
                    .base_dir()
                    .join("organizations/<organization-id>/memory")
                    .display()
                    .to_string(),
                lifecycle: "long_lived_tenant".to_string(),
                default_shareability: "organization_shared".to_string(),
                default_visibility: "internal".to_string(),
                notes: "Organization-scoped operating memory, tenant decisions, policies and reusable knowledge that should not leak across organizations.".to_string(),
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
                shareability: "organization_shared".to_string(),
                meaning: "safe to reuse inside the current organization/tenant but not across organizations".to_string(),
                allowed_scopes: vec!["organization".to_string(), "project".to_string()],
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

fn memory_governance_config_path(project_root: &Path) -> PathBuf {
    project_root.join(".forge").join("memory-governance.json")
}

fn load_project_memory_governance(project_root: Option<&Path>) -> ProjectMemoryGovernanceReport {
    let Some(project_root) = project_root else {
        let memory_level = "MEMORY_STANDARD".to_string();
        return ProjectMemoryGovernanceReport {
            schema_version: MEMORY_GOVERNANCE_SCHEMA_VERSION.to_string(),
            status: "not_requested".to_string(),
            project_root: String::new(),
            config_path: String::new(),
            default_scopes: default_scopes_for_level(&memory_level),
            default_audience: default_audience_for_memory_level(&memory_level),
            privacy_mode: "classified_visibility".to_string(),
            retention_mode: "governed_retention".to_string(),
            memory_level,
            approval: None,
            issues: Vec::new(),
        };
    };

    let config_path = memory_governance_config_path(project_root);
    let default_level = "MEMORY_STANDARD".to_string();
    if !config_path.exists() {
        return ProjectMemoryGovernanceReport {
            schema_version: MEMORY_GOVERNANCE_SCHEMA_VERSION.to_string(),
            status: "missing".to_string(),
            project_root: project_root.display().to_string(),
            config_path: config_path.display().to_string(),
            default_scopes: default_scopes_for_level(&default_level),
            default_audience: default_audience_for_memory_level(&default_level),
            privacy_mode: "classified_visibility".to_string(),
            retention_mode: "governed_retention".to_string(),
            memory_level: default_level,
            approval: None,
            issues: Vec::new(),
        };
    }

    match fs::read(&config_path)
        .with_context(|| format!("failed to read {}", config_path.display()))
        .and_then(|bytes| {
            serde_json::from_slice::<MemoryGovernanceConfigReport>(&bytes)
                .context("invalid memory governance JSON")
        }) {
        Ok(config) => ProjectMemoryGovernanceReport {
            schema_version: config.schema_version,
            status: "configured".to_string(),
            project_root: project_root.display().to_string(),
            config_path: config_path.display().to_string(),
            memory_level: config.memory_level,
            default_scopes: config.default_scopes,
            default_audience: config.default_audience,
            privacy_mode: config.privacy_mode,
            retention_mode: config.retention_mode,
            approval: Some(config.approval),
            issues: Vec::new(),
        },
        Err(error) => ProjectMemoryGovernanceReport {
            schema_version: MEMORY_GOVERNANCE_SCHEMA_VERSION.to_string(),
            status: "invalid".to_string(),
            project_root: project_root.display().to_string(),
            config_path: config_path.display().to_string(),
            default_scopes: default_scopes_for_level(&default_level),
            default_audience: default_audience_for_memory_level(&default_level),
            privacy_mode: "classified_visibility".to_string(),
            retention_mode: "governed_retention".to_string(),
            memory_level: default_level,
            approval: None,
            issues: vec![error.to_string()],
        },
    }
}

fn default_scopes_for_level(memory_level: &str) -> Vec<String> {
    let all_scopes = vec![
        "global".to_string(),
        "organization".to_string(),
        "project".to_string(),
        "processing".to_string(),
    ];
    apply_memory_level(&all_scopes, memory_level)
}

fn default_audience_for_memory_level(memory_level: &str) -> String {
    memory_level_policies()
        .into_iter()
        .find(|policy| policy.level == memory_level)
        .map(|policy| policy.default_audience)
        .unwrap_or_else(|| "internal".to_string())
}

fn normalize_default_audience(value: &str) -> Result<String> {
    let normalized = value.trim().to_ascii_lowercase().replace('-', "_");
    match normalized.as_str() {
        "public" | "external" | "customer" | "cliente" | "internal" | "manager" | "gestor"
        | "operator" | "private" => Ok(normalized),
        "" => bail!("memory governance default_audience is required"),
        other => bail!("unsupported memory governance default_audience: {other}"),
    }
}

fn normalize_governance_mode(value: &str, default_value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase().replace('-', "_");
    if normalized.is_empty() {
        default_value.to_string()
    } else {
        normalized
    }
}

fn bind_memory_to_workflow(
    store: &ForgeStore,
    workflow_id: Option<&str>,
    action: &str,
    organization_id: &mut Option<String>,
    requested_scopes: &[String],
) -> Result<Option<MemoryWorkflowBinding>> {
    let Some(workflow_id) = workflow_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    ensure_workflow_policy(store, workflow_id, action)?;
    let workflow = store.load_workflow(workflow_id)?;
    let context = &workflow.intent.operating_context;
    let workflow_organization_id = context.organization.id.trim().to_string();
    if let Some(requested_organization_id) = organization_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if requested_organization_id != workflow_organization_id {
            bail!(
                "memory organization {requested_organization_id} does not match workflow {workflow_id} organization {workflow_organization_id}"
            );
        }
    } else if !workflow_organization_id.is_empty() {
        *organization_id = Some(workflow_organization_id.clone());
    }

    let memory_level = memory_level_for_workflow_scope(&context.memory_scope);
    let allowed_scopes = allowed_memory_scopes_for_workflow(&context.memory_scope, &memory_level);
    let enforced = context.tenant_policy_mode == "enforce";
    if enforced {
        for scope in requested_scopes {
            if !allowed_scopes.contains(scope) {
                bail!(
                    "workflow {workflow_id} memory scope {scope} is outside allowed workflow memory scopes [{}] for memory_scope {}",
                    allowed_scopes.join(", "),
                    context.memory_scope
                );
            }
        }
    }

    Ok(Some(MemoryWorkflowBinding {
        workflow_id: workflow_id.to_string(),
        organization_id: workflow_organization_id,
        brand_id: context.brand.id.clone(),
        product_id: context.product.id.clone(),
        user_id: context.user.id.clone(),
        channel_id: context.channel.id.clone(),
        tenant_policy_mode: context.tenant_policy_mode.clone(),
        memory_scope: context.memory_scope.clone(),
        allowed_scopes,
        enforced,
    }))
}

fn memory_level_for_workflow_scope(memory_scope: &str) -> String {
    let normalized = memory_scope.trim().to_ascii_lowercase().replace('-', "_");
    if normalized.contains("none") || normalized.contains("disabled") {
        "none".to_string()
    } else if normalized.contains("admin") {
        "admin".to_string()
    } else if normalized.contains("full") {
        "full".to_string()
    } else if (normalized.contains("session")
        || normalized.contains("processing")
        || normalized.contains("run")
        || normalized.contains("thread"))
        && !normalized.contains("project")
        && !normalized.contains("organization")
        && !normalized.contains("tenant")
        && !normalized.contains("global")
    {
        "session".to_string()
    } else if normalized.contains("project")
        && !normalized.contains("organization")
        && !normalized.contains("tenant")
        && !normalized.contains("global")
    {
        "short_term".to_string()
    } else {
        "standard".to_string()
    }
}

fn allowed_memory_scopes_for_workflow(memory_scope: &str, memory_level: &str) -> Vec<String> {
    let normalized = memory_scope.trim().to_ascii_lowercase().replace('-', "_");
    let mut scopes = Vec::new();
    if normalized.contains("none") || normalized.contains("disabled") || memory_level == "none" {
        return scopes;
    }
    push_workflow_memory_scope(&mut scopes, "global", normalized.contains("global"));
    push_workflow_memory_scope(
        &mut scopes,
        "organization",
        normalized.contains("organization") || normalized.contains("tenant") || normalized == "org",
    );
    push_workflow_memory_scope(&mut scopes, "project", normalized.contains("project"));
    push_workflow_memory_scope(
        &mut scopes,
        "processing",
        normalized.contains("session")
            || normalized.contains("processing")
            || normalized.contains("run")
            || normalized.contains("thread"),
    );
    if scopes.is_empty() {
        scopes.extend([
            "organization".to_string(),
            "project".to_string(),
            "processing".to_string(),
        ]);
    }

    let allowed_by_level = match memory_level {
        "session" => vec!["processing"],
        "short_term" => vec!["project", "processing"],
        _ => vec!["global", "organization", "project", "processing"],
    };
    scopes
        .into_iter()
        .filter(|scope| allowed_by_level.contains(&scope.as_str()))
        .collect()
}

fn push_workflow_memory_scope(scopes: &mut Vec<String>, scope: &str, enabled: bool) {
    if enabled && !scopes.iter().any(|existing| existing == scope) {
        scopes.push(scope.to_string());
    }
}

pub fn search_memory(
    store: &ForgeStore,
    options: MemorySearchOptions,
) -> Result<MemorySearchReport> {
    let mut options = options;
    let project_governance = load_project_memory_governance(options.project_root.as_deref());
    let has_project_governance = project_governance.status == "configured";
    let explicit_scopes = !options.scopes.is_empty();
    let normalized_input_scopes = if explicit_scopes {
        normalize_scopes(&options.scopes)
    } else if has_project_governance {
        project_governance.default_scopes.clone()
    } else {
        normalize_scopes(&options.scopes)
    };
    let workflow_binding = bind_memory_to_workflow(
        store,
        options.workflow_id.as_deref(),
        "memory search",
        &mut options.organization_id,
        if explicit_scopes || has_project_governance {
            normalized_input_scopes.as_slice()
        } else {
            &[]
        },
    )?;
    let requested_scopes = if explicit_scopes {
        normalized_input_scopes
    } else if let Some(binding) = workflow_binding.as_ref() {
        binding.allowed_scopes.clone()
    } else {
        normalized_input_scopes
    };
    let requested_scopes_source = if explicit_scopes {
        "explicit"
    } else if has_project_governance {
        "project_governance"
    } else if workflow_binding.is_some() {
        "workflow_binding"
    } else {
        "default"
    }
    .to_string();
    let memory_level_source = if options.memory_level.is_some() {
        "explicit"
    } else if has_project_governance {
        "project_governance"
    } else {
        "default"
    }
    .to_string();
    let memory_level = options
        .memory_level
        .as_deref()
        .map(|value| normalize_memory_level(Some(value)))
        .unwrap_or_else(|| {
            if has_project_governance {
                project_governance.memory_level.clone()
            } else {
                normalize_memory_level(None)
            }
        });
    let audience_source = if options.audience.is_some() {
        "explicit"
    } else if has_project_governance {
        "project_governance"
    } else {
        "default"
    }
    .to_string();
    let audience = match options.audience.as_deref() {
        Some(value) => normalize_default_audience(value)?,
        None if has_project_governance => project_governance.default_audience.clone(),
        None => "private".to_string(),
    };
    let scopes = apply_memory_level(&requested_scopes, &memory_level);
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
                let allowed = audience_can_read(&audience, &visibility, &shareability);
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
        audience,
        memory_level,
        requested_scopes,
        effective_scopes: scopes,
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
            memory_level_rule:
                "memory_level reduces effective scopes before file reads; MEMORY_NONE disables retrieval, MEMORY_SESSION limits to processing memory, MEMORY_SHORT_TERM allows processing/project memory, and standard/full/admin can inspect all configured scopes subject to audience visibility gates"
                    .to_string(),
            project_governance_status: project_governance.status,
            project_governance_config_path: project_governance.config_path,
            memory_level_source,
            requested_scopes_source,
            audience_source,
            denied_result_count,
            workflow_binding,
            temporary_memory_rule:
                "processing memory is temporary by default and should be deleted or promoted explicitly during final packaging"
                    .to_string(),
        },
    })
}

pub fn promote_memory(
    store: &ForgeStore,
    options: MemoryPromotionOptions,
) -> Result<MemoryPromotionReport> {
    let mut options = options;
    let from_scope = normalize_memory_scope(&options.from_scope).ok_or_else(|| {
        anyhow::anyhow!("unsupported source memory scope: {}", options.from_scope)
    })?;
    let to_scope = normalize_memory_scope(&options.to_scope)
        .ok_or_else(|| anyhow::anyhow!("unsupported target memory scope: {}", options.to_scope))?;
    if from_scope == to_scope {
        bail!("source and target memory scopes must differ");
    }
    if to_scope == "processing" {
        bail!("memory promotion target must be project, organization or global");
    }
    if !matches!(
        from_scope.as_str(),
        "processing" | "project" | "organization"
    ) {
        bail!("memory promotion source must be processing, project or organization");
    }
    if options.summary.trim().is_empty() {
        bail!("memory promotion requires a curated --summary");
    }
    if options.approved_by.trim().is_empty() {
        bail!("memory promotion requires --approved-by");
    }
    if options.reason.trim().is_empty() {
        bail!("memory promotion requires --reason");
    }
    if !options.source_path.is_file() {
        bail!(
            "memory promotion source file does not exist: {}",
            options.source_path.display()
        );
    }
    if let (Some(start), Some(end)) = (options.source_start_line, options.source_end_line) {
        if start == 0 || end == 0 || end < start {
            bail!("source line range must be positive and ordered");
        }
    }
    let workflow_binding = bind_memory_to_workflow(
        store,
        options.workflow_id.as_deref(),
        "memory promote",
        &mut options.organization_id,
        &[from_scope.clone(), to_scope.clone()],
    )?;

    let visibility = normalize_visibility(&options.visibility);
    let shareability = normalize_shareability(
        options
            .shareability
            .as_deref()
            .unwrap_or_else(|| default_shareability(&to_scope)),
    );
    ensure_shareability_allowed_for_target(&to_scope, &shareability)?;

    let approved_at = Utc::now().to_rfc3339();
    let promotion_id = format!("mem_{}", Uuid::new_v4().to_string().replace('-', ""));
    let source_stem = options
        .source_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(sanitize_filename_component)
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| "memory".to_string());
    let target_root = promotion_target_root(store, &options, &to_scope);
    let target_filename = format!(
        "{}-{}-{}.md",
        Utc::now().format("%Y%m%dT%H%M%SZ"),
        source_stem,
        &promotion_id[promotion_id.len().saturating_sub(12)..]
    );
    let target_path = target_root.join(target_filename);
    let promoted_content = render_promoted_memory(&PromotedMemoryRenderInput {
        promotion_id: &promotion_id,
        from_scope: &from_scope,
        to_scope: &to_scope,
        source_path: &options.source_path,
        source_start_line: options.source_start_line,
        source_end_line: options.source_end_line,
        summary: options.summary.trim(),
        approved_by: options.approved_by.trim(),
        approved_at: &approved_at,
        reason: options.reason.trim(),
        visibility: &visibility,
        shareability: &shareability,
    });
    let promoted_memory_sha256 = hex_sha256(promoted_content.as_bytes());
    let summary_sha256 = hex_sha256(options.summary.trim().as_bytes());

    if !options.dry_run {
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create memory directory {}", parent.display())
            })?;
        }
        fs::write(&target_path, promoted_content.as_bytes()).with_context(|| {
            format!("failed to write promoted memory {}", target_path.display())
        })?;
    }

    let report = MemoryPromotionReport {
        schema_version: "forge.memory_promotion.v1".to_string(),
        status: if options.dry_run {
            "memory_promotion_planned".to_string()
        } else {
            "memory_promotion_recorded".to_string()
        },
        promotion_id: promotion_id.clone(),
        from_scope,
        to_scope,
        source_path: options.source_path.display().to_string(),
        source_start_line: options.source_start_line,
        source_end_line: options.source_end_line,
        target_path: target_path.display().to_string(),
        target_written: !options.dry_run,
        visibility,
        shareability,
        approved_by: options.approved_by.trim().to_string(),
        approved_at,
        reason: options.reason.trim().to_string(),
        summary_sha256,
        promoted_memory_sha256,
        governance: MemoryPromotionGovernance {
            raw_source_copied: false,
            approval_required: true,
            approval_recorded: true,
            classification_required: true,
            classification_recorded: true,
            allowed_targets: vec![
                "project".to_string(),
                "organization".to_string(),
                "global".to_string(),
            ],
            workflow_binding,
            rule: "Forge promotes only curated summaries with explicit approval, classification and source lineage; raw processing memory is not copied by default."
                .to_string(),
        },
    };

    if !options.dry_run {
        let report_json = serde_json::to_value(&report)?;
        let workflow_binding = report.governance.workflow_binding.as_ref();
        store.save_memory_promotion(MemoryPromotionWrite {
            id: &report.promotion_id,
            workflow_id: workflow_binding
                .map(|binding| binding.workflow_id.as_str())
                .unwrap_or(""),
            organization_id: workflow_binding
                .map(|binding| binding.organization_id.as_str())
                .unwrap_or(""),
            brand_id: workflow_binding
                .map(|binding| binding.brand_id.as_str())
                .unwrap_or(""),
            product_id: workflow_binding
                .map(|binding| binding.product_id.as_str())
                .unwrap_or(""),
            user_id: workflow_binding
                .map(|binding| binding.user_id.as_str())
                .unwrap_or(""),
            channel_id: workflow_binding
                .map(|binding| binding.channel_id.as_str())
                .unwrap_or(""),
            from_scope: &report.from_scope,
            to_scope: &report.to_scope,
            source_path: &report.source_path,
            target_path: &report.target_path,
            visibility: &report.visibility,
            shareability: &report.shareability,
            approved_by: &report.approved_by,
            reason: &report.reason,
            summary_sha256: &report.summary_sha256,
            promoted_memory_sha256: &report.promoted_memory_sha256,
            data: &report_json,
        })?;
    }

    Ok(report)
}

pub fn list_memory_promotions(
    store: &ForgeStore,
    from_scope: Option<String>,
    to_scope: Option<String>,
    approved_by: Option<String>,
    workflow_id: Option<String>,
) -> Result<MemoryPromotionIndexReport> {
    let mut organization_id = None;
    let workflow_binding = bind_memory_to_workflow(
        store,
        workflow_id.as_deref(),
        "memory promotions",
        &mut organization_id,
        &[],
    )?;
    let normalized_from = from_scope
        .as_deref()
        .map(|scope| {
            normalize_memory_scope(scope)
                .ok_or_else(|| anyhow::anyhow!("unsupported source memory scope: {scope}"))
        })
        .transpose()?;
    let normalized_to = to_scope
        .as_deref()
        .map(|scope| {
            normalize_memory_scope(scope)
                .ok_or_else(|| anyhow::anyhow!("unsupported target memory scope: {scope}"))
        })
        .transpose()?;
    let approved_by_filter = approved_by
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let workflow_id_filter = workflow_binding
        .as_ref()
        .map(|binding| binding.workflow_id.as_str());
    let organization_id_filter = workflow_binding
        .as_ref()
        .map(|binding| binding.organization_id.as_str());
    let brand_id_filter = workflow_binding
        .as_ref()
        .map(|binding| binding.brand_id.as_str());
    let product_id_filter = workflow_binding
        .as_ref()
        .map(|binding| binding.product_id.as_str());
    let records = store.list_memory_promotions(MemoryPromotionQuery {
        from_scope: normalized_from.as_deref(),
        to_scope: normalized_to.as_deref(),
        approved_by: approved_by_filter.as_deref(),
        workflow_id: workflow_id_filter,
        organization_id: organization_id_filter,
        brand_id: brand_id_filter,
        product_id: product_id_filter,
    })?;
    let promotions = records
        .into_iter()
        .map(memory_promotion_index_entry_from_record)
        .collect::<Vec<_>>();
    Ok(MemoryPromotionIndexReport {
        schema_version: "forge.memory_promotion_index.v1".to_string(),
        status: "memory_promotion_index_loaded".to_string(),
        filters: MemoryPromotionIndexFilters {
            workflow_id: workflow_binding
                .as_ref()
                .map(|binding| binding.workflow_id.clone()),
            organization_id: workflow_binding
                .as_ref()
                .map(|binding| binding.organization_id.clone()),
            brand_id: workflow_binding
                .as_ref()
                .map(|binding| binding.brand_id.clone()),
            product_id: workflow_binding
                .as_ref()
                .map(|binding| binding.product_id.clone()),
            from_scope: normalized_from,
            to_scope: normalized_to,
            approved_by: approved_by_filter,
        },
        promotion_count: promotions.len(),
        promotions,
    })
}

pub fn memory_retention_report(
    store: &ForgeStore,
    options: MemoryRetentionOptions,
) -> Result<MemoryRetentionReport> {
    let mut options = options;
    let explicit_scopes = !options.scopes.is_empty();
    let normalized_input_scopes = normalize_scopes(&options.scopes);
    let workflow_binding = bind_memory_to_workflow(
        store,
        options.workflow_id.as_deref(),
        "memory retention",
        &mut options.organization_id,
        if explicit_scopes {
            normalized_input_scopes.as_slice()
        } else {
            &[]
        },
    )?;
    let requested_scopes = if explicit_scopes {
        normalized_input_scopes
    } else if let Some(binding) = workflow_binding.as_ref() {
        binding.allowed_scopes.clone()
    } else {
        normalized_input_scopes
    };
    let search_options = MemorySearchOptions {
        query: String::new(),
        workflow_id: None,
        scopes: requested_scopes.clone(),
        audience: Some("private".to_string()),
        visibility: None,
        memory_level: Some("admin".to_string()),
        run_id: options.run_id,
        organization_id: options.organization_id,
        limit: DEFAULT_LIMIT,
        global_root: options.global_root,
        organization_root: options.organization_root,
        project_root: options.project_root,
        processing_root: options.processing_root,
    };
    let roots = resolve_roots(store, &search_options, &requested_scopes);
    let mut items = Vec::new();
    for root in &roots {
        if !root.root.exists() {
            continue;
        }
        for file in markdown_files(&root.root)? {
            let content = fs::read_to_string(&file)
                .with_context(|| format!("failed to read memory file {}", file.display()))?;
            let metadata = parse_metadata(&content);
            let visibility = normalize_visibility(
                metadata
                    .visibility
                    .as_deref()
                    .unwrap_or_else(|| default_visibility(&root.scope)),
            );
            let shareability = normalize_shareability(
                metadata
                    .shareability
                    .as_deref()
                    .unwrap_or_else(|| default_shareability(&root.scope)),
            );
            let lifecycle = metadata
                .lifecycle
                .clone()
                .unwrap_or_else(|| root.lifecycle.clone());
            let retention = metadata.retention.clone().unwrap_or_else(|| {
                if root.scope == "processing" {
                    "temporary".to_string()
                } else {
                    "persistent".to_string()
                }
            });
            let (action, reason) =
                retention_action_for_memory(&root.scope, &lifecycle, &retention, &shareability);
            items.push(MemoryRetentionItem {
                scope: root.scope.clone(),
                path: file.display().to_string(),
                visibility,
                shareability,
                lifecycle,
                retention,
                action,
                reason,
            });
        }
    }
    items.sort_by(|left, right| {
        left.scope
            .cmp(&right.scope)
            .then_with(|| left.path.cmp(&right.path))
    });
    let keep_count = items.iter().filter(|item| item.action == "keep").count();
    let promote_or_delete_count = items
        .iter()
        .filter(|item| item.action == "classify_then_promote_or_delete")
        .count();
    let delete_candidate_count = items
        .iter()
        .filter(|item| item.action == "delete_after_final_packaging")
        .count();
    Ok(MemoryRetentionReport {
        schema_version: "forge.memory_retention.v1".to_string(),
        status: "memory_retention_evaluated".to_string(),
        requested_scopes,
        searched_roots: roots
            .iter()
            .map(|root| MemorySearchRoot {
                scope: root.scope.clone(),
                root: root.root.display().to_string(),
                exists: root.root.exists(),
                lifecycle: root.lifecycle.clone(),
            })
            .collect(),
        item_count: items.len(),
        keep_count,
        promote_or_delete_count,
        delete_candidate_count,
        items,
        governance: MemoryRetentionGovernance {
            destructive_actions_performed: false,
            deletion_requires_explicit_future_command: true,
            workflow_binding,
            processing_rule:
                "processing memory is temporary unless metadata asks for curated promotion; this report never deletes files"
                    .to_string(),
            promoted_rule:
                "promoted, project, organization and global memory is kept until a future explicit retention/expiry policy changes it"
                    .to_string(),
        },
    })
}

pub fn memory_cleanup_report(
    store: &ForgeStore,
    options: MemoryCleanupOptions,
) -> Result<MemoryCleanupReport> {
    let mode = normalize_cleanup_mode(&options.mode)?;
    let approved_by = options
        .approved_by
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let reason = options
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if !options.dry_run {
        if approved_by.is_none() {
            bail!("memory cleanup requires --approved-by unless --dry-run is used");
        }
        if reason.is_none() {
            bail!("memory cleanup requires --reason unless --dry-run is used");
        }
        if !options.confirm {
            bail!("memory cleanup requires --confirm unless --dry-run is used");
        }
    }

    let archive_root = options
        .archive_root
        .clone()
        .unwrap_or_else(|| store.base_dir().join("memory-archive"));
    let retention = memory_retention_report(
        store,
        MemoryRetentionOptions {
            workflow_id: options.workflow_id,
            scopes: options.scopes,
            run_id: options.run_id,
            organization_id: options.organization_id,
            global_root: options.global_root,
            organization_root: options.organization_root,
            project_root: options.project_root,
            processing_root: options.processing_root,
        },
    )?;
    let requested_scopes = retention.requested_scopes.clone();
    let retention_source_schema = retention.schema_version.clone();
    let workflow_binding = retention.governance.workflow_binding.clone();
    let mut items = Vec::new();
    for item in retention.items {
        if item.scope != "processing" || item.action != "delete_after_final_packaging" {
            items.push(MemoryCleanupItem {
                scope: item.scope,
                path: item.path,
                retention_action: item.action,
                cleanup_action: "skipped".to_string(),
                target_path: None,
                reason:
                    "cleanup only touches processing memory classified as delete_after_final_packaging"
                        .to_string(),
            });
            continue;
        }
        let source_path = PathBuf::from(&item.path);
        match mode.as_str() {
            "archive" => {
                let target_path = archive_target_path(&archive_root, &source_path);
                if !options.dry_run {
                    if let Some(parent) = target_path.parent() {
                        fs::create_dir_all(parent).with_context(|| {
                            format!("failed to create archive directory {}", parent.display())
                        })?;
                    }
                    archive_memory_file(&source_path, &target_path)?;
                }
                items.push(MemoryCleanupItem {
                    scope: item.scope,
                    path: item.path,
                    retention_action: item.action,
                    cleanup_action: if options.dry_run {
                        "archive_planned".to_string()
                    } else {
                        "archived".to_string()
                    },
                    target_path: Some(target_path.display().to_string()),
                    reason:
                        "processing memory was classified as temporary and eligible for cleanup"
                            .to_string(),
                });
            }
            "delete" => {
                if !options.dry_run {
                    fs::remove_file(&source_path).with_context(|| {
                        format!("failed to delete memory file {}", source_path.display())
                    })?;
                }
                items.push(MemoryCleanupItem {
                    scope: item.scope,
                    path: item.path,
                    retention_action: item.action,
                    cleanup_action: if options.dry_run {
                        "delete_planned".to_string()
                    } else {
                        "deleted".to_string()
                    },
                    target_path: None,
                    reason:
                        "processing memory was classified as temporary and eligible for cleanup"
                            .to_string(),
                });
            }
            _ => unreachable!("cleanup mode is normalized before execution"),
        }
    }
    let archived_count = items
        .iter()
        .filter(|item| item.cleanup_action == "archived")
        .count();
    let deleted_count = items
        .iter()
        .filter(|item| item.cleanup_action == "deleted")
        .count();
    let skipped_count = items
        .iter()
        .filter(|item| item.cleanup_action == "skipped")
        .count();
    let approval_recorded = options.dry_run || approved_by.is_some();
    let confirm_recorded = options.dry_run || options.confirm;
    let destructive_actions_performed = !options.dry_run && (archived_count + deleted_count > 0);
    Ok(MemoryCleanupReport {
        schema_version: "forge.memory_cleanup.v1".to_string(),
        status: if options.dry_run {
            "memory_cleanup_planned".to_string()
        } else {
            "memory_cleanup_executed".to_string()
        },
        mode,
        dry_run: options.dry_run,
        approved_by,
        reason,
        requested_scopes,
        item_count: items.len(),
        archived_count,
        deleted_count,
        skipped_count,
        items,
        governance: MemoryCleanupGovernance {
            approval_required: !options.dry_run,
            approval_recorded,
            confirm_required: !options.dry_run,
            confirm_recorded,
            destructive_actions_performed,
            retention_source_schema,
            workflow_binding,
            rule: "memory cleanup is a separate approval-gated command and only archives/deletes processing memory that retention classified as delete_after_final_packaging"
                .to_string(),
        },
    })
}

fn memory_promotion_index_entry_from_record(
    record: StoredMemoryPromotionRecord,
) -> MemoryPromotionIndexEntry {
    MemoryPromotionIndexEntry {
        promotion_id: record.id,
        workflow_id: record.workflow_id,
        organization_id: record.organization_id,
        brand_id: record.brand_id,
        product_id: record.product_id,
        user_id: record.user_id,
        channel_id: record.channel_id,
        from_scope: record.from_scope,
        to_scope: record.to_scope,
        source_path: record.source_path,
        target_path: record.target_path,
        visibility: record.visibility,
        shareability: record.shareability,
        approved_by: record.approved_by,
        reason: record.reason,
        summary_sha256: record.summary_sha256,
        promoted_memory_sha256: record.promoted_memory_sha256,
        created_at: record.created_at,
        report: record.data,
    }
}

fn retention_action_for_memory(
    scope: &str,
    lifecycle: &str,
    retention: &str,
    shareability: &str,
) -> (String, String) {
    let retention_lower = retention.trim().to_ascii_lowercase();
    let lifecycle_lower = lifecycle.trim().to_ascii_lowercase();
    if scope == "processing"
        && (retention_lower.contains("promote") || shareability == "manager_shared")
    {
        return (
            "classify_then_promote_or_delete".to_string(),
            "processing memory asks for classification/promotion before cleanup".to_string(),
        );
    }
    if scope == "processing"
        && (retention_lower.contains("temporary")
            || retention_lower.contains("ephemeral")
            || lifecycle_lower.contains("run_lived"))
    {
        return (
            "delete_after_final_packaging".to_string(),
            "run-lived processing memory should be removed after final packaging unless promoted"
                .to_string(),
        );
    }
    (
        "keep".to_string(),
        "persistent or promoted memory is retained by the current policy".to_string(),
    )
}

fn normalize_cleanup_mode(mode: &str) -> Result<String> {
    match mode.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "" | "archive" => Ok("archive".to_string()),
        "delete" | "remove" => Ok("delete".to_string()),
        other => bail!("unsupported memory cleanup mode: {other}"),
    }
}

fn archive_target_path(archive_root: &Path, source_path: &Path) -> PathBuf {
    let file_name = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(sanitize_filename_component)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "memory.md".to_string());
    archive_root.join("processing").join(format!(
        "{}-{}",
        Utc::now().format("%Y%m%dT%H%M%SZ"),
        file_name
    ))
}

fn archive_memory_file(source_path: &Path, target_path: &Path) -> Result<()> {
    match fs::rename(source_path, target_path) {
        Ok(()) => Ok(()),
        Err(_) => {
            fs::copy(source_path, target_path).with_context(|| {
                format!(
                    "failed to copy memory file {} to {}",
                    source_path.display(),
                    target_path.display()
                )
            })?;
            fs::remove_file(source_path).with_context(|| {
                format!(
                    "failed to remove archived memory file {}",
                    source_path.display()
                )
            })?;
            Ok(())
        }
    }
}

fn normalize_scopes(scopes: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    if scopes.is_empty() {
        return vec![
            "global".to_string(),
            "organization".to_string(),
            "project".to_string(),
            "processing".to_string(),
        ];
    }
    for scope in scopes {
        let scope = scope.trim().to_ascii_lowercase();
        if matches!(
            scope.as_str(),
            "global" | "organization" | "project" | "processing"
        ) && !normalized.contains(&scope)
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

fn normalize_memory_scope(scope: &str) -> Option<String> {
    match scope.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "global" => Some("global".to_string()),
        "organization" | "org" | "tenant" => Some("organization".to_string()),
        "project" => Some("project".to_string()),
        "processing" | "session" | "run" | "thread" => Some("processing".to_string()),
        _ => None,
    }
}

fn memory_level_policies() -> Vec<MemoryLevelPolicy> {
    vec![
        MemoryLevelPolicy {
            level: "MEMORY_NONE".to_string(),
            allowed_scopes: Vec::new(),
            default_audience: "public".to_string(),
            can_read_private: false,
            lifecycle: "disabled".to_string(),
            notes: "No historical memory is retrieved; only explicit task context may be used."
                .to_string(),
        },
        MemoryLevelPolicy {
            level: "MEMORY_SESSION".to_string(),
            allowed_scopes: vec!["processing".to_string()],
            default_audience: "private".to_string(),
            can_read_private: true,
            lifecycle: "run_lived_ephemeral".to_string(),
            notes: "Only the current run/thread processing memory can be searched.".to_string(),
        },
        MemoryLevelPolicy {
            level: "MEMORY_SHORT_TERM".to_string(),
            allowed_scopes: vec!["project".to_string(), "processing".to_string()],
            default_audience: "manager".to_string(),
            can_read_private: true,
            lifecycle: "project_and_run_lived".to_string(),
            notes: "Project memory plus current processing memory; global cross-project memory is excluded."
                .to_string(),
        },
        MemoryLevelPolicy {
            level: "MEMORY_STANDARD".to_string(),
            allowed_scopes: vec![
                "global".to_string(),
                "organization".to_string(),
                "project".to_string(),
                "processing".to_string(),
            ],
            default_audience: "internal".to_string(),
            can_read_private: false,
            lifecycle: "bounded_default".to_string(),
            notes: "Default Forge memory posture: all configured scopes can be searched, still gated by audience and visibility."
                .to_string(),
        },
        MemoryLevelPolicy {
            level: "MEMORY_FULL".to_string(),
            allowed_scopes: vec![
                "global".to_string(),
                "organization".to_string(),
                "project".to_string(),
                "processing".to_string(),
            ],
            default_audience: "private".to_string(),
            can_read_private: true,
            lifecycle: "full_local_governed".to_string(),
            notes: "Full local governed search for operators; visibility/shareability gates still apply unless audience is private."
                .to_string(),
        },
        MemoryLevelPolicy {
            level: "MEMORY_ADMIN".to_string(),
            allowed_scopes: vec![
                "global".to_string(),
                "organization".to_string(),
                "project".to_string(),
                "processing".to_string(),
            ],
            default_audience: "private".to_string(),
            can_read_private: true,
            lifecycle: "admin_audit".to_string(),
            notes: "Administrative audit posture for governance/debugging, with explicit result lineage and no hidden state."
                .to_string(),
        },
    ]
}

fn normalize_memory_level(value: Option<&str>) -> String {
    match value
        .unwrap_or("standard")
        .trim()
        .to_ascii_uppercase()
        .replace('-', "_")
        .as_str()
    {
        "NONE" | "MEMORY_NONE" => "MEMORY_NONE".to_string(),
        "SESSION" | "MEMORY_SESSION" => "MEMORY_SESSION".to_string(),
        "SHORT_TERM" | "SHORTTERM" | "MEMORY_SHORT_TERM" | "MEMORY_SHORTTERM" => {
            "MEMORY_SHORT_TERM".to_string()
        }
        "FULL" | "MEMORY_FULL" => "MEMORY_FULL".to_string(),
        "ADMIN" | "MEMORY_ADMIN" => "MEMORY_ADMIN".to_string(),
        _ => "MEMORY_STANDARD".to_string(),
    }
}

fn apply_memory_level(scopes: &[String], memory_level: &str) -> Vec<String> {
    let allowed = memory_level_policies()
        .into_iter()
        .find(|policy| policy.level == memory_level)
        .map(|policy| policy.allowed_scopes)
        .unwrap_or_else(|| {
            vec![
                "global".to_string(),
                "organization".to_string(),
                "project".to_string(),
                "processing".to_string(),
            ]
        });

    scopes
        .iter()
        .filter(|scope| allowed.contains(scope))
        .cloned()
        .collect()
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
            "organization" => roots.push(MemoryRoot {
                scope: scope.clone(),
                root: options.organization_root.clone().unwrap_or_else(|| {
                    let organization = options
                        .organization_id
                        .clone()
                        .unwrap_or_else(|| "default-org".to_string());
                    store
                        .base_dir()
                        .join("organizations")
                        .join(organization)
                        .join("memory")
                }),
                lifecycle: "long_lived_tenant".to_string(),
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

struct PromotedMemoryRenderInput<'a> {
    promotion_id: &'a str,
    from_scope: &'a str,
    to_scope: &'a str,
    source_path: &'a Path,
    source_start_line: Option<usize>,
    source_end_line: Option<usize>,
    summary: &'a str,
    approved_by: &'a str,
    approved_at: &'a str,
    reason: &'a str,
    visibility: &'a str,
    shareability: &'a str,
}

fn promotion_target_root(
    store: &ForgeStore,
    options: &MemoryPromotionOptions,
    to_scope: &str,
) -> PathBuf {
    match to_scope {
        "global" => options
            .global_root
            .clone()
            .unwrap_or_else(default_global_memory_root),
        "organization" => options.organization_root.clone().unwrap_or_else(|| {
            let organization = options
                .organization_id
                .clone()
                .unwrap_or_else(|| "default-org".to_string());
            store
                .base_dir()
                .join("organizations")
                .join(organization)
                .join("memory")
        }),
        "project" => options
            .project_root
            .clone()
            .unwrap_or_else(|| store.base_dir().join("memory")),
        _ => store.base_dir().join("memory"),
    }
}

fn ensure_shareability_allowed_for_target(scope: &str, shareability: &str) -> Result<()> {
    let allowed = match scope {
        "global" => matches!(shareability, "global_shared"),
        "organization" => matches!(shareability, "organization_shared"),
        "project" => matches!(
            shareability,
            "project_shared" | "manager_shared" | "organization_shared"
        ),
        _ => false,
    };
    if !allowed {
        bail!("shareability {shareability} is not allowed for target scope {scope}");
    }
    Ok(())
}

fn render_promoted_memory(input: &PromotedMemoryRenderInput<'_>) -> String {
    let mut content = String::new();
    content.push_str("---\n");
    content.push_str("schema_version: forge.promoted_memory.v1\n");
    content.push_str(&format!("promotion_id: {}\n", input.promotion_id));
    content.push_str(&format!("visibility: {}\n", input.visibility));
    content.push_str(&format!("shareability: {}\n", input.shareability));
    content.push_str("lifecycle: promoted\n");
    content.push_str("retention: persistent\n");
    content.push_str(&format!("source_scope: {}\n", input.from_scope));
    content.push_str(&format!("target_scope: {}\n", input.to_scope));
    content.push_str(&format!(
        "source_path: {}\n",
        yaml_quote(&input.source_path.display().to_string())
    ));
    if let Some(start) = input.source_start_line {
        content.push_str(&format!("source_start_line: {start}\n"));
    }
    if let Some(end) = input.source_end_line {
        content.push_str(&format!("source_end_line: {end}\n"));
    }
    content.push_str(&format!("approved_by: {}\n", yaml_quote(input.approved_by)));
    content.push_str(&format!("approved_at: {}\n", input.approved_at));
    content.push_str(&format!("promotion_reason: {}\n", yaml_quote(input.reason)));
    content.push_str("raw_source_copied: false\n");
    content.push_str("---\n\n");
    content.push_str("# Promoted Memory\n\n");
    content.push_str(input.summary.trim());
    content.push_str("\n\n## Audit\n\n");
    content.push_str(&format!("- Source: `{}`", input.source_path.display()));
    if input.source_start_line.is_some() || input.source_end_line.is_some() {
        content.push_str(&format!(
            " lines {}-{}",
            input
                .source_start_line
                .map(|line| line.to_string())
                .unwrap_or_else(|| "?".to_string()),
            input
                .source_end_line
                .map(|line| line.to_string())
                .unwrap_or_else(|| "?".to_string())
        ));
    }
    content.push('\n');
    content.push_str(&format!(
        "- Approved by: {} at {}\n",
        input.approved_by, input.approved_at
    ));
    content.push_str(&format!("- Reason: {}\n", input.reason));
    content.push_str("- Raw source copied: false\n");
    content
}

fn sanitize_filename_component(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if matches!(ch, '-' | '_' | '.') {
            out.push(ch);
        } else if ch.is_whitespace() {
            out.push('-');
        }
    }
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    out.trim_matches(['-', '.', '_']).to_string()
}

fn yaml_quote(value: &str) -> String {
    format!("{:?}", value)
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
        "organization" => "organization_shared",
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
        "organization" | "organization_shared" | "tenant" | "tenant_shared" => {
            "organization_shared".to_string()
        }
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
            visibility == "public"
                && matches!(
                    shareability,
                    "global_shared" | "organization_shared" | "project_shared"
                )
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
