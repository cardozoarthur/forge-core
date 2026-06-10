use crate::checkpoint::TaskCheckpoint;
use crate::graph::Workflow;
use crate::intent::OperatingContextSpec;
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::{Path, PathBuf};

pub struct ForgeStore {
    path: PathBuf,
    connection: Connection,
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
pub struct InboundEventRecord {
    pub id: String,
    pub origin: String,
    pub action: String,
    pub status: String,
    pub data: serde_json::Value,
    pub created_at: String,
    pub processed_at: Option<String>,
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

impl ForgeStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create store directory {}", parent.display())
            })?;
        }
        let connection = Connection::open(&path)
            .with_context(|| format!("failed to open SQLite store {}", path.display()))?;
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
        let store = Self { path, connection };
        store.migrate()?;
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
            CREATE TABLE IF NOT EXISTS event_inbox (
                id TEXT PRIMARY KEY,
                origin TEXT NOT NULL,
                action TEXT NOT NULL,
                status TEXT NOT NULL,
                data_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                processed_at TEXT
            );
            CREATE TABLE IF NOT EXISTS event_services (
                id TEXT PRIMARY KEY,
                service_kind TEXT NOT NULL,
                status TEXT NOT NULL,
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
            CREATE TABLE IF NOT EXISTS runs (
                id TEXT PRIMARY KEY,
                workflow_id TEXT NOT NULL,
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
            "#,
        )?;
        self.ensure_memory_promotion_tenant_columns()?;
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
        from_scope: Option<&str>,
        to_scope: Option<&str>,
        approved_by: Option<&str>,
        workflow_id: Option<&str>,
        organization_id: Option<&str>,
        brand_id: Option<&str>,
        product_id: Option<&str>,
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
            if workflow_id.is_some_and(|filter| filter != workflow_id_value.as_str()) {
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
            if from_scope.is_some_and(|filter| filter != from_scope_value.as_str()) {
                continue;
            }
            if to_scope.is_some_and(|filter| filter != to_scope_value.as_str()) {
                continue;
            }
            if approved_by.is_some_and(|filter| filter != approved_by_value.as_str()) {
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
        self.insert_global_event(
            "workflow_event",
            &event_id,
            Some(workflow_id),
            kind,
            &extract_event_origin(data),
            "recorded",
            data,
            &tenant_context,
        )?;
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

    pub fn try_save_event_service(&self, service: EventServiceWrite<'_>) -> Result<bool> {
        let changed = self.connection.execute(
            r#"
            INSERT INTO event_services (
                id,
                service_kind,
                status,
                lease_owner,
                lease_id,
                lease_acquired_at,
                lease_expires_at,
                last_heartbeat_at,
                heartbeat_ttl_seconds,
                data_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(id) DO UPDATE SET
                service_kind=excluded.service_kind,
                status=excluded.status,
                lease_owner=excluded.lease_owner,
                lease_id=excluded.lease_id,
                lease_acquired_at=excluded.lease_acquired_at,
                lease_expires_at=excluded.lease_expires_at,
                last_heartbeat_at=excluded.last_heartbeat_at,
                heartbeat_ttl_seconds=excluded.heartbeat_ttl_seconds,
                data_json=excluded.data_json,
                updated_at=CURRENT_TIMESTAMP
            WHERE event_services.lease_expires_at <= ?11
               OR event_services.status IN ('completed', 'completed_with_failures', 'failed', 'stopped')
            "#,
            params![
                service.id,
                service.service_kind,
                service.status,
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
        self.connection.execute(
            r#"
            INSERT INTO event_services (
                id,
                service_kind,
                status,
                lease_owner,
                lease_id,
                lease_acquired_at,
                lease_expires_at,
                last_heartbeat_at,
                heartbeat_ttl_seconds,
                data_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(id) DO UPDATE SET
                service_kind=excluded.service_kind,
                status=excluded.status,
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
                   data_json, created_at, updated_at
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
    ) -> Result<Vec<StoredEventServiceRecord>> {
        let limit = limit.max(1);
        let kind_filter = service_kind
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let status_filter = status.map(str::trim).filter(|value| !value.is_empty());
        let mut statement = self.connection.prepare(
            r#"
            SELECT id, service_kind, status, lease_owner, lease_id, lease_acquired_at,
                   lease_expires_at, last_heartbeat_at, heartbeat_ttl_seconds,
                   data_json, created_at, updated_at
            FROM event_services
            WHERE (?1 IS NULL OR service_kind = ?1)
              AND (?2 IS NULL OR status = ?2)
            ORDER BY updated_at DESC, created_at DESC, id ASC
            LIMIT ?3
            "#,
        )?;
        let rows = statement.query_map(
            params![
                kind_filter,
                status_filter,
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

    fn insert_global_event(
        &self,
        source: &str,
        source_id: &str,
        workflow_id: Option<&str>,
        kind: &str,
        origin: &str,
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
            INSERT INTO global_events (
                source, source_id, workflow_id, kind, origin, status,
                organization_id, brand_id, product_id, user_id, channel_id,
                tenant_context_json, data_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
            params![
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
                serde_json::to_string(tenant_context)?,
                serde_json::to_string(data)?,
            ],
        )?;
        Ok(())
    }

    pub fn record_global_event(
        &self,
        source: &str,
        source_id: &str,
        workflow_id: Option<&str>,
        kind: &str,
        origin: &str,
        status: &str,
        data: &serde_json::Value,
        tenant_context: &serde_json::Value,
    ) -> Result<i64> {
        self.insert_global_event(
            source,
            source_id,
            workflow_id,
            kind,
            origin,
            status,
            data,
            tenant_context,
        )?;
        Ok(self.connection.last_insert_rowid())
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
        addon_id: &str,
        permission_id: &str,
        status: &str,
        risk: &str,
        approved_by: &str,
        source: &str,
        data: &serde_json::Value,
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
                addon_id,
                permission_id,
                status,
                risk,
                approved_by,
                source,
                serde_json::to_string(data)?,
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
            ORDER BY created_at DESC, id DESC
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
            if addon_id.is_some_and(|filter| filter != addon_id_value.as_str()) {
                continue;
            }
            if contract_id.is_some_and(|filter| filter != contract_id_value.as_str()) {
                continue;
            }
            if status.is_some_and(|filter| filter != status_value.as_str()) {
                continue;
            }
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

    pub fn save_identity_membership(
        &self,
        subject_scope: &str,
        subject_id: &str,
        organization_id: &str,
        brand_id: &str,
        product_id: &str,
        role: &str,
        status: &str,
        source: &str,
        data: &serde_json::Value,
    ) -> Result<()> {
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
                subject_scope,
                subject_id,
                organization_id,
                brand_id,
                product_id,
                role,
                status,
                source,
                serde_json::to_string(data)?,
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

    pub fn save_identity_link(
        &self,
        id: &str,
        left_scope: &str,
        left_id: &str,
        right_scope: &str,
        right_id: &str,
        link_type: &str,
        status: &str,
        source: &str,
        data: &serde_json::Value,
    ) -> Result<()> {
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
                id,
                left_scope,
                left_id,
                right_scope,
                right_id,
                link_type,
                status,
                source,
                serde_json::to_string(data)?,
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
    ) -> Result<()> {
        self.connection.execute(
            r#"
            INSERT INTO event_inbox (id, origin, action, status, data_json)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![id, origin, action, status, serde_json::to_string(data)?],
        )?;
        self.insert_global_event(
            "event_inbox",
            id,
            None,
            "inbound_event_ingested",
            origin,
            status,
            &serde_json::json!({
                "event_id": id,
                "origin": origin,
                "action": action,
                "status": status,
                "data": data,
            }),
            &default_operating_context_json(),
        )?;
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
            self.insert_global_event(
                "event_inbox",
                id,
                workflow_id.as_deref(),
                "inbound_event_status_updated",
                &event.origin,
                status,
                &serde_json::json!({
                    "event_id": event.id.clone(),
                    "origin": event.origin.clone(),
                    "action": event.action.clone(),
                    "status": status,
                    "data": data,
                }),
                &default_operating_context_json(),
            )?;
        }
        Ok(())
    }

    pub fn load_inbound_event(&self, id: &str) -> Result<InboundEventRecord> {
        let row: Option<(
            String,
            String,
            String,
            String,
            String,
            String,
            Option<String>,
        )> = self
            .connection
            .query_row(
                r#"
                SELECT id, origin, action, status, data_json, created_at, processed_at
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
                    ))
                },
            )
            .optional()?;
        let (id, origin, action, status, data_json, created_at, processed_at) =
            row.with_context(|| format!("inbound event not found: {id}"))?;
        Ok(InboundEventRecord {
            id,
            origin,
            action,
            status,
            data: serde_json::from_str(&data_json)?,
            created_at,
            processed_at,
        })
    }

    pub fn list_inbound_events(
        &self,
        status: Option<&str>,
        limit: usize,
    ) -> Result<Vec<InboundEventRecord>> {
        let limit = limit.max(1) as i64;
        let query = if status.is_some() {
            r#"
            SELECT id, origin, action, status, data_json, created_at, processed_at
            FROM event_inbox
            WHERE status = ?1
            ORDER BY created_at DESC, id DESC
            LIMIT ?2
            "#
        } else {
            r#"
            SELECT id, origin, action, status, data_json, created_at, processed_at
            FROM event_inbox
            ORDER BY created_at DESC, id DESC
            LIMIT ?1
            "#
        };
        let mut statement = self.connection.prepare(query)?;
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
            })
        };
        let rows = if let Some(status) = status {
            statement.query_map(params![status, limit], map_row)?
        } else {
            statement.query_map(params![limit], map_row)?
        };
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

    pub fn save_run(
        &self,
        id: &str,
        workflow_id: &str,
        status: &str,
        data: &serde_json::Value,
    ) -> Result<()> {
        self.connection.execute(
            r#"
            INSERT INTO runs (id, workflow_id, status, data_json, updated_at)
            VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP)
            ON CONFLICT(id) DO UPDATE SET
                workflow_id=excluded.workflow_id,
                status=excluded.status,
                data_json=excluded.data_json,
                updated_at=CURRENT_TIMESTAMP
            "#,
            params![id, workflow_id, status, serde_json::to_string(data)?],
        )?;
        if let Ok(workflow) = self.load_workflow(workflow_id) {
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

    pub fn try_save_task_lease(&self, lease: TaskLeaseWrite<'_>) -> Result<bool> {
        let changed = self.connection.execute(
            r#"
            INSERT INTO task_leases (
                workflow_id,
                task_id,
                lease_id,
                executor,
                acquired_at,
                expires_at,
                data_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(workflow_id, task_id) DO UPDATE SET
                lease_id=excluded.lease_id,
                executor=excluded.executor,
                acquired_at=excluded.acquired_at,
                expires_at=excluded.expires_at,
                data_json=excluded.data_json
            WHERE task_leases.expires_at <= ?8
            "#,
            params![
                lease.workflow_id,
                lease.task_id,
                lease.lease_id,
                lease.executor,
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
        self.connection.execute(
            r#"
            INSERT INTO task_checkpoints (
                id,
                workflow_id,
                task_id,
                executor,
                state,
                created_at,
                data_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                &checkpoint.checkpoint_id,
                &checkpoint.workflow_id,
                &checkpoint.task_id,
                &checkpoint.executor,
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

fn stored_event_service_from_row(row: &Row<'_>) -> rusqlite::Result<StoredEventServiceRecord> {
    let heartbeat_ttl_seconds = row.get::<_, i64>(8)?.max(0) as u64;
    let data_json = row.get::<_, String>(9)?;
    Ok(StoredEventServiceRecord {
        id: row.get(0)?,
        service_kind: row.get(1)?,
        status: row.get(2)?,
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
