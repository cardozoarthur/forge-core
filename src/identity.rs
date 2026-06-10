use crate::artifact::hex_sha256;
use crate::intent::{ContextIdentityRef, OperatingContextSpec};
use crate::storage::{
    ForgeStore, StoredIdentityLinkRecord, StoredIdentityMembershipRecord, StoredIdentityRecord,
    TenantIndexRecord,
};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

pub const OPERATING_CONTEXT_LOAD_SCHEMA_VERSION: &str = "forge.operating_context_load.v1";
pub const IDENTITY_REGISTRY_SCHEMA_VERSION: &str = "forge.identity_registry.v1";
pub const IDENTITY_MEMBERSHIP_SCHEMA_VERSION: &str = "forge.identity_memberships.v1";
pub const IDENTITY_MEMBERSHIP_UPDATE_SCHEMA_VERSION: &str = "forge.identity_membership_update.v1";
pub const IDENTITY_LINK_SCHEMA_VERSION: &str = "forge.identity_link.v1";
pub const IDENTITY_LINKS_SCHEMA_VERSION: &str = "forge.identity_links.v1";
pub const IDENTITY_RESOLVE_SCHEMA_VERSION: &str = "forge.identity_resolve.v1";
pub const IDENTITY_SYNC_SCHEMA_VERSION: &str = "forge.identity_sync.v1";
pub const TENANT_INDEX_SCHEMA_VERSION: &str = "forge.tenant_index.v1";
pub const TENANT_AUDIT_SCHEMA_VERSION: &str = "forge.tenant_audit.v1";
pub const TENANT_POLICY_SCHEMA_VERSION: &str = "forge.tenant_policy.v1";
pub const IDENTITY_MEMBERSHIP_PERMISSION_SCHEMA_VERSION: &str =
    "forge.identity_membership_permissions.v1";

#[derive(Debug, Clone, Serialize)]
pub struct OperatingContextLoadReport {
    pub schema_version: String,
    pub status: String,
    pub source: String,
    pub project_root: String,
    pub context: OperatingContextSpec,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdentityRegistryReport {
    pub schema_version: String,
    pub status: String,
    pub identity_count: usize,
    pub identities: Vec<IdentityRegistryView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdentitySyncReport {
    pub schema_version: String,
    pub status: String,
    pub synced_count: usize,
    pub membership_count: usize,
    pub source: String,
    pub project_root: String,
    pub identities: Vec<IdentityRegistryView>,
    pub memberships: Vec<IdentityMembershipView>,
    pub context: OperatingContextLoadReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdentityRegistryView {
    pub scope: String,
    pub id: String,
    pub label: String,
    pub source: String,
    pub updated_at: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdentityMembershipReport {
    pub schema_version: String,
    pub status: String,
    pub membership_count: usize,
    pub memberships: Vec<IdentityMembershipView>,
}

#[derive(Debug, Clone)]
pub struct IdentityMembershipUpdateInput {
    pub subject_scope: String,
    pub subject_id: String,
    pub organization_id: String,
    pub brand_id: String,
    pub product_id: String,
    pub role: Option<String>,
    pub status: Option<String>,
    pub grant_permissions: Vec<String>,
    pub revoke_grants: Vec<String>,
    pub deny_permissions: Vec<String>,
    pub remove_denies: Vec<String>,
    pub expires_at: Option<String>,
    pub clear_expires_at: bool,
    pub not_before: Option<String>,
    pub clear_not_before: bool,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdentityMembershipUpdateReport {
    pub schema_version: String,
    pub status: String,
    pub updated: bool,
    pub source: String,
    pub before: IdentityMembershipView,
    pub after: IdentityMembershipView,
    pub changes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct IdentityLinkInput {
    pub left_scope: String,
    pub left_id: String,
    pub right_scope: String,
    pub right_id: String,
    pub link_type: String,
    pub source: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdentityLinkReport {
    pub schema_version: String,
    pub status: String,
    pub link: IdentityLinkView,
    pub left_identity: IdentityRegistryView,
    pub right_identity: IdentityRegistryView,
    pub resolved: IdentityResolveReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdentityLinksReport {
    pub schema_version: String,
    pub status: String,
    pub link_count: usize,
    pub links: Vec<IdentityLinkView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdentityResolveReport {
    pub schema_version: String,
    pub status: String,
    pub requested_scope: String,
    pub requested_id: String,
    pub canonical_identity: IdentityAliasView,
    pub identity_count: usize,
    pub link_count: usize,
    pub identities: Vec<IdentityAliasView>,
    pub links: Vec<IdentityLinkView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdentityAliasView {
    pub scope: String,
    pub id: String,
    pub label: String,
    pub source: String,
    pub data: serde_json::Value,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdentityLinkView {
    pub id: String,
    pub left_scope: String,
    pub left_id: String,
    pub right_scope: String,
    pub right_id: String,
    pub link_type: String,
    pub status: String,
    pub source: String,
    pub created_at: String,
    pub updated_at: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdentityMembershipView {
    pub subject_scope: String,
    pub subject_id: String,
    pub organization_id: String,
    pub brand_id: String,
    pub product_id: String,
    pub role: String,
    pub permissions: Vec<String>,
    pub permission_grants: Vec<String>,
    pub permission_denies: Vec<String>,
    pub expires_at: Option<String>,
    pub not_before: Option<String>,
    pub expired: bool,
    pub not_yet_valid: bool,
    pub environments: Vec<String>,
    pub status: String,
    pub source: String,
    pub updated_at: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct TenantIndexReport {
    pub schema_version: String,
    pub status: String,
    pub resource_count: usize,
    pub resources: Vec<TenantIndexView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TenantIndexView {
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
    pub updated_at: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct TenantAuditReport {
    pub schema_version: String,
    pub status: String,
    pub expected_resource_count: usize,
    pub indexed_resource_count: usize,
    pub missing_count: usize,
    pub missing_resources: Vec<TenantAuditMissingResource>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TenantAuditMissingResource {
    pub resource_type: String,
    pub resource_id: String,
    pub workflow_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TenantPolicyReport {
    pub schema_version: String,
    pub status: String,
    pub mode: String,
    pub allowed: bool,
    pub action: String,
    pub required_permission: String,
    pub workflow_id: String,
    pub organization_id: String,
    pub brand_id: String,
    pub product_id: String,
    pub user_id: String,
    pub channel_id: String,
    pub membership_count: usize,
    pub active_membership_count: usize,
    pub expired_membership_count: usize,
    pub not_yet_valid_membership_count: usize,
    pub membership_roles: Vec<String>,
    pub granted_permissions: Vec<String>,
    pub denied_permissions: Vec<String>,
    pub indexed_resource_count: usize,
    pub missing_tenant_index_count: usize,
    pub decisions: Vec<TenantPolicyDecision>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TenantPolicyDecision {
    pub gate: String,
    pub status: String,
    pub reason: String,
}

pub fn load_project_operating_context(project_root: &Path) -> Result<OperatingContextSpec> {
    Ok(inspect_project_operating_context(project_root)?.context)
}

pub fn list_identity_registry(
    store: &ForgeStore,
    scope: Option<&str>,
    id: Option<&str>,
) -> Result<IdentityRegistryReport> {
    let identities = store
        .list_identity_records(scope, id)?
        .into_iter()
        .map(identity_view_from_record)
        .collect::<Vec<_>>();
    Ok(IdentityRegistryReport {
        schema_version: IDENTITY_REGISTRY_SCHEMA_VERSION.to_string(),
        status: "identity_registry_loaded".to_string(),
        identity_count: identities.len(),
        identities,
    })
}

pub fn list_identity_memberships(
    store: &ForgeStore,
    subject_scope: Option<&str>,
    subject_id: Option<&str>,
    organization_id: Option<&str>,
    brand_id: Option<&str>,
    product_id: Option<&str>,
    status: Option<&str>,
) -> Result<IdentityMembershipReport> {
    let memberships = store
        .list_identity_memberships(
            subject_scope,
            subject_id,
            organization_id,
            brand_id,
            product_id,
            status,
        )?
        .into_iter()
        .map(membership_view_from_record)
        .collect::<Vec<_>>();
    Ok(IdentityMembershipReport {
        schema_version: IDENTITY_MEMBERSHIP_SCHEMA_VERSION.to_string(),
        status: "identity_memberships_loaded".to_string(),
        membership_count: memberships.len(),
        memberships,
    })
}

pub fn update_identity_membership(
    store: &ForgeStore,
    input: IdentityMembershipUpdateInput,
) -> Result<IdentityMembershipUpdateReport> {
    let records = store.list_identity_memberships(
        Some(input.subject_scope.as_str()),
        Some(input.subject_id.as_str()),
        Some(input.organization_id.as_str()),
        Some(input.brand_id.as_str()),
        Some(input.product_id.as_str()),
        None,
    )?;
    let record = records.into_iter().next().with_context(|| {
        format!(
            "identity membership not found for {}/{} in {}/{}/{}",
            input.subject_scope,
            input.subject_id,
            input.organization_id,
            input.brand_id,
            input.product_id
        )
    })?;
    let before = membership_view_from_record(record.clone());
    let mut role = record.role.clone();
    let mut status = record.status.clone();
    let mut data = record.data.clone();
    let mut changes = Vec::new();

    if let Some(next_role) = input
        .role
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if next_role != role {
            role = next_role.to_string();
            data["role"] = json!(role);
            data["role_default_permissions"] = json!(role_default_permissions(&role));
            changes.push(format!("role={role}"));
        }
    }
    if let Some(next_status) = input
        .status
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if next_status != status {
            status = next_status.to_string();
            changes.push(format!("status={status}"));
        }
    }

    apply_permission_list_change(
        &mut data,
        "permission_grants",
        &input.grant_permissions,
        true,
        &mut changes,
        "grant",
    );
    apply_permission_list_change(
        &mut data,
        "permission_grants",
        &input.revoke_grants,
        false,
        &mut changes,
        "revoke_grant",
    );
    apply_permission_list_change(
        &mut data,
        "permission_denies",
        &input.deny_permissions,
        true,
        &mut changes,
        "deny",
    );
    apply_permission_list_change(
        &mut data,
        "permission_denies",
        &input.remove_denies,
        false,
        &mut changes,
        "remove_deny",
    );

    if input.clear_expires_at {
        if data.get("expires_at").is_some() {
            if let Some(object) = data.as_object_mut() {
                object.remove("expires_at");
            }
            changes.push("clear_expires_at".to_string());
        }
    } else if let Some(expires_at) = input.expires_at.as_deref() {
        validate_rfc3339_timestamp("expires_at", expires_at)?;
        if data.get("expires_at").and_then(serde_json::Value::as_str) != Some(expires_at) {
            data["expires_at"] = json!(expires_at);
            changes.push(format!("expires_at={expires_at}"));
        }
    }
    if input.clear_not_before {
        let mut cleared = false;
        if let Some(object) = data.as_object_mut() {
            cleared |= object.remove("not_before").is_some();
            cleared |= object.remove("valid_from").is_some();
        }
        if cleared {
            changes.push("clear_not_before".to_string());
        }
    } else if let Some(not_before) = input.not_before.as_deref() {
        validate_rfc3339_timestamp("not_before", not_before)?;
        if data.get("not_before").and_then(serde_json::Value::as_str) != Some(not_before) {
            data["not_before"] = json!(not_before);
            if let Some(object) = data.as_object_mut() {
                object.remove("valid_from");
            }
            changes.push(format!("not_before={not_before}"));
        }
    }
    data["membership_update_source"] = json!(input.source);
    data["membership_update_schema_version"] = json!(IDENTITY_MEMBERSHIP_UPDATE_SCHEMA_VERSION);

    store.save_identity_membership(
        &record.subject_scope,
        &record.subject_id,
        &record.organization_id,
        &record.brand_id,
        &record.product_id,
        &role,
        &status,
        &input.source,
        &data,
    )?;
    let after = store
        .list_identity_memberships(
            Some(record.subject_scope.as_str()),
            Some(record.subject_id.as_str()),
            Some(record.organization_id.as_str()),
            Some(record.brand_id.as_str()),
            Some(record.product_id.as_str()),
            None,
        )?
        .into_iter()
        .next()
        .map(membership_view_from_record)
        .context("updated identity membership was not found after save")?;
    Ok(IdentityMembershipUpdateReport {
        schema_version: IDENTITY_MEMBERSHIP_UPDATE_SCHEMA_VERSION.to_string(),
        status: "identity_membership_updated".to_string(),
        updated: !changes.is_empty(),
        source: input.source,
        before,
        after,
        changes,
    })
}

pub fn link_identity(store: &ForgeStore, input: IdentityLinkInput) -> Result<IdentityLinkReport> {
    let left_scope = normalize_required_identity_part("left_scope", &input.left_scope)?;
    let left_id = normalize_required_identity_part("left_id", &input.left_id)?;
    let right_scope = normalize_required_identity_part("right_scope", &input.right_scope)?;
    let right_id = normalize_required_identity_part("right_id", &input.right_id)?;
    if left_scope == right_scope && left_id == right_id {
        bail!("identity link requires two different identities");
    }
    let link_type = normalize_link_type(&input.link_type);
    let source = input.source.trim();
    let source = if source.is_empty() {
        "forge_cli"
    } else {
        source
    };
    let (left, right) = canonical_identity_pair(&left_scope, &left_id, &right_scope, &right_id);
    let link_id = identity_link_id(&left.0, &left.1, &right.0, &right.1);

    let left_identity = ensure_identity_record_for_link(
        store,
        &left_scope,
        &left_id,
        source,
        input.reason.as_deref(),
    )?;
    let right_identity = ensure_identity_record_for_link(
        store,
        &right_scope,
        &right_id,
        source,
        input.reason.as_deref(),
    )?;
    let data = json!({
        "schema_version": IDENTITY_LINK_SCHEMA_VERSION,
        "requested_left": {
            "scope": left_scope,
            "id": left_id,
        },
        "requested_right": {
            "scope": right_scope,
            "id": right_id,
        },
        "canonical_left": {
            "scope": left.0,
            "id": left.1,
        },
        "canonical_right": {
            "scope": right.0,
            "id": right.1,
        },
        "reason": input.reason,
    });
    store.save_identity_link(
        &link_id, &left.0, &left.1, &right.0, &right.1, &link_type, "active", source, &data,
    )?;
    let link = store
        .list_identity_links(Some(&left.0), Some(&left.1), Some("active"))?
        .into_iter()
        .find(|record| record.id == link_id)
        .map(identity_link_view_from_record)
        .context("identity link was not found after save")?;
    let resolved = resolve_identity(store, &left_scope, &left_id)?;
    Ok(IdentityLinkReport {
        schema_version: IDENTITY_LINK_SCHEMA_VERSION.to_string(),
        status: "identity_linked".to_string(),
        link,
        left_identity,
        right_identity,
        resolved,
    })
}

pub fn unlink_identity(store: &ForgeStore, input: IdentityLinkInput) -> Result<IdentityLinkReport> {
    let left_scope = normalize_required_identity_part("left_scope", &input.left_scope)?;
    let left_id = normalize_required_identity_part("left_id", &input.left_id)?;
    let right_scope = normalize_required_identity_part("right_scope", &input.right_scope)?;
    let right_id = normalize_required_identity_part("right_id", &input.right_id)?;
    if left_scope == right_scope && left_id == right_id {
        bail!("identity unlink requires two different identities");
    }
    let source = input.source.trim();
    let source = if source.is_empty() {
        "forge_cli"
    } else {
        source
    };
    let (left, right) = canonical_identity_pair(&left_scope, &left_id, &right_scope, &right_id);
    let link_id = identity_link_id(&left.0, &left.1, &right.0, &right.1);
    let existing = store
        .list_identity_links(Some(&left.0), Some(&left.1), None)?
        .into_iter()
        .find(|record| record.id == link_id)
        .with_context(|| format!("identity link not found: {link_id}"))?;
    let data = json!({
        "schema_version": IDENTITY_LINK_SCHEMA_VERSION,
        "previous": existing.data,
        "unlink_reason": input.reason,
        "unlinked_by": source,
    });
    let updated = store.update_identity_link_status(&link_id, "unlinked", source, &data)?;
    if !updated {
        bail!("identity link not found: {link_id}");
    }
    let link = store
        .list_identity_links(Some(&left.0), Some(&left.1), Some("unlinked"))?
        .into_iter()
        .find(|record| record.id == link_id)
        .map(identity_link_view_from_record)
        .context("identity link was not found after unlink")?;
    let left_identity = identity_registry_view_for_key(store, &left_scope, &left_id)?;
    let right_identity = identity_registry_view_for_key(store, &right_scope, &right_id)?;
    let resolved = resolve_identity(store, &left_scope, &left_id)?;
    Ok(IdentityLinkReport {
        schema_version: IDENTITY_LINK_SCHEMA_VERSION.to_string(),
        status: "identity_unlinked".to_string(),
        link,
        left_identity,
        right_identity,
        resolved,
    })
}

pub fn list_identity_links(
    store: &ForgeStore,
    scope: Option<&str>,
    id: Option<&str>,
    status: Option<&str>,
) -> Result<IdentityLinksReport> {
    let links = store
        .list_identity_links(scope, id, status)?
        .into_iter()
        .map(identity_link_view_from_record)
        .collect::<Vec<_>>();
    Ok(IdentityLinksReport {
        schema_version: IDENTITY_LINKS_SCHEMA_VERSION.to_string(),
        status: "identity_links_loaded".to_string(),
        link_count: links.len(),
        links,
    })
}

pub fn resolve_identity(
    store: &ForgeStore,
    scope: &str,
    id: &str,
) -> Result<IdentityResolveReport> {
    let scope = normalize_required_identity_part("scope", scope)?;
    let id = normalize_required_identity_part("id", id)?;
    let keys = resolved_identity_keys(store, &scope, &id)?;
    let key_set = keys.iter().cloned().collect::<BTreeSet<_>>();
    let links = store
        .list_identity_links(None, None, Some("active"))?
        .into_iter()
        .filter(|record| {
            key_set.contains(&(record.left_scope.clone(), record.left_id.clone()))
                && key_set.contains(&(record.right_scope.clone(), record.right_id.clone()))
        })
        .map(identity_link_view_from_record)
        .collect::<Vec<_>>();
    let identities = keys
        .iter()
        .map(|(scope, id)| identity_alias_view_for_key(store, scope, id))
        .collect::<Result<Vec<_>>>()?;
    let canonical_identity = identities
        .iter()
        .cloned()
        .min_by_key(|identity| {
            (
                canonical_identity_priority(&identity.scope),
                identity.scope.clone(),
                identity.id.clone(),
            )
        })
        .unwrap_or_else(|| IdentityAliasView {
            scope: scope.clone(),
            id: id.clone(),
            label: format!("{scope}:{id}"),
            source: "unregistered".to_string(),
            data: json!({
                "schema_version": IDENTITY_RESOLVE_SCHEMA_VERSION,
                "registered": false,
            }),
            updated_at: String::new(),
        });
    Ok(IdentityResolveReport {
        schema_version: IDENTITY_RESOLVE_SCHEMA_VERSION.to_string(),
        status: "identity_resolved".to_string(),
        requested_scope: scope,
        requested_id: id,
        canonical_identity,
        identity_count: identities.len(),
        link_count: links.len(),
        identities,
        links,
    })
}

pub fn sync_project_operating_context(
    store: &ForgeStore,
    project_root: &Path,
) -> Result<IdentitySyncReport> {
    let context = inspect_project_operating_context(project_root)?;
    let identity_refs = context_identity_refs(&context.context);
    for identity in &identity_refs {
        store.save_identity_record(
            &identity.scope,
            &identity.id,
            &identity.label,
            &context.source,
            &identity_data(&context.context, identity),
        )?;
    }
    store.save_identity_membership(
        &context.context.user.scope,
        &context.context.user.id,
        &context.context.organization.id,
        &context.context.brand.id,
        &context.context.product.id,
        "operator",
        "active",
        &context.source,
        &membership_data(&context.context, "operator"),
    )?;
    let identities = store
        .list_identity_records(None, None)?
        .into_iter()
        .filter(|record| {
            identity_refs
                .iter()
                .any(|identity| identity.scope == record.scope && identity.id == record.id)
        })
        .map(identity_view_from_record)
        .collect::<Vec<_>>();
    let memberships = store
        .list_identity_memberships(
            Some(&context.context.user.scope),
            Some(&context.context.user.id),
            Some(&context.context.organization.id),
            Some(&context.context.brand.id),
            Some(&context.context.product.id),
            Some("active"),
        )?
        .into_iter()
        .map(membership_view_from_record)
        .collect::<Vec<_>>();
    Ok(IdentitySyncReport {
        schema_version: IDENTITY_SYNC_SCHEMA_VERSION.to_string(),
        status: "identity_registry_synced".to_string(),
        synced_count: identities.len(),
        membership_count: memberships.len(),
        source: context.source.clone(),
        project_root: context.project_root.clone(),
        identities,
        memberships,
        context,
    })
}

pub fn list_tenant_index(
    store: &ForgeStore,
    resource_type: Option<&str>,
    organization_id: Option<&str>,
    brand_id: Option<&str>,
    product_id: Option<&str>,
    workflow_id: Option<&str>,
) -> Result<TenantIndexReport> {
    let resources = store
        .list_tenant_index(
            resource_type,
            organization_id,
            brand_id,
            product_id,
            workflow_id,
        )?
        .into_iter()
        .map(tenant_index_view_from_record)
        .collect::<Vec<_>>();
    Ok(TenantIndexReport {
        schema_version: TENANT_INDEX_SCHEMA_VERSION.to_string(),
        status: "tenant_index_loaded".to_string(),
        resource_count: resources.len(),
        resources,
    })
}

pub fn audit_tenant_index(store: &ForgeStore) -> Result<TenantAuditReport> {
    let mut expected = Vec::new();
    for workflow in store.load_workflows()? {
        expected.push((
            "workflow".to_string(),
            workflow.id.clone(),
            workflow.id.clone(),
            "workflow row should carry tenant context".to_string(),
        ));
        for artifact in &workflow.artifacts {
            expected.push((
                "artifact".to_string(),
                artifact.id.clone(),
                workflow.id.clone(),
                "workflow artifact should carry tenant context".to_string(),
            ));
        }
        for event in store.load_workflow_events(&workflow.id)? {
            expected.push((
                "event".to_string(),
                event.id.to_string(),
                workflow.id.clone(),
                "workflow event should carry tenant context".to_string(),
            ));
        }
    }

    for run in store.load_runs()? {
        let workflow_id = run
            .get("workflow_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let run_id = run
            .get("run_id")
            .and_then(serde_json::Value::as_str)
            .or_else(|| run.get("id").and_then(serde_json::Value::as_str))
            .unwrap_or_default();
        if !workflow_id.is_empty() && !run_id.is_empty() {
            expected.push((
                "run".to_string(),
                run_id.to_string(),
                workflow_id.to_string(),
                "async run should carry tenant context".to_string(),
            ));
        }
    }

    let indexed = store
        .list_tenant_index(None, None, None, None, None)?
        .into_iter()
        .map(|record| (record.resource_type, record.resource_id))
        .collect::<BTreeSet<_>>();
    let missing_resources = expected
        .iter()
        .filter(|(resource_type, resource_id, _workflow_id, _reason)| {
            !indexed.contains(&(resource_type.clone(), resource_id.clone()))
        })
        .map(
            |(resource_type, resource_id, workflow_id, reason)| TenantAuditMissingResource {
                resource_type: resource_type.clone(),
                resource_id: resource_id.clone(),
                workflow_id: workflow_id.clone(),
                reason: reason.clone(),
            },
        )
        .collect::<Vec<_>>();

    Ok(TenantAuditReport {
        schema_version: TENANT_AUDIT_SCHEMA_VERSION.to_string(),
        status: if missing_resources.is_empty() {
            "tenant_index_complete".to_string()
        } else {
            "tenant_index_missing_resources".to_string()
        },
        expected_resource_count: expected.len(),
        indexed_resource_count: indexed.len(),
        missing_count: missing_resources.len(),
        missing_resources,
    })
}

pub fn evaluate_tenant_policy(
    store: &ForgeStore,
    workflow_id: &str,
    mode: &str,
) -> Result<TenantPolicyReport> {
    evaluate_tenant_policy_for_action(store, workflow_id, mode, "tenant policy")
}

pub fn evaluate_tenant_policy_for_action(
    store: &ForgeStore,
    workflow_id: &str,
    mode: &str,
    action: &str,
) -> Result<TenantPolicyReport> {
    let workflow = store.load_workflow(workflow_id)?;
    let context = &workflow.intent.operating_context;
    let mode = if mode == "enforce" {
        "enforce"
    } else {
        "audit"
    };
    let required_permission = required_permission_for_action(action);
    let active_memberships = list_active_memberships_for_resolved_identity(
        store,
        &context.user.scope,
        &context.user.id,
        &context.organization.id,
        &context.brand.id,
        &context.product.id,
    )?;
    let memberships = active_memberships
        .iter()
        .filter(|membership| membership_is_current(membership))
        .collect::<Vec<_>>();
    let expired_membership_count = active_memberships
        .iter()
        .filter(|membership| membership_expired(membership))
        .count();
    let not_yet_valid_membership_count = active_memberships
        .iter()
        .filter(|membership| membership_not_yet_valid(membership))
        .count();
    let membership_roles = memberships
        .iter()
        .map(|membership| membership.role.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let granted_permissions = memberships
        .iter()
        .flat_map(|membership| effective_membership_permissions(membership))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let denied_permissions = memberships
        .iter()
        .flat_map(|membership| json_string_array(&membership.data, "permission_denies"))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let membership_has_permission = memberships.iter().any(|membership| {
        membership_grants_permission(membership, &required_permission)
            && !membership_denies_permission(membership, &required_permission)
    });

    let tenant_rows = store.list_tenant_index(
        None,
        Some(&context.organization.id),
        Some(&context.brand.id),
        Some(&context.product.id),
        Some(&workflow.id),
    )?;
    let indexed = tenant_rows
        .iter()
        .map(|record| (record.resource_type.clone(), record.resource_id.clone()))
        .collect::<BTreeSet<_>>();
    let expected = expected_resources_for_workflow(store, workflow_id)?;
    let missing_tenant_index_count = expected
        .iter()
        .filter(|(resource_type, resource_id)| {
            !indexed.contains(&(resource_type.clone(), resource_id.clone()))
        })
        .count();

    let mut decisions = Vec::new();
    let explicit_context = has_explicit_operating_context(context);
    decisions.push(TenantPolicyDecision {
        gate: "explicit_operating_context".to_string(),
        status: if explicit_context {
            "allowed"
        } else {
            "denied"
        }
        .to_string(),
        reason: if explicit_context {
            "workflow carries explicit organization, brand, product, user and channel context"
                .to_string()
        } else {
            "workflow still depends on default or anonymous operating context".to_string()
        },
    });

    decisions.push(TenantPolicyDecision {
        gate: "active_membership".to_string(),
        status: if active_memberships.is_empty() {
            "denied"
        } else {
            "allowed"
        }
        .to_string(),
        reason: if active_memberships.is_empty() {
            format!(
                "no active membership for user {} in {}/{}/{}",
                context.user.id, context.organization.id, context.brand.id, context.product.id
            )
        } else {
            format!(
                "{} active membership(s) authorize user {} for {}/{}/{}",
                active_memberships.len(),
                context.user.id,
                context.organization.id,
                context.brand.id,
                context.product.id
            )
        },
    });

    decisions.push(TenantPolicyDecision {
        gate: "membership_validity".to_string(),
        status: if memberships.is_empty() {
            "denied"
        } else {
            "allowed"
        }
        .to_string(),
        reason: if memberships.is_empty() {
            format!(
                "no active membership for user {} is currently valid; expired={}, not_yet_valid={}",
                context.user.id, expired_membership_count, not_yet_valid_membership_count
            )
        } else {
            format!(
                "{} active membership(s) are currently valid for user {}",
                memberships.len(),
                context.user.id
            )
        },
    });

    decisions.push(TenantPolicyDecision {
        gate: "membership_permission".to_string(),
        status: if membership_has_permission {
            "allowed"
        } else {
            "denied"
        }
        .to_string(),
        reason: if membership_has_permission {
            format!(
                "active membership role(s) [{}] grant permission {} for action {action}",
                membership_roles.join(", "),
                required_permission
            )
        } else if memberships.is_empty() {
            format!(
                "no currently valid active membership can grant permission {} for action {action}",
                required_permission
            )
        } else {
            format!(
                "active membership role(s) [{}] do not grant permission {} for action {action}",
                membership_roles.join(", "),
                required_permission
            )
        },
    });

    decisions.push(TenantPolicyDecision {
        gate: "tenant_index_coverage".to_string(),
        status: if missing_tenant_index_count == 0 {
            "allowed"
        } else {
            "denied"
        }
        .to_string(),
        reason: if missing_tenant_index_count == 0 {
            "workflow, run, artifact and event resources are indexed for this tenant".to_string()
        } else {
            format!(
                "{missing_tenant_index_count} workflow resource(s) are missing tenant_index rows"
            )
        },
    });

    let allowed = decisions
        .iter()
        .all(|decision| decision.status == "allowed");
    Ok(TenantPolicyReport {
        schema_version: TENANT_POLICY_SCHEMA_VERSION.to_string(),
        status: if allowed {
            "tenant_policy_allowed".to_string()
        } else if mode == "enforce" {
            "tenant_policy_denied".to_string()
        } else {
            "tenant_policy_would_deny".to_string()
        },
        mode: mode.to_string(),
        allowed,
        action: action.to_string(),
        required_permission,
        workflow_id: workflow.id,
        organization_id: context.organization.id.clone(),
        brand_id: context.brand.id.clone(),
        product_id: context.product.id.clone(),
        user_id: context.user.id.clone(),
        channel_id: context.channel.id.clone(),
        membership_count: memberships.len(),
        active_membership_count: active_memberships.len(),
        expired_membership_count,
        not_yet_valid_membership_count,
        membership_roles,
        granted_permissions,
        denied_permissions,
        indexed_resource_count: tenant_rows.len(),
        missing_tenant_index_count,
        decisions,
    })
}

pub fn ensure_operating_context_policy(
    store: &ForgeStore,
    context: &OperatingContextSpec,
    action: &str,
) -> Result<()> {
    if context.tenant_policy_mode != "enforce" {
        return Ok(());
    }
    if !has_explicit_operating_context(context) {
        anyhow::bail!(
            "multi-tenant enforcement blocked {action}: organization, brand, product, user and channel must be explicit"
        );
    }
    let active_memberships = list_active_memberships_for_resolved_identity(
        store,
        &context.user.scope,
        &context.user.id,
        &context.organization.id,
        &context.brand.id,
        &context.product.id,
    )?;
    let memberships = active_memberships
        .iter()
        .filter(|membership| membership_is_current(membership))
        .collect::<Vec<_>>();
    if active_memberships.is_empty() {
        anyhow::bail!(
            "multi-tenant enforcement blocked {action}: no active membership for user {} in {}/{}/{}",
            context.user.id,
            context.organization.id,
            context.brand.id,
            context.product.id
        );
    }
    if memberships.is_empty() {
        anyhow::bail!(
            "multi-tenant enforcement blocked {action}: active membership for user {} is expired or not yet valid in {}/{}/{}",
            context.user.id,
            context.organization.id,
            context.brand.id,
            context.product.id
        );
    }
    let required_permission = required_permission_for_action(action);
    let membership_has_permission = memberships.iter().any(|membership| {
        membership_grants_permission(membership, &required_permission)
            && !membership_denies_permission(membership, &required_permission)
    });
    if !membership_has_permission {
        anyhow::bail!(
            "multi-tenant enforcement blocked {action}: active membership for user {} does not grant permission {}",
            context.user.id,
            required_permission
        );
    }
    Ok(())
}

pub fn ensure_workflow_policy(store: &ForgeStore, workflow_id: &str, action: &str) -> Result<()> {
    let workflow = store.load_workflow(workflow_id)?;
    if workflow.intent.operating_context.tenant_policy_mode != "enforce" {
        return Ok(());
    }
    let report = evaluate_tenant_policy_for_action(store, workflow_id, "enforce", action)?;
    if report.allowed {
        return Ok(());
    }
    let denied_gates = report
        .decisions
        .iter()
        .filter(|decision| decision.status != "allowed")
        .map(|decision| format!("{}: {}", decision.gate, decision.reason))
        .collect::<Vec<_>>()
        .join("; ");
    anyhow::bail!(
        "multi-tenant enforcement blocked {action}: workflow {workflow_id} failed tenant policy ({denied_gates})"
    );
}

pub fn inspect_project_operating_context(
    project_root: &Path,
) -> Result<OperatingContextLoadReport> {
    let project_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let Some(path) = find_operating_context_file(&project_root) else {
        return Ok(OperatingContextLoadReport {
            schema_version: OPERATING_CONTEXT_LOAD_SCHEMA_VERSION.to_string(),
            status: "default_context".to_string(),
            source: "built_in_defaults".to_string(),
            project_root: project_root.display().to_string(),
            context: OperatingContextSpec::default(),
            warnings: vec![
                "No .forge/operating-context.yaml, .yml or .json file was found".to_string(),
            ],
        });
    };

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read operating context file {}", path.display()))?;
    let value: serde_yaml::Value = serde_yaml::from_str(&content)
        .with_context(|| format!("invalid operating context file {}", path.display()))?;
    let context_value = nested_operating_context_value(&value).unwrap_or(value);
    let context: OperatingContextSpec = serde_yaml::from_value(context_value)
        .with_context(|| format!("invalid operating context payload {}", path.display()))?;

    Ok(OperatingContextLoadReport {
        schema_version: OPERATING_CONTEXT_LOAD_SCHEMA_VERSION.to_string(),
        status: "loaded".to_string(),
        source: path.display().to_string(),
        project_root: project_root.display().to_string(),
        context,
        warnings: Vec::new(),
    })
}

fn context_identity_refs(context: &OperatingContextSpec) -> Vec<ContextIdentityRef> {
    vec![
        context.organization.clone(),
        context.brand.clone(),
        context.product.clone(),
        context.user.clone(),
        context.channel.clone(),
    ]
}

fn identity_data(
    context: &OperatingContextSpec,
    identity: &ContextIdentityRef,
) -> serde_json::Value {
    json!({
        "scope": identity.scope,
        "id": identity.id,
        "label": identity.label,
        "organization_id": context.organization.id,
        "brand_id": context.brand.id,
        "product_id": context.product.id,
        "user_id": context.user.id,
        "channel_id": context.channel.id,
        "memory_scope": context.memory_scope,
        "personality_scope": context.personality_scope,
        "tenant_policy_mode": context.tenant_policy_mode,
        "brand_identity": context.brand_identity,
        "design_system": context.design_system,
        "operating_policy": context.operating_policy,
    })
}

fn membership_data(context: &OperatingContextSpec, role: &str) -> serde_json::Value {
    json!({
        "role_permissions_schema_version": IDENTITY_MEMBERSHIP_PERMISSION_SCHEMA_VERSION,
        "subject_scope": context.user.scope,
        "subject_id": context.user.id,
        "organization_id": context.organization.id,
        "brand_id": context.brand.id,
        "product_id": context.product.id,
        "channel_id": context.channel.id,
        "role": role,
        "role_default_permissions": role_default_permissions(role),
        "environments": membership_environments_from_context(context),
        "memory_scope": context.memory_scope,
        "personality_scope": context.personality_scope,
        "tenant_policy_mode": context.tenant_policy_mode,
        "brand_identity": context.brand_identity,
        "design_system": context.design_system,
        "operating_policy": context.operating_policy,
        "source": "operating_context",
    })
}

fn identity_view_from_record(record: StoredIdentityRecord) -> IdentityRegistryView {
    IdentityRegistryView {
        scope: record.scope,
        id: record.id,
        label: record.label,
        source: record.source,
        updated_at: record.updated_at,
        data: record.data,
    }
}

fn identity_link_view_from_record(record: StoredIdentityLinkRecord) -> IdentityLinkView {
    IdentityLinkView {
        id: record.id,
        left_scope: record.left_scope,
        left_id: record.left_id,
        right_scope: record.right_scope,
        right_id: record.right_id,
        link_type: record.link_type,
        status: record.status,
        source: record.source,
        created_at: record.created_at,
        updated_at: record.updated_at,
        data: record.data,
    }
}

fn identity_alias_view_for_key(
    store: &ForgeStore,
    scope: &str,
    id: &str,
) -> Result<IdentityAliasView> {
    let record = store
        .list_identity_records(Some(scope), Some(id))?
        .into_iter()
        .next();
    Ok(match record {
        Some(record) => IdentityAliasView {
            scope: record.scope,
            id: record.id,
            label: record.label,
            source: record.source,
            data: record.data,
            updated_at: record.updated_at,
        },
        None => IdentityAliasView {
            scope: scope.to_string(),
            id: id.to_string(),
            label: format!("{scope}:{id}"),
            source: "unregistered".to_string(),
            data: json!({
                "schema_version": IDENTITY_RESOLVE_SCHEMA_VERSION,
                "registered": false,
            }),
            updated_at: String::new(),
        },
    })
}

fn identity_registry_view_for_key(
    store: &ForgeStore,
    scope: &str,
    id: &str,
) -> Result<IdentityRegistryView> {
    let record = store
        .list_identity_records(Some(scope), Some(id))?
        .into_iter()
        .next()
        .with_context(|| format!("identity not found in registry: {scope}:{id}"))?;
    Ok(identity_view_from_record(record))
}

fn ensure_identity_record_for_link(
    store: &ForgeStore,
    scope: &str,
    id: &str,
    source: &str,
    reason: Option<&str>,
) -> Result<IdentityRegistryView> {
    if let Some(record) = store
        .list_identity_records(Some(scope), Some(id))?
        .into_iter()
        .next()
    {
        return Ok(identity_view_from_record(record));
    }
    let label = format!("{scope}:{id}");
    store.save_identity_record(
        scope,
        id,
        &label,
        source,
        &json!({
            "schema_version": IDENTITY_LINK_SCHEMA_VERSION,
            "scope": scope,
            "id": id,
            "label": label,
            "source": source,
            "reason": reason,
            "registered_by": "identity_link",
        }),
    )?;
    identity_registry_view_for_key(store, scope, id)
}

fn resolved_identity_keys(
    store: &ForgeStore,
    scope: &str,
    id: &str,
) -> Result<Vec<(String, String)>> {
    let mut keys = BTreeSet::new();
    keys.insert((scope.to_string(), id.to_string()));
    let links = store.list_identity_links(None, None, Some("active"))?;
    let mut changed = true;
    while changed {
        changed = false;
        for link in &links {
            let left = (link.left_scope.clone(), link.left_id.clone());
            let right = (link.right_scope.clone(), link.right_id.clone());
            if keys.contains(&left) && keys.insert(right.clone()) {
                changed = true;
            }
            if keys.contains(&right) && keys.insert(left) {
                changed = true;
            }
        }
    }
    Ok(keys.into_iter().collect())
}

fn list_active_memberships_for_resolved_identity(
    store: &ForgeStore,
    subject_scope: &str,
    subject_id: &str,
    organization_id: &str,
    brand_id: &str,
    product_id: &str,
) -> Result<Vec<StoredIdentityMembershipRecord>> {
    let mut memberships = Vec::new();
    let mut seen = BTreeSet::new();
    for (scope, id) in resolved_identity_keys(store, subject_scope, subject_id)? {
        for membership in store.list_identity_memberships(
            Some(&scope),
            Some(&id),
            Some(organization_id),
            Some(brand_id),
            Some(product_id),
            Some("active"),
        )? {
            let key = (
                membership.subject_scope.clone(),
                membership.subject_id.clone(),
                membership.organization_id.clone(),
                membership.brand_id.clone(),
                membership.product_id.clone(),
            );
            if seen.insert(key) {
                memberships.push(membership);
            }
        }
    }
    Ok(memberships)
}

fn canonical_identity_pair(
    left_scope: &str,
    left_id: &str,
    right_scope: &str,
    right_id: &str,
) -> ((String, String), (String, String)) {
    let left = (left_scope.to_string(), left_id.to_string());
    let right = (right_scope.to_string(), right_id.to_string());
    if left <= right {
        (left, right)
    } else {
        (right, left)
    }
}

fn identity_link_id(left_scope: &str, left_id: &str, right_scope: &str, right_id: &str) -> String {
    let digest = hex_sha256(format!("{left_scope}:{left_id}->{right_scope}:{right_id}").as_bytes());
    format!("identity-link-{}", &digest[..16])
}

fn normalize_required_identity_part(name: &str, value: &str) -> Result<String> {
    let normalized = value.trim();
    if normalized.is_empty() {
        bail!("{name} is required");
    }
    Ok(normalized.to_string())
}

fn normalize_link_type(link_type: &str) -> String {
    let normalized = link_type.trim();
    if normalized.is_empty() {
        "same_person".to_string()
    } else {
        normalized.to_string()
    }
}

fn canonical_identity_priority(scope: &str) -> u8 {
    match scope {
        "user" | "person" => 0,
        "web" | "account" => 1,
        "email" => 2,
        "telegram" => 3,
        "discord" => 4,
        "whatsapp" => 5,
        "phone" | "sms" => 6,
        "channel" => 7,
        _ => 20,
    }
}

fn membership_view_from_record(record: StoredIdentityMembershipRecord) -> IdentityMembershipView {
    let permissions = effective_membership_permissions(&record);
    let environments = membership_environments(&record.data);
    let permission_grants = json_string_array(&record.data, "permission_grants");
    let permission_denies = json_string_array(&record.data, "permission_denies");
    let expires_at = membership_timestamp(&record.data, "expires_at");
    let not_before = membership_timestamp(&record.data, "not_before")
        .or_else(|| membership_timestamp(&record.data, "valid_from"));
    let expired = membership_expired(&record);
    let not_yet_valid = membership_not_yet_valid(&record);
    IdentityMembershipView {
        subject_scope: record.subject_scope,
        subject_id: record.subject_id,
        organization_id: record.organization_id,
        brand_id: record.brand_id,
        product_id: record.product_id,
        role: record.role,
        permissions,
        permission_grants,
        permission_denies,
        expires_at,
        not_before,
        expired,
        not_yet_valid,
        environments,
        status: record.status,
        source: record.source,
        updated_at: record.updated_at,
        data: record.data,
    }
}

fn apply_permission_list_change(
    data: &mut serde_json::Value,
    key: &str,
    permissions: &[String],
    add: bool,
    changes: &mut Vec<String>,
    label: &str,
) {
    let mut set = json_string_array(data, key)
        .into_iter()
        .collect::<BTreeSet<_>>();
    for permission in permissions {
        let permission = permission.trim();
        if permission.is_empty() {
            continue;
        }
        let changed = if add {
            set.insert(permission.to_string())
        } else {
            set.remove(permission)
        };
        if changed {
            changes.push(format!("{label}:{permission}"));
        }
    }
    data[key] = serde_json::Value::Array(set.into_iter().map(serde_json::Value::String).collect());
}

fn validate_rfc3339_timestamp(field: &str, value: &str) -> Result<()> {
    DateTime::parse_from_rfc3339(value)
        .with_context(|| format!("{field} must be an RFC3339 timestamp"))?;
    Ok(())
}

fn tenant_index_view_from_record(record: TenantIndexRecord) -> TenantIndexView {
    TenantIndexView {
        resource_type: record.resource_type,
        resource_id: record.resource_id,
        workflow_id: record.workflow_id,
        organization_id: record.organization_id,
        brand_id: record.brand_id,
        product_id: record.product_id,
        user_id: record.user_id,
        channel_id: record.channel_id,
        memory_scope: record.memory_scope,
        personality_scope: record.personality_scope,
        source: record.source,
        updated_at: record.updated_at,
        data: record.data,
    }
}

fn expected_resources_for_workflow(
    store: &ForgeStore,
    workflow_id: &str,
) -> Result<Vec<(String, String)>> {
    let workflow = store.load_workflow(workflow_id)?;
    let mut expected = vec![("workflow".to_string(), workflow.id.clone())];
    for artifact in &workflow.artifacts {
        expected.push(("artifact".to_string(), artifact.id.clone()));
    }
    for event in store.load_workflow_events(workflow_id)? {
        expected.push(("event".to_string(), event.id.to_string()));
    }
    for run in store.load_runs()? {
        let run_workflow_id = run
            .get("workflow_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        if run_workflow_id != workflow_id {
            continue;
        }
        let run_id = run
            .get("run_id")
            .and_then(serde_json::Value::as_str)
            .or_else(|| run.get("id").and_then(serde_json::Value::as_str))
            .unwrap_or_default();
        if !run_id.is_empty() {
            expected.push(("run".to_string(), run_id.to_string()));
        }
    }
    Ok(expected)
}

fn has_explicit_operating_context(context: &OperatingContextSpec) -> bool {
    ![
        context.organization.id.as_str(),
        context.brand.id.as_str(),
        context.product.id.as_str(),
        context.user.id.as_str(),
        context.channel.id.as_str(),
    ]
    .iter()
    .any(|id| id.is_empty() || *id == "default" || id.starts_with("default-"))
        && context.user.id != "anonymous"
}

fn required_permission_for_action(action: &str) -> String {
    let normalized = action.to_ascii_lowercase();
    if normalized == "plan"
        || normalized == "request start"
        || normalized.contains("start workflow")
    {
        "workflow:create"
    } else if normalized.contains("context request")
        || normalized.contains("tenant policy")
        || normalized.contains("status")
        || normalized.contains("inspect")
        || normalized.contains("list")
    {
        "context:read"
    } else if normalized.contains("handoff")
        || normalized.contains("lease")
        || normalized.contains("checkpoint")
        || normalized.contains("drive")
        || normalized.contains("step")
        || normalized.contains("resume")
        || normalized.contains("recover")
        || normalized.contains("cancel")
        || normalized.contains("switch")
    {
        "workflow:execute"
    } else if normalized.contains("final audit")
        || normalized.contains("final-package")
        || normalized.contains("deliver")
        || normalized.contains("delivery")
    {
        "workflow:deliver"
    } else if normalized.contains("schedule")
        || normalized.contains("loop state")
        || normalized.contains("run due")
    {
        "schedule:manage"
    } else if normalized.contains("human interaction") || normalized.contains("interaction") {
        "human:interact"
    } else if normalized.contains("patch") {
        "patch:apply"
    } else if normalized.contains("ops modifier") {
        "ops:modify"
    } else if normalized.contains("product decision") {
        "workflow:governance"
    } else if normalized.contains("addon") {
        "addon:manage"
    } else {
        "workflow:mutate"
    }
    .to_string()
}

fn effective_membership_permissions(record: &StoredIdentityMembershipRecord) -> Vec<String> {
    let mut permissions = role_default_permissions(&record.role)
        .into_iter()
        .chain(json_string_array(&record.data, "permission_grants"))
        .collect::<BTreeSet<_>>();
    for denied in json_string_array(&record.data, "permission_denies") {
        permissions
            .retain(|permission| permission != &denied && !permission_matches(&denied, permission));
    }
    permissions.into_iter().collect()
}

fn membership_grants_permission(record: &StoredIdentityMembershipRecord, required: &str) -> bool {
    effective_membership_permissions(record)
        .iter()
        .any(|permission| permission_matches(permission, required))
}

fn membership_denies_permission(record: &StoredIdentityMembershipRecord, required: &str) -> bool {
    json_string_array(&record.data, "permission_denies")
        .iter()
        .any(|permission| permission_matches(permission, required))
}

fn membership_is_current(record: &StoredIdentityMembershipRecord) -> bool {
    !membership_expired(record) && !membership_not_yet_valid(record)
}

fn membership_expired(record: &StoredIdentityMembershipRecord) -> bool {
    membership_timestamp(&record.data, "expires_at")
        .and_then(|value| DateTime::parse_from_rfc3339(&value).ok())
        .is_some_and(|expires_at| expires_at.with_timezone(&Utc) <= Utc::now())
}

fn membership_not_yet_valid(record: &StoredIdentityMembershipRecord) -> bool {
    membership_timestamp(&record.data, "not_before")
        .or_else(|| membership_timestamp(&record.data, "valid_from"))
        .and_then(|value| DateTime::parse_from_rfc3339(&value).ok())
        .is_some_and(|not_before| not_before.with_timezone(&Utc) > Utc::now())
}

fn membership_timestamp(data: &serde_json::Value, key: &str) -> Option<String> {
    data.get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn role_default_permissions(role: &str) -> Vec<String> {
    let permissions = match role.to_ascii_lowercase().as_str() {
        "owner" | "admin" => return all_permissions(),
        "operator" => vec![
            "addon:manage",
            "context:read",
            "human:interact",
            "identity:sync",
            "ops:modify",
            "patch:apply",
            "schedule:manage",
            "workflow:create",
            "workflow:deliver",
            "workflow:execute",
            "workflow:governance",
            "workflow:mutate",
            "workflow:read",
        ],
        "executor" | "agent" => vec![
            "context:read",
            "human:interact",
            "workflow:deliver",
            "workflow:execute",
            "workflow:read",
        ],
        "viewer" | "auditor" => vec!["context:read", "workflow:read"],
        "billing" => vec!["billing:manage", "context:read"],
        _ => vec!["context:read"],
    };
    permissions.into_iter().map(str::to_string).collect()
}

fn all_permissions() -> Vec<String> {
    vec![
        "addon:manage",
        "billing:manage",
        "context:read",
        "human:interact",
        "identity:sync",
        "ops:modify",
        "patch:apply",
        "schedule:manage",
        "tenant:*",
        "workflow:create",
        "workflow:deliver",
        "workflow:execute",
        "workflow:governance",
        "workflow:mutate",
        "workflow:read",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn permission_matches(permission: &str, required_permission: &str) -> bool {
    if permission == required_permission || permission == "tenant:*" || permission == "*" {
        return true;
    }
    permission
        .strip_suffix(":*")
        .is_some_and(|prefix| required_permission.starts_with(&format!("{prefix}:")))
}

fn membership_environments_from_context(context: &OperatingContextSpec) -> Vec<String> {
    if context.channel.id.contains("local") || context.channel.id.contains("cli") {
        vec!["local".to_string()]
    } else {
        vec![context.channel.id.clone()]
    }
}

fn membership_environments(data: &serde_json::Value) -> Vec<String> {
    let environments = json_string_array(data, "environments");
    if environments.is_empty() {
        vec!["default".to_string()]
    } else {
        environments
    }
}

fn json_string_array(data: &serde_json::Value, key: &str) -> Vec<String> {
    data.get(key)
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn find_operating_context_file(project_root: &Path) -> Option<PathBuf> {
    [
        ".forge/operating-context.yaml",
        ".forge/operating-context.yml",
        ".forge/operating-context.json",
    ]
    .into_iter()
    .map(|relative| project_root.join(relative))
    .find(|path| path.is_file())
}

fn nested_operating_context_value(value: &serde_yaml::Value) -> Option<serde_yaml::Value> {
    let serde_yaml::Value::Mapping(mapping) = value else {
        return None;
    };
    mapping
        .get(serde_yaml::Value::String("operating_context".to_string()))
        .cloned()
}
