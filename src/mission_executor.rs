//! Policy-bound, at-most-once execution for persisted mission tasks.
//!
//! Forge owns the execution claim and receipt. The worktree module owns sandbox
//! planning and process isolation; this module binds that evidence to the exact
//! mission revision, task, agent, executor policy, command and approval scope.

use crate::executor::ExecutorState;
use crate::mission::{load_mission, load_squad, MissionMode, SkillGateMode};
use crate::storage::{open_configured_connection, ForgeStore};
use crate::worktree::{
    plan_worktree_sandbox, run_worktree_sandbox, WorktreeSandboxPlan, WorktreeSandboxReceipt,
    WorktreeSandboxRequest,
};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

pub const MISSION_EXECUTION_PLAN_SCHEMA_VERSION: &str = "forge.mission.execution_plan.v1";
pub const MISSION_EXECUTION_APPROVAL_SCHEMA_VERSION: &str = "forge.mission.execution_approval.v1";
pub const MISSION_EXECUTION_RECEIPT_SCHEMA_VERSION: &str = "forge.mission.execution_receipt.v3";
pub const LEGACY_MISSION_EXECUTION_RECEIPT_SCHEMA_VERSION: &str =
    "forge.mission.execution_receipt.v2";
pub const MISSION_EXECUTION_CLAIM_SCHEMA_VERSION: &str = "forge.mission.execution_claim.v1";
pub const MISSION_EXECUTION_METRIC_SCHEMA_VERSION: &str = "forge.mission.execution_metric.v1";
pub const MISSION_EXECUTION_LIST_SCHEMA_VERSION: &str = "forge.mission.execution_receipt_list.v1";
pub const MISSION_EXECUTION_RECONCILIATION_SCHEMA_VERSION: &str =
    "forge.mission.execution_reconciliation.v1";

const MIN_EXECUTION_LEASE_SECONDS: u64 = 300;
const EXECUTION_LEASE_MARGIN_SECONDS: u64 = 30;
const MAX_APPROVAL_TTL_SECONDS: u64 = 900;
const METRIC_SOURCE_READ_ONLY_WORKTREE: &str = "bubblewrap_read_only_worktree";
const METRIC_SOURCE_NETWORK_ISOLATION: &str = "sandbox_network_isolation";
const METRIC_SOURCE_DETERMINISTIC_LOCAL_COST: &str =
    "deterministic_local_command_network_isolation";
const GATE_EVIDENCE_OBSERVATION_SCHEMA_VERSION: &str = "forge.mission.gate_evidence_observation.v1";
const GATE_EVIDENCE_OBSERVATION_PREFIX: &str = "FORGE_GATE_EVIDENCE:";
const TEST_EXECUTION_OBSERVATION_SCHEMA_VERSION: &str =
    "forge.mission.test_execution_observation.v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionExecutionApproval {
    pub schema_version: String,
    pub approval_scope_sha256: String,
    pub approved_by: String,
    pub issued_at: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionExecutionRequest {
    pub idempotency_key: String,
    pub mission_id: String,
    pub workflow_id: String,
    pub expected_mission_revision: u64,
    pub task_id: String,
    pub agent_id: String,
    pub executor_id: String,
    pub worktree: Option<String>,
    pub purpose: String,
    pub command: Vec<String>,
    #[serde(default)]
    pub requested_evidence: Vec<String>,
    #[serde(default)]
    pub approval: Option<MissionExecutionApproval>,
    pub dry_run: bool,
    #[serde(default)]
    pub allow_trusted_process_runtime: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MissionExecutionDecision {
    pub check: String,
    pub allowed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionExecutionPlan {
    pub schema_version: String,
    pub status: String,
    pub allowed: bool,
    pub receipt_id: String,
    pub request_sha256: String,
    pub approval_scope_sha256: String,
    pub mission_id: String,
    pub workflow_id: String,
    pub mission_revision: u64,
    pub task_id: String,
    pub agent_id: String,
    pub executor_id: String,
    pub command_sha256: String,
    pub worktree_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requested_evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gate_evidence_contract: Vec<MissionExecutionGateEvidenceContract>,
    pub policy_trace: Vec<MissionExecutionDecision>,
    pub sandbox_plan: Option<WorktreeSandboxPlan>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MissionExecutionGateEvidenceContract {
    pub evidence_kind: String,
    pub gate_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionSandboxEvidence {
    pub status: String,
    pub runtime: String,
    pub filesystem_isolation_enforced: bool,
    pub network_isolation_enforced: bool,
    pub max_command_seconds: u64,
    pub max_output_bytes: usize,
    pub config_sha256: String,
    pub command_sha256: String,
    pub duration_ms: u128,
    pub timed_out: bool,
    pub exit_code: Option<i32>,
    pub stdout_sha256: String,
    pub stderr_sha256: String,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub output_truncated: bool,
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub worktree_read_only: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub writes_restricted_to_sandbox: bool,
}

impl From<&WorktreeSandboxReceipt> for MissionSandboxEvidence {
    fn from(receipt: &WorktreeSandboxReceipt) -> Self {
        Self {
            status: receipt.status.clone(),
            runtime: receipt.runtime.clone(),
            filesystem_isolation_enforced: receipt.plan.filesystem_isolation_enforced,
            network_isolation_enforced: receipt.plan.network_isolation_enforced,
            max_command_seconds: receipt.plan.max_command_seconds,
            max_output_bytes: receipt.plan.max_output_bytes,
            config_sha256: receipt.config_sha256.clone(),
            command_sha256: receipt.command_sha256.clone(),
            duration_ms: receipt.duration_ms,
            timed_out: receipt.timed_out,
            exit_code: receipt.exit_code,
            stdout_sha256: receipt.stdout.sha256.clone(),
            stderr_sha256: receipt.stderr.sha256.clone(),
            stdout_bytes: receipt.stdout.total_bytes,
            stderr_bytes: receipt.stderr.total_bytes,
            output_truncated: receipt.stdout.truncated || receipt.stderr.truncated,
            error: receipt.error.clone(),
            command: receipt.plan.command.clone(),
            worktree_read_only: receipt.plan.runtime == "bubblewrap"
                && receipt.plan.filesystem_isolation_enforced,
            writes_restricted_to_sandbox: receipt.plan.runtime == "bubblewrap"
                && receipt.plan.filesystem_isolation_enforced,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MissionExecutionEvidence {
    pub kind: String,
    pub locator: String,
    pub sha256: String,
    pub bytes: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation: Option<serde_json::Value>,
}

#[derive(Debug, Default)]
struct MissionExecutionObservations {
    tests_passed: Option<serde_json::Value>,
    gate_evidence: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum MissionExecutionClaimKind {
    ExecutionCompleted,
    TestsPassed,
}

impl MissionExecutionClaimKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExecutionCompleted => "execution_completed",
            Self::TestsPassed => "tests_passed",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum MissionExecutionReceiptClaimKind {
    ExecutionCompleted,
    TestsPassed,
    GateEvidence,
}

impl MissionExecutionReceiptClaimKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExecutionCompleted => "execution_completed",
            Self::TestsPassed => "tests_passed",
            Self::GateEvidence => "gate_evidence",
        }
    }
}

impl From<MissionExecutionClaimKind> for MissionExecutionReceiptClaimKind {
    fn from(kind: MissionExecutionClaimKind) -> Self {
        match kind {
            MissionExecutionClaimKind::ExecutionCompleted => Self::ExecutionCompleted,
            MissionExecutionClaimKind::TestsPassed => Self::TestsPassed,
        }
    }
}

impl PartialEq<MissionExecutionClaimKind> for MissionExecutionReceiptClaimKind {
    fn eq(&self, other: &MissionExecutionClaimKind) -> bool {
        matches!(
            (self, other),
            (
                Self::ExecutionCompleted,
                MissionExecutionClaimKind::ExecutionCompleted
            ) | (Self::TestsPassed, MissionExecutionClaimKind::TestsPassed)
        )
    }
}

impl PartialEq<MissionExecutionReceiptClaimKind> for MissionExecutionClaimKind {
    fn eq(&self, other: &MissionExecutionReceiptClaimKind) -> bool {
        other == self
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MissionExecutionClaimScope {
    Operational,
    BoundedSimulation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MissionExecutionClaimReceipt {
    pub schema_version: String,
    pub kind: MissionExecutionReceiptClaimKind,
    pub scope: MissionExecutionClaimScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gate_ids: Vec<String>,
    pub locator: String,
    pub sha256: String,
    pub command_sha256: String,
    pub status: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MissionExecutionMetricStatus {
    Observed,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(bound(serialize = "T: Serialize", deserialize = "T: Deserialize<'de>"))]
pub struct MissionExecutionMetricObservation<T> {
    pub schema_version: String,
    pub status: MissionExecutionMetricStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<T>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
#[serde(bound(serialize = "T: Serialize", deserialize = "T: Deserialize<'de>"))]
pub enum MissionExecutionMetric<T> {
    Explicit(MissionExecutionMetricObservation<T>),
    Legacy(T),
}

impl<T> MissionExecutionMetric<T> {
    fn observed(value: T, source: &str, evidence_sha256: String) -> Self {
        Self::Explicit(MissionExecutionMetricObservation {
            schema_version: MISSION_EXECUTION_METRIC_SCHEMA_VERSION.to_string(),
            status: MissionExecutionMetricStatus::Observed,
            value: Some(value),
            source: Some(source.to_string()),
            evidence_sha256: Some(evidence_sha256),
            reason: None,
        })
    }

    fn unknown(reason: &str) -> Self {
        Self::Explicit(MissionExecutionMetricObservation {
            schema_version: MISSION_EXECUTION_METRIC_SCHEMA_VERSION.to_string(),
            status: MissionExecutionMetricStatus::Unknown,
            value: None,
            source: None,
            evidence_sha256: None,
            reason: Some(reason.to_string()),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResolvedMissionExecutionMetrics {
    pub cost_usd: MissionExecutionMetricObservation<f64>,
    pub files_changed: MissionExecutionMetricObservation<usize>,
    pub external_calls: MissionExecutionMetricObservation<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionExecutionReceipt {
    pub schema_version: String,
    pub receipt_id: String,
    pub receipt_sha256: String,
    pub request_sha256: String,
    pub approval_scope_sha256: String,
    pub command_sha256: String,
    pub status: String,
    pub allowed: bool,
    pub execution_attempted: bool,
    pub executed: bool,
    pub mission_id: String,
    pub workflow_id: String,
    pub mission_revision: u64,
    pub task_id: String,
    pub agent_id: String,
    pub executor_id: String,
    pub worktree_id: Option<String>,
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: u128,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub tests_passed: usize,
    pub tests_failed: usize,
    /// Legacy field retained for wire compatibility. Operational semantic claims
    /// are accepted only from `claims`, never from caller-authored strings.
    #[serde(default)]
    pub validations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claims: Vec<MissionExecutionClaimReceipt>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requested_evidence: Vec<String>,
    #[serde(default)]
    pub evidence: Vec<MissionExecutionEvidence>,
    pub observed_cost_usd: MissionExecutionMetric<f64>,
    pub observed_files_changed: MissionExecutionMetric<usize>,
    pub observed_external_calls: MissionExecutionMetric<usize>,
    pub approval: Option<MissionExecutionApproval>,
    pub policy_trace: Vec<MissionExecutionDecision>,
    pub sandbox: Option<MissionSandboxEvidence>,
    #[serde(default)]
    pub consumed_at: Option<String>,
    #[serde(default)]
    pub consumed_by_submission: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerifiedMissionExecutionClaims {
    pub receipt_id: String,
    pub receipt_sha256: String,
    pub mission_revision: u64,
    pub claims: Vec<MissionExecutionClaimKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gate_evidence: Vec<VerifiedMissionGateEvidenceClaim>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerifiedMissionGateEvidenceClaim {
    pub evidence_kind: String,
    pub gate_ids: Vec<String>,
    pub scope: MissionExecutionClaimScope,
    pub locator: String,
    pub sha256: String,
    pub command_sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissionExecutionResult {
    pub status: String,
    pub persisted: bool,
    pub replayed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<MissionExecutionPlan>,
    pub receipt: MissionExecutionReceipt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionExecutionReconcileRequest {
    pub receipt_id: String,
    pub outcome: String,
    pub approved_by: String,
    pub reason: String,
    pub confirm_no_effect_retry: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionExecutionReconciliation {
    pub schema_version: String,
    pub reconciliation_id: String,
    pub reconciliation_sha256: String,
    pub receipt_id: String,
    pub mission_id: String,
    pub workflow_id: String,
    pub mission_revision: u64,
    pub task_id: String,
    pub previous_state: String,
    pub resulting_state: String,
    pub outcome: String,
    pub approved_by: String,
    pub reason: String,
    pub confirmed_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissionExecutionReconcileResult {
    pub status: String,
    pub replayed: bool,
    pub reconciliation: MissionExecutionReconciliation,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissionExecutionRecord {
    pub receipt_id: String,
    pub mission_id: String,
    pub workflow_id: String,
    pub mission_revision: u64,
    pub task_id: String,
    pub agent_id: String,
    pub executor_id: String,
    pub state: String,
    pub execution_started_at: Option<String>,
    pub consumed_at: Option<String>,
    pub consumed_by_submission: Option<String>,
    pub receipt: Option<MissionExecutionReceipt>,
    pub reconciliation: Option<MissionExecutionReconciliation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissionExecutionListReport {
    pub schema_version: String,
    pub status: String,
    pub records: Vec<MissionExecutionRecord>,
}

#[derive(Debug)]
struct ExecutionClaim {
    owner_token: String,
    replay: Option<MissionExecutionReceipt>,
}

#[derive(Debug)]
struct ExecutionDbRecord {
    receipt_id: String,
    mission_id: String,
    workflow_id: String,
    mission_revision: u64,
    task_id: String,
    agent_id: String,
    executor_id: String,
    state: String,
    request_sha256: String,
    execution_started_at: Option<String>,
    receipt_json: Option<String>,
    consumed_at: Option<String>,
    consumed_by_submission: Option<String>,
    lease_expires_at: Option<String>,
    reconciliation_json: Option<String>,
}

pub fn plan_mission_execution(
    store: &ForgeStore,
    request: &MissionExecutionRequest,
) -> Result<MissionExecutionPlan> {
    validate_shape(request)?;
    let mission = load_mission(store, &request.mission_id)?;
    let mut trace = Vec::new();
    decide(
        &mut trace,
        "workflow_identity",
        mission.workflow_id == request.workflow_id,
        "mission and requested workflow must match",
    );
    decide(
        &mut trace,
        "mission_revision",
        mission.revision == request.expected_mission_revision,
        "execution must bind the current persisted mission revision",
    );

    let task = mission.tasks.iter().find(|task| task.id == request.task_id);
    let agent = mission
        .agents
        .iter()
        .find(|agent| agent.instance_id == request.agent_id);
    decide(
        &mut trace,
        "mission_task",
        task.is_some(),
        "task must belong to the persisted mission",
    );
    decide(
        &mut trace,
        "mission_agent",
        agent.is_some(),
        "agent instance must belong to the persisted mission",
    );
    decide(
        &mut trace,
        "task_execution_state",
        task.is_some_and(|task| matches!(task.status.as_str(), "running" | "repairing")),
        "mission task must be running or repairing",
    );
    decide(
        &mut trace,
        "task_assignment",
        task.is_some_and(|task| task.assigned_agent_id.as_deref() == Some(&request.agent_id)),
        "persisted task assignment must name the executing agent",
    );
    decide(
        &mut trace,
        "agent_execution_state",
        agent.is_some_and(|agent| agent.status == "running"),
        "executing agent instance must be running",
    );
    decide(
        &mut trace,
        "role_assignment",
        task.zip(agent)
            .is_some_and(|(task, agent)| task.owner_role == agent.role),
        "task owner role must match the executing agent role",
    );

    let harness = agent.and_then(|agent| {
        mission.harnesses.iter().rev().find(|harness| {
            harness.task_id == request.task_id && harness.agent_id == agent.definition_id
        })
    });
    decide(
        &mut trace,
        "harness_resolution",
        harness.is_some_and(|harness| {
            !harness.runtime.trim().is_empty()
                && !harness.provider.trim().is_empty()
                && !harness.model.trim().is_empty()
                && !harness.effort.trim().is_empty()
                && !harness.skills.is_empty()
                && !harness.tools.is_empty()
        }),
        "task requires a complete persisted harness resolution",
    );

    let squad = load_squad(store, &mission.squad_id, Some(&mission.squad_version))?;
    let gate_evidence_contract = resolve_requested_gate_evidence(&mission, &squad, request)?;
    decide(
        &mut trace,
        "gate_evidence_contract",
        true,
        if gate_evidence_contract.is_empty() {
            "no semantic gate evidence was requested"
        } else {
            "every requested semantic evidence kind belongs to the task gate contract"
        },
    );
    let semantic_evidence_command_allowed =
        command_can_observe_requested_evidence(&request.command, &gate_evidence_contract);
    decide(
        &mut trace,
        "semantic_evidence_command",
        semantic_evidence_command_allowed,
        "semantic gate evidence requires a result-bearing command; cargo metadata cannot prove gates and review_passed requires cargo test",
    );
    let member = agent.and_then(|agent| {
        squad
            .roster
            .iter()
            .find(|member| member.agent.id == agent.definition_id)
    });
    decide(
        &mut trace,
        "roster_member",
        member.is_some(),
        "executing agent definition must belong to the pinned squad roster",
    );
    let task_index = mission
        .tasks
        .iter()
        .position(|candidate| candidate.id == request.task_id);
    let independent_gate = squad.gates.get(1);
    let reviewer_protected_evidence_requested = independent_gate.is_some_and(|gate| {
        gate.approval_policy == "reviewer_anti_affinity"
            && gate_evidence_contract
                .iter()
                .any(|evidence| evidence.gate_ids.iter().any(|id| id == &gate.id))
    });
    let delivery_task = mission.tasks.get(1);
    let delivery_producer_id = delivery_task.and_then(|delivery_task| {
        mission
            .handoffs
            .iter()
            .rev()
            .find(|handoff| {
                handoff.task_id == delivery_task.id
                    && matches!(handoff.status.as_str(), "accepted" | "consumed")
            })
            .map(|handoff| handoff.from_agent.as_str())
            .or(delivery_task.assigned_agent_id.as_deref())
    });
    let delivery_producer = delivery_producer_id.and_then(|producer_id| {
        mission
            .agents
            .iter()
            .find(|candidate| candidate.instance_id == producer_id)
    });
    let independent_assurance_allowed = !reviewer_protected_evidence_requested
        || (task_index
            .is_some_and(|index| index == mission.tasks.len().saturating_sub(1) && index != 1)
            && member.is_some_and(|member| member.reviewer_anti_affinity)
            && agent
                .zip(delivery_producer)
                .is_some_and(|(reviewer, producer)| {
                    reviewer.instance_id != producer.instance_id
                        && reviewer.definition_id != producer.definition_id
                        && reviewer.role != producer.role
                }));
    decide(
        &mut trace,
        "reviewer_anti_affinity",
        independent_assurance_allowed,
        "independent assurance evidence requires an anti-affinity reviewer distinct from the delivery producer",
    );
    decide(
        &mut trace,
        "harness_identity",
        harness.zip(member).is_some_and(|(harness, member)| {
            harness.role == member.role
                && harness.runtime == member.agent.runtime
                && harness.provider == member.agent.provider
                && harness.model == member.agent.model
                && harness.effort == member.agent.effort
        }),
        "role, runtime, provider, model and effort must match the pinned roster",
    );
    decide(
        &mut trace,
        "harness_skills",
        harness
            .zip(member)
            .is_some_and(|(harness, member)| skill_policy_allows(member, &harness.skills)),
        "resolved skills must satisfy the roster skill gate",
    );
    decide(
        &mut trace,
        "harness_tools",
        harness.zip(member).is_some_and(|(harness, member)| {
            harness
                .tools
                .iter()
                .all(|tool| member.agent.tools.iter().any(|allowed| allowed == tool))
        }),
        "resolved tools must be allowed by the pinned agent definition",
    );

    let command_name = request
        .command
        .first()
        .map(|command| command_basename(command))
        .unwrap_or_default();
    decide(
        &mut trace,
        "shell_allowlist",
        member.is_some_and(|member| {
            member
                .agent
                .permissions
                .shell_allow
                .iter()
                .any(|allowed| allowed == &command_name)
        }),
        "command executable must be present in the agent shell allowlist",
    );

    let canonical_executor = canonical_executor_id(&request.executor_id);
    let executor_states = store
        .load_executor_states()?
        .into_iter()
        .map(serde_json::from_value::<ExecutorState>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let executor = executor_states
        .iter()
        .find(|executor| executor.id == canonical_executor);
    decide(
        &mut trace,
        "executor_identity",
        harness
            .is_some_and(|harness| canonical_executor_id(&harness.runtime) == canonical_executor),
        "requested executor must equal the canonical harness runtime",
    );
    decide(
        &mut trace,
        "executor_policy",
        executor
            .is_some_and(|executor| executor.installed && executor.configured && executor.allowed),
        "executor must be installed, configured and explicitly allowed",
    );

    decide(
        &mut trace,
        "mission_budget",
        mission.cost.total_usd < mission.budget_usd,
        "mission budget must have remaining capacity before execution",
    );
    decide(
        &mut trace,
        "agent_limits",
        agent.zip(member).is_some_and(|(agent, member)| {
            agent.cost_usd < member.agent.limits.max_cost_usd
                && agent.files_changed < member.agent.limits.max_files_changed
                && agent.runtime_milliseconds
                    < member
                        .agent
                        .limits
                        .max_runtime_seconds
                        .saturating_mul(1_000)
        }),
        "agent cost, file and runtime limits must have remaining capacity",
    );

    let requested_worktree = request.worktree.as_deref();
    decide(
        &mut trace,
        "worktree_supplied",
        requested_worktree.is_some_and(|value| !value.trim().is_empty()),
        "mission execution requires a registered worktree selector",
    );
    decide(
        &mut trace,
        "mission_worktree",
        mission
            .worktree
            .as_deref()
            .is_some_and(|pinned| requested_worktree == Some(pinned)),
        "requested worktree must equal the worktree pinned by the mission",
    );

    let mut sandbox_plan = None;
    match plan_worktree_sandbox(store, sandbox_request(request)) {
        Ok(plan) => {
            decide(
                &mut trace,
                "worktree_guardrails",
                plan.allowed,
                "registration, approved config, binding and sandbox policy must pass",
            );
            let isolated = plan.filesystem_isolation_enforced
                || (request.allow_trusted_process_runtime && plan.runtime == "process");
            decide(
                &mut trace,
                "filesystem_isolation",
                isolated,
                "bubblewrap is required unless trusted process runtime is explicitly scoped",
            );
            let trusted_process_scope =
                request.allow_trusted_process_runtime && plan.runtime == "process";
            let network_allowed = if trusted_process_scope {
                true
            } else {
                match member.map(|member| member.agent.permissions.network.as_str()) {
                    Some("deny" | "restricted") => plan.network_isolation_enforced,
                    Some(_) => true,
                    None => false,
                }
            };
            decide(
                &mut trace,
                "network_policy",
                network_allowed,
                "agent network policy must be isolated or explicitly covered by the trusted process approval scope",
            );
            decide(
                &mut trace,
                "sandbox_runtime_limit",
                member.is_some_and(|member| {
                    plan.max_command_seconds <= member.agent.limits.max_runtime_seconds
                }),
                "sandbox timeout must not exceed the agent runtime limit",
            );
            sandbox_plan = Some(plan);
        }
        Err(_) => {
            decide(
                &mut trace,
                "worktree_guardrails",
                false,
                "worktree registration or configuration could not be resolved",
            );
            decide(
                &mut trace,
                "filesystem_isolation",
                false,
                "sandbox plan was unavailable",
            );
            decide(
                &mut trace,
                "network_policy",
                false,
                "sandbox plan was unavailable",
            );
            decide(
                &mut trace,
                "sandbox_runtime_limit",
                false,
                "sandbox plan was unavailable",
            );
        }
    }

    let request_sha256 = request_scope_sha256(request)?;
    let command_sha256 = sha256(&serde_json::to_vec(&request.command)?);
    let receipt_id = receipt_id(request);
    let approval_scope_sha256 = hash_json(&serde_json::json!({
        "schema_version": MISSION_EXECUTION_APPROVAL_SCHEMA_VERSION,
        "request_sha256": request_sha256,
        "mission_revision": mission.revision,
        "executor_id": canonical_executor,
        "command_sha256": command_sha256,
        "worktree_id": sandbox_plan.as_ref().map(|plan| plan.worktree_id.as_str()),
        "config_sha256": sandbox_plan.as_ref().map(|plan| plan.config_sha256.as_str()),
        "allow_trusted_process_runtime": request.allow_trusted_process_runtime,
        "requested_evidence": request.requested_evidence,
        "gate_evidence_contract": gate_evidence_contract,
        "policy_trace": trace,
    }))?;
    let allowed = trace.iter().all(|decision| decision.allowed);
    Ok(MissionExecutionPlan {
        schema_version: MISSION_EXECUTION_PLAN_SCHEMA_VERSION.to_string(),
        status: if allowed { "ready" } else { "blocked" }.to_string(),
        allowed,
        receipt_id,
        request_sha256,
        approval_scope_sha256,
        mission_id: mission.id,
        workflow_id: mission.workflow_id,
        mission_revision: mission.revision,
        task_id: request.task_id.clone(),
        agent_id: request.agent_id.clone(),
        executor_id: canonical_executor,
        command_sha256,
        worktree_id: sandbox_plan.as_ref().map(|plan| plan.worktree_id.clone()),
        requested_evidence: request.requested_evidence.clone(),
        gate_evidence_contract,
        policy_trace: trace,
        sandbox_plan,
    })
}

pub fn build_mission_execution_approval(
    plan: &MissionExecutionPlan,
    approved_by: &str,
    ttl_seconds: u64,
) -> Result<MissionExecutionApproval> {
    if approved_by.trim().is_empty() {
        bail!("mission execution approval requires a non-empty approver");
    }
    if ttl_seconds == 0 || ttl_seconds > MAX_APPROVAL_TTL_SECONDS {
        bail!(
            "mission execution approval TTL must be between 1 and {MAX_APPROVAL_TTL_SECONDS} seconds"
        );
    }
    let issued_at = Utc::now();
    let ttl = i64::try_from(ttl_seconds).context("approval TTL exceeds supported range")?;
    let expires_at = issued_at
        .checked_add_signed(Duration::seconds(ttl))
        .context("approval expiry overflow")?;
    Ok(MissionExecutionApproval {
        schema_version: MISSION_EXECUTION_APPROVAL_SCHEMA_VERSION.to_string(),
        approval_scope_sha256: plan.approval_scope_sha256.clone(),
        approved_by: approved_by.trim().to_string(),
        issued_at: issued_at.to_rfc3339(),
        expires_at: expires_at.to_rfc3339(),
    })
}

pub fn execute_mission_command(
    store: &ForgeStore,
    request: MissionExecutionRequest,
) -> Result<MissionExecutionResult> {
    let plan = plan_mission_execution(store, &request)?;
    if request.dry_run {
        let receipt = dry_run_receipt(store, &request, &plan)?;
        return Ok(MissionExecutionResult {
            status: receipt.status.clone(),
            persisted: false,
            replayed: false,
            plan: Some(plan),
            receipt,
        });
    }
    validate_approval(&plan, request.approval.as_ref())?;
    let claim = claim_execution(store, &request, &plan)?;
    if let Some(receipt) = claim.replay {
        return Ok(MissionExecutionResult {
            status: receipt.status.clone(),
            persisted: true,
            replayed: true,
            plan: None,
            receipt,
        });
    }

    let started = Utc::now();
    if !plan.allowed {
        let receipt = build_receipt(
            store,
            &request,
            &plan,
            started,
            Utc::now(),
            "blocked_by_policy",
            false,
            false,
            None,
            MissionExecutionObservations::default(),
        )?;
        finalize_claim(store, &claim.owner_token, receipt.clone())?;
        return Ok(MissionExecutionResult {
            status: receipt.status.clone(),
            persisted: true,
            replayed: false,
            plan: None,
            receipt,
        });
    }

    mark_execution_started(store, &plan.receipt_id, &claim.owner_token, started)?;
    let sandbox_receipt = run_worktree_sandbox(store, sandbox_request(&request), true);
    let finished = Utc::now();
    let (status, attempted, executed, sandbox, observations) = match sandbox_receipt {
        Ok(receipt) => {
            let completed = receipt.status == "sandbox_completed"
                && receipt.exit_code == Some(0)
                && !receipt.timed_out;
            let observations = if completed {
                observe_execution_semantics(
                    &request.command,
                    &plan.gate_evidence_contract,
                    &receipt,
                )
            } else {
                MissionExecutionObservations::default()
            };
            let status = if !receipt.executed {
                "blocked_before_start"
            } else if completed {
                "completed"
            } else if receipt.timed_out {
                "timed_out"
            } else {
                "failed"
            };
            (
                status,
                receipt.execution_attempted,
                receipt.executed,
                Some(MissionSandboxEvidence::from(&receipt)),
                observations,
            )
        }
        Err(_) => (
            "indeterminate",
            true,
            false,
            None,
            MissionExecutionObservations::default(),
        ),
    };
    let receipt = build_receipt(
        store,
        &request,
        &plan,
        started,
        finished,
        status,
        attempted,
        executed,
        sandbox,
        observations,
    )?;
    finalize_claim(store, &claim.owner_token, receipt.clone())?;
    Ok(MissionExecutionResult {
        status: receipt.status.clone(),
        persisted: true,
        replayed: false,
        plan: None,
        receipt,
    })
}

pub fn list_mission_execution_receipts(
    store: &ForgeStore,
    mission_id: Option<&str>,
    task_id: Option<&str>,
) -> Result<MissionExecutionListReport> {
    ensure_consumption_column(store)?;
    let connection = open_configured_connection(store.path())?;
    let mut statement = connection.prepare(
        r#"
        SELECT execution.receipt_id, execution.mission_id, execution.workflow_id,
               execution.mission_revision, execution.task_id, execution.agent_id,
               execution.executor_id, execution.state, execution.request_sha256,
               execution.execution_started_at, execution.receipt_json,
               execution.consumed_at, execution.consumed_by_submission,
               execution.lease_expires_at,
               (
                   SELECT reconciliation.data_json
                   FROM mission_execution_reconciliations AS reconciliation
                   WHERE reconciliation.receipt_id = execution.receipt_id
                   ORDER BY reconciliation.created_at DESC, reconciliation.reconciliation_id DESC
                   LIMIT 1
               )
        FROM mission_execution_receipts AS execution
        ORDER BY execution.created_at, execution.receipt_id
        "#,
    )?;
    let rows = statement.query_map([], map_execution_db_record)?;
    let mut records = Vec::new();
    for row in rows {
        let row = row?;
        if mission_id.is_some_and(|expected| row.mission_id != expected)
            || task_id.is_some_and(|expected| row.task_id != expected)
        {
            continue;
        }
        records.push(to_public_record(row)?);
    }
    Ok(MissionExecutionListReport {
        schema_version: MISSION_EXECUTION_LIST_SCHEMA_VERSION.to_string(),
        status: "listed".to_string(),
        records,
    })
}

pub fn inspect_mission_execution_receipt(
    store: &ForgeStore,
    receipt_id: &str,
) -> Result<MissionExecutionRecord> {
    let record = load_db_record(store, receipt_id)?
        .with_context(|| format!("mission execution receipt not found: {receipt_id}"))?;
    to_public_record(record)
}

pub fn reconcile_mission_execution(
    store: &ForgeStore,
    request: MissionExecutionReconcileRequest,
) -> Result<MissionExecutionReconcileResult> {
    validate_reconciliation_request(&request)?;
    let mut connection = open_configured_connection(store.path())?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;

    let existing: Option<String> = transaction
        .query_row(
            r#"
            SELECT data_json
            FROM mission_execution_reconciliations
            WHERE receipt_id=?1
            "#,
            [&request.receipt_id],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(existing) = existing {
        let reconciliation: MissionExecutionReconciliation = serde_json::from_str(&existing)?;
        if reconciliation.outcome != request.outcome.trim()
            || reconciliation.approved_by != request.approved_by.trim()
            || reconciliation.reason != request.reason.trim()
        {
            bail!(
                "mission execution was already reconciled with different approval or outcome evidence"
            );
        }
        transaction.commit()?;
        return Ok(MissionExecutionReconcileResult {
            status: reconciliation.resulting_state.clone(),
            replayed: true,
            reconciliation,
        });
    }

    let execution = load_db_record_from_connection(&transaction, &request.receipt_id)?
        .with_context(|| {
            format!(
                "mission execution receipt not found: {}",
                request.receipt_id
            )
        })?;
    if !matches!(
        execution.state.as_str(),
        "indeterminate" | "failed" | "timed_out"
    ) {
        bail!(
            "mission execution state `{}` cannot be reconciled for a no-effect retry",
            execution.state
        );
    }
    if execution.execution_started_at.is_none() {
        bail!("mission execution reconciliation requires evidence that execution was started");
    }

    let confirmed_at = Utc::now().to_rfc3339();
    let resulting_state = "reconciled_no_effect_retry".to_string();
    let mut reconciliation = MissionExecutionReconciliation {
        schema_version: MISSION_EXECUTION_RECONCILIATION_SCHEMA_VERSION.to_string(),
        reconciliation_id: format!("mission_reconcile_{}", Uuid::new_v4().simple()),
        reconciliation_sha256: String::new(),
        receipt_id: execution.receipt_id.clone(),
        mission_id: execution.mission_id.clone(),
        workflow_id: execution.workflow_id.clone(),
        mission_revision: execution.mission_revision,
        task_id: execution.task_id.clone(),
        previous_state: execution.state.clone(),
        resulting_state: resulting_state.clone(),
        outcome: request.outcome.trim().to_string(),
        approved_by: request.approved_by.trim().to_string(),
        reason: request.reason.trim().to_string(),
        confirmed_at: confirmed_at.clone(),
    };
    reconciliation.reconciliation_sha256 = hash_reconciliation(&reconciliation)?;
    let reconciliation_json = serde_json::to_string(&reconciliation)?;
    let changed = transaction.execute(
        r#"
        UPDATE mission_execution_receipts
        SET state=?1, owner_token=NULL, lease_expires_at=NULL, updated_at=?2
        WHERE receipt_id=?3 AND state=?4
        "#,
        params![
            resulting_state,
            confirmed_at,
            execution.receipt_id,
            execution.state
        ],
    )?;
    if changed != 1 {
        bail!("mission execution state changed before reconciliation could be committed");
    }
    transaction.execute(
        r#"
        INSERT INTO mission_execution_reconciliations (
            reconciliation_id, receipt_id, mission_id, workflow_id, mission_revision,
            task_id, previous_state, resulting_state, outcome, approved_by, reason,
            data_sha256, data_json, created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
        "#,
        params![
            reconciliation.reconciliation_id,
            reconciliation.receipt_id,
            reconciliation.mission_id,
            reconciliation.workflow_id,
            i64::try_from(reconciliation.mission_revision)
                .context("mission revision exceeds SQLite range")?,
            reconciliation.task_id,
            reconciliation.previous_state,
            reconciliation.resulting_state,
            reconciliation.outcome,
            reconciliation.approved_by,
            reconciliation.reason,
            reconciliation.reconciliation_sha256,
            reconciliation_json,
            reconciliation.confirmed_at,
        ],
    )?;
    transaction.execute(
        "INSERT INTO events (workflow_id, kind, data_json) VALUES (?1, ?2, ?3)",
        params![
            reconciliation.workflow_id,
            "mission.execution.reconciled",
            serde_json::to_string(&reconciliation)?,
        ],
    )?;
    transaction.commit()?;

    Ok(MissionExecutionReconcileResult {
        status: resulting_state,
        replayed: false,
        reconciliation,
    })
}

pub fn load_mission_execution_receipt(
    store: &ForgeStore,
    receipt_id: &str,
) -> Result<MissionExecutionReceipt> {
    inspect_mission_execution_receipt(store, receipt_id)?
        .receipt
        .with_context(|| {
            format!("mission execution {receipt_id} has no final receipt and cannot be submitted")
        })
}

pub fn claim_mission_execution_receipt_for_submission(
    store: &ForgeStore,
    receipt_id: &str,
    mission_id: &str,
    expected_mission_revision: u64,
    task_id: &str,
    agent_id: &str,
    submission_key: &str,
) -> Result<MissionExecutionReceipt> {
    if submission_key.trim().is_empty() {
        bail!("mission submission receipt claim requires an idempotency key");
    }
    ensure_consumption_column(store)?;
    let mut connection = open_configured_connection(store.path())?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mission_data_json: Option<String> = transaction
        .query_row(
            "SELECT data_json FROM forge_missions WHERE id=?1",
            [mission_id],
            |row| row.get(0),
        )
        .optional()?;
    let mission_data_json =
        mission_data_json.with_context(|| format!("mission not found: {mission_id}"))?;
    let current_mission_revision = serde_json::from_str::<serde_json::Value>(&mission_data_json)?
        .get("revision")
        .and_then(serde_json::Value::as_u64)
        .context("persisted mission revision is missing or invalid")?;
    if current_mission_revision != expected_mission_revision {
        bail!(
            "mission revision changed before receipt claim: expected {}, found {}",
            expected_mission_revision,
            current_mission_revision
        );
    }

    let existing: Option<(i64, Option<String>, Option<String>)> = transaction
        .query_row(
            r#"
            SELECT mission_revision, consumed_by_submission, receipt_json
            FROM mission_execution_receipts
            WHERE receipt_id=?1
            "#,
            [receipt_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((stored_revision, existing, receipt_json)) = existing else {
        bail!("mission execution receipt disappeared before submission claim");
    };
    let stored_revision =
        u64::try_from(stored_revision).context("negative mission execution receipt revision")?;
    if stored_revision != expected_mission_revision {
        bail!(
            "mission execution receipt revision {} does not match expected mission revision {}",
            stored_revision,
            expected_mission_revision
        );
    }
    let receipt_json =
        receipt_json.context("mission execution has no final receipt and cannot be submitted")?;
    let receipt: MissionExecutionReceipt = serde_json::from_str(&receipt_json)?;
    validate_submittable_receipt(&receipt, mission_id, task_id, agent_id)?;
    if receipt.mission_revision != expected_mission_revision {
        bail!(
            "mission execution receipt payload revision {} does not match expected mission revision {}",
            receipt.mission_revision,
            expected_mission_revision
        );
    }
    if let Some(existing) = existing {
        if existing != submission_key {
            bail!("mission execution receipt was already consumed by another submission");
        }
    } else {
        let changed = transaction.execute(
            r#"
            UPDATE mission_execution_receipts
            SET consumed_at=?1, consumed_by_submission=?2, updated_at=?1
            WHERE receipt_id=?3
              AND mission_revision=?4
              AND consumed_by_submission IS NULL
            "#,
            params![
                Utc::now().to_rfc3339(),
                submission_key,
                receipt_id,
                i64::try_from(expected_mission_revision)
                    .context("mission revision exceeds SQLite range")?,
            ],
        )?;
        if changed != 1 {
            bail!("mission execution receipt could not be claimed for submission");
        }
    }
    transaction.commit()?;
    Ok(receipt)
}

pub fn release_mission_execution_receipt_submission_claim(
    store: &ForgeStore,
    receipt_id: &str,
    submission_key: &str,
) -> Result<()> {
    if submission_key.trim().is_empty() {
        bail!("mission submission receipt release requires an idempotency key");
    }
    ensure_consumption_column(store)?;
    let mut connection = open_configured_connection(store.path())?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let changed = transaction.execute(
        r#"
        UPDATE mission_execution_receipts
        SET consumed_at=NULL, consumed_by_submission=NULL, updated_at=?1
        WHERE receipt_id=?2 AND consumed_by_submission=?3
        "#,
        params![Utc::now().to_rfc3339(), receipt_id, submission_key],
    )?;
    if changed != 1 {
        let owner: Option<Option<String>> = transaction
            .query_row(
                "SELECT consumed_by_submission FROM mission_execution_receipts WHERE receipt_id=?1",
                [receipt_id],
                |row| row.get(0),
            )
            .optional()?;
        match owner {
            None => {
                bail!("mission execution receipt disappeared before submission claim release")
            }
            Some(None) => {
                transaction.commit()?;
                return Ok(());
            }
            Some(Some(owner)) => {
                bail!(
                    "mission execution receipt submission claim belongs to `{owner}`, not `{submission_key}`"
                )
            }
        }
    }
    transaction.commit()?;
    Ok(())
}

pub fn verify_mission_execution_receipt(receipt: &MissionExecutionReceipt) -> Result<()> {
    if receipt.schema_version != MISSION_EXECUTION_RECEIPT_SCHEMA_VERSION
        && receipt.schema_version != LEGACY_MISSION_EXECUTION_RECEIPT_SCHEMA_VERSION
    {
        bail!("unsupported mission execution receipt schema");
    }
    let mut canonical = receipt.clone();
    let observed = canonical.receipt_sha256.clone();
    canonical.receipt_sha256.clear();
    canonical.consumed_at = None;
    canonical.consumed_by_submission = None;
    let expected = hash_json(&canonical)?;
    if observed != expected {
        bail!("mission execution receipt hash mismatch");
    }
    resolved_mission_execution_metrics(receipt)?;
    verify_typed_claims(receipt)?;
    Ok(())
}

pub fn resolved_mission_execution_metrics(
    receipt: &MissionExecutionReceipt,
) -> Result<ResolvedMissionExecutionMetrics> {
    let sandbox = receipt.sandbox.as_ref();
    let sandbox_sha256 = sandbox.map(hash_json).transpose()?;
    let legacy = receipt.schema_version == LEGACY_MISSION_EXECUTION_RECEIPT_SCHEMA_VERSION;
    if receipt.schema_version == MISSION_EXECUTION_RECEIPT_SCHEMA_VERSION
        && (matches!(receipt.observed_cost_usd, MissionExecutionMetric::Legacy(_))
            || matches!(
                receipt.observed_files_changed,
                MissionExecutionMetric::Legacy(_)
            )
            || matches!(
                receipt.observed_external_calls,
                MissionExecutionMetric::Legacy(_)
            ))
    {
        bail!("mission execution receipt v3 requires explicit metric observations");
    }

    let cost_usd = resolve_cost_metric(
        &receipt.observed_cost_usd,
        legacy,
        sandbox,
        sandbox_sha256.as_deref(),
    )?;
    let files_changed = resolve_count_metric(
        &receipt.observed_files_changed,
        legacy,
        sandbox,
        sandbox_sha256.as_deref(),
        CountMetricKind::FilesChanged,
    )?;
    let external_calls = resolve_count_metric(
        &receipt.observed_external_calls,
        legacy,
        sandbox,
        sandbox_sha256.as_deref(),
        CountMetricKind::ExternalCalls,
    )?;
    Ok(ResolvedMissionExecutionMetrics {
        cost_usd,
        files_changed,
        external_calls,
    })
}

#[derive(Debug, Clone, Copy)]
enum CountMetricKind {
    FilesChanged,
    ExternalCalls,
}

fn resolve_cost_metric(
    metric: &MissionExecutionMetric<f64>,
    legacy: bool,
    sandbox: Option<&MissionSandboxEvidence>,
    sandbox_sha256: Option<&str>,
) -> Result<MissionExecutionMetricObservation<f64>> {
    match metric {
        MissionExecutionMetric::Explicit(observation) => {
            validate_metric_shape(observation)?;
            match observation.status {
                MissionExecutionMetricStatus::Unknown => Ok(observation.clone()),
                MissionExecutionMetricStatus::Observed => {
                    let value = observation
                        .value
                        .context("observed cost is missing its value")?;
                    if !value.is_finite() || value < 0.0 {
                        bail!("observed execution cost must be finite and non-negative");
                    }
                    if value != 0.0
                        || observation.source.as_deref()
                            != Some(METRIC_SOURCE_DETERMINISTIC_LOCAL_COST)
                        || !sandbox.is_some_and(sandbox_proves_zero_external_cost)
                    {
                        bail!("observed zero execution cost lacks deterministic isolation proof");
                    }
                    validate_metric_evidence(observation, sandbox_sha256)?;
                    Ok(observation.clone())
                }
            }
        }
        MissionExecutionMetric::Legacy(value) => {
            if !legacy {
                bail!("legacy scalar execution cost is unsupported by this receipt schema");
            }
            if *value == 0.0 && sandbox.is_some_and(sandbox_proves_zero_external_cost) {
                Ok(observed_metric(
                    0.0,
                    METRIC_SOURCE_DETERMINISTIC_LOCAL_COST,
                    sandbox_sha256,
                )?)
            } else {
                Ok(unknown_metric(
                    "legacy_cost_has_no_trusted_observation_source",
                ))
            }
        }
    }
}

fn resolve_count_metric(
    metric: &MissionExecutionMetric<usize>,
    legacy: bool,
    sandbox: Option<&MissionSandboxEvidence>,
    sandbox_sha256: Option<&str>,
    kind: CountMetricKind,
) -> Result<MissionExecutionMetricObservation<usize>> {
    let (source, proof, unknown_reason) = match kind {
        CountMetricKind::FilesChanged => (
            METRIC_SOURCE_READ_ONLY_WORKTREE,
            sandbox.is_some_and(sandbox_proves_zero_files_changed),
            "worktree_file_changes_were_not_observed",
        ),
        CountMetricKind::ExternalCalls => (
            METRIC_SOURCE_NETWORK_ISOLATION,
            sandbox.is_some_and(|sandbox| sandbox.network_isolation_enforced),
            "external_calls_were_not_observed",
        ),
    };
    match metric {
        MissionExecutionMetric::Explicit(observation) => {
            validate_metric_shape(observation)?;
            match observation.status {
                MissionExecutionMetricStatus::Unknown => Ok(observation.clone()),
                MissionExecutionMetricStatus::Observed => {
                    let value = observation
                        .value
                        .context("observed execution count is missing its value")?;
                    if value != 0 || observation.source.as_deref() != Some(source) || !proof {
                        bail!("observed zero execution count lacks sandbox isolation proof");
                    }
                    validate_metric_evidence(observation, sandbox_sha256)?;
                    Ok(observation.clone())
                }
            }
        }
        MissionExecutionMetric::Legacy(value) => {
            if !legacy {
                bail!("legacy scalar execution count is unsupported by this receipt schema");
            }
            if *value == 0 && proof {
                Ok(observed_metric(0, source, sandbox_sha256)?)
            } else {
                Ok(unknown_metric(unknown_reason))
            }
        }
    }
}

fn validate_metric_shape<T>(observation: &MissionExecutionMetricObservation<T>) -> Result<()> {
    if observation.schema_version != MISSION_EXECUTION_METRIC_SCHEMA_VERSION {
        bail!("unsupported mission execution metric schema");
    }
    match observation.status {
        MissionExecutionMetricStatus::Observed => {
            if observation.value.is_none()
                || observation
                    .source
                    .as_deref()
                    .is_none_or(|source| source.trim().is_empty())
                || observation
                    .evidence_sha256
                    .as_deref()
                    .is_none_or(|sha256| sha256.len() != 64)
                || observation.reason.is_some()
            {
                bail!("observed mission execution metric has incomplete provenance");
            }
        }
        MissionExecutionMetricStatus::Unknown => {
            if observation.value.is_some()
                || observation.source.is_some()
                || observation.evidence_sha256.is_some()
                || observation
                    .reason
                    .as_deref()
                    .is_none_or(|reason| reason.trim().is_empty())
            {
                bail!("unknown mission execution metric cannot carry a value or fake provenance");
            }
        }
    }
    Ok(())
}

fn validate_metric_evidence<T>(
    observation: &MissionExecutionMetricObservation<T>,
    sandbox_sha256: Option<&str>,
) -> Result<()> {
    if observation.evidence_sha256.as_deref() != sandbox_sha256 {
        bail!("mission execution metric provenance does not match sandbox evidence");
    }
    Ok(())
}

fn observed_metric<T>(
    value: T,
    source: &str,
    sandbox_sha256: Option<&str>,
) -> Result<MissionExecutionMetricObservation<T>> {
    let evidence_sha256 =
        sandbox_sha256.context("observed mission execution metric requires sandbox evidence")?;
    Ok(MissionExecutionMetricObservation {
        schema_version: MISSION_EXECUTION_METRIC_SCHEMA_VERSION.to_string(),
        status: MissionExecutionMetricStatus::Observed,
        value: Some(value),
        source: Some(source.to_string()),
        evidence_sha256: Some(evidence_sha256.to_string()),
        reason: None,
    })
}

fn unknown_metric<T>(reason: &str) -> MissionExecutionMetricObservation<T> {
    MissionExecutionMetricObservation {
        schema_version: MISSION_EXECUTION_METRIC_SCHEMA_VERSION.to_string(),
        status: MissionExecutionMetricStatus::Unknown,
        value: None,
        source: None,
        evidence_sha256: None,
        reason: Some(reason.to_string()),
    }
}

fn sandbox_proves_zero_files_changed(sandbox: &MissionSandboxEvidence) -> bool {
    sandbox.runtime == "bubblewrap"
        && sandbox.filesystem_isolation_enforced
        && sandbox.worktree_read_only
        && sandbox.writes_restricted_to_sandbox
}

fn sandbox_proves_zero_external_cost(sandbox: &MissionSandboxEvidence) -> bool {
    sandbox.network_isolation_enforced
        && sandbox
            .command
            .first()
            .map(|command| command_basename(command))
            .is_some_and(|command| matches!(command.as_str(), "cargo" | "git"))
}

pub fn verified_mission_execution_claims(
    receipt: &MissionExecutionReceipt,
    mission_id: &str,
    workflow_id: &str,
    mission_revision: u64,
    task_id: &str,
    agent_id: &str,
) -> Result<VerifiedMissionExecutionClaims> {
    verify_mission_execution_receipt(receipt)?;
    if receipt.mission_id != mission_id
        || receipt.workflow_id != workflow_id
        || receipt.mission_revision != mission_revision
        || receipt.task_id != task_id
        || receipt.agent_id != agent_id
    {
        bail!("mission execution claims do not match the requested operational scope");
    }
    Ok(VerifiedMissionExecutionClaims {
        receipt_id: receipt.receipt_id.clone(),
        receipt_sha256: receipt.receipt_sha256.clone(),
        mission_revision: receipt.mission_revision,
        claims: receipt
            .claims
            .iter()
            .filter_map(|claim| match claim.kind {
                MissionExecutionReceiptClaimKind::ExecutionCompleted => {
                    Some(MissionExecutionClaimKind::ExecutionCompleted)
                }
                MissionExecutionReceiptClaimKind::TestsPassed => {
                    Some(MissionExecutionClaimKind::TestsPassed)
                }
                MissionExecutionReceiptClaimKind::GateEvidence => None,
            })
            .collect(),
        gate_evidence: receipt
            .claims
            .iter()
            .filter(|claim| claim.kind == MissionExecutionReceiptClaimKind::GateEvidence)
            .map(|claim| VerifiedMissionGateEvidenceClaim {
                evidence_kind: claim
                    .evidence_kind
                    .clone()
                    .expect("verified gate evidence claim must carry an evidence kind"),
                gate_ids: claim.gate_ids.clone(),
                scope: claim.scope,
                locator: claim.locator.clone(),
                sha256: claim.sha256.clone(),
                command_sha256: claim.command_sha256.clone(),
            })
            .collect(),
    })
}

fn verify_typed_claims(receipt: &MissionExecutionReceipt) -> Result<()> {
    if !receipt.validations.is_empty() {
        bail!("mission execution receipt contains unsupported free-form semantic claims");
    }
    let expected_sandbox_locator = format!("mission-execution://{}/sandbox", receipt.receipt_id);
    let successful = receipt.status == "completed"
        && receipt.allowed
        && receipt.execution_attempted
        && receipt.executed
        && receipt.exit_code == Some(0)
        && !receipt.timed_out
        && receipt.sandbox.as_ref().is_some_and(|sandbox| {
            sandbox.status == "sandbox_completed"
                && sandbox.exit_code == Some(0)
                && !sandbox.timed_out
        });
    let expected_sandbox_sha256 = receipt.sandbox.as_ref().map(hash_json).transpose()?;
    let mut requested_evidence = BTreeSet::new();
    for evidence_kind in &receipt.requested_evidence {
        if evidence_kind.trim().is_empty() || evidence_kind.trim() != evidence_kind {
            bail!("mission execution receipt contains an invalid requested evidence kind");
        }
        if matches!(
            evidence_kind.as_str(),
            "execution_completed" | "tests_passed"
        ) {
            bail!("mission execution receipt requests a reserved built-in evidence kind");
        }
        if !requested_evidence.insert(evidence_kind.clone()) {
            bail!("mission execution receipt contains duplicate requested evidence");
        }
    }

    let mut evidence_by_kind = BTreeMap::new();
    let mut evidence_locators = BTreeSet::new();
    let mut observed_tests_passed = false;
    let mut observed_gate_evidence = BTreeSet::new();
    for evidence in &receipt.evidence {
        if evidence.kind.trim().is_empty()
            || evidence.kind.trim() != evidence.kind
            || evidence.locator.trim().is_empty()
            || evidence.locator.trim() != evidence.locator
        {
            bail!("mission execution receipt contains invalid evidence metadata");
        }
        if evidence_by_kind
            .insert(evidence.kind.as_str(), evidence)
            .is_some()
        {
            bail!("mission execution receipt contains duplicate evidence kinds");
        }
        if !evidence_locators.insert(evidence.locator.as_str()) {
            bail!("mission execution receipt contains duplicate evidence locators");
        }
        if let Some(observation) = evidence.observation.as_ref() {
            let bytes = serde_json::to_vec(observation)?.len();
            if evidence.bytes != bytes || evidence.sha256 != hash_json(observation)? {
                bail!("mission execution observation evidence failed integrity validation");
            }
        }

        if evidence.kind == "tests_passed" {
            let observation = evidence
                .observation
                .as_ref()
                .context("tests_passed evidence requires a semantic observation")?;
            let sandbox = receipt
                .sandbox
                .as_ref()
                .context("tests_passed evidence requires sandbox evidence")?;
            let expected_locator = format!(
                "mission-execution://{}/observations/tests_passed",
                receipt.receipt_id
            );
            if !successful
                || evidence.locator != expected_locator
                || !test_observation_is_valid(observation, sandbox)
            {
                bail!("tests_passed evidence is not backed by a valid test observation");
            }
            observed_tests_passed = true;
        } else if let Some(evidence_kind) = evidence.kind.strip_prefix("gate_evidence:") {
            if evidence_kind.trim().is_empty()
                || evidence_kind.trim() != evidence_kind
                || !requested_evidence.contains(evidence_kind)
            {
                bail!("mission execution receipt contains unrequested gate evidence");
            }
            let observation = evidence
                .observation
                .as_ref()
                .context("gate evidence requires a semantic observation")?;
            let sandbox = receipt
                .sandbox
                .as_ref()
                .context("gate evidence requires sandbox evidence")?;
            let expected_locator = format!(
                "mission-execution://{}/observations/{evidence_kind}",
                receipt.receipt_id
            );
            if !successful
                || evidence.locator != expected_locator
                || !gate_observation_is_valid(evidence_kind, observation, sandbox)
            {
                bail!("gate evidence is not backed by a valid semantic observation");
            }
            observed_gate_evidence.insert(evidence_kind.to_string());
        } else if evidence.observation.is_some() {
            bail!("mission execution receipt contains unsupported observation evidence");
        }
    }

    let mut builtin_kinds = BTreeSet::new();
    let mut gate_evidence_kinds = BTreeSet::new();
    for claim in &receipt.claims {
        if claim.schema_version != MISSION_EXECUTION_CLAIM_SCHEMA_VERSION {
            bail!("unsupported mission execution claim schema");
        }
        if claim.scope != MissionExecutionClaimScope::Operational {
            bail!("bounded or non-operational evidence cannot satisfy mission execution claims");
        }
        if claim.command_sha256 != receipt.command_sha256
            || claim.status != receipt.status
            || claim.exit_code != receipt.exit_code
            || claim.timed_out != receipt.timed_out
        {
            bail!("mission execution claim evidence does not match its receipt");
        }
        if !successful {
            bail!("unsuccessful mission execution cannot carry passed typed claims");
        }
        match claim.kind {
            MissionExecutionReceiptClaimKind::ExecutionCompleted => {
                if claim.evidence_kind.is_some() || !claim.gate_ids.is_empty() {
                    bail!("built-in execution claims cannot carry gate evidence metadata");
                }
                if !builtin_kinds.insert(claim.kind) {
                    bail!("mission execution receipt contains a duplicate built-in typed claim");
                }
                let expected_sha256 = expected_sandbox_sha256
                    .as_ref()
                    .context("execution_completed claim requires sandbox evidence")?;
                if claim.locator != expected_sandbox_locator
                    || claim.sha256.as_str() != expected_sha256.as_str()
                {
                    bail!("execution_completed claim does not match sandbox evidence");
                }
            }
            MissionExecutionReceiptClaimKind::TestsPassed => {
                if claim.evidence_kind.is_some() || !claim.gate_ids.is_empty() {
                    bail!("built-in execution claims cannot carry gate evidence metadata");
                }
                if !builtin_kinds.insert(claim.kind) {
                    bail!("mission execution receipt contains a duplicate built-in typed claim");
                }
                let evidence = evidence_by_kind
                    .get("tests_passed")
                    .context("tests_passed claim requires observation evidence")?;
                if claim.locator != evidence.locator || claim.sha256 != evidence.sha256 {
                    bail!("tests_passed claim does not match its observation evidence");
                }
            }
            MissionExecutionReceiptClaimKind::GateEvidence => {
                let evidence_kind = claim
                    .evidence_kind
                    .as_deref()
                    .context("gate evidence claim requires an evidence_kind")?;
                if evidence_kind.trim().is_empty() || evidence_kind.trim() != evidence_kind {
                    bail!("gate evidence claim contains an invalid evidence_kind");
                }
                if matches!(evidence_kind, "execution_completed" | "tests_passed") {
                    bail!("gate evidence claim cannot replace a reserved built-in claim");
                }
                if !requested_evidence.contains(evidence_kind) {
                    bail!("gate evidence claim was not explicitly requested");
                }
                if claim.gate_ids.is_empty() {
                    bail!("gate evidence claim must identify at least one quality gate");
                }
                let mut gate_ids = BTreeSet::new();
                for gate_id in &claim.gate_ids {
                    if gate_id.trim().is_empty()
                        || gate_id.trim() != gate_id
                        || !gate_ids.insert(gate_id)
                    {
                        bail!("gate evidence claim contains invalid or duplicate gate ids");
                    }
                }
                if !gate_evidence_kinds.insert(evidence_kind.to_string()) {
                    bail!("mission execution receipt contains duplicate gate evidence claims");
                }
                let evidence_key = format!("gate_evidence:{evidence_kind}");
                let evidence = evidence_by_kind
                    .get(evidence_key.as_str())
                    .context("gate evidence claim requires observation evidence")?;
                if claim.locator != evidence.locator || claim.sha256 != evidence.sha256 {
                    bail!("gate evidence claim does not match its observation evidence");
                }
            }
        }
    }
    let execution_claims = receipt
        .claims
        .iter()
        .filter(|claim| claim.kind == MissionExecutionReceiptClaimKind::ExecutionCompleted)
        .count();
    let test_claims = receipt
        .claims
        .iter()
        .filter(|claim| claim.kind == MissionExecutionReceiptClaimKind::TestsPassed)
        .count();
    if execution_claims != usize::from(successful)
        || test_claims != usize::from(observed_tests_passed)
        || receipt.tests_passed != usize::from(observed_tests_passed)
        || receipt.tests_passed > 1
    {
        bail!("mission execution typed claims disagree with the recorded outcome");
    }
    if gate_evidence_kinds != observed_gate_evidence {
        bail!(
            "mission execution gate evidence claims do not correspond to valid semantic observations"
        );
    }
    Ok(())
}

fn validate_submittable_receipt(
    receipt: &MissionExecutionReceipt,
    mission_id: &str,
    task_id: &str,
    agent_id: &str,
) -> Result<()> {
    verify_mission_execution_receipt(receipt)?;
    if receipt.mission_id != mission_id
        || receipt.task_id != task_id
        || receipt.agent_id != agent_id
    {
        bail!("mission execution receipt identity does not match the submission");
    }
    if receipt.status != "completed"
        || !receipt.allowed
        || !receipt.execution_attempted
        || !receipt.executed
        || receipt.exit_code != Some(0)
        || receipt.timed_out
        || receipt.tests_failed != 0
    {
        bail!("mission execution receipt is not a successful submittable execution");
    }
    Ok(())
}

fn dry_run_receipt(
    store: &ForgeStore,
    request: &MissionExecutionRequest,
    plan: &MissionExecutionPlan,
) -> Result<MissionExecutionReceipt> {
    let now = Utc::now();
    build_receipt(
        store,
        request,
        plan,
        now,
        now,
        if plan.allowed { "planned" } else { "blocked" },
        false,
        false,
        None,
        MissionExecutionObservations::default(),
    )
}

fn build_metric_observations(
    executed: bool,
    sandbox: Option<&MissionSandboxEvidence>,
) -> Result<(
    MissionExecutionMetric<f64>,
    MissionExecutionMetric<usize>,
    MissionExecutionMetric<usize>,
)> {
    let sandbox_sha256 = sandbox.map(hash_json).transpose()?;
    if !executed {
        return Ok((
            MissionExecutionMetric::unknown("execution_did_not_start"),
            MissionExecutionMetric::unknown("execution_did_not_start"),
            MissionExecutionMetric::unknown("execution_did_not_start"),
        ));
    }
    let cost_usd = if sandbox.is_some_and(sandbox_proves_zero_external_cost) {
        MissionExecutionMetric::observed(
            0.0,
            METRIC_SOURCE_DETERMINISTIC_LOCAL_COST,
            sandbox_sha256
                .clone()
                .context("isolated cost observation requires sandbox evidence")?,
        )
    } else {
        MissionExecutionMetric::unknown("no_trusted_cost_observer")
    };
    let files_changed = if sandbox.is_some_and(sandbox_proves_zero_files_changed) {
        MissionExecutionMetric::observed(
            0,
            METRIC_SOURCE_READ_ONLY_WORKTREE,
            sandbox_sha256
                .clone()
                .context("read-only worktree observation requires sandbox evidence")?,
        )
    } else {
        MissionExecutionMetric::unknown("worktree_file_changes_were_not_observed")
    };
    let external_calls = if sandbox.is_some_and(|sandbox| sandbox.network_isolation_enforced) {
        MissionExecutionMetric::observed(
            0,
            METRIC_SOURCE_NETWORK_ISOLATION,
            sandbox_sha256.context("network isolation observation requires sandbox evidence")?,
        )
    } else {
        MissionExecutionMetric::unknown("external_calls_were_not_observed")
    };
    Ok((cost_usd, files_changed, external_calls))
}

#[allow(clippy::too_many_arguments)]
fn build_receipt(
    _store: &ForgeStore,
    request: &MissionExecutionRequest,
    plan: &MissionExecutionPlan,
    started: DateTime<Utc>,
    finished: DateTime<Utc>,
    status: &str,
    execution_attempted: bool,
    executed: bool,
    sandbox: Option<MissionSandboxEvidence>,
    observations: MissionExecutionObservations,
) -> Result<MissionExecutionReceipt> {
    if plan.requested_evidence != request.requested_evidence {
        bail!("mission execution plan evidence scope changed before receipt construction");
    }
    let successful = status == "completed"
        && plan.allowed
        && execution_attempted
        && executed
        && sandbox.as_ref().is_some_and(|sandbox| {
            sandbox.status == "sandbox_completed"
                && sandbox.exit_code == Some(0)
                && !sandbox.timed_out
        });
    let sandbox_locator = format!("mission-execution://{}/sandbox", plan.receipt_id);
    let sandbox_sha256 = sandbox.as_ref().map(hash_json).transpose()?;
    let mut evidence = sandbox
        .as_ref()
        .map(|sandbox| {
            vec![
                MissionExecutionEvidence {
                    kind: "sandbox_config".to_string(),
                    locator: "worktree.sandbox.config".to_string(),
                    sha256: sandbox.config_sha256.clone(),
                    bytes: 0,
                    observation: None,
                },
                MissionExecutionEvidence {
                    kind: "stdout".to_string(),
                    locator: "worktree.sandbox.stdout".to_string(),
                    sha256: sandbox.stdout_sha256.clone(),
                    bytes: sandbox.stdout_bytes,
                    observation: None,
                },
                MissionExecutionEvidence {
                    kind: "stderr".to_string(),
                    locator: "worktree.sandbox.stderr".to_string(),
                    sha256: sandbox.stderr_sha256.clone(),
                    bytes: sandbox.stderr_bytes,
                    observation: None,
                },
            ]
        })
        .unwrap_or_default();
    let observation_evidence =
        |kind: String, locator: String, observation: serde_json::Value| -> Result<_> {
            let bytes = serde_json::to_vec(&observation)?.len();
            Ok(MissionExecutionEvidence {
                kind,
                locator,
                sha256: hash_json(&observation)?,
                bytes,
                observation: Some(observation),
            })
        };
    let mut claims = Vec::new();
    if successful {
        let sandbox_sha256 = sandbox_sha256
            .as_ref()
            .context("successful mission execution is missing sandbox evidence")?;
        let claim = |kind, evidence_kind, gate_ids, locator, sha256| MissionExecutionClaimReceipt {
            schema_version: MISSION_EXECUTION_CLAIM_SCHEMA_VERSION.to_string(),
            kind,
            scope: MissionExecutionClaimScope::Operational,
            evidence_kind,
            gate_ids,
            locator,
            sha256,
            command_sha256: plan.command_sha256.clone(),
            status: status.to_string(),
            exit_code: sandbox.as_ref().and_then(|sandbox| sandbox.exit_code),
            timed_out: sandbox.as_ref().is_some_and(|sandbox| sandbox.timed_out),
        };
        claims.push(claim(
            MissionExecutionReceiptClaimKind::ExecutionCompleted,
            None,
            Vec::new(),
            sandbox_locator,
            sandbox_sha256.clone(),
        ));
        if let Some(observation) = observations.tests_passed {
            let observed = observation_evidence(
                "tests_passed".to_string(),
                format!(
                    "mission-execution://{}/observations/tests_passed",
                    plan.receipt_id
                ),
                observation,
            )?;
            claims.push(claim(
                MissionExecutionReceiptClaimKind::TestsPassed,
                None,
                Vec::new(),
                observed.locator.clone(),
                observed.sha256.clone(),
            ));
            evidence.push(observed);
        }
        for contract in &plan.gate_evidence_contract {
            let Some(observation) = observations.gate_evidence.get(&contract.evidence_kind) else {
                continue;
            };
            let observed = observation_evidence(
                format!("gate_evidence:{}", contract.evidence_kind),
                format!(
                    "mission-execution://{}/observations/{}",
                    plan.receipt_id, contract.evidence_kind
                ),
                observation.clone(),
            )?;
            claims.push(claim(
                MissionExecutionReceiptClaimKind::GateEvidence,
                Some(contract.evidence_kind.clone()),
                contract.gate_ids.clone(),
                observed.locator.clone(),
                observed.sha256.clone(),
            ));
            evidence.push(observed);
        }
    }
    let duration_ms = sandbox.as_ref().map_or(0, |sandbox| sandbox.duration_ms);
    let (observed_cost_usd, observed_files_changed, observed_external_calls) =
        build_metric_observations(executed, sandbox.as_ref())?;
    let mut receipt = MissionExecutionReceipt {
        schema_version: MISSION_EXECUTION_RECEIPT_SCHEMA_VERSION.to_string(),
        receipt_id: plan.receipt_id.clone(),
        receipt_sha256: String::new(),
        request_sha256: plan.request_sha256.clone(),
        approval_scope_sha256: plan.approval_scope_sha256.clone(),
        command_sha256: plan.command_sha256.clone(),
        status: status.to_string(),
        allowed: plan.allowed,
        execution_attempted,
        executed,
        mission_id: plan.mission_id.clone(),
        workflow_id: plan.workflow_id.clone(),
        mission_revision: plan.mission_revision,
        task_id: plan.task_id.clone(),
        agent_id: plan.agent_id.clone(),
        executor_id: plan.executor_id.clone(),
        worktree_id: plan.worktree_id.clone(),
        started_at: started.to_rfc3339(),
        finished_at: finished.to_rfc3339(),
        duration_ms,
        exit_code: sandbox.as_ref().and_then(|sandbox| sandbox.exit_code),
        timed_out: sandbox.as_ref().is_some_and(|sandbox| sandbox.timed_out),
        tests_passed: usize::from(
            successful && evidence.iter().any(|item| item.kind == "tests_passed"),
        ),
        tests_failed: usize::from(
            !successful && recognized_test_command(&request.command) && execution_attempted,
        ),
        validations: Vec::new(),
        claims,
        requested_evidence: plan.requested_evidence.clone(),
        evidence,
        observed_cost_usd,
        observed_files_changed,
        observed_external_calls,
        approval: request.approval.clone(),
        policy_trace: plan.policy_trace.clone(),
        sandbox,
        consumed_at: None,
        consumed_by_submission: None,
    };
    receipt.receipt_sha256 = hash_json(&receipt)?;
    Ok(receipt)
}

fn validate_approval(
    plan: &MissionExecutionPlan,
    approval: Option<&MissionExecutionApproval>,
) -> Result<()> {
    let approval = approval.context("mission execution requires explicit approval")?;
    if approval.schema_version != MISSION_EXECUTION_APPROVAL_SCHEMA_VERSION {
        bail!("unsupported mission execution approval schema");
    }
    if approval.approved_by.trim().is_empty()
        || approval.approval_scope_sha256 != plan.approval_scope_sha256
    {
        bail!("mission execution approval does not match the planned scope");
    }
    let issued_at = DateTime::parse_from_rfc3339(&approval.issued_at)?.with_timezone(&Utc);
    let expires_at = DateTime::parse_from_rfc3339(&approval.expires_at)?.with_timezone(&Utc);
    let now = Utc::now();
    if expires_at <= issued_at
        || now < issued_at - Duration::seconds(5)
        || now > expires_at
        || expires_at - issued_at > Duration::seconds(MAX_APPROVAL_TTL_SECONDS as i64)
    {
        bail!("mission execution approval is expired or outside the allowed TTL");
    }
    Ok(())
}

fn validate_reconciliation_request(request: &MissionExecutionReconcileRequest) -> Result<()> {
    if request.receipt_id.trim().is_empty() {
        bail!("mission execution reconciliation requires a receipt id");
    }
    if request.outcome.trim() != "no_effect_retry" {
        bail!(
            "unsupported mission execution reconciliation outcome; only `no_effect_retry` is safe"
        );
    }
    if request.approved_by.trim().is_empty() {
        bail!("mission execution reconciliation requires --approved-by");
    }
    if request.reason.trim().is_empty() {
        bail!("mission execution reconciliation requires a non-empty reason");
    }
    if !request.confirm_no_effect_retry {
        bail!("mission execution reconciliation requires explicit no-effect retry confirmation");
    }
    Ok(())
}

fn hash_reconciliation(reconciliation: &MissionExecutionReconciliation) -> Result<String> {
    let mut canonical = reconciliation.clone();
    canonical.reconciliation_sha256.clear();
    hash_json(&canonical)
}

fn execution_lease_seconds(plan: &MissionExecutionPlan) -> Result<i64> {
    let sandbox_budget = plan
        .sandbox_plan
        .as_ref()
        .map_or(0, |sandbox| sandbox.max_command_seconds);
    execution_lease_seconds_for_budget(sandbox_budget)
}

fn execution_lease_seconds_for_budget(sandbox_budget: u64) -> Result<i64> {
    let lease_seconds = sandbox_budget
        .saturating_add(EXECUTION_LEASE_MARGIN_SECONDS)
        .max(MIN_EXECUTION_LEASE_SECONDS);
    i64::try_from(lease_seconds).context("mission execution lease exceeds supported range")
}

fn claim_execution(
    store: &ForgeStore,
    request: &MissionExecutionRequest,
    plan: &MissionExecutionPlan,
) -> Result<ExecutionClaim> {
    ensure_consumption_column(store)?;
    let mut connection = open_configured_connection(store.path())?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let existing = load_db_record_from_connection(&transaction, &plan.receipt_id)?;
    let now = Utc::now();
    let owner_token = format!("mission_exec_owner_{}", Uuid::new_v4().simple());
    let lease_expires_at = now + Duration::seconds(execution_lease_seconds(plan)?);
    if let Some(existing) = existing {
        if existing.request_sha256 != plan.request_sha256 {
            bail!("idempotency key was already used for a different execution request");
        }
        if let Some(receipt_json) = existing.receipt_json {
            let mut receipt: MissionExecutionReceipt = serde_json::from_str(&receipt_json)?;
            receipt.consumed_at = existing.consumed_at;
            receipt.consumed_by_submission = existing.consumed_by_submission;
            transaction.commit()?;
            return Ok(ExecutionClaim {
                owner_token: String::new(),
                replay: Some(receipt),
            });
        }
        if existing.state == "indeterminate" {
            bail!(
                "mission execution outcome is indeterminate after execution start; reconcile it instead of re-executing"
            );
        }
        let lease_expired = existing
            .lease_expires_at
            .as_deref()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .is_none_or(|expiry| expiry.with_timezone(&Utc) <= now);
        if existing.execution_started_at.is_some() || existing.state == "running" {
            if !lease_expired {
                bail!("mission execution is already running under an active lease");
            }
            transaction.execute(
                r#"
                UPDATE mission_execution_receipts
                SET state='indeterminate', owner_token=NULL, lease_expires_at=NULL, updated_at=?1
                WHERE receipt_id=?2 AND receipt_json IS NULL
                "#,
                params![now.to_rfc3339(), plan.receipt_id],
            )?;
            transaction.commit()?;
            bail!(
                "mission execution outcome is indeterminate after execution start; reconcile it instead of re-executing"
            );
        }
        if !lease_expired {
            bail!("mission execution is already claimed by another caller");
        }
        let changed = transaction.execute(
            r#"
            UPDATE mission_execution_receipts
            SET state='reserved', owner_token=?1, lease_expires_at=?2, updated_at=?3
            WHERE receipt_id=?4 AND execution_started_at IS NULL
            "#,
            params![
                owner_token,
                lease_expires_at.to_rfc3339(),
                now.to_rfc3339(),
                plan.receipt_id
            ],
        )?;
        if changed != 1 {
            bail!("mission execution claim could not be safely reacquired");
        }
    } else {
        normalize_expired_assignment_claims(
            &transaction,
            &plan.mission_id,
            plan.mission_revision,
            &plan.task_id,
            now,
        )?;
        if let Some((receipt_id, state)) = load_assignment_guard_from_connection(
            &transaction,
            &plan.mission_id,
            plan.mission_revision,
            &plan.task_id,
        )? {
            transaction.commit()?;
            bail!(
                "mission assignment already has protected execution {receipt_id} in state {state}"
            );
        }
        transaction.execute(
            r#"
            INSERT INTO mission_execution_receipts
                (receipt_id, idempotency_key, mission_id, workflow_id, mission_revision,
                 task_id, agent_id, executor_id, worktree_id, command_sha256,
                 request_sha256, approval_scope_sha256, state, owner_token,
                 lease_expires_at, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                    'reserved', ?13, ?14, ?15, ?15)
            "#,
            params![
                plan.receipt_id,
                request.idempotency_key,
                plan.mission_id,
                plan.workflow_id,
                i64::try_from(plan.mission_revision)
                    .context("mission revision exceeds SQLite range")?,
                plan.task_id,
                plan.agent_id,
                plan.executor_id,
                plan.worktree_id,
                plan.command_sha256,
                plan.request_sha256,
                plan.approval_scope_sha256,
                owner_token,
                lease_expires_at.to_rfc3339(),
                now.to_rfc3339(),
            ],
        )?;
    }
    transaction.commit()?;
    Ok(ExecutionClaim {
        owner_token,
        replay: None,
    })
}

fn mark_execution_started(
    store: &ForgeStore,
    receipt_id: &str,
    owner_token: &str,
    started: DateTime<Utc>,
) -> Result<()> {
    let connection = open_configured_connection(store.path())?;
    let changed = connection.execute(
        r#"
        UPDATE mission_execution_receipts
        SET state='running', execution_started_at=?1, updated_at=?1
        WHERE receipt_id=?2 AND state='reserved' AND owner_token=?3
        "#,
        params![started.to_rfc3339(), receipt_id, owner_token],
    )?;
    if changed != 1 {
        bail!("mission execution claim was lost before process start");
    }
    Ok(())
}

fn finalize_claim(
    store: &ForgeStore,
    owner_token: &str,
    receipt: MissionExecutionReceipt,
) -> Result<()> {
    let mut connection = open_configured_connection(store.path())?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let receipt_json = serde_json::to_string(&receipt)?;
    let updated_at = Utc::now().to_rfc3339();
    let changed = transaction.execute(
        r#"
        UPDATE mission_execution_receipts
        SET state=?1, receipt_sha256=?2, receipt_json=?3, owner_token=NULL,
            lease_expires_at=NULL, updated_at=?4
        WHERE receipt_id=?5 AND owner_token=?6
        "#,
        params![
            receipt.status,
            receipt.receipt_sha256,
            receipt_json,
            updated_at,
            receipt.receipt_id,
            owner_token,
        ],
    )?;
    if changed != 1 {
        bail!("mission execution finished after its claim was lost; state is indeterminate");
    }
    transaction.execute(
        "INSERT INTO events (workflow_id, kind, data_json) VALUES (?1, ?2, ?3)",
        params![
            receipt.workflow_id,
            "mission.execution.receipt",
            receipt_json,
        ],
    )?;
    transaction.commit()?;
    Ok(())
}

fn load_db_record(store: &ForgeStore, receipt_id: &str) -> Result<Option<ExecutionDbRecord>> {
    ensure_consumption_column(store)?;
    let connection = open_configured_connection(store.path())?;
    load_db_record_from_connection(&connection, receipt_id)
}

fn load_db_record_from_connection(
    connection: &rusqlite::Connection,
    receipt_id: &str,
) -> Result<Option<ExecutionDbRecord>> {
    connection
        .query_row(
            r#"
            SELECT execution.receipt_id, execution.mission_id, execution.workflow_id,
                   execution.mission_revision, execution.task_id, execution.agent_id,
                   execution.executor_id, execution.state, execution.request_sha256,
                   execution.execution_started_at, execution.receipt_json,
                   execution.consumed_at, execution.consumed_by_submission,
                   execution.lease_expires_at,
                   (
                       SELECT reconciliation.data_json
                       FROM mission_execution_reconciliations AS reconciliation
                       WHERE reconciliation.receipt_id = execution.receipt_id
                       ORDER BY reconciliation.created_at DESC,
                                reconciliation.reconciliation_id DESC
                       LIMIT 1
                   )
            FROM mission_execution_receipts AS execution
            WHERE execution.receipt_id=?1
            "#,
            [receipt_id],
            map_execution_db_record,
        )
        .optional()
        .map_err(Into::into)
}

fn normalize_expired_assignment_claims(
    connection: &rusqlite::Connection,
    mission_id: &str,
    mission_revision: u64,
    task_id: &str,
    now: DateTime<Utc>,
) -> Result<()> {
    let revision =
        i64::try_from(mission_revision).context("mission revision exceeds SQLite range")?;
    let now = now.to_rfc3339();
    connection.execute(
        r#"
        UPDATE mission_execution_receipts
        SET state='indeterminate', owner_token=NULL, lease_expires_at=NULL, updated_at=?1
        WHERE mission_id=?2 AND mission_revision=?3 AND task_id=?4
          AND state IN ('reserved', 'running')
          AND execution_started_at IS NOT NULL
          AND (lease_expires_at IS NULL OR lease_expires_at<=?1)
        "#,
        params![now, mission_id, revision, task_id],
    )?;
    connection.execute(
        r#"
        UPDATE mission_execution_receipts
        SET state='reservation_expired', owner_token=NULL, lease_expires_at=NULL, updated_at=?1
        WHERE mission_id=?2 AND mission_revision=?3 AND task_id=?4
          AND state='reserved'
          AND execution_started_at IS NULL
          AND (lease_expires_at IS NULL OR lease_expires_at<=?1)
        "#,
        params![now, mission_id, revision, task_id],
    )?;
    Ok(())
}

fn load_assignment_guard_from_connection(
    connection: &rusqlite::Connection,
    mission_id: &str,
    mission_revision: u64,
    task_id: &str,
) -> Result<Option<(String, String)>> {
    let revision =
        i64::try_from(mission_revision).context("mission revision exceeds SQLite range")?;
    connection
        .query_row(
            r#"
            SELECT receipt_id, state
            FROM mission_execution_receipts
            WHERE mission_id=?1 AND mission_revision=?2 AND task_id=?3
              AND state IN (
                  'reserved',
                  'running',
                  'completed',
                  'failed',
                  'timed_out',
                  'indeterminate'
              )
            ORDER BY created_at DESC, receipt_id DESC
            LIMIT 1
            "#,
            params![mission_id, revision, task_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(Into::into)
}

fn map_execution_db_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ExecutionDbRecord> {
    let revision: i64 = row.get(3)?;
    Ok(ExecutionDbRecord {
        receipt_id: row.get(0)?,
        mission_id: row.get(1)?,
        workflow_id: row.get(2)?,
        mission_revision: u64::try_from(revision).unwrap_or_default(),
        task_id: row.get(4)?,
        agent_id: row.get(5)?,
        executor_id: row.get(6)?,
        state: row.get(7)?,
        request_sha256: row.get(8)?,
        execution_started_at: row.get(9)?,
        receipt_json: row.get(10)?,
        consumed_at: row.get(11)?,
        consumed_by_submission: row.get(12)?,
        lease_expires_at: row.get(13)?,
        reconciliation_json: row.get(14)?,
    })
}

fn to_public_record(record: ExecutionDbRecord) -> Result<MissionExecutionRecord> {
    let mut receipt = record
        .receipt_json
        .as_deref()
        .map(serde_json::from_str::<MissionExecutionReceipt>)
        .transpose()?;
    if let Some(receipt) = receipt.as_mut() {
        receipt.consumed_at = record.consumed_at.clone();
        receipt.consumed_by_submission = record.consumed_by_submission.clone();
    }
    let reconciliation = record
        .reconciliation_json
        .as_deref()
        .map(serde_json::from_str::<MissionExecutionReconciliation>)
        .transpose()?;
    Ok(MissionExecutionRecord {
        receipt_id: record.receipt_id,
        mission_id: record.mission_id,
        workflow_id: record.workflow_id,
        mission_revision: record.mission_revision,
        task_id: record.task_id,
        agent_id: record.agent_id,
        executor_id: record.executor_id,
        state: record.state,
        execution_started_at: record.execution_started_at,
        consumed_at: record.consumed_at,
        consumed_by_submission: record.consumed_by_submission,
        receipt,
        reconciliation,
    })
}

fn ensure_consumption_column(store: &ForgeStore) -> Result<()> {
    let connection = open_configured_connection(store.path())?;
    let mut statement = connection.prepare("PRAGMA table_info(mission_execution_receipts)")?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
    let mut has_column = false;
    for column in columns {
        has_column |= column? == "consumed_by_submission";
    }
    drop(statement);
    if !has_column {
        connection.execute(
            "ALTER TABLE mission_execution_receipts ADD COLUMN consumed_by_submission TEXT",
            [],
        )?;
    }
    Ok(())
}

fn resolve_requested_gate_evidence(
    mission: &crate::mission::MissionRecord,
    squad: &crate::mission::SquadDefinition,
    request: &MissionExecutionRequest,
) -> Result<Vec<MissionExecutionGateEvidenceContract>> {
    if request.requested_evidence.is_empty() {
        return Ok(Vec::new());
    }
    if mission.mode == MissionMode::Simulation {
        bail!(
            "bounded mission simulation cannot issue operational gate evidence; start a real mission"
        );
    }
    let task_index = mission
        .tasks
        .iter()
        .position(|task| task.id == request.task_id)
        .with_context(|| {
            format!(
                "cannot resolve gate evidence contract for unknown task {}",
                request.task_id
            )
        })?;
    let mut gate_indexes = BTreeSet::from([task_index]);
    if task_index == mission.tasks.len().saturating_sub(1) {
        gate_indexes.extend(1..squad.gates.len());
    }

    let mut allowed = BTreeMap::<String, BTreeSet<String>>::new();
    for gate_index in gate_indexes {
        if let Some(gate) = squad.gates.get(gate_index) {
            for evidence_kind in &gate.required_evidence {
                allowed
                    .entry(evidence_kind.clone())
                    .or_default()
                    .insert(gate.id.clone());
            }
        }
    }

    let mut seen = BTreeSet::new();
    let mut contract = Vec::with_capacity(request.requested_evidence.len());
    for requested in &request.requested_evidence {
        let evidence_kind = requested.trim();
        if evidence_kind.is_empty() {
            bail!("requested gate evidence cannot be empty");
        }
        if evidence_kind != requested {
            bail!("requested gate evidence names must not contain surrounding whitespace");
        }
        if matches!(evidence_kind, "execution_completed" | "tests_passed") {
            bail!(
                "requested gate evidence `{evidence_kind}` is reserved for Forge runtime verification"
            );
        }
        if !seen.insert(evidence_kind.to_string()) {
            bail!("requested gate evidence `{evidence_kind}` is duplicated");
        }
        let gate_ids = allowed
            .get(evidence_kind)
            .with_context(|| {
                format!(
                    "requested gate evidence `{evidence_kind}` is not allowed for task {}",
                    request.task_id
                )
            })?
            .iter()
            .cloned()
            .collect();
        contract.push(MissionExecutionGateEvidenceContract {
            evidence_kind: evidence_kind.to_string(),
            gate_ids,
        });
    }
    Ok(contract)
}

fn request_scope_sha256(request: &MissionExecutionRequest) -> Result<String> {
    hash_json(&serde_json::json!({
        "idempotency_key": request.idempotency_key,
        "mission_id": request.mission_id,
        "workflow_id": request.workflow_id,
        "expected_mission_revision": request.expected_mission_revision,
        "task_id": request.task_id,
        "agent_id": request.agent_id,
        "executor_id": canonical_executor_id(&request.executor_id),
        "worktree": request.worktree,
        "purpose": request.purpose,
        "command": request.command,
        "requested_evidence": request.requested_evidence,
        "dry_run": request.dry_run,
        "allow_trusted_process_runtime": request.allow_trusted_process_runtime,
    }))
}

fn validate_shape(request: &MissionExecutionRequest) -> Result<()> {
    if request.idempotency_key.trim().is_empty()
        || request.mission_id.trim().is_empty()
        || request.workflow_id.trim().is_empty()
        || request.task_id.trim().is_empty()
        || request.agent_id.trim().is_empty()
        || request.executor_id.trim().is_empty()
        || request.purpose.trim().is_empty()
        || request
            .command
            .first()
            .is_none_or(|command| command.trim().is_empty())
    {
        bail!("mission execution identifiers, purpose, idempotency key and command are required");
    }
    Ok(())
}

fn validate_shape_for_alias(executor_id: &str) -> String {
    executor_id.trim().to_ascii_lowercase().replace('_', "-")
}

fn canonical_executor_id(executor_id: &str) -> String {
    match validate_shape_for_alias(executor_id).as_str() {
        "codex-cli" => "codex".to_string(),
        "opencode-cli" => "opencode".to_string(),
        "agy-cli" | "antigravity-cli" | "antigravity" => "agy".to_string(),
        other => other.to_string(),
    }
}

fn command_basename(command: &str) -> String {
    Path::new(command)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(command)
        .to_string()
}

fn command_can_observe_requested_evidence(
    command: &[String],
    contract: &[MissionExecutionGateEvidenceContract],
) -> bool {
    if contract.is_empty() {
        return true;
    }
    if cargo_metadata_only_command(command) {
        return false;
    }
    if command
        .first()
        .is_some_and(|value| command_basename(value) == "cargo")
        && trusted_host_cargo_identity(command).is_none()
    {
        return false;
    }
    !contract
        .iter()
        .any(|evidence| evidence.evidence_kind == "review_passed")
        || recognized_test_command(command)
}

fn cargo_metadata_only_command(command: &[String]) -> bool {
    if command
        .first()
        .map(|value| command_basename(value))
        .as_deref()
        != Some("cargo")
    {
        return false;
    }
    let mut arguments = command.iter().skip(1);
    let first = arguments.next();
    let subcommand = if first.is_some_and(|argument| argument.starts_with('+')) {
        arguments.next()
    } else {
        first
    };
    subcommand.is_some_and(|argument| {
        matches!(
            argument.as_str(),
            "--version" | "-V" | "version" | "--help" | "-h" | "help"
        )
    })
}

#[derive(Debug, Clone)]
struct TrustedCargoIdentity {
    executable_sha256: String,
}

fn trusted_host_cargo_identity(command: &[String]) -> Option<TrustedCargoIdentity> {
    let requested = command.first()?;
    if command_basename(requested) != "cargo" {
        return None;
    }
    let requested = resolve_executable(requested)?;
    let mut candidates = resolve_executable("cargo").into_iter().collect::<Vec<_>>();
    if let Some(rustup) = resolve_executable("rustup") {
        let output = Command::new(rustup)
            .args(["which", "cargo"])
            .output()
            .ok()?;
        if output.status.success() {
            if let Ok(path) = String::from_utf8(output.stdout) {
                if let Ok(path) = fs::canonicalize(path.trim()) {
                    candidates.push(path);
                }
            }
        }
    }
    if !candidates
        .iter()
        .any(|candidate| same_executable(&requested, candidate))
    {
        return None;
    }
    Some(TrustedCargoIdentity {
        executable_sha256: sha256_file(&requested).ok()?,
    })
}

fn resolve_executable(command: &str) -> Option<PathBuf> {
    let path = Path::new(command);
    if path.components().count() > 1 {
        return fs::canonicalize(path).ok();
    }
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .map(|directory| directory.join(command))
        .find(|candidate| candidate.is_file())
        .and_then(|candidate| fs::canonicalize(candidate).ok())
}

fn same_executable(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        fs::metadata(left)
            .ok()
            .zip(fs::metadata(right).ok())
            .is_some_and(|(left, right)| left.dev() == right.dev() && left.ino() == right.ino())
    }
    #[cfg(not(unix))]
    {
        false
    }
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)
        .with_context(|| format!("failed to inspect executable {}", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn observe_execution_semantics(
    command: &[String],
    contract: &[MissionExecutionGateEvidenceContract],
    receipt: &WorktreeSandboxReceipt,
) -> MissionExecutionObservations {
    let cargo_command = command
        .first()
        .is_some_and(|value| command_basename(value) == "cargo");
    let trusted_cargo = cargo_command
        .then(|| trusted_host_cargo_identity(command))
        .flatten();
    if cargo_command && trusted_cargo.is_none() {
        return MissionExecutionObservations::default();
    }

    let tests_passed = trusted_cargo
        .as_ref()
        .filter(|_| recognized_test_command(command))
        .and_then(|identity| cargo_test_observation(receipt, identity));
    let cargo_test_without_tests = recognized_test_command(command) && tests_passed.is_none();
    let explicit = parse_gate_evidence_observations(&receipt.stdout.content);
    let mut gate_evidence = BTreeMap::new();
    for evidence in contract {
        if cargo_test_without_tests {
            continue;
        }
        let observation = if evidence.evidence_kind == "review_passed" {
            tests_passed.clone()
        } else {
            explicit
                .get(&evidence.evidence_kind)
                .filter(|value| semantic_observation_satisfies(&evidence.evidence_kind, value))
                .map(|value| {
                    serde_json::json!({
                        "schema_version": GATE_EVIDENCE_OBSERVATION_SCHEMA_VERSION,
                        "source": "sandbox_stdout",
                        "stream_sha256": receipt.stdout.sha256,
                        "value": value,
                    })
                })
        };
        if let Some(observation) = observation {
            gate_evidence.insert(evidence.evidence_kind.clone(), observation);
        }
    }
    MissionExecutionObservations {
        tests_passed,
        gate_evidence,
    }
}

fn cargo_test_observation(
    receipt: &WorktreeSandboxReceipt,
    identity: &TrustedCargoIdentity,
) -> Option<serde_json::Value> {
    let mut summaries = 0_u64;
    let mut passed = 0_u64;
    let mut failed = 0_u64;
    let mut ignored = 0_u64;
    let mut measured = 0_u64;
    let mut filtered_out = 0_u64;
    for line in receipt
        .stdout
        .content
        .lines()
        .chain(receipt.stderr.content.lines())
    {
        let Some(index) = line.find("test result:") else {
            continue;
        };
        let summary = &line[index + "test result:".len()..];
        let Some(summary_passed) = summary_count(summary, "passed") else {
            continue;
        };
        let Some(summary_failed) = summary_count(summary, "failed") else {
            continue;
        };
        summaries = summaries.saturating_add(1);
        passed = passed.saturating_add(summary_passed);
        failed = failed.saturating_add(summary_failed);
        ignored = ignored.saturating_add(summary_count(summary, "ignored").unwrap_or(0));
        measured = measured.saturating_add(summary_count(summary, "measured").unwrap_or(0));
        filtered_out =
            filtered_out.saturating_add(summary_count(summary, "filtered out").unwrap_or(0));
    }
    if summaries == 0 || passed == 0 || failed != 0 {
        return None;
    }
    Some(serde_json::json!({
        "schema_version": TEST_EXECUTION_OBSERVATION_SCHEMA_VERSION,
        "source": "cargo_test_summary",
        "toolchain_sha256": identity.executable_sha256,
        "stdout_sha256": receipt.stdout.sha256,
        "stderr_sha256": receipt.stderr.sha256,
        "summaries": summaries,
        "passed": passed,
        "failed": failed,
        "ignored": ignored,
        "measured": measured,
        "filtered_out": filtered_out,
    }))
}

fn summary_count(summary: &str, label: &str) -> Option<u64> {
    summary.split(';').find_map(|segment| {
        let before = segment.trim().strip_suffix(label)?.trim();
        before.split_whitespace().next_back()?.parse().ok()
    })
}

fn parse_gate_evidence_observations(content: &str) -> BTreeMap<String, serde_json::Value> {
    let mut observations = BTreeMap::new();
    let mut duplicates = BTreeSet::new();
    for line in content.lines() {
        let Some(index) = line.find(GATE_EVIDENCE_OBSERVATION_PREFIX) else {
            continue;
        };
        let payload = line[index + GATE_EVIDENCE_OBSERVATION_PREFIX.len()..].trim();
        let Ok(envelope) = serde_json::from_str::<serde_json::Value>(payload) else {
            continue;
        };
        if envelope["schema_version"] != GATE_EVIDENCE_OBSERVATION_SCHEMA_VERSION {
            continue;
        }
        let Some(evidence) = envelope["evidence"].as_object() else {
            continue;
        };
        for (kind, value) in evidence {
            if observations.insert(kind.clone(), value.clone()).is_some() {
                duplicates.insert(kind.clone());
            }
        }
    }
    for duplicate in duplicates {
        observations.remove(&duplicate);
    }
    observations
}

fn semantic_observation_satisfies(kind: &str, value: &serde_json::Value) -> bool {
    match kind {
        "requirements_summary" => match value {
            serde_json::Value::String(value) => !value.trim().is_empty(),
            serde_json::Value::Array(value) => !value.is_empty(),
            serde_json::Value::Object(value) => !value.is_empty(),
            _ => false,
        },
        "acceptance_criteria" => value.as_array().is_some_and(|criteria| {
            !criteria.is_empty() && criteria.iter().all(value_is_meaningful)
        }),
        "structured_delivery" => value.as_object().is_some_and(|delivery| {
            delivery
                .get("status")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|status| matches!(status, "completed" | "repaired"))
                && delivery
                    .get("summary")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|summary| !summary.trim().is_empty())
        }),
        "no_unresolved_risks" => {
            value == &serde_json::Value::Bool(true)
                || value.as_array().is_some_and(Vec::is_empty)
                || value.as_object().is_some_and(|object| {
                    object
                        .get("unresolved_risks")
                        .and_then(serde_json::Value::as_array)
                        .is_some_and(Vec::is_empty)
                        || object.get("count").and_then(serde_json::Value::as_u64) == Some(0)
                })
        }
        "review_passed" => false,
        _ => value_is_meaningful(value),
    }
}

fn value_is_meaningful(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(value) => *value,
        serde_json::Value::Number(_) => true,
        serde_json::Value::String(value) => !value.trim().is_empty(),
        serde_json::Value::Array(value) => !value.is_empty(),
        serde_json::Value::Object(value) => !value.is_empty(),
    }
}

fn test_observation_is_valid(
    observation: &serde_json::Value,
    sandbox: &MissionSandboxEvidence,
) -> bool {
    let Some(identity) = trusted_host_cargo_identity(&sandbox.command) else {
        return false;
    };
    observation["schema_version"] == TEST_EXECUTION_OBSERVATION_SCHEMA_VERSION
        && observation["source"] == "cargo_test_summary"
        && observation["toolchain_sha256"].as_str() == Some(identity.executable_sha256.as_str())
        && observation["stdout_sha256"] == sandbox.stdout_sha256
        && observation["stderr_sha256"] == sandbox.stderr_sha256
        && observation["summaries"]
            .as_u64()
            .is_some_and(|count| count > 0)
        && observation["passed"]
            .as_u64()
            .is_some_and(|count| count > 0)
        && observation["failed"] == 0
}

fn gate_observation_is_valid(
    kind: &str,
    observation: &serde_json::Value,
    sandbox: &MissionSandboxEvidence,
) -> bool {
    if kind == "review_passed" {
        return test_observation_is_valid(observation, sandbox);
    }
    observation["schema_version"] == GATE_EVIDENCE_OBSERVATION_SCHEMA_VERSION
        && observation["source"] == "sandbox_stdout"
        && observation["stream_sha256"] == sandbox.stdout_sha256
        && semantic_observation_satisfies(kind, &observation["value"])
}

fn recognized_test_command(command: &[String]) -> bool {
    if command
        .first()
        .map(|value| command_basename(value))
        .as_deref()
        != Some("cargo")
    {
        return false;
    }
    let mut arguments = command.iter().skip(1);
    let first = arguments.next();
    let subcommand = if first.is_some_and(|argument| argument.starts_with('+')) {
        arguments.next()
    } else {
        first
    };
    subcommand.is_some_and(|argument| argument == "test")
        && !command
            .iter()
            .any(|argument| matches!(argument.as_str(), "--no-run" | "--help" | "-h" | "--list"))
}

fn skill_policy_allows(member: &crate::mission::RosterMember, skills: &[String]) -> bool {
    match member.skill_policy.mode {
        SkillGateMode::Unrestricted | SkillGateMode::Inherited => true,
        SkillGateMode::Allowlist => skills.iter().all(|skill| {
            member
                .skill_policy
                .allowed
                .iter()
                .any(|allowed| allowed == skill)
        }),
        SkillGateMode::Denylist => skills.iter().all(|skill| {
            !member
                .skill_policy
                .denied
                .iter()
                .any(|denied| denied == skill)
        }),
        SkillGateMode::None => skills.is_empty(),
        SkillGateMode::ApprovalRequired => true,
    }
}

fn sandbox_request(request: &MissionExecutionRequest) -> WorktreeSandboxRequest {
    WorktreeSandboxRequest {
        worktree: request.worktree.clone().unwrap_or_default(),
        purpose: request.purpose.clone(),
        workflow_id: Some(request.workflow_id.clone()),
        task_id: Some(request.task_id.clone()),
        command: request.command.clone(),
    }
}

fn receipt_id(request: &MissionExecutionRequest) -> String {
    format!(
        "mission_exec_{}",
        &sha256(format!("{}\n{}", request.mission_id, request.idempotency_key).as_bytes())[..24]
    )
}

fn decide(trace: &mut Vec<MissionExecutionDecision>, check: &str, allowed: bool, detail: &str) {
    trace.push(MissionExecutionDecision {
        check: check.to_string(),
        allowed,
        detail: detail.to_string(),
    });
}

fn hash_json(value: &impl Serialize) -> Result<String> {
    Ok(sha256(&serde_json::to_vec(value)?))
}

fn sha256(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    format!("{:x}", digest.finalize())
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[cfg(test)]
mod tests {
    use super::{execution_lease_seconds_for_budget, recognized_test_command};

    fn command(arguments: &[&str]) -> Vec<String> {
        arguments
            .iter()
            .map(|argument| (*argument).to_string())
            .collect()
    }

    #[test]
    fn only_explicit_cargo_test_commands_produce_test_claims() {
        assert!(recognized_test_command(&command(&["cargo", "test"])));
        assert!(recognized_test_command(&command(&[
            "/tmp/fixture/cargo",
            "+stable",
            "test",
            "--all"
        ])));
        assert!(!recognized_test_command(&command(&["cargo", "--version"])));
        assert!(!recognized_test_command(&command(&[
            "cargo", "check", "--tests"
        ])));
        assert!(!recognized_test_command(&command(&["git", "test"])));
        assert!(!recognized_test_command(&command(&[
            "cargo", "test", "--no-run"
        ])));
        assert!(!recognized_test_command(&command(&[
            "cargo", "test", "--help"
        ])));
        assert!(!recognized_test_command(&command(&[
            "cargo", "test", "--", "--list"
        ])));
    }

    #[test]
    fn execution_lease_covers_the_sandbox_budget_and_margin() {
        assert_eq!(execution_lease_seconds_for_budget(0).unwrap(), 300);
        assert_eq!(execution_lease_seconds_for_budget(269).unwrap(), 300);
        assert_eq!(execution_lease_seconds_for_budget(300).unwrap(), 330);
        assert_eq!(execution_lease_seconds_for_budget(900).unwrap(), 930);
    }
}
