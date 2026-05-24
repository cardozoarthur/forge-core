use crate::artifact::hex_sha256;
use crate::context::ContextReplayShardRef;
use crate::graph::{ArtifactRecord, AtomicTask, ExecutorKind};
use crate::handoff::{build_task_handoff, TaskHandoffReport};
use crate::lease::TaskLease;
use crate::storage::ForgeStore;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const CLUSTER_NODE_SCHEMA_VERSION: &str = "forge.cluster_node.v1";
const CLUSTER_REGISTRY_SCHEMA_VERSION: &str = "forge.cluster_registry.v2";
const CLUSTER_NODE_SCHEDULING_SCHEMA_VERSION: &str = "forge.cluster_node_scheduling.v1";
const CLUSTER_PLACEMENT_SCHEMA_VERSION: &str = "forge.cluster_placement.v1";
const CLUSTER_PLACEMENT_REQUIREMENTS_SCHEMA_VERSION: &str =
    "forge.cluster_placement_requirements.v1";
const CLUSTER_TASK_HANDOFF_SCHEMA_VERSION: &str = "forge.cluster_task_handoff.v1";
const CLUSTER_SYNC_MANIFEST_SCHEMA_VERSION: &str = "forge.cluster_sync_manifest.v1";
const CLUSTER_NODE_LEASE_SCHEMA_VERSION: &str = "forge.cluster_node_lease.v1";
const CLUSTER_NODE_LEASE_REGISTRY_SCHEMA_VERSION: &str = "forge.cluster_node_lease_registry.v1";

#[derive(Debug, Clone)]
pub struct ClusterNodeInput {
    pub node_id: String,
    pub name: String,
    pub endpoint: Option<String>,
    pub os: String,
    pub arch: String,
    pub cpu_cores: u16,
    pub memory_gb: u32,
    pub gpus: Vec<String>,
    pub installed_software: Vec<String>,
    pub capabilities: Vec<String>,
    pub python_available: bool,
    pub node_available: bool,
    pub docker_available: bool,
    pub gpu_available: bool,
    pub network_reachable: bool,
    pub status: String,
    pub trust_level: String,
    pub sandbox_permissions: Vec<String>,
    pub cost_per_hour_usd: f64,
    pub latency_ms: u32,
    pub reliability: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterNode {
    pub schema_version: String,
    pub node_id: String,
    pub name: String,
    pub endpoint: Option<String>,
    pub os: String,
    pub arch: String,
    pub cpu_cores: u16,
    pub memory_gb: u32,
    pub gpus: Vec<String>,
    pub installed_software: Vec<String>,
    pub capabilities: Vec<String>,
    pub python_available: bool,
    pub node_available: bool,
    pub docker_available: bool,
    pub gpu_available: bool,
    pub network_reachable: bool,
    pub status: String,
    pub cost_per_hour_usd: f64,
    pub latency_ms: u32,
    pub reliability: f64,
    pub trust_level: String,
    pub sandbox_permissions: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterRegisterReport {
    pub status: String,
    pub node: ClusterNode,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ClusterRegistrySummary {
    pub total_nodes: usize,
    pub online_nodes: usize,
    pub reachable_nodes: usize,
    pub linux_nodes: usize,
    pub windows_nodes: usize,
    pub python_nodes: usize,
    pub nodejs_nodes: usize,
    pub docker_nodes: usize,
    pub gpu_nodes: usize,
    pub metatrader5_nodes: usize,
    pub schedulable_nodes: usize,
    pub busy_schedulable_nodes: usize,
    pub idle_schedulable_nodes: usize,
    pub active_leases: usize,
    pub expired_leases: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterRegistryReport {
    pub schema_version: String,
    pub status: String,
    pub summary: ClusterRegistrySummary,
    pub nodes: Vec<ClusterNode>,
    pub scheduling: Vec<ClusterNodeSchedulingStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterNodeSchedulingStatus {
    pub schema_version: String,
    pub node_id: String,
    pub schedulable: bool,
    pub scheduling_status: String,
    pub active_lease_count: usize,
    pub expired_lease_count: usize,
    pub blockers: Vec<String>,
    pub remote_execution_enabled: bool,
    pub external_mutation_allowed: bool,
    pub trust_policy: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterPlacementRequirements {
    pub schema_version: String,
    pub workflow_id: String,
    pub task_id: String,
    pub executor: String,
    pub policy_mode: String,
    pub required_capabilities: Vec<String>,
    pub required_sandbox_permissions: Vec<String>,
    pub required_trust: String,
    pub mutation_allowed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterPlacementCandidate {
    pub node_id: String,
    pub eligible: bool,
    pub score: i64,
    pub active_lease_count: usize,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterPlacementReport {
    pub schema_version: String,
    pub status: String,
    pub workflow_id: String,
    pub task_id: String,
    pub requirements: ClusterPlacementRequirements,
    pub selected_node: Option<ClusterNode>,
    pub candidates: Vec<ClusterPlacementCandidate>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterTaskHandoffReport {
    pub schema_version: String,
    pub status: String,
    pub allowed: bool,
    pub workflow_id: String,
    pub task_id: String,
    pub selected_node_id: Option<String>,
    pub remote_execution_enabled: bool,
    pub external_mutation_allowed: bool,
    pub trust_policy: String,
    pub placement: ClusterPlacementReport,
    pub task_handoff: Option<TaskHandoffReport>,
    pub cluster_node_lease: Option<ClusterNodeLeaseRef>,
    pub sync_manifest: Option<ClusterSyncManifest>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterNodeLeaseRef {
    pub node_id: String,
    pub workflow_id: String,
    pub task_id: String,
    pub lease_id: String,
    pub lease_scope: String,
    pub lease_expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterSyncManifest {
    pub schema_version: String,
    pub workflow_id: String,
    pub task_id: String,
    pub selected_node_id: String,
    pub lease_id: Option<String>,
    pub context_sha256: String,
    pub context_routing_cache_key: String,
    pub context_routing_lineage_sha256: String,
    pub checkpoint_ref: Option<ClusterCheckpointRef>,
    pub shard_refs: Vec<ContextReplayShardRef>,
    pub artifact_refs: Vec<ArtifactRecord>,
    pub sync_mode: String,
    pub manifest_sha256: String,
    pub remote_execution_enabled: bool,
    pub external_mutation_allowed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterCheckpointRef {
    pub checkpoint_id: String,
    pub context_sha256: String,
    pub context_routing_cache_key: Option<String>,
    pub workflow_revision: u64,
    pub state: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterNodeLeaseRegistryFilter {
    pub node_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ClusterNodeLeaseRegistrySummary {
    pub total_leases: usize,
    pub active_leases: usize,
    pub expired_leases: usize,
    pub registered_node_leases: usize,
    pub unregistered_executor_leases: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterNodeLeaseRecord {
    pub schema_version: String,
    pub node_id: String,
    pub node_name: Option<String>,
    pub endpoint: Option<String>,
    pub workflow_id: String,
    pub workflow_goal: Option<String>,
    pub task_id: String,
    pub task_title: Option<String>,
    pub lease_id: String,
    pub lease_scope: String,
    pub acquired_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub lease_status: String,
    pub trust_level: String,
    pub sandbox_permissions: Vec<String>,
    pub network_reachable: bool,
    pub remote_execution_enabled: bool,
    pub external_mutation_allowed: bool,
    pub trust_policy: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClusterNodeLeaseRegistryReport {
    pub schema_version: String,
    pub status: String,
    pub filter: ClusterNodeLeaseRegistryFilter,
    pub summary: ClusterNodeLeaseRegistrySummary,
    pub leases: Vec<ClusterNodeLeaseRecord>,
}

pub fn register_cluster_node(
    store: &ForgeStore,
    input: ClusterNodeInput,
) -> Result<ClusterRegisterReport> {
    let node_id = input.node_id.trim();
    if node_id.is_empty() {
        bail!("node id cannot be empty");
    }
    if input.cpu_cores == 0 {
        bail!("cpu cores must be greater than zero");
    }
    if input.memory_gb == 0 {
        bail!("memory gb must be greater than zero");
    }
    if !(0.0..=1.0).contains(&input.reliability) {
        bail!("reliability must be between 0.0 and 1.0");
    }
    if input.cost_per_hour_usd < 0.0 {
        bail!("cost per hour cannot be negative");
    }

    let now = Utc::now();
    let existing_created_at = store
        .load_cluster_node(node_id)?
        .map(serde_json::from_value::<ClusterNode>)
        .transpose()?
        .map(|node| node.created_at)
        .unwrap_or(now);
    let node = ClusterNode::from_input(input, existing_created_at, now);
    store.save_cluster_node(&node.node_id, &serde_json::to_value(&node)?)?;

    Ok(ClusterRegisterReport {
        status: "registered".to_string(),
        node,
    })
}

pub fn list_cluster_nodes(store: &ForgeStore) -> Result<ClusterRegistryReport> {
    let nodes = load_cluster_nodes(store)?;
    let lease_pressure = load_cluster_lease_pressure(store)?;
    let scheduling = nodes
        .iter()
        .map(|node| {
            let pressure = lease_pressure
                .get(&node.node_id)
                .copied()
                .unwrap_or_default();
            node_scheduling_status(node, pressure)
        })
        .collect::<Vec<_>>();
    let summary = summarize_nodes(&nodes, &scheduling);
    Ok(ClusterRegistryReport {
        schema_version: CLUSTER_REGISTRY_SCHEMA_VERSION.to_string(),
        status: "listed".to_string(),
        summary,
        nodes,
        scheduling,
    })
}

pub fn list_cluster_node_leases(
    store: &ForgeStore,
    node_id: Option<&str>,
) -> Result<ClusterNodeLeaseRegistryReport> {
    let node_filter = node_id
        .map(normalize_node_filter)
        .filter(|value| !value.is_empty());
    let nodes = load_cluster_nodes(store)?;
    let nodes_by_id = nodes
        .into_iter()
        .map(|node| (node.node_id.clone(), node))
        .collect::<BTreeMap<_, _>>();
    let leases = store
        .load_task_leases()?
        .into_iter()
        .map(serde_json::from_value::<TaskLease>)
        .collect::<Result<Vec<_>, _>>()?;

    let now = Utc::now();
    let mut summary = ClusterNodeLeaseRegistrySummary::default();
    let mut records = Vec::new();
    for lease in leases {
        if node_filter
            .as_ref()
            .is_some_and(|filter| lease.executor != filter.as_str())
        {
            continue;
        }
        summary.total_leases += 1;
        let Some(node) = nodes_by_id.get(&lease.executor) else {
            summary.unregistered_executor_leases += 1;
            continue;
        };
        let workflow = store.load_workflow(&lease.workflow_id).ok();
        let task_title = workflow.as_ref().and_then(|workflow| {
            workflow
                .tasks
                .iter()
                .find(|task| task.id == lease.task_id)
                .map(|task| task.title.clone())
        });
        let lease_status = if lease.expires_at > now {
            summary.active_leases += 1;
            "active"
        } else {
            summary.expired_leases += 1;
            "expired"
        };
        summary.registered_node_leases += 1;
        records.push(ClusterNodeLeaseRecord {
            schema_version: CLUSTER_NODE_LEASE_SCHEMA_VERSION.to_string(),
            node_id: node.node_id.clone(),
            node_name: Some(node.name.clone()),
            endpoint: node.endpoint.clone(),
            workflow_id: lease.workflow_id,
            workflow_goal: workflow.map(|workflow| workflow.goal),
            task_id: lease.task_id,
            task_title,
            lease_id: lease.lease_id,
            lease_scope: "task_on_cluster_node".to_string(),
            acquired_at: lease.acquired_at,
            expires_at: lease.expires_at,
            lease_status: lease_status.to_string(),
            trust_level: node.trust_level.clone(),
            sandbox_permissions: node.sandbox_permissions.clone(),
            network_reachable: node.network_reachable,
            remote_execution_enabled: false,
            external_mutation_allowed: false,
            trust_policy: "explicit_trust_required_no_external_mutation".to_string(),
        });
    }

    Ok(ClusterNodeLeaseRegistryReport {
        schema_version: CLUSTER_NODE_LEASE_REGISTRY_SCHEMA_VERSION.to_string(),
        status: "listed".to_string(),
        filter: ClusterNodeLeaseRegistryFilter {
            node_id: node_filter,
        },
        summary,
        leases: records,
    })
}

pub fn place_task_on_cluster(
    store: &ForgeStore,
    workflow_id: &str,
    task_id: &str,
) -> Result<ClusterPlacementReport> {
    let workflow = store.load_workflow(workflow_id)?;
    let task = workflow
        .tasks
        .iter()
        .find(|candidate| candidate.id == task_id)
        .with_context(|| format!("task not found in workflow {workflow_id}: {task_id}"))?;
    let requirements = placement_requirements(workflow_id, task);
    let nodes = load_cluster_nodes(store)?;
    let active_lease_counts = load_active_cluster_lease_counts(store)?;
    let mut candidates = nodes
        .iter()
        .map(|node| {
            evaluate_candidate(
                node,
                &requirements,
                active_lease_counts
                    .get(&node.node_id)
                    .copied()
                    .unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .eligible
            .cmp(&left.eligible)
            .then_with(|| right.score.cmp(&left.score))
            .then_with(|| left.node_id.cmp(&right.node_id))
    });

    let selected_node = candidates
        .iter()
        .find(|candidate| candidate.eligible)
        .and_then(|candidate| nodes.iter().find(|node| node.node_id == candidate.node_id))
        .cloned();
    let status = if selected_node.is_some() {
        "placement_selected"
    } else {
        "placement_blocked"
    };
    let reason = selected_node
        .as_ref()
        .map(|node| {
            format!(
                "selected {} by deterministic capability and trust policy",
                node.node_id
            )
        })
        .unwrap_or_else(|| {
            "no registered cluster node satisfies task placement requirements".to_string()
        });

    Ok(ClusterPlacementReport {
        schema_version: CLUSTER_PLACEMENT_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        workflow_id: workflow_id.to_string(),
        task_id: task_id.to_string(),
        requirements,
        selected_node,
        candidates,
        reason,
    })
}

pub fn build_cluster_task_handoff(
    store: &ForgeStore,
    workflow_id: &str,
    task_id: &str,
    budget: usize,
    ttl_seconds: u64,
) -> Result<ClusterTaskHandoffReport> {
    let workflow = store.load_workflow(workflow_id)?;
    let placement = place_task_on_cluster(store, workflow_id, task_id)?;
    let Some(selected_node) = placement.selected_node.as_ref() else {
        return Ok(ClusterTaskHandoffReport {
            schema_version: CLUSTER_TASK_HANDOFF_SCHEMA_VERSION.to_string(),
            status: "placement_blocked".to_string(),
            allowed: false,
            workflow_id: workflow_id.to_string(),
            task_id: task_id.to_string(),
            selected_node_id: None,
            remote_execution_enabled: false,
            external_mutation_allowed: false,
            trust_policy: "explicit_trust_required_no_external_mutation".to_string(),
            placement,
            task_handoff: None,
            cluster_node_lease: None,
            sync_manifest: None,
            reason: "no eligible cluster node available for task handoff".to_string(),
        });
    };

    let selected_node_id = selected_node.node_id.clone();
    let task_handoff = build_task_handoff(
        store,
        workflow_id,
        task_id,
        &selected_node_id,
        budget,
        ttl_seconds,
    )?;
    let cluster_node_lease = task_handoff
        .lease
        .as_ref()
        .map(|lease| ClusterNodeLeaseRef {
            node_id: selected_node_id.clone(),
            workflow_id: lease.workflow_id.clone(),
            task_id: lease.task_id.clone(),
            lease_id: lease.lease_id.clone(),
            lease_scope: "task_on_cluster_node".to_string(),
            lease_expires_at: lease.expires_at,
        });
    let sync_manifest = build_sync_manifest(&workflow, &selected_node_id, &task_handoff);
    let allowed = task_handoff.allowed;
    let status = if allowed {
        "cluster_handoff_ready".to_string()
    } else {
        task_handoff.status.clone()
    };
    let reason = if allowed {
        format!(
            "selected {selected_node_id} and prepared a content-addressed sync manifest without remote execution"
        )
    } else {
        task_handoff
            .reason
            .clone()
            .unwrap_or_else(|| "cluster handoff blocked before remote execution".to_string())
    };

    Ok(ClusterTaskHandoffReport {
        schema_version: CLUSTER_TASK_HANDOFF_SCHEMA_VERSION.to_string(),
        status,
        allowed,
        workflow_id: workflow_id.to_string(),
        task_id: task_id.to_string(),
        selected_node_id: Some(selected_node_id),
        remote_execution_enabled: false,
        external_mutation_allowed: false,
        trust_policy: "explicit_trust_required_no_external_mutation".to_string(),
        placement,
        task_handoff: Some(task_handoff),
        cluster_node_lease,
        sync_manifest: Some(sync_manifest),
        reason,
    })
}

impl ClusterNode {
    fn from_input(
        input: ClusterNodeInput,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        let node_available = input.node_available;
        let docker_available = input.docker_available;
        let gpu_available = input.gpu_available || !input.gpus.is_empty();
        let mut capability_set = normalize_set(&input.capabilities);
        if input.python_available {
            capability_set.insert("python".to_string());
        }
        if node_available {
            capability_set.insert("nodejs".to_string());
        }
        if docker_available {
            capability_set.insert("docker".to_string());
        }
        if gpu_available {
            capability_set.insert("gpu".to_string());
        }
        Self {
            schema_version: CLUSTER_NODE_SCHEMA_VERSION.to_string(),
            node_id: input.node_id.trim().to_string(),
            name: input.name.trim().to_string(),
            endpoint: input
                .endpoint
                .map(|endpoint| endpoint.trim().to_string())
                .filter(|endpoint| !endpoint.is_empty()),
            os: normalize_token(&input.os),
            arch: normalize_token(&input.arch),
            cpu_cores: input.cpu_cores,
            memory_gb: input.memory_gb,
            gpus: clean_list(input.gpus),
            installed_software: clean_list(input.installed_software),
            capabilities: capability_set.into_iter().collect(),
            python_available: input.python_available,
            node_available,
            docker_available,
            gpu_available,
            network_reachable: input.network_reachable,
            status: normalize_token(&input.status),
            cost_per_hour_usd: input.cost_per_hour_usd,
            latency_ms: input.latency_ms,
            reliability: input.reliability,
            trust_level: normalize_token(&input.trust_level),
            sandbox_permissions: normalize_set(&input.sandbox_permissions)
                .into_iter()
                .collect(),
            created_at,
            updated_at,
        }
    }
}

fn build_sync_manifest(
    workflow: &crate::graph::Workflow,
    selected_node_id: &str,
    task_handoff: &TaskHandoffReport,
) -> ClusterSyncManifest {
    let checkpoint_ref = task_handoff
        .context
        .latest_checkpoint
        .as_ref()
        .map(|checkpoint| ClusterCheckpointRef {
            checkpoint_id: checkpoint.checkpoint_id.clone(),
            context_sha256: checkpoint.context_sha256.clone(),
            context_routing_cache_key: checkpoint.context_routing_cache_key.clone(),
            workflow_revision: checkpoint.workflow_revision,
            state: checkpoint.state.clone(),
            created_at: checkpoint.created_at,
        });

    let mut manifest = ClusterSyncManifest {
        schema_version: CLUSTER_SYNC_MANIFEST_SCHEMA_VERSION.to_string(),
        workflow_id: task_handoff.workflow_id.clone(),
        task_id: task_handoff.task_id.clone(),
        selected_node_id: selected_node_id.to_string(),
        lease_id: task_handoff
            .lease
            .as_ref()
            .map(|lease| lease.lease_id.clone()),
        context_sha256: task_handoff.context.context_sha256.clone(),
        context_routing_cache_key: task_handoff.context.routing_fingerprint.cache_key.clone(),
        context_routing_lineage_sha256: task_handoff
            .context
            .routing_fingerprint
            .lineage_sha256
            .clone(),
        checkpoint_ref,
        shard_refs: task_handoff.context.replay_manifest.shard_refs.clone(),
        artifact_refs: workflow.artifacts.clone(),
        sync_mode: "content_addressed_hash_manifest_only".to_string(),
        manifest_sha256: String::new(),
        remote_execution_enabled: false,
        external_mutation_allowed: false,
    };
    manifest.manifest_sha256 = sync_manifest_sha256(&manifest);
    manifest
}

fn sync_manifest_sha256(manifest: &ClusterSyncManifest) -> String {
    let hash_input = serde_json::json!([
        &manifest.schema_version,
        &manifest.workflow_id,
        &manifest.task_id,
        &manifest.selected_node_id,
        &manifest.lease_id,
        &manifest.context_sha256,
        &manifest.context_routing_cache_key,
        &manifest.context_routing_lineage_sha256,
        &manifest.checkpoint_ref,
        &manifest.shard_refs,
        &manifest.artifact_refs,
        &manifest.sync_mode,
        &manifest.remote_execution_enabled,
        &manifest.external_mutation_allowed
    ]);
    hex_sha256(
        &serde_json::to_vec(&hash_input)
            .expect("cluster sync manifest hash input should serialize"),
    )
}

fn load_cluster_nodes(store: &ForgeStore) -> Result<Vec<ClusterNode>> {
    store
        .load_cluster_nodes()?
        .into_iter()
        .map(serde_json::from_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn summarize_nodes(
    nodes: &[ClusterNode],
    scheduling: &[ClusterNodeSchedulingStatus],
) -> ClusterRegistrySummary {
    let mut summary = ClusterRegistrySummary {
        total_nodes: nodes.len(),
        ..ClusterRegistrySummary::default()
    };
    for node in nodes {
        if node.status == "online" {
            summary.online_nodes += 1;
        }
        if node.network_reachable {
            summary.reachable_nodes += 1;
        }
        if node.os.contains("linux") {
            summary.linux_nodes += 1;
        }
        if node.os.contains("windows") {
            summary.windows_nodes += 1;
        }
        if node.python_available || has_capability(node, "python") {
            summary.python_nodes += 1;
        }
        if node.node_available || has_capability(node, "nodejs") {
            summary.nodejs_nodes += 1;
        }
        if node.docker_available || has_capability(node, "docker") {
            summary.docker_nodes += 1;
        }
        if node.gpu_available || has_capability(node, "gpu") {
            summary.gpu_nodes += 1;
        }
        if has_capability(node, "metatrader5")
            || node
                .installed_software
                .iter()
                .any(|software| software.to_lowercase().contains("metatrader 5"))
        {
            summary.metatrader5_nodes += 1;
        }
    }
    for posture in scheduling {
        summary.active_leases += posture.active_lease_count;
        summary.expired_leases += posture.expired_lease_count;
        if posture.schedulable {
            summary.schedulable_nodes += 1;
            if posture.active_lease_count > 0 {
                summary.busy_schedulable_nodes += 1;
            } else {
                summary.idle_schedulable_nodes += 1;
            }
        }
    }
    summary
}

fn placement_requirements(workflow_id: &str, task: &AtomicTask) -> ClusterPlacementRequirements {
    let mut required_capabilities = Vec::new();
    let mut required_sandbox_permissions = Vec::new();
    if task.execution_policy.mode == "local_code_node" {
        if let Some(runtime) = &task.execution_policy.code_runtime {
            required_capabilities.push(normalize_token(&runtime.language));
            required_sandbox_permissions.push(normalize_token(&runtime.sandbox));
        }
    }
    if required_capabilities.is_empty() {
        required_capabilities.push(executor_kind(&task.executor).to_string());
    }

    ClusterPlacementRequirements {
        schema_version: CLUSTER_PLACEMENT_REQUIREMENTS_SCHEMA_VERSION.to_string(),
        workflow_id: workflow_id.to_string(),
        task_id: task.id.clone(),
        executor: executor_kind(&task.executor).to_string(),
        policy_mode: task.execution_policy.mode.clone(),
        required_capabilities,
        required_sandbox_permissions,
        required_trust: "trusted_lan_or_local".to_string(),
        mutation_allowed: false,
    }
}

fn evaluate_candidate(
    node: &ClusterNode,
    requirements: &ClusterPlacementRequirements,
    active_lease_count: usize,
) -> ClusterPlacementCandidate {
    let mut reasons = Vec::new();
    if node.status != "online" {
        reasons.push(format!("status is {}", node.status));
    }
    if !node.network_reachable {
        reasons.push("network unreachable".to_string());
    }
    if !trusted_for_placement(&node.trust_level) {
        reasons.push(format!("trust level {} is not allowed", node.trust_level));
    }
    for capability in &requirements.required_capabilities {
        if !has_capability(node, capability) {
            reasons.push(format!("missing capability {capability}"));
        }
    }
    for sandbox_permission in &requirements.required_sandbox_permissions {
        if !node.sandbox_permissions.contains(sandbox_permission) {
            reasons.push(format!("missing sandbox permission {sandbox_permission}"));
        }
    }

    let eligible = reasons.is_empty();
    if eligible {
        reasons.push("matches deterministic placement requirements".to_string());
        if active_lease_count > 0 {
            reasons.push(format!("active leases {active_lease_count}"));
        }
    }
    ClusterPlacementCandidate {
        node_id: node.node_id.clone(),
        eligible,
        score: placement_score(node, requirements, active_lease_count),
        active_lease_count,
        reasons,
    }
}

fn placement_score(
    node: &ClusterNode,
    requirements: &ClusterPlacementRequirements,
    active_lease_count: usize,
) -> i64 {
    let matched_capabilities = requirements
        .required_capabilities
        .iter()
        .filter(|capability| has_capability(node, capability))
        .count() as i64;
    let reliability_score = (node.reliability * 10_000.0).round() as i64;
    reliability_score + matched_capabilities * 1_000
        - i64::from(node.latency_ms)
        - (node.cost_per_hour_usd * 100.0).round() as i64
        - (active_lease_count as i64 * 10_000)
}

fn load_active_cluster_lease_counts(store: &ForgeStore) -> Result<BTreeMap<String, usize>> {
    Ok(load_cluster_lease_pressure(store)?
        .into_iter()
        .map(|(node_id, pressure)| (node_id, pressure.active_lease_count))
        .collect())
}

#[derive(Debug, Clone, Copy, Default)]
struct ClusterLeasePressure {
    active_lease_count: usize,
    expired_lease_count: usize,
}

fn load_cluster_lease_pressure(
    store: &ForgeStore,
) -> Result<BTreeMap<String, ClusterLeasePressure>> {
    let leases = store
        .load_task_leases()?
        .into_iter()
        .map(serde_json::from_value::<TaskLease>)
        .collect::<Result<Vec<_>, _>>()?;
    let now = Utc::now();
    let mut pressure = BTreeMap::new();
    for lease in leases {
        let entry = pressure
            .entry(lease.executor)
            .or_insert_with(ClusterLeasePressure::default);
        if lease.expires_at > now {
            entry.active_lease_count += 1;
        } else {
            entry.expired_lease_count += 1;
        }
    }
    Ok(pressure)
}

fn node_scheduling_status(
    node: &ClusterNode,
    pressure: ClusterLeasePressure,
) -> ClusterNodeSchedulingStatus {
    let blockers = node_registry_blockers(node);
    let schedulable = blockers.is_empty();
    let scheduling_status = if !schedulable {
        "blocked"
    } else if pressure.active_lease_count > 0 {
        "busy"
    } else {
        "idle"
    };

    ClusterNodeSchedulingStatus {
        schema_version: CLUSTER_NODE_SCHEDULING_SCHEMA_VERSION.to_string(),
        node_id: node.node_id.clone(),
        schedulable,
        scheduling_status: scheduling_status.to_string(),
        active_lease_count: pressure.active_lease_count,
        expired_lease_count: pressure.expired_lease_count,
        blockers,
        remote_execution_enabled: false,
        external_mutation_allowed: false,
        trust_policy: "explicit_trust_required_no_external_mutation".to_string(),
    }
}

fn node_registry_blockers(node: &ClusterNode) -> Vec<String> {
    let mut blockers = Vec::new();
    if node.status != "online" {
        blockers.push(format!("status is {}", node.status));
    }
    if !node.network_reachable {
        blockers.push("network unreachable".to_string());
    }
    if !trusted_for_placement(&node.trust_level) {
        blockers.push(format!("trust level {} is not allowed", node.trust_level));
    }
    blockers
}

fn has_capability(node: &ClusterNode, capability: &str) -> bool {
    let capability = normalize_token(capability);
    node.capabilities.iter().any(|item| item == &capability)
}

fn trusted_for_placement(trust_level: &str) -> bool {
    matches!(
        trust_level,
        "local" | "trusted" | "trusted_lan" | "trusted-lan"
    )
}

fn executor_kind(executor: &ExecutorKind) -> &'static str {
    match executor {
        ExecutorKind::Ai => "ai",
        ExecutorKind::Command => "command",
        ExecutorKind::Wait => "wait",
        ExecutorKind::Notification => "notification",
        ExecutorKind::Mixed => "mixed",
    }
}

fn clean_list(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn normalize_set(values: &[String]) -> BTreeSet<String> {
    values
        .iter()
        .map(|value| normalize_token(value))
        .filter(|value| !value.is_empty())
        .collect()
}

fn normalize_token(value: &str) -> String {
    value.trim().to_lowercase().replace(' ', "_")
}

fn normalize_node_filter(value: &str) -> String {
    value.trim().to_string()
}
