use crate::graph::{
    build_tasks, create_workflow, task as graph_task, AtomicTask, ExecutorKind, TaskStatus,
    ValidationRule, Workflow, WorkflowRevision,
};
use crate::intent::parse_intent;
use crate::mission_executor::{
    claim_mission_execution_receipt_for_submission, load_mission_execution_receipt,
    release_mission_execution_receipt_submission_claim, resolved_mission_execution_metrics,
    verified_mission_execution_claims, MissionExecutionClaimKind, MissionExecutionReceipt,
};
use crate::storage::{
    open_configured_connection, replace_workflow_tenant_projection_on_connection, ForgeStore,
};
use crate::validation::validate_workflow;
use crate::worktree::{register_worktree, WorktreeRegisterOptions};
use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use uuid::Uuid;

pub const SQUAD_SCHEMA_VERSION: &str = "forge.squad.v1";
pub const MISSION_SCHEMA_VERSION: &str = "forge.mission.v1";
pub const MISSION_SIMULATION_SCHEMA_VERSION: &str = "forge.mission.simulation.v1";
const MISSION_EVENT_SCHEMA_VERSION: &str = "forge.mission.event.v1";
const AGENT_HANDOFF_SCHEMA_VERSION: &str = "forge.agent_handoff.v1";
const MISSION_DRIVE_LEASE_SECONDS: i64 = 300;
const MISSION_INBOX_LEASE_SECONDS: i64 = 300;
const HANDOFF_PHASE_QUEUED: &str = "queued";
const HANDOFF_PHASE_CLAIMED: &str = "claimed";
const HANDOFF_PHASE_WOKEN: &str = "woken";
const HANDOFF_PHASE_OUTCOME_PERSISTED: &str = "outcome_persisted";
const HANDOFF_PHASE_FINALIZED: &str = "finalized";

#[cfg(test)]
thread_local! {
    static MISSION_FAILPOINT: RefCell<Option<&'static str>> = const { RefCell::new(None) };
}

#[cfg(test)]
#[allow(dead_code)]
fn set_mission_failpoint(point: Option<&'static str>) {
    MISSION_FAILPOINT.with(|configured| {
        *configured.borrow_mut() = point;
    });
}

fn mission_failpoint(point: &str) -> Result<()> {
    #[cfg(test)]
    {
        let should_fail = MISSION_FAILPOINT.with(|configured| {
            let matches = configured
                .borrow()
                .as_ref()
                .is_some_and(|value| *value == point);
            if matches {
                configured.borrow_mut().take();
            }
            matches
        });
        if should_fail {
            bail!("injected mission failpoint: {point}");
        }
    }
    #[cfg(not(test))]
    let _ = point;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MissionMode {
    Manual,
    Assisted,
    Squad,
    Agentic,
    Workflow,
    SupervisedAutonomy,
    Scheduled,
    EventTriggered,
    Incident,
    Simulation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MissionStatus {
    Draft,
    Intake,
    Planning,
    WaitingApproval,
    Running,
    Blocked,
    Reviewing,
    Repairing,
    Completed,
    Failed,
    Cancelled,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrchestrationTopology {
    SequentialPipeline,
    FanOutFanIn,
    MapReduce,
    Debate,
    Voting,
    Judge,
    PlannerExecutor,
    ScoutBuilderReviewer,
    ControllerWorkers,
    Blackboard,
    Swarm,
    RedTeamBlueTeam,
    ActorModel,
    SupervisorTree,
    Saga,
    StateMachine,
    Quorum,
    Tournament,
    RetryEscalationTree,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillGateMode {
    Unrestricted,
    Allowlist,
    Denylist,
    None,
    Inherited,
    ApprovalRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentSpawnPolicy {
    Eager,
    OnDemand,
    WarmPool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPermissionPolicy {
    #[serde(default)]
    pub allowed_capabilities: Vec<String>,
    #[serde(default)]
    pub denied_capabilities: Vec<String>,
    #[serde(default)]
    pub filesystem_allow: Vec<String>,
    #[serde(default)]
    pub shell_allow: Vec<String>,
    pub network: String,
}

impl AgentPermissionPolicy {
    fn denies(&self, capability: &str) -> bool {
        self.denied_capabilities
            .iter()
            .any(|denied| denied == capability)
    }

    fn allows(&self, capability: &str) -> bool {
        self.allowed_capabilities
            .iter()
            .any(|allowed| allowed == capability)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLimits {
    pub max_files_changed: usize,
    pub max_runtime_seconds: u64,
    pub max_cost_usd: f64,
    pub max_children: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentContract {
    pub input: String,
    pub output: String,
    pub handoff: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDefinition {
    pub id: String,
    pub role: String,
    pub runtime: String,
    pub provider: String,
    pub model: String,
    pub effort: String,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    pub permissions: AgentPermissionPolicy,
    pub contract: AgentContract,
    pub limits: AgentLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPolicy {
    pub mode: SkillGateMode,
    #[serde(default)]
    pub allowed: Vec<String>,
    #[serde(default)]
    pub denied: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RosterMember {
    pub role: String,
    pub agent: AgentDefinition,
    pub min_instances: usize,
    pub max_instances: usize,
    pub spawn: AgentSpawnPolicy,
    pub required: bool,
    #[serde(default)]
    pub substitutes: Vec<String>,
    #[serde(default)]
    pub affinity: Vec<String>,
    pub reviewer_anti_affinity: bool,
    pub skill_policy: SkillPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationPolicy {
    pub max_depth: usize,
    pub max_children_per_agent: usize,
    #[serde(default)]
    pub roles_allowed_to_spawn: Vec<String>,
    pub max_branch_cost_usd: f64,
    pub max_branch_runtime_seconds: u64,
    pub max_files_per_branch: usize,
    pub review_required: bool,
    pub cascade_cancel: bool,
    pub context_isolation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentLifecyclePolicy {
    pub spawn: AgentSpawnPolicy,
    pub idle_timeout_seconds: u64,
    pub preserve_session: bool,
    pub collect_outputs_before_close: bool,
    pub max_concurrent_agents: usize,
    pub scale_to_zero: bool,
    pub backpressure: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityGateDefinition {
    pub id: String,
    pub trigger: String,
    pub validator: String,
    #[serde(default)]
    pub required_evidence: Vec<String>,
    pub approval_policy: String,
    pub failure_action: String,
    pub timeout_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostPolicy {
    pub currency: String,
    pub mission_limit_usd: f64,
    pub branch_limit_usd: f64,
    #[serde(default)]
    pub aggregate_dimensions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SquadDependencySet {
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub addons: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub mcp_servers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SquadDistribution {
    pub origin: String,
    pub channel: String,
    pub signed: bool,
    pub signature: Option<String>,
    pub trusted: bool,
    pub auto_update: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionRecipe {
    pub id: String,
    pub objective_type: String,
    #[serde(default)]
    pub required_intake_fields: Vec<String>,
    #[serde(default)]
    pub optional_intake_fields: Vec<String>,
    #[serde(default)]
    pub defaults: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SquadDefinition {
    pub schema_version: String,
    pub id: String,
    pub name: String,
    pub version: String,
    pub objective: String,
    pub orchestrator: AgentDefinition,
    pub roster: Vec<RosterMember>,
    pub topology: OrchestrationTopology,
    pub invocation_policy: String,
    pub handoff_policy: String,
    pub lifecycle_policy: AgentLifecyclePolicy,
    pub delegation_policy: DelegationPolicy,
    pub cost_policy: CostPolicy,
    pub dependencies: SquadDependencySet,
    pub distribution: SquadDistribution,
    #[serde(default)]
    pub gates: Vec<QualityGateDefinition>,
    #[serde(default)]
    pub recipes: Vec<MissionRecipe>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SquadValidationReport {
    pub schema_version: String,
    pub status: String,
    pub valid: bool,
    pub squad_id: String,
    pub squad_version: String,
    pub composition_sha256: String,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SquadInstallReport {
    pub schema_version: String,
    pub status: String,
    pub squad_id: String,
    pub squad_version: String,
    pub composition_sha256: String,
    pub validation: SquadValidationReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SquadCatalogReport {
    pub schema_version: String,
    pub status: String,
    pub squads: Vec<SquadDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionTask {
    pub id: String,
    pub title: String,
    pub owner_role: String,
    pub status: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
    pub expected_output: String,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    pub progress_percent: u8,
    #[serde(default)]
    pub artifacts: Vec<String>,
    pub cost_usd: f64,
    #[serde(default)]
    pub assigned_agent_id: Option<String>,
    #[serde(default)]
    pub attempt: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionAgentInstance {
    pub instance_id: String,
    pub definition_id: String,
    pub role: String,
    pub status: String,
    pub spawned_on_demand: bool,
    pub parent_instance_id: Option<String>,
    pub session_preserved: bool,
    pub depth: usize,
    pub cost_usd: f64,
    pub runtime_milliseconds: u64,
    pub files_changed: usize,
    pub spawned_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredAgentDelivery {
    pub task_id: String,
    pub status: String,
    pub summary: String,
    #[serde(default)]
    pub artifacts: Vec<String>,
    pub tests_passed: usize,
    pub tests_failed: usize,
    #[serde(default)]
    pub risks: Vec<String>,
    #[serde(default)]
    pub followups: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentHandoff {
    pub schema_version: String,
    pub id: String,
    pub idempotency_key: String,
    pub mission_id: String,
    pub from_agent: String,
    pub to_agent: String,
    pub task_id: String,
    pub status: String,
    pub summary: String,
    pub delivery: StructuredAgentDelivery,
    #[serde(default)]
    pub validations: Vec<String>,
    #[serde(default)]
    pub unresolved_questions: Vec<String>,
    pub recommended_next_action: String,
    pub created_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionInboxItem {
    pub id: String,
    pub handoff_id: String,
    pub recipient_agent: String,
    pub status: String,
    pub enqueued_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
    pub wakeup_event_sequence: usize,
    #[serde(default)]
    pub attempts: usize,
    #[serde(default = "default_inbox_max_attempts")]
    pub max_attempts: usize,
    #[serde(default)]
    pub lease_owner: Option<String>,
    #[serde(default)]
    pub lease_expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_error: Option<String>,
}

fn default_inbox_max_attempts() -> usize {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionEvent {
    pub schema_version: String,
    pub id: String,
    pub sequence: usize,
    pub kind: String,
    pub status: String,
    pub actor: String,
    pub task_id: Option<String>,
    pub correlation_id: Option<String>,
    pub caused_by_sequence: Option<usize>,
    pub summary: String,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateResult {
    pub gate_id: String,
    pub attempt: usize,
    pub status: String,
    pub validator: String,
    #[serde(default)]
    pub evidence: Vec<String>,
    pub failure_action: String,
    pub repair_cycle: usize,
    pub supersedes_attempt: Option<usize>,
    pub evaluated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessResolution {
    pub task_id: String,
    pub agent_id: String,
    pub role: String,
    pub runtime: String,
    pub provider: String,
    pub model: String,
    pub effort: String,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    pub resolved_from: String,
    #[serde(default)]
    pub overrode: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MissionCostLedger {
    pub total_usd: f64,
    pub tokens: u64,
    pub runtime_milliseconds: u64,
    pub cpu_milliseconds: u64,
    pub retries: usize,
    pub agent_spawns: usize,
    pub external_calls: usize,
    pub files_changed: usize,
    pub human_time_seconds: u64,
    pub by_role_usd: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionRecord {
    pub schema_version: String,
    pub id: String,
    pub workflow_id: String,
    pub tenant_id: String,
    pub workspace_id: String,
    pub objective: String,
    pub mode: MissionMode,
    pub status: MissionStatus,
    pub squad_id: String,
    pub squad_version: String,
    pub squad_composition_sha256: String,
    pub orchestrator_instance_id: String,
    pub worktree: Option<String>,
    pub budget_usd: f64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub tasks: Vec<MissionTask>,
    #[serde(default)]
    pub agents: Vec<MissionAgentInstance>,
    #[serde(default)]
    pub handoffs: Vec<AgentHandoff>,
    #[serde(default)]
    pub inbox: Vec<MissionInboxItem>,
    #[serde(default)]
    pub gates: Vec<GateResult>,
    #[serde(default)]
    pub events: Vec<MissionEvent>,
    #[serde(default)]
    pub harnesses: Vec<HarnessResolution>,
    pub cost: MissionCostLedger,
    pub rework_cycles: usize,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionSimulationReport {
    pub schema_version: String,
    pub status: String,
    pub simulation: String,
    pub bounded: bool,
    pub model_execution_performed: bool,
    pub external_mutation_performed: bool,
    pub orchestrator_restricted: bool,
    pub on_demand_spawn_proven: bool,
    pub event_driven_handoff_proven: bool,
    pub validation_before_promotion_proven: bool,
    pub rework_cycle_proven: bool,
    pub exact_composition_recorded: bool,
    pub incremental_persistence_proven: bool,
    pub hierarchy_limits_enforced: bool,
    pub cost_limits_enforced: bool,
    pub inbox_wakeup_proven: bool,
    #[serde(default)]
    pub proof_scope: Vec<String>,
    #[serde(default)]
    pub not_proven: Vec<String>,
    pub mission: MissionRecord,
    pub validation: SquadValidationReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionListReport {
    pub schema_version: String,
    pub status: String,
    pub missions: Vec<MissionRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionAssignment {
    pub task: MissionTask,
    pub agent: MissionAgentInstance,
    pub harness: HarnessResolution,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionStartReport {
    pub schema_version: String,
    pub status: String,
    pub mission: MissionRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionSubmission {
    pub idempotency_key: String,
    #[serde(default)]
    pub execution_receipt_id: String,
    pub task_id: String,
    pub agent_id: String,
    pub status: String,
    pub summary: String,
    #[serde(default)]
    pub artifacts: Vec<String>,
    #[serde(default)]
    pub validations: Vec<String>,
    #[serde(default)]
    pub risks: Vec<String>,
    #[serde(default)]
    pub followups: Vec<String>,
    #[serde(default)]
    pub tests_passed: usize,
    #[serde(default)]
    pub tests_failed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionSubmitReport {
    pub schema_version: String,
    pub status: String,
    pub mission_id: String,
    pub handoff_id: String,
    pub inbox_id: String,
    pub producer_revision: u64,
    pub deduplicated: bool,
    pub accepted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionDriveReport {
    pub schema_version: String,
    pub status: String,
    pub action: String,
    pub mission_id: String,
    pub revision: u64,
    pub assignment: Option<MissionAssignment>,
    pub handoff_id: Option<String>,
    pub mission: MissionRecord,
}

fn restricted_orchestrator(id: &str, role: &str) -> AgentDefinition {
    AgentDefinition {
        id: id.to_string(),
        role: role.to_string(),
        runtime: "forge-control-plane".to_string(),
        provider: "forge".to_string(),
        model: "policy-only".to_string(),
        effort: "adaptive".to_string(),
        skills: vec![
            "objective-decomposition".to_string(),
            "delegation".to_string(),
            "validation-governance".to_string(),
        ],
        tools: vec![
            "spawn_agent".to_string(),
            "assign_task".to_string(),
            "read_status".to_string(),
            "request_revision".to_string(),
            "consolidate_outputs".to_string(),
        ],
        permissions: AgentPermissionPolicy {
            allowed_capabilities: vec![
                "spawn_agent".to_string(),
                "assign_task".to_string(),
                "read_status".to_string(),
                "request_revision".to_string(),
                "consolidate_outputs".to_string(),
            ],
            denied_capabilities: vec![
                "shell".to_string(),
                "modify_files".to_string(),
                "commit".to_string(),
                "deploy".to_string(),
            ],
            filesystem_allow: Vec::new(),
            shell_allow: Vec::new(),
            network: "deny".to_string(),
        },
        contract: AgentContract {
            input: "mission_objective.v1".to_string(),
            output: "mission_consolidation.v1".to_string(),
            handoff: "forge.agent_handoff.v1".to_string(),
        },
        limits: AgentLimits {
            max_files_changed: 0,
            max_runtime_seconds: 3600,
            max_cost_usd: 2.0,
            max_children: 8,
        },
    }
}

fn worker_agent(id: &str, role: &str, effort: &str, skills: &[&str]) -> AgentDefinition {
    AgentDefinition {
        id: id.to_string(),
        role: role.to_string(),
        runtime: "codex-cli".to_string(),
        provider: "openai".to_string(),
        model: "auto".to_string(),
        effort: effort.to_string(),
        skills: skills.iter().map(|skill| (*skill).to_string()).collect(),
        tools: vec!["filesystem".to_string(), "tests".to_string()],
        permissions: AgentPermissionPolicy {
            allowed_capabilities: vec![
                "read_workspace".to_string(),
                "execute_assigned_task".to_string(),
                "create_handoff".to_string(),
            ],
            denied_capabilities: vec!["deploy".to_string()],
            filesystem_allow: vec!["src/**".to_string(), "tests/**".to_string()],
            shell_allow: vec!["cargo".to_string(), "git".to_string()],
            network: "restricted".to_string(),
        },
        contract: AgentContract {
            input: "implementation_task.v1".to_string(),
            output: "structured_agent_delivery.v1".to_string(),
            handoff: "forge.agent_handoff.v1".to_string(),
        },
        limits: AgentLimits {
            max_files_changed: 20,
            max_runtime_seconds: 2700,
            max_cost_usd: 5.0,
            max_children: 0,
        },
    }
}

#[derive(Clone, Copy)]
struct OriginalRoleSpec {
    role: &'static str,
    effort: &'static str,
    skills: &'static [&'static str],
    max_instances: usize,
}

struct OriginalSquadSpec {
    id: &'static str,
    name: &'static str,
    version: &'static str,
    objective: &'static str,
    objective_type: &'static str,
    recipe_id: &'static str,
    topology: OrchestrationTopology,
    roles: [OriginalRoleSpec; 3],
    intake_evidence: [&'static str; 2],
    delivery_evidence: [&'static str; 2],
}

fn original_role(
    role: &'static str,
    effort: &'static str,
    skills: &'static [&'static str],
    max_instances: usize,
) -> OriginalRoleSpec {
    OriginalRoleSpec {
        role,
        effort,
        skills,
        max_instances,
    }
}

fn original_squad_specs() -> Vec<OriginalSquadSpec> {
    vec![
        OriginalSquadSpec {
            id: "bug-triage",
            name: "Bug Triage",
            version: "1.0.0",
            objective: "Reproduce, classify and isolate defects before delivering an independently validated repair recommendation.",
            objective_type: "defect_triage",
            recipe_id: "triage-defect",
            topology: OrchestrationTopology::ScoutBuilderReviewer,
            roles: [
                original_role(
                    "scout",
                    "medium",
                    &["bug-reproduction", "log-analysis"],
                    3,
                ),
                original_role(
                    "incident_responder",
                    "high",
                    &["root-cause-analysis", "debugging"],
                    3,
                ),
                original_role(
                    "reviewer",
                    "high",
                    &["regression-testing", "fix-validation"],
                    2,
                ),
            ],
            intake_evidence: ["reproduction_evidence", "severity_assessment"],
            delivery_evidence: ["root_cause_evidence", "regression_tests_passed"],
        },
        OriginalSquadSpec {
            id: "security-audit",
            name: "Security Audit",
            version: "1.0.0",
            objective: "Discover attack surfaces, assess vulnerabilities and issue independently audited remediation evidence.",
            objective_type: "security_audit",
            recipe_id: "audit-security-posture",
            topology: OrchestrationTopology::RedTeamBlueTeam,
            roles: [
                original_role(
                    "scout",
                    "high",
                    &["threat-modeling", "attack-surface-discovery"],
                    2,
                ),
                original_role(
                    "security_reviewer",
                    "maximum",
                    &["vulnerability-analysis", "secure-coding"],
                    3,
                ),
                original_role(
                    "auditor",
                    "high",
                    &["compliance-evidence", "risk-assessment"],
                    2,
                ),
            ],
            intake_evidence: ["threat_model", "asset_inventory"],
            delivery_evidence: ["verified_findings", "remediation_priorities"],
        },
        OriginalSquadSpec {
            id: "architecture-review",
            name: "Architecture Review",
            version: "1.0.0",
            objective: "Evaluate system boundaries and trade-offs, then produce a traceable architecture decision with independent challenge.",
            objective_type: "architecture_review",
            recipe_id: "review-architecture",
            topology: OrchestrationTopology::Judge,
            roles: [
                original_role(
                    "scout",
                    "medium",
                    &["system-discovery", "requirements-analysis"],
                    2,
                ),
                original_role(
                    "reviewer",
                    "maximum",
                    &["architecture-analysis", "design-review"],
                    3,
                ),
                original_role(
                    "auditor",
                    "high",
                    &["decision-records", "risk-assessment"],
                    2,
                ),
            ],
            intake_evidence: ["system_context", "quality_attributes"],
            delivery_evidence: ["architecture_decision", "tradeoff_analysis"],
        },
        OriginalSquadSpec {
            id: "migration-squad",
            name: "Migration Squad",
            version: "1.0.0",
            objective: "Plan and execute reversible migrations with explicit reconciliation and rollback evidence.",
            objective_type: "system_migration",
            recipe_id: "execute-migration",
            topology: OrchestrationTopology::Saga,
            roles: [
                original_role(
                    "scout",
                    "high",
                    &["dependency-inventory", "data-profiling"],
                    2,
                ),
                original_role(
                    "builder",
                    "high",
                    &["migration-engineering", "rollback-design"],
                    3,
                ),
                original_role(
                    "tester",
                    "high",
                    &["migration-validation", "data-reconciliation"],
                    2,
                ),
            ],
            intake_evidence: ["dependency_inventory", "rollback_plan"],
            delivery_evidence: ["reconciliation_passed", "rollback_verified"],
        },
        OriginalSquadSpec {
            id: "incident-response",
            name: "Incident Response",
            version: "1.0.0",
            objective: "Triage, contain and recover incidents while preserving a complete operational evidence trail.",
            objective_type: "incident_response",
            recipe_id: "respond-to-incident",
            topology: OrchestrationTopology::RetryEscalationTree,
            roles: [
                original_role(
                    "observer",
                    "high",
                    &["service-observability", "log-analysis"],
                    3,
                ),
                original_role(
                    "incident_responder",
                    "maximum",
                    &["incident-containment", "service-recovery"],
                    4,
                ),
                original_role(
                    "reviewer",
                    "high",
                    &["recovery-validation", "post-incident-analysis"],
                    2,
                ),
            ],
            intake_evidence: ["incident_timeline", "impact_assessment"],
            delivery_evidence: ["containment_verified", "service_health_restored"],
        },
        OriginalSquadSpec {
            id: "research-squad",
            name: "Research Squad",
            version: "1.0.0",
            objective: "Collect source-grounded evidence, synthesize findings and independently verify claims and citations.",
            objective_type: "research",
            recipe_id: "conduct-research",
            topology: OrchestrationTopology::MapReduce,
            roles: [
                original_role(
                    "researcher",
                    "high",
                    &["source-discovery", "evidence-synthesis"],
                    4,
                ),
                original_role(
                    "data_engineer",
                    "high",
                    &["data-analysis", "reproducibility"],
                    2,
                ),
                original_role(
                    "reviewer",
                    "high",
                    &["fact-checking", "citation-review"],
                    2,
                ),
            ],
            intake_evidence: ["research_question", "source_criteria"],
            delivery_evidence: ["source_index", "claims_verified"],
        },
        OriginalSquadSpec {
            id: "content-studio",
            name: "Content Studio",
            version: "1.0.0",
            objective: "Research, produce and independently review publishable content against audience and brand constraints.",
            objective_type: "content_production",
            recipe_id: "produce-content",
            topology: OrchestrationTopology::SequentialPipeline,
            roles: [
                original_role(
                    "researcher",
                    "medium",
                    &["audience-research", "editorial-planning"],
                    3,
                ),
                original_role(
                    "builder",
                    "high",
                    &["content-production", "brand-guidelines"],
                    4,
                ),
                original_role(
                    "reviewer",
                    "high",
                    &["editorial-review", "fact-checking"],
                    2,
                ),
            ],
            intake_evidence: ["audience_brief", "editorial_constraints"],
            delivery_evidence: ["content_artifact", "editorial_review_passed"],
        },
        OriginalSquadSpec {
            id: "crm-operations",
            name: "CRM Operations",
            version: "1.0.0",
            objective: "Operate customer lifecycle data and automations with privacy, quality and audit controls.",
            objective_type: "crm_operations",
            recipe_id: "operate-crm",
            topology: OrchestrationTopology::ControllerWorkers,
            roles: [
                original_role(
                    "data_engineer",
                    "high",
                    &["crm-data-quality", "customer-segmentation"],
                    3,
                ),
                original_role(
                    "operator",
                    "high",
                    &["crm-automation", "customer-lifecycle"],
                    3,
                ),
                original_role(
                    "auditor",
                    "high",
                    &["privacy-review", "operations-validation"],
                    2,
                ),
            ],
            intake_evidence: ["data_scope", "consent_policy"],
            delivery_evidence: ["automation_evidence", "data_quality_passed"],
        },
        OriginalSquadSpec {
            id: "sales-squad",
            name: "Sales Squad",
            version: "1.0.0",
            objective: "Research opportunities, prepare accountable sales material and independently validate commercial commitments.",
            objective_type: "sales_enablement",
            recipe_id: "prepare-sales-opportunity",
            topology: OrchestrationTopology::FanOutFanIn,
            roles: [
                original_role(
                    "researcher",
                    "medium",
                    &["account-research", "opportunity-qualification"],
                    4,
                ),
                original_role(
                    "executor",
                    "high",
                    &["sales-enablement", "proposal-development"],
                    3,
                ),
                original_role(
                    "reviewer",
                    "high",
                    &["commercial-review", "commitment-risk"],
                    2,
                ),
            ],
            intake_evidence: ["account_context", "qualification_criteria"],
            delivery_evidence: ["proposal_artifact", "commitments_verified"],
        },
        OriginalSquadSpec {
            id: "customer-support",
            name: "Customer Support",
            version: "1.0.0",
            objective: "Resolve customer requests through safe escalation, evidence-backed answers and independent resolution review.",
            objective_type: "customer_support",
            recipe_id: "resolve-customer-request",
            topology: OrchestrationTopology::RetryEscalationTree,
            roles: [
                original_role(
                    "human_liaison",
                    "medium",
                    &["customer-intake", "knowledge-retrieval"],
                    4,
                ),
                original_role(
                    "incident_responder",
                    "high",
                    &["issue-resolution", "safe-escalation"],
                    3,
                ),
                original_role(
                    "reviewer",
                    "high",
                    &["resolution-quality", "customer-safety"],
                    2,
                ),
            ],
            intake_evidence: ["customer_request", "impact_classification"],
            delivery_evidence: ["resolution_evidence", "customer_safety_checked"],
        },
        OriginalSquadSpec {
            id: "data-analysis",
            name: "Data Analysis",
            version: "1.0.0",
            objective: "Prepare trustworthy data, derive decision-grade findings and independently verify reproducibility.",
            objective_type: "data_analysis",
            recipe_id: "analyze-data",
            topology: OrchestrationTopology::MapReduce,
            roles: [
                original_role(
                    "data_engineer",
                    "high",
                    &["data-profiling", "data-preparation"],
                    3,
                ),
                original_role(
                    "researcher",
                    "high",
                    &["statistical-analysis", "insight-synthesis"],
                    4,
                ),
                original_role(
                    "reviewer",
                    "high",
                    &["reproducibility", "decision-review"],
                    2,
                ),
            ],
            intake_evidence: ["data_contract", "analysis_question"],
            delivery_evidence: ["reproducible_analysis", "findings_validated"],
        },
        OriginalSquadSpec {
            id: "infrastructure-operations",
            name: "Infrastructure Operations",
            version: "1.0.0",
            objective: "Plan bounded infrastructure changes and validate reliability, security and rollback readiness before promotion.",
            objective_type: "infrastructure_operations",
            recipe_id: "operate-infrastructure",
            topology: OrchestrationTopology::ControllerWorkers,
            roles: [
                original_role(
                    "observer",
                    "high",
                    &["infrastructure-observability", "capacity-analysis"],
                    3,
                ),
                original_role(
                    "operator",
                    "high",
                    &["infrastructure-automation", "change-management"],
                    3,
                ),
                original_role(
                    "reviewer",
                    "maximum",
                    &["reliability-validation", "security-baseline"],
                    2,
                ),
            ],
            intake_evidence: ["change_scope", "rollback_plan"],
            delivery_evidence: ["change_validation", "reliability_checks_passed"],
        },
        OriginalSquadSpec {
            id: "product-discovery",
            name: "Product Discovery",
            version: "1.0.0",
            objective: "Synthesize user and market evidence into testable opportunities with independent product challenge.",
            objective_type: "product_discovery",
            recipe_id: "discover-product-opportunity",
            topology: OrchestrationTopology::Debate,
            roles: [
                original_role(
                    "researcher",
                    "high",
                    &["user-research", "market-analysis"],
                    4,
                ),
                original_role(
                    "planner",
                    "high",
                    &["opportunity-framing", "experiment-design"],
                    3,
                ),
                original_role(
                    "reviewer",
                    "high",
                    &["evidence-review", "product-strategy"],
                    2,
                ),
            ],
            intake_evidence: ["user_problem", "research_evidence"],
            delivery_evidence: ["opportunity_hypothesis", "experiment_plan"],
        },
        OriginalSquadSpec {
            id: "qa-factory",
            name: "QA Factory",
            version: "1.0.0",
            objective: "Design risk-based coverage, execute tests and independently determine release quality.",
            objective_type: "quality_assurance",
            recipe_id: "validate-quality",
            topology: OrchestrationTopology::FanOutFanIn,
            roles: [
                original_role(
                    "planner",
                    "high",
                    &["test-planning", "risk-based-testing"],
                    2,
                ),
                original_role(
                    "tester",
                    "high",
                    &["test-automation", "exploratory-testing"],
                    5,
                ),
                original_role(
                    "reviewer",
                    "high",
                    &["defect-analysis", "release-quality"],
                    2,
                ),
            ],
            intake_evidence: ["quality_risks", "test_strategy"],
            delivery_evidence: ["test_results", "defect_disposition"],
        },
        OriginalSquadSpec {
            id: "release-squad",
            name: "Release Squad",
            version: "1.0.0",
            objective: "Coordinate a reversible release and promote it only after independent production-readiness validation.",
            objective_type: "release",
            recipe_id: "coordinate-release",
            topology: OrchestrationTopology::Saga,
            roles: [
                original_role(
                    "planner",
                    "high",
                    &["release-planning", "change-inventory"],
                    2,
                ),
                original_role(
                    "deployer",
                    "high",
                    &["deployment-coordination", "rollback-planning"],
                    3,
                ),
                original_role(
                    "reviewer",
                    "maximum",
                    &["release-validation", "production-readiness"],
                    2,
                ),
            ],
            intake_evidence: ["release_manifest", "rollback_plan"],
            delivery_evidence: ["deployment_evidence", "production_smoke_passed"],
        },
    ]
}

fn role_substitutes(role: &str, position: usize) -> Vec<String> {
    let candidates: &[&str] = match position {
        0 => &["scout", "researcher", "observer"],
        1 => &["executor", "builder", "operator"],
        _ => &["reviewer", "tester", "auditor"],
    };
    candidates
        .iter()
        .copied()
        .filter(|candidate| *candidate != role)
        .map(str::to_string)
        .collect()
}

fn is_assurance_role(role: &str) -> bool {
    matches!(
        role,
        "reviewer" | "tester" | "security_reviewer" | "auditor"
    )
}

fn original_squad(spec: OriginalSquadSpec) -> SquadDefinition {
    let gate_namespace = spec.id.replace('-', "_");
    let dependency_skills = spec
        .roles
        .iter()
        .flat_map(|role| role.skills.iter().copied())
        .map(str::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let roster = spec
        .roles
        .iter()
        .enumerate()
        .map(|(position, role)| {
            let agent_id = format!("{}-{}", spec.id, role.role.replace('_', "-"));
            RosterMember {
                role: role.role.to_string(),
                agent: worker_agent(&agent_id, role.role, role.effort, role.skills),
                min_instances: 0,
                max_instances: role.max_instances,
                spawn: AgentSpawnPolicy::OnDemand,
                required: true,
                substitutes: role_substitutes(role.role, position),
                affinity: role
                    .skills
                    .iter()
                    .map(|skill| (*skill).to_string())
                    .collect(),
                reviewer_anti_affinity: is_assurance_role(role.role),
                skill_policy: SkillPolicy {
                    mode: SkillGateMode::Allowlist,
                    allowed: role
                        .skills
                        .iter()
                        .map(|skill| (*skill).to_string())
                        .collect(),
                    denied: vec!["unreviewed-deployment".to_string()],
                },
            }
        })
        .collect();
    let intake_role = spec.roles[0].role;
    let delivery_role = spec.roles[1].role;
    let assurance_role = spec.roles[2].role;
    SquadDefinition {
        schema_version: SQUAD_SCHEMA_VERSION.to_string(),
        id: spec.id.to_string(),
        name: spec.name.to_string(),
        version: spec.version.to_string(),
        objective: spec.objective.to_string(),
        orchestrator: restricted_orchestrator(&format!("{}-maestro", spec.id), "orchestrator"),
        roster,
        topology: spec.topology,
        invocation_policy: "workflow_or_mission_explicit_invocation".to_string(),
        handoff_policy: "typed_event_immediate_orchestrator_wakeup".to_string(),
        lifecycle_policy: AgentLifecyclePolicy {
            spawn: AgentSpawnPolicy::OnDemand,
            idle_timeout_seconds: 30,
            preserve_session: true,
            collect_outputs_before_close: true,
            max_concurrent_agents: 8,
            scale_to_zero: true,
            backpressure: "bounded_queue".to_string(),
        },
        delegation_policy: DelegationPolicy {
            max_depth: 4,
            max_children_per_agent: 8,
            roles_allowed_to_spawn: vec![
                "supervisor".to_string(),
                "orchestrator".to_string(),
                "controller".to_string(),
            ],
            max_branch_cost_usd: 5.0,
            max_branch_runtime_seconds: 2700,
            max_files_per_branch: 20,
            review_required: true,
            cascade_cancel: true,
            context_isolation: "task_local_with_explicit_handoff".to_string(),
        },
        cost_policy: CostPolicy {
            currency: "USD".to_string(),
            mission_limit_usd: 20.0,
            branch_limit_usd: 5.0,
            aggregate_dimensions: vec![
                "mission".to_string(),
                "squad".to_string(),
                "agent".to_string(),
                "task".to_string(),
                "provider".to_string(),
                "model".to_string(),
                "branch".to_string(),
            ],
        },
        dependencies: SquadDependencySet {
            skills: dependency_skills,
            addons: Vec::new(),
            tools: vec!["forge".to_string()],
            mcp_servers: vec!["forge".to_string()],
        },
        distribution: SquadDistribution {
            origin: "forge-original".to_string(),
            channel: "stable".to_string(),
            signed: true,
            signature: Some(format!("forge-original:{}:{}", spec.id, spec.version)),
            trusted: true,
            auto_update: false,
        },
        gates: vec![
            QualityGateDefinition {
                id: format!("{gate_namespace}_intake_ready"),
                trigger: format!("{intake_role}_handoff"),
                validator: "contract".to_string(),
                required_evidence: spec
                    .intake_evidence
                    .iter()
                    .map(|evidence| (*evidence).to_string())
                    .collect(),
                approval_policy: "deterministic".to_string(),
                failure_action: "request_revision".to_string(),
                timeout_action: "block".to_string(),
            },
            QualityGateDefinition {
                id: format!("{gate_namespace}_delivery_validated"),
                trigger: format!("{delivery_role}_handoff"),
                validator: assurance_role.to_string(),
                required_evidence: spec
                    .delivery_evidence
                    .iter()
                    .map(|evidence| (*evidence).to_string())
                    .collect(),
                approval_policy: "reviewer_anti_affinity".to_string(),
                failure_action: format!("return_to_{delivery_role}"),
                timeout_action: "block".to_string(),
            },
            QualityGateDefinition {
                id: format!("{gate_namespace}_mission_ready"),
                trigger: format!("{assurance_role}_handoff"),
                validator: "orchestrator_policy".to_string(),
                required_evidence: vec![
                    "structured_delivery".to_string(),
                    "no_unresolved_risks".to_string(),
                ],
                approval_policy: "validation_before_promotion".to_string(),
                failure_action: "repair".to_string(),
                timeout_action: "block".to_string(),
            },
        ],
        recipes: vec![MissionRecipe {
            id: spec.recipe_id.to_string(),
            objective_type: spec.objective_type.to_string(),
            required_intake_fields: vec![
                "objective".to_string(),
                "acceptance_criteria".to_string(),
            ],
            optional_intake_fields: vec![
                "constraints".to_string(),
                "related_artifacts".to_string(),
            ],
            defaults: BTreeMap::from([
                ("squad".to_string(), spec.id.to_string()),
                ("review".to_string(), "required".to_string()),
                ("handoff".to_string(), "typed_event".to_string()),
            ]),
        }],
    }
}

fn software_factory_squad() -> SquadDefinition {
    let orchestrator = restricted_orchestrator("mission-pilot", "orchestrator");
    let scout = worker_agent(
        "repository-scout",
        "scout",
        "medium",
        &["repository-exploration", "requirements"],
    );
    let builder = worker_agent(
        "rust-builder",
        "builder",
        "high",
        &["rust-backend", "secure-coding", "unit-testing"],
    );
    let reviewer = worker_agent(
        "independent-reviewer",
        "reviewer",
        "high",
        &["code-review", "test-analysis", "security-baseline"],
    );
    SquadDefinition {
        schema_version: SQUAD_SCHEMA_VERSION.to_string(),
        id: "software-factory".to_string(),
        name: "Software Factory".to_string(),
        version: "1.0.0".to_string(),
        objective: "Deliver validated software changes through scout, builder and independent reviewer roles.".to_string(),
        orchestrator,
        roster: vec![
            RosterMember {
                role: "scout".to_string(),
                agent: scout,
                min_instances: 0,
                max_instances: 2,
                spawn: AgentSpawnPolicy::OnDemand,
                required: true,
                substitutes: vec!["researcher".to_string()],
                affinity: vec!["repository_analysis".to_string()],
                reviewer_anti_affinity: false,
                skill_policy: SkillPolicy {
                    mode: SkillGateMode::Allowlist,
                    allowed: vec![
                        "repository-exploration".to_string(),
                        "requirements".to_string(),
                    ],
                    denied: vec!["deployment".to_string()],
                },
            },
            RosterMember {
                role: "builder".to_string(),
                agent: builder,
                min_instances: 0,
                max_instances: 3,
                spawn: AgentSpawnPolicy::OnDemand,
                required: true,
                substitutes: vec!["executor".to_string()],
                affinity: vec!["rust".to_string()],
                reviewer_anti_affinity: false,
                skill_policy: SkillPolicy {
                    mode: SkillGateMode::Allowlist,
                    allowed: vec![
                        "rust-backend".to_string(),
                        "secure-coding".to_string(),
                        "unit-testing".to_string(),
                    ],
                    denied: vec!["unreviewed-deployment".to_string()],
                },
            },
            RosterMember {
                role: "reviewer".to_string(),
                agent: reviewer,
                min_instances: 0,
                max_instances: 2,
                spawn: AgentSpawnPolicy::OnDemand,
                required: true,
                substitutes: vec!["auditor".to_string(), "tester".to_string()],
                affinity: vec!["validation".to_string()],
                reviewer_anti_affinity: true,
                skill_policy: SkillPolicy {
                    mode: SkillGateMode::Allowlist,
                    allowed: vec![
                        "code-review".to_string(),
                        "test-analysis".to_string(),
                        "security-baseline".to_string(),
                    ],
                    denied: vec!["implementation-ownership".to_string()],
                },
            },
        ],
        topology: OrchestrationTopology::ScoutBuilderReviewer,
        invocation_policy: "workflow_or_mission_explicit_invocation".to_string(),
        handoff_policy: "typed_event_immediate_orchestrator_wakeup".to_string(),
        lifecycle_policy: AgentLifecyclePolicy {
            spawn: AgentSpawnPolicy::OnDemand,
            idle_timeout_seconds: 30,
            preserve_session: true,
            collect_outputs_before_close: true,
            max_concurrent_agents: 8,
            scale_to_zero: true,
            backpressure: "bounded_queue".to_string(),
        },
        delegation_policy: DelegationPolicy {
            max_depth: 4,
            max_children_per_agent: 8,
            roles_allowed_to_spawn: vec![
                "supervisor".to_string(),
                "orchestrator".to_string(),
                "controller".to_string(),
            ],
            max_branch_cost_usd: 5.0,
            max_branch_runtime_seconds: 2700,
            max_files_per_branch: 20,
            review_required: true,
            cascade_cancel: true,
            context_isolation: "task_local_with_explicit_handoff".to_string(),
        },
        cost_policy: CostPolicy {
            currency: "USD".to_string(),
            mission_limit_usd: 20.0,
            branch_limit_usd: 5.0,
            aggregate_dimensions: vec![
                "mission".to_string(),
                "squad".to_string(),
                "agent".to_string(),
                "task".to_string(),
                "provider".to_string(),
                "model".to_string(),
                "branch".to_string(),
            ],
        },
        dependencies: SquadDependencySet {
            skills: vec![
                "repository-exploration".to_string(),
                "rust-backend".to_string(),
                "code-review".to_string(),
            ],
            addons: Vec::new(),
            tools: vec!["cargo".to_string(), "git".to_string()],
            mcp_servers: vec!["forge".to_string()],
        },
        distribution: SquadDistribution {
            origin: "forge-original".to_string(),
            channel: "stable".to_string(),
            signed: true,
            signature: Some("forge-original:software-factory:1.0.0".to_string()),
            trusted: true,
            auto_update: false,
        },
        gates: vec![
            QualityGateDefinition {
                id: "requirements_ready".to_string(),
                trigger: "scout_handoff".to_string(),
                validator: "contract".to_string(),
                required_evidence: vec![
                    "requirements_summary".to_string(),
                    "acceptance_criteria".to_string(),
                ],
                approval_policy: "deterministic".to_string(),
                failure_action: "request_revision".to_string(),
                timeout_action: "block".to_string(),
            },
            QualityGateDefinition {
                id: "implementation_validated".to_string(),
                trigger: "builder_handoff".to_string(),
                validator: "independent_reviewer".to_string(),
                required_evidence: vec!["tests_passed".to_string(), "review_passed".to_string()],
                approval_policy: "reviewer_anti_affinity".to_string(),
                failure_action: "return_to_builder".to_string(),
                timeout_action: "block".to_string(),
            },
            QualityGateDefinition {
                id: "mission_outcome_ready".to_string(),
                trigger: "review_complete".to_string(),
                validator: "orchestrator_policy".to_string(),
                required_evidence: vec![
                    "structured_delivery".to_string(),
                    "no_unresolved_risks".to_string(),
                ],
                approval_policy: "validation_before_promotion".to_string(),
                failure_action: "repair".to_string(),
                timeout_action: "block".to_string(),
            },
        ],
        recipes: vec![MissionRecipe {
            id: "rust-api-feature".to_string(),
            objective_type: "backend_feature".to_string(),
            required_intake_fields: vec![
                "feature_description".to_string(),
                "acceptance_criteria".to_string(),
            ],
            optional_intake_fields: vec![
                "related_issue".to_string(),
                "migration_required".to_string(),
            ],
            defaults: BTreeMap::from([
                ("language".to_string(), "rust".to_string()),
                ("tests".to_string(), "required".to_string()),
                ("security_review".to_string(), "true".to_string()),
            ]),
        }],
    }
}

pub fn builtin_squad_catalog() -> SquadCatalogReport {
    let mut squads = vec![software_factory_squad()];
    squads.extend(original_squad_specs().into_iter().map(original_squad));
    SquadCatalogReport {
        schema_version: "forge.squad.catalog.v1".to_string(),
        status: "ready".to_string(),
        squads,
    }
}

fn squad_digest(squad: &SquadDefinition) -> Result<String> {
    let data = serde_json::to_vec(squad)?;
    Ok(format!("{:x}", Sha256::digest(data)))
}

pub fn validate_squad_definition(squad: &SquadDefinition) -> Result<SquadValidationReport> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    if squad.schema_version != SQUAD_SCHEMA_VERSION {
        errors.push(format!(
            "unsupported squad schema {}; expected {SQUAD_SCHEMA_VERSION}",
            squad.schema_version
        ));
    }
    if squad.id.trim().is_empty() || squad.name.trim().is_empty() || squad.version.trim().is_empty()
    {
        errors.push("squad id, name and version are required".to_string());
    }
    if squad.roster.is_empty() {
        errors.push("squad roster must not be empty".to_string());
    }
    let orchestrator = &squad.orchestrator;
    for required in [
        "spawn_agent",
        "assign_task",
        "read_status",
        "request_revision",
        "consolidate_outputs",
    ] {
        if !orchestrator.permissions.allows(required) {
            errors.push(format!(
                "orchestrator must structurally allow capability {required}"
            ));
        }
    }
    for forbidden in ["shell", "modify_files", "commit", "deploy"] {
        if !orchestrator.permissions.denies(forbidden) {
            errors.push(format!(
                "orchestrator must structurally deny capability {forbidden}"
            ));
        }
        if orchestrator.permissions.allows(forbidden)
            || orchestrator.tools.iter().any(|tool| tool == forbidden)
        {
            errors.push(format!(
                "orchestrator cannot expose forbidden capability {forbidden} through tools or allowlists"
            ));
        }
    }
    if !orchestrator.permissions.filesystem_allow.is_empty()
        || !orchestrator.permissions.shell_allow.is_empty()
    {
        errors.push("orchestrator cannot receive filesystem or shell allowlists".to_string());
    }
    if orchestrator.runtime != "forge-control-plane"
        || orchestrator.provider != "forge"
        || orchestrator.model != "policy-only"
    {
        errors
            .push("orchestrator must use the non-executing Forge policy control plane".to_string());
    }
    if orchestrator.permissions.network != "deny" || orchestrator.limits.max_files_changed != 0 {
        errors.push(
            "orchestrator must deny network access and have a zero file-change limit".to_string(),
        );
    }
    if orchestrator
        .tools
        .iter()
        .any(|tool| !orchestrator.permissions.allows(tool))
    {
        errors.push("every orchestrator tool must be backed by an allowed capability".to_string());
    }
    if squad.delegation_policy.max_depth == 0 || squad.delegation_policy.max_children_per_agent == 0
    {
        errors.push("delegation depth and child limits must be positive".to_string());
    }
    if orchestrator.limits.max_children == 0
        || orchestrator.limits.max_children > squad.delegation_policy.max_children_per_agent
        || !squad
            .delegation_policy
            .roles_allowed_to_spawn
            .iter()
            .any(|role| role == &orchestrator.role)
    {
        errors.push(
            "orchestrator child capacity must be positive, policy-bounded and explicitly authorized"
                .to_string(),
        );
    }
    if !squad.delegation_policy.review_required {
        errors.push("production squads must require review".to_string());
    }
    if squad.lifecycle_policy.max_concurrent_agents == 0 {
        errors.push("max_concurrent_agents must be positive".to_string());
    }
    if squad.handoff_policy != "typed_event_immediate_orchestrator_wakeup" {
        errors.push(
            "production squads must use typed event-driven handoffs with immediate wakeup"
                .to_string(),
        );
    }
    if !squad.cost_policy.mission_limit_usd.is_finite()
        || !squad.cost_policy.branch_limit_usd.is_finite()
        || !squad.delegation_policy.max_branch_cost_usd.is_finite()
        || squad.cost_policy.mission_limit_usd <= 0.0
        || squad.cost_policy.branch_limit_usd <= 0.0
        || squad.delegation_policy.max_branch_cost_usd <= 0.0
        || squad.cost_policy.branch_limit_usd > squad.cost_policy.mission_limit_usd
        || squad.delegation_policy.max_branch_cost_usd > squad.cost_policy.branch_limit_usd
    {
        errors.push(
            "mission and branch cost limits must be finite, positive and monotonically bounded"
                .to_string(),
        );
    }
    let mut roles = BTreeSet::new();
    for member in &squad.roster {
        if member.min_instances > member.max_instances || member.max_instances == 0 {
            errors.push(format!(
                "roster role {} has invalid min/max instances",
                member.role
            ));
        }
        if !roles.insert(member.role.clone()) {
            errors.push(format!("duplicate roster role {}", member.role));
        }
        if member.role == "reviewer" && !member.reviewer_anti_affinity {
            errors.push("reviewer role must enforce anti-affinity".to_string());
        }
        if matches!(member.skill_policy.mode, SkillGateMode::Allowlist)
            && member.skill_policy.allowed.is_empty()
        {
            errors.push(format!(
                "allowlisted role {} must declare allowed skills",
                member.role
            ));
        }
        if member.agent.role != member.role {
            errors.push(format!(
                "roster role {} must match its agent role {}",
                member.role, member.agent.role
            ));
        }
        let role_may_spawn = squad
            .delegation_policy
            .roles_allowed_to_spawn
            .iter()
            .any(|role| role == &member.role);
        if !role_may_spawn && member.agent.limits.max_children != 0 {
            errors.push(format!(
                "non-delegating role {} must have a zero child limit",
                member.role
            ));
        }
        if member.agent.limits.max_children > squad.delegation_policy.max_children_per_agent {
            errors.push(format!(
                "role {} exceeds the squad child limit",
                member.role
            ));
        }
        if !member.agent.limits.max_cost_usd.is_finite()
            || member.agent.limits.max_cost_usd <= 0.0
            || member.agent.limits.max_cost_usd > squad.cost_policy.branch_limit_usd
            || member.agent.limits.max_runtime_seconds
                > squad.delegation_policy.max_branch_runtime_seconds
            || member.agent.limits.max_files_changed > squad.delegation_policy.max_files_per_branch
        {
            errors.push(format!(
                "role {} exceeds a branch cost, runtime or file limit",
                member.role
            ));
        }
    }
    if squad.gates.is_empty() {
        errors.push("squad must define at least one quality gate".to_string());
    }
    for gate in &squad.gates {
        if gate.required_evidence.is_empty() {
            errors.push(format!("gate {} must require evidence", gate.id));
        }
    }
    if squad.distribution.origin != "forge-original"
        && (!squad.distribution.signed
            || squad.distribution.signature.is_none()
            || !squad.distribution.trusted)
    {
        errors.push(
            "external squad packages must be signed, trusted and carry a signature".to_string(),
        );
    }
    if squad.distribution.auto_update {
        warnings.push(
            "auto_update is enabled; missions still pin the exact installed version".to_string(),
        );
    }
    let valid = errors.is_empty();
    Ok(SquadValidationReport {
        schema_version: "forge.squad.validation.v1".to_string(),
        status: if valid { "valid" } else { "invalid" }.to_string(),
        valid,
        squad_id: squad.id.clone(),
        squad_version: squad.version.clone(),
        composition_sha256: squad_digest(squad)?,
        errors,
        warnings,
    })
}

pub fn install_squad(store: &ForgeStore, squad: &SquadDefinition) -> Result<SquadInstallReport> {
    let validation = validate_squad_definition(squad)?;
    if !validation.valid {
        bail!(
            "squad {}@{} is invalid: {}",
            squad.id,
            squad.version,
            validation.errors.join("; ")
        );
    }
    let connection = open_configured_connection(store.path())?;
    let existing: Option<String> = connection
        .query_row(
            "SELECT composition_sha256 FROM squad_definitions WHERE id = ?1 AND version = ?2",
            params![squad.id, squad.version],
            |row| row.get(0),
        )
        .optional()?;
    let status = match existing {
        Some(existing_digest) if existing_digest == validation.composition_sha256 => {
            "already_installed"
        }
        Some(_) => {
            bail!(
                "squad {}@{} is immutable and already installed with different bytes; install a new version",
                squad.id,
                squad.version
            )
        }
        None => {
            connection.execute(
                r#"
                INSERT INTO squad_definitions
                    (id, version, name, composition_sha256, data_json, installed_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                "#,
                params![
                    squad.id,
                    squad.version,
                    squad.name,
                    validation.composition_sha256,
                    serde_json::to_string(squad)?,
                    Utc::now().to_rfc3339(),
                ],
            )?;
            "installed"
        }
    };
    Ok(SquadInstallReport {
        schema_version: "forge.squad.install.v1".to_string(),
        status: status.to_string(),
        squad_id: squad.id.clone(),
        squad_version: squad.version.clone(),
        composition_sha256: validation.composition_sha256.clone(),
        validation,
    })
}

pub fn install_builtin_squads(store: &ForgeStore) -> Result<Vec<SquadInstallReport>> {
    builtin_squad_catalog()
        .squads
        .iter()
        .map(|squad| install_squad(store, squad))
        .collect()
}

pub fn load_squad(store: &ForgeStore, id: &str, version: Option<&str>) -> Result<SquadDefinition> {
    let connection = open_configured_connection(store.path())?;
    let data_json: Option<String> = if let Some(version) = version {
        connection
            .query_row(
                "SELECT data_json FROM squad_definitions WHERE id = ?1 AND version = ?2",
                params![id, version],
                |row| row.get(0),
            )
            .optional()?
    } else {
        connection
            .query_row(
                r#"
                SELECT data_json FROM squad_definitions
                WHERE id = ?1
                ORDER BY installed_at DESC, version DESC
                LIMIT 1
                "#,
                [id],
                |row| row.get(0),
            )
            .optional()?
    };
    let data_json = data_json.with_context(|| {
        format!(
            "squad not found: {}{}",
            id,
            version.map(|value| format!("@{value}")).unwrap_or_default()
        )
    })?;
    Ok(serde_json::from_str(&data_json)?)
}

pub fn list_installed_squads(store: &ForgeStore) -> Result<SquadCatalogReport> {
    let connection = open_configured_connection(store.path())?;
    let mut statement = connection
        .prepare("SELECT data_json FROM squad_definitions ORDER BY id ASC, installed_at DESC")?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut squads = Vec::new();
    for row in rows {
        squads.push(serde_json::from_str(&row?)?);
    }
    Ok(SquadCatalogReport {
        schema_version: "forge.squad.catalog.v1".to_string(),
        status: "ready".to_string(),
        squads,
    })
}

pub fn clone_squad(
    store: &ForgeStore,
    source_id: &str,
    source_version: Option<&str>,
    new_id: &str,
    new_name: &str,
    new_version: &str,
) -> Result<SquadInstallReport> {
    let mut squad = load_squad(store, source_id, source_version)?;
    squad.id = new_id.to_string();
    squad.name = new_name.to_string();
    squad.version = new_version.to_string();
    squad.distribution = SquadDistribution {
        origin: "local-fork".to_string(),
        channel: "project".to_string(),
        signed: true,
        signature: Some(format!("local-approved:{new_id}:{new_version}")),
        trusted: true,
        auto_update: false,
    };
    install_squad(store, &squad)
}

pub fn read_squad_manifest(path: &std::path::Path) -> Result<SquadDefinition> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read squad manifest {}", path.display()))?;
    serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse squad manifest {}", path.display()))
}

fn ensure_mission_runtime_schema(store: &ForgeStore) -> Result<()> {
    store.with_immediate_transaction(|connection| {
        connection.execute_batch(
            r#"
        CREATE TABLE IF NOT EXISTS mission_runtime_inbox (
            id TEXT PRIMARY KEY,
            mission_id TEXT NOT NULL,
            handoff_id TEXT NOT NULL UNIQUE,
            recipient_agent TEXT NOT NULL,
            status TEXT NOT NULL,
            attempts INTEGER NOT NULL DEFAULT 0,
            max_attempts INTEGER NOT NULL DEFAULT 3,
            lease_owner TEXT,
            lease_expires_at TEXT,
            last_error TEXT,
            enqueued_at TEXT NOT NULL,
            consumed_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_mission_runtime_inbox_pending
            ON mission_runtime_inbox(mission_id, status, enqueued_at);
        CREATE TABLE IF NOT EXISTS mission_runtime_checkpoints (
            mission_id TEXT NOT NULL,
            revision INTEGER NOT NULL,
            status TEXT NOT NULL,
            data_sha256 TEXT NOT NULL,
            data_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            PRIMARY KEY (mission_id, revision)
        );
            CREATE TABLE IF NOT EXISTS mission_drive_leases (
                mission_id TEXT PRIMARY KEY,
                owner TEXT NOT NULL,
                lease_expires_at TEXT NOT NULL,
                acquired_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS mission_handoff_processing (
                handoff_id TEXT PRIMARY KEY,
                mission_id TEXT NOT NULL,
                phase TEXT NOT NULL,
                outcome TEXT,
                lease_owner TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_mission_handoff_processing_mission_phase
                ON mission_handoff_processing(mission_id, phase, updated_at);
            "#,
        )?;
        Ok(())
    })
}

fn load_squad_on_connection(
    connection: &rusqlite::Connection,
    mission: &MissionRecord,
) -> Result<SquadDefinition> {
    let data_json: Option<String> = connection
        .query_row(
            "SELECT data_json FROM squad_definitions WHERE id = ?1 AND version = ?2",
            params![mission.squad_id, mission.squad_version],
            |row| row.get(0),
        )
        .optional()?;
    let data_json = data_json.with_context(|| {
        format!(
            "mission squad not found while projecting workflow: {}@{}",
            mission.squad_id, mission.squad_version
        )
    })?;
    Ok(serde_json::from_str(&data_json)?)
}

fn normalized_mission_workflow_task(task: &AtomicTask) -> AtomicTask {
    let mut normalized = task.clone();
    normalized.status = TaskStatus::Pending;
    normalized.work_item.backlog_state = "ready".to_string();
    normalized.work_item.goal_validation.definitively_ready = false;
    for subtask in &mut normalized.work_item.subtasks {
        subtask.status = TaskStatus::Pending;
    }
    normalized
}

fn mission_workflow_structure_matches(
    workflow: &Workflow,
    expected_tasks: &[AtomicTask],
) -> Result<bool> {
    if workflow.tasks.len() != expected_tasks.len() {
        return Ok(false);
    }
    let current = workflow
        .tasks
        .iter()
        .map(normalized_mission_workflow_task)
        .collect::<Vec<_>>();
    let expected = expected_tasks
        .iter()
        .map(normalized_mission_workflow_task)
        .collect::<Vec<_>>();
    Ok(serde_json::to_value(current)? == serde_json::to_value(expected)?)
}

fn workflow_task_graph_sha256(tasks: &[AtomicTask]) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(tasks)?)))
}

fn pristine_legacy_planner_graph(
    connection: &rusqlite::Connection,
    workflow: &Workflow,
) -> Result<bool> {
    if workflow.status != "pending"
        || workflow
            .revisions
            .iter()
            .any(|revision| revision.change_type != "worktree_binding_update")
    {
        return Ok(false);
    }
    if serde_json::to_value(&workflow.tasks)?
        != serde_json::to_value(build_tasks(&workflow.intent))?
    {
        return Ok(false);
    }
    let run_count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM runs WHERE workflow_id = ?1",
        [&workflow.id],
        |row| row.get(0),
    )?;
    let lease_count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM task_leases WHERE workflow_id = ?1",
        [&workflow.id],
        |row| row.get(0),
    )?;
    let checkpoint_count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM task_checkpoints WHERE workflow_id = ?1",
        [&workflow.id],
        |row| row.get(0),
    )?;
    Ok(run_count == 0 && lease_count == 0 && checkpoint_count == 0)
}

fn latest_gate_passed(mission: &MissionRecord, squad: &SquadDefinition, gate_index: usize) -> bool {
    squad.gates.get(gate_index).is_some_and(|definition| {
        mission
            .gates
            .iter()
            .rev()
            .find(|gate| gate.gate_id == definition.id)
            .is_some_and(|gate| gate.status == "passed")
    })
}

fn latest_task_handoff_is_authoritatively_accepted(mission: &MissionRecord, task_id: &str) -> bool {
    let Some(handoff) = mission
        .handoffs
        .iter()
        .rev()
        .find(|handoff| handoff.task_id == task_id)
    else {
        return false;
    };
    if handoff.status != "accepted" || !submission_passes(handoff) {
        return false;
    }
    let Some(completed_event) = mission.events.iter().rev().find(|event| {
        event.kind == "mission.task.completed" && event.task_id.as_deref() == Some(task_id)
    }) else {
        return false;
    };
    let correlated_acceptance = mission.events.iter().rev().any(|event| {
        event.kind == "agent.handoff.accepted"
            && event.task_id.as_deref() == Some(task_id)
            && event.correlation_id.as_deref() == Some(handoff.id.as_str())
            && event.sequence > completed_event.sequence
    });
    let accepted_after_completion = handoff
        .accepted_at
        .is_some_and(|accepted_at| accepted_at >= completed_event.occurred_at);
    correlated_acceptance || accepted_after_completion
}

fn project_mission_state_onto_workflow(
    workflow: &mut Workflow,
    mission: &MissionRecord,
    squad: &SquadDefinition,
) -> Result<bool> {
    if workflow.tasks.len() != 3 || mission.tasks.len() != 3 {
        bail!("mission workflow projection requires exactly three canonical tasks");
    }
    let before = serde_json::to_vec(workflow)?;
    let all_gates_passed = latest_required_gates_passed(mission, squad);

    for (index, (workflow_task, mission_task)) in
        workflow.tasks.iter_mut().zip(&mission.tasks).enumerate()
    {
        let accepted = mission_task.status == "completed"
            && latest_task_handoff_is_authoritatively_accepted(mission, &mission_task.id);
        let (completed, definitively_ready) = match index {
            0 => {
                let ready = accepted && latest_gate_passed(mission, squad, 0);
                (ready, ready)
            }
            1 => {
                let review_ready =
                    squad.gates.get(1).is_none() || latest_gate_passed(mission, squad, 1);
                (accepted, accepted && review_ready)
            }
            2 => {
                let ready = accepted && all_gates_passed;
                (ready, ready)
            }
            _ => unreachable!("three-task mission graph was checked above"),
        };

        workflow_task.status = if completed {
            TaskStatus::Completed
        } else {
            match mission_task.status.as_str() {
                "pending" | "repairing" => TaskStatus::Pending,
                "running" | "completed" => TaskStatus::Running,
                unsupported => bail!(
                    "unsupported mission task status `{unsupported}` for {}",
                    mission_task.id
                ),
            }
        };
        workflow_task.work_item.backlog_state = if completed {
            "done"
        } else {
            match mission_task.status.as_str() {
                "pending" => "ready",
                "repairing" => "rework_required",
                "running" => "in_progress",
                "completed" => "validation_pending",
                _ => unreachable!("mission task status was checked above"),
            }
        }
        .to_string();
        workflow_task.work_item.goal_validation.definitively_ready = definitively_ready;
        for subtask in &mut workflow_task.work_item.subtasks {
            subtask.status = if completed {
                TaskStatus::Completed
            } else {
                TaskStatus::Pending
            };
        }
    }

    workflow.status = if mission.status == MissionStatus::Blocked {
        "blocked".to_string()
    } else if workflow.tasks.iter().all(|task| {
        task.status == TaskStatus::Completed && task.work_item.goal_validation.definitively_ready
    }) {
        if mission.status == MissionStatus::Completed {
            "completed".to_string()
        } else {
            "promotion_ready".to_string()
        }
    } else if mission.status == MissionStatus::Planning {
        "pending".to_string()
    } else {
        "running".to_string()
    };

    Ok(before != serde_json::to_vec(workflow)?)
}

fn persist_mission_row(connection: &rusqlite::Connection, mission: &MissionRecord) -> Result<()> {
    let original_workflow_json: Option<String> = connection
        .query_row(
            "SELECT data_json FROM workflows WHERE id = ?1",
            [&mission.workflow_id],
            |row| row.get(0),
        )
        .optional()?;
    let original_workflow_json = original_workflow_json
        .with_context(|| format!("mission workflow not found: {}", mission.workflow_id))?;
    let mut workflow: Workflow = serde_json::from_str(&original_workflow_json)?;
    if workflow.id != mission.workflow_id {
        bail!(
            "mission workflow identity mismatch: expected {}, loaded {}",
            mission.workflow_id,
            workflow.id
        );
    }
    let expected_tasks = mission_atomic_tasks_from_record(mission)?;
    let structure_matches = mission_workflow_structure_matches(&workflow, &expected_tasks)?;
    let mut repaired_legacy_graph = None;
    if !structure_matches {
        if !pristine_legacy_planner_graph(connection, &workflow)? {
            bail!(
                "mission workflow {} diverged from mission {} task graph; refusing to overwrite non-pristine workflow state",
                workflow.id,
                mission.id
            );
        }
        let before_sha256 = workflow_task_graph_sha256(&workflow.tasks)?;
        workflow.tasks = expected_tasks;
        let after_sha256 = workflow_task_graph_sha256(&workflow.tasks)?;
        repaired_legacy_graph = Some((before_sha256, after_sha256));
    }

    let squad = load_squad_on_connection(connection, mission)?;
    let projection_changed = project_mission_state_onto_workflow(&mut workflow, mission, &squad)?;
    if let Some((before_sha256, after_sha256)) = repaired_legacy_graph {
        push_mission_workflow_revision(
            &mut workflow,
            "mission_legacy_graph_repaired",
            format!(
                "replaced pristine planner graph with mission {} revision {}; graph_sha256 {} -> {}",
                mission.id, mission.revision, before_sha256, after_sha256
            ),
        );
    } else if projection_changed {
        push_mission_workflow_revision(
            &mut workflow,
            "mission_state_projected",
            format!(
                "projected mission {} revision {} onto its canonical workflow graph",
                mission.id, mission.revision
            ),
        );
    }

    if mission.status == MissionStatus::Completed {
        let report = validate_workflow(&workflow);
        if !report.promotable {
            bail!(
                "completed mission {} cannot persist a non-promotable workflow {}: {} failed rule(s)",
                mission.id,
                workflow.id,
                report.failed_rules.len()
            );
        }
    }

    let next_workflow_json = serde_json::to_string(&workflow)?;
    if next_workflow_json != original_workflow_json {
        let changed = connection.execute(
            r#"
            UPDATE workflows
            SET goal = ?1, status = ?2, data_json = ?3
            WHERE id = ?4 AND data_json = ?5
            "#,
            params![
                workflow.goal,
                workflow.status,
                next_workflow_json,
                workflow.id,
                original_workflow_json,
            ],
        )?;
        if changed != 1 {
            bail!(
                "mission workflow {} changed concurrently during state projection",
                workflow.id
            );
        }
        replace_workflow_tenant_projection_on_connection(connection, &workflow)?;
    }

    let data_json = serde_json::to_string(mission)?;
    let current_mission_json: Option<String> = connection
        .query_row(
            "SELECT data_json FROM forge_missions WHERE id = ?1",
            [&mission.id],
            |row| row.get(0),
        )
        .optional()?;
    let status = format!("{:?}", mission.status).to_lowercase();
    match current_mission_json {
        None => {
            connection.execute(
                r#"
                INSERT INTO forge_missions
                    (id, workflow_id, squad_id, squad_version, status, data_json, created_at, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
                params![
                    mission.id,
                    mission.workflow_id,
                    mission.squad_id,
                    mission.squad_version,
                    status,
                    data_json,
                    mission.created_at.to_rfc3339(),
                    mission.updated_at.to_rfc3339(),
                ],
            )?;
        }
        Some(current_json) if current_json == data_json => {}
        Some(current_json) => {
            let current: MissionRecord = serde_json::from_str(&current_json)?;
            let extends_persisted_history = mission.revision >= current.revision
                && mission.events.len() >= current.events.len()
                && serde_json::to_value(&mission.events[..current.events.len()])?
                    == serde_json::to_value(&current.events)?;
            if !extends_persisted_history {
                bail!(
                    "stale mission {} revision {}; persisted revision is {} and its event history is not an exact prefix",
                    mission.id,
                    mission.revision,
                    current.revision,
                );
            }
            let changed = connection.execute(
                r#"
                UPDATE forge_missions
                SET status = ?1, data_json = ?2, updated_at = ?3
                WHERE id = ?4 AND data_json = ?5
                "#,
                params![
                    status,
                    data_json,
                    mission.updated_at.to_rfc3339(),
                    mission.id,
                    current_json,
                ],
            )?;
            if changed != 1 {
                bail!(
                    "mission {} changed concurrently while persisting revision {}",
                    mission.id,
                    mission.revision
                );
            }
        }
    }
    for agent in &mission.agents {
        persist_agent_instance_row(connection, &mission.id, agent)?;
    }
    persist_mission_checkpoint(connection, mission, &data_json)
}

fn persist_mission_checkpoint(
    connection: &rusqlite::Connection,
    mission: &MissionRecord,
    data_json: &str,
) -> Result<()> {
    let data_sha256 = format!("{:x}", Sha256::digest(data_json.as_bytes()));
    connection.execute(
        r#"
        INSERT INTO mission_runtime_checkpoints
            (mission_id, revision, status, data_sha256, data_json, created_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(mission_id, revision) DO NOTHING
        "#,
        params![
            mission.id,
            i64::try_from(mission.revision).context("mission revision exceeds SQLite range")?,
            format!("{:?}", mission.status).to_lowercase(),
            data_sha256,
            data_json,
            Utc::now().to_rfc3339(),
        ],
    )?;
    Ok(())
}

fn save_mission(store: &ForgeStore, mission: &MissionRecord) -> Result<()> {
    ensure_mission_runtime_schema(store)?;
    store.with_immediate_transaction(|connection| persist_mission_row(connection, mission))
}

fn persist_agent_instance_row(
    connection: &rusqlite::Connection,
    mission_id: &str,
    agent: &MissionAgentInstance,
) -> Result<()> {
    connection.execute(
        r#"
        INSERT INTO mission_agent_instances
            (id, mission_id, role, parent_id, depth, status, data_json, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        ON CONFLICT(id) DO UPDATE SET
            status=excluded.status,
            data_json=excluded.data_json,
            updated_at=excluded.updated_at
        "#,
        params![
            agent.instance_id,
            mission_id,
            agent.role,
            agent.parent_instance_id,
            i64::try_from(agent.depth).context("agent depth exceeds SQLite integer range")?,
            agent.status,
            serde_json::to_string(agent)?,
            agent.spawned_at.to_rfc3339(),
            agent.updated_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

fn persist_handoff_row(connection: &rusqlite::Connection, handoff: &AgentHandoff) -> Result<()> {
    connection.execute(
        r#"
        INSERT INTO mission_handoffs
            (id, mission_id, task_id, from_agent, to_agent, status, idempotency_key,
             data_json, created_at, accepted_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        ON CONFLICT(id) DO UPDATE SET
            status=excluded.status,
            data_json=excluded.data_json,
            accepted_at=excluded.accepted_at
        "#,
        params![
            handoff.id,
            handoff.mission_id,
            handoff.task_id,
            handoff.from_agent,
            handoff.to_agent,
            handoff.status,
            handoff.idempotency_key,
            serde_json::to_string(handoff)?,
            handoff.created_at.to_rfc3339(),
            handoff
                .accepted_at
                .map(|accepted_at| accepted_at.to_rfc3339()),
        ],
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn append_correlated_event_in_memory(
    mission: &mut MissionRecord,
    kind: &str,
    status: &str,
    actor: &str,
    task_id: Option<&str>,
    correlation_id: Option<&str>,
    caused_by_sequence: Option<usize>,
    summary: &str,
) -> Result<MissionEvent> {
    let sequence = mission
        .events
        .len()
        .checked_add(1)
        .context("mission event sequence overflow")?;
    let event = MissionEvent {
        schema_version: MISSION_EVENT_SCHEMA_VERSION.to_string(),
        id: format!("event_{}", Uuid::new_v4().simple()),
        sequence,
        kind: kind.to_string(),
        status: status.to_string(),
        actor: actor.to_string(),
        task_id: task_id.map(str::to_string),
        correlation_id: correlation_id.map(str::to_string),
        caused_by_sequence,
        summary: summary.to_string(),
        occurred_at: Utc::now(),
    };
    mission.events.push(event.clone());
    mission.revision = mission
        .revision
        .checked_add(1)
        .context("mission revision overflow")?;
    mission.updated_at = Utc::now();
    Ok(event)
}

fn persist_local_mission_event(
    connection: &rusqlite::Connection,
    workflow_id: &str,
    event: &MissionEvent,
) -> Result<()> {
    connection.execute(
        "INSERT INTO events (workflow_id, kind, data_json) VALUES (?1, ?2, ?3)",
        params![workflow_id, event.kind, serde_json::to_string(event)?],
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn append_correlated_event(
    store: &ForgeStore,
    mission: &mut MissionRecord,
    kind: &str,
    status: &str,
    actor: &str,
    task_id: Option<&str>,
    correlation_id: Option<&str>,
    caused_by_sequence: Option<usize>,
    summary: &str,
) -> Result<usize> {
    let before = mission.clone();
    let event = append_correlated_event_in_memory(
        mission,
        kind,
        status,
        actor,
        task_id,
        correlation_id,
        caused_by_sequence,
        summary,
    )?;
    let persisted = store.with_transaction(|| {
        store.record_event(&mission.workflow_id, kind, &serde_json::to_value(&event)?)?;
        save_mission(store, mission)?;
        mission_failpoint("append_event_before_commit")
    });
    if let Err(error) = persisted {
        *mission = before;
        return Err(error);
    }
    Ok(event.sequence)
}

fn append_event(
    store: &ForgeStore,
    mission: &mut MissionRecord,
    kind: &str,
    status: &str,
    actor: &str,
    task_id: Option<&str>,
    summary: &str,
) -> Result<usize> {
    append_correlated_event(
        store, mission, kind, status, actor, task_id, None, None, summary,
    )
}

fn mission_transition_allowed(from: &MissionStatus, to: &MissionStatus) -> bool {
    matches!(
        (from, to),
        (MissionStatus::Planning, MissionStatus::Running)
            | (MissionStatus::Running, MissionStatus::Reviewing)
            | (MissionStatus::Running, MissionStatus::Repairing)
            | (MissionStatus::Running, MissionStatus::Blocked)
            | (MissionStatus::Reviewing, MissionStatus::Repairing)
            | (MissionStatus::Reviewing, MissionStatus::Blocked)
            | (MissionStatus::Repairing, MissionStatus::Reviewing)
            | (MissionStatus::Repairing, MissionStatus::Running)
            | (MissionStatus::Repairing, MissionStatus::Blocked)
            | (MissionStatus::Blocked, MissionStatus::Running)
            | (MissionStatus::Reviewing, MissionStatus::Completed)
    )
}

fn transition_mission(
    store: &ForgeStore,
    mission: &mut MissionRecord,
    status: MissionStatus,
    kind: &str,
    actor: &str,
    summary: &str,
) -> Result<()> {
    if !mission_transition_allowed(&mission.status, &status) {
        bail!(
            "invalid bounded mission transition {:?} -> {:?}",
            mission.status,
            status
        );
    }
    let mut candidate = mission.clone();
    candidate.status = status;
    let persisted_status = format!("{:?}", candidate.status).to_lowercase();
    append_event(
        store,
        &mut candidate,
        kind,
        &persisted_status,
        actor,
        None,
        summary,
    )?;
    *mission = candidate;
    Ok(())
}

fn start_task(
    store: &ForgeStore,
    mission: &mut MissionRecord,
    task_id: &str,
    actor: &str,
) -> Result<()> {
    let index = mission
        .tasks
        .iter()
        .position(|task| task.id == task_id)
        .with_context(|| format!("mission task not found: {task_id}"))?;
    if mission.tasks[index].status != "pending" {
        bail!(
            "task {task_id} cannot start from {}",
            mission.tasks[index].status
        );
    }
    for dependency in &mission.tasks[index].dependencies {
        let dependency_ready = mission
            .tasks
            .iter()
            .any(|task| task.id == *dependency && task.status == "completed");
        if !dependency_ready {
            bail!("task {task_id} is blocked by incomplete dependency {dependency}");
        }
    }
    mission.tasks[index].status = "running".to_string();
    mission.tasks[index].progress_percent = 1;
    append_event(
        store,
        mission,
        "mission.task.started",
        "running",
        actor,
        Some(task_id),
        "task dependencies satisfied and bounded work started",
    )?;
    Ok(())
}

fn complete_task(
    store: &ForgeStore,
    mission: &mut MissionRecord,
    task_id: &str,
    actor: &str,
    artifacts: Vec<String>,
) -> Result<()> {
    let task = mission
        .tasks
        .iter_mut()
        .find(|task| task.id == task_id)
        .with_context(|| format!("mission task not found: {task_id}"))?;
    if task.status == "completed" {
        if task.artifacts != artifacts {
            bail!("task {task_id} was already completed with different artifacts");
        }
        return Ok(());
    }
    if task.status != "running" && task.status != "repairing" {
        bail!("task {task_id} cannot complete from {}", task.status);
    }
    task.status = "completed".to_string();
    task.progress_percent = 100;
    task.artifacts = artifacts;
    append_event(
        store,
        mission,
        "mission.task.completed",
        "completed",
        actor,
        Some(task_id),
        "task output and acceptance evidence checkpointed",
    )?;
    Ok(())
}

fn reopen_task_for_repair(
    store: &ForgeStore,
    mission: &mut MissionRecord,
    task_id: &str,
    actor: &str,
) -> Result<()> {
    let task = mission
        .tasks
        .iter_mut()
        .find(|task| task.id == task_id)
        .with_context(|| format!("mission task not found: {task_id}"))?;
    if task.status == "repairing" {
        return Ok(());
    }
    if task.status != "completed" {
        bail!("task {task_id} can only be repaired after an initial completion");
    }
    task.status = "repairing".to_string();
    task.progress_percent = 75;
    append_event(
        store,
        mission,
        "mission.task.reopened",
        "repairing",
        actor,
        Some(task_id),
        "failed independent gate reopened the task for one bounded repair",
    )?;
    Ok(())
}

fn record_gate_result(
    store: &ForgeStore,
    mission: &mut MissionRecord,
    squad: &SquadDefinition,
    gate_id: &str,
    status: &str,
    evidence: Vec<String>,
) -> Result<()> {
    if status != "passed" && status != "failed" {
        bail!("unsupported gate status {status}");
    }
    let definition = squad
        .gates
        .iter()
        .find(|gate| gate.id == gate_id)
        .with_context(|| format!("quality gate not found: {gate_id}"))?;
    if status == "passed"
        && definition
            .required_evidence
            .iter()
            .any(|required| !evidence.iter().any(|item| item == required))
    {
        bail!("gate {gate_id} cannot pass without all required evidence");
    }
    if mission.gates.iter().rev().any(|gate| {
        gate.gate_id == gate_id
            && gate.repair_cycle == mission.rework_cycles
            && gate.status == status
            && gate.evidence == evidence
    }) {
        return Ok(());
    }
    let prior = mission
        .gates
        .iter()
        .rev()
        .find(|gate| gate.gate_id == gate_id);
    let attempt = prior.map_or(1, |gate| gate.attempt + 1);
    let supersedes_attempt = prior.map(|gate| gate.attempt);
    mission.gates.push(GateResult {
        gate_id: gate_id.to_string(),
        attempt,
        status: status.to_string(),
        validator: definition.validator.clone(),
        evidence,
        failure_action: definition.failure_action.clone(),
        repair_cycle: mission.rework_cycles,
        supersedes_attempt,
        evaluated_at: Utc::now(),
    });
    append_correlated_event(
        store,
        mission,
        "mission.quality_gate.evaluated",
        status,
        &definition.validator,
        None,
        Some(&format!("gate:{gate_id}")),
        None,
        &format!("gate {gate_id} attempt {attempt} evaluated as {status}"),
    )?;
    Ok(())
}

fn latest_required_gates_passed(mission: &MissionRecord, squad: &SquadDefinition) -> bool {
    squad.gates.iter().all(|definition| {
        mission
            .gates
            .iter()
            .rev()
            .find(|gate| gate.gate_id == definition.id)
            .is_some_and(|gate| gate.status == "passed")
    })
}

fn spawn_agent(
    store: &ForgeStore,
    mission: &mut MissionRecord,
    member: &RosterMember,
    requested_by: &str,
) -> Result<String> {
    let squad = load_squad(store, &mission.squad_id, Some(&mission.squad_version))?;
    let parent = mission
        .agents
        .iter()
        .find(|agent| agent.instance_id == requested_by)
        .with_context(|| format!("spawning agent not found: {requested_by}"))?;
    if parent.status != "running" {
        bail!("terminated agent {requested_by} cannot spawn children");
    }
    if !squad
        .delegation_policy
        .roles_allowed_to_spawn
        .iter()
        .any(|role| role == &parent.role)
    {
        bail!("role {} is not authorized to spawn agents", parent.role);
    }
    let parent_limit = if parent.definition_id == squad.orchestrator.id {
        squad.orchestrator.limits.max_children
    } else {
        squad
            .roster
            .iter()
            .find(|candidate| candidate.agent.id == parent.definition_id)
            .map_or(0, |candidate| candidate.agent.limits.max_children)
    }
    .min(squad.delegation_policy.max_children_per_agent);
    let children = mission
        .agents
        .iter()
        .filter(|agent| agent.parent_instance_id.as_deref() == Some(requested_by))
        .count();
    if children >= parent_limit {
        bail!("agent {requested_by} reached its structural child limit");
    }
    let active = mission
        .agents
        .iter()
        .filter(|agent| agent.status == "running")
        .count();
    if active >= squad.lifecycle_policy.max_concurrent_agents {
        bail!("mission agent concurrency limit reached");
    }
    let active_role_instances = mission
        .agents
        .iter()
        .filter(|agent| agent.role == member.role && agent.status == "running")
        .count();
    if active_role_instances >= member.max_instances {
        bail!("role {} reached its active instance limit", member.role);
    }
    let depth = parent.depth + 1;
    if depth > squad.delegation_policy.max_depth {
        bail!("agent hierarchy depth limit reached");
    }
    let instance_id = format!("agent_{}", Uuid::new_v4().simple());
    let now = Utc::now();
    let agent = MissionAgentInstance {
        instance_id: instance_id.clone(),
        definition_id: member.agent.id.clone(),
        role: member.role.clone(),
        status: "running".to_string(),
        spawned_on_demand: matches!(member.spawn, AgentSpawnPolicy::OnDemand),
        parent_instance_id: Some(requested_by.to_string()),
        session_preserved: squad.lifecycle_policy.preserve_session,
        depth,
        cost_usd: 0.0,
        runtime_milliseconds: 0,
        files_changed: 0,
        spawned_at: now,
        updated_at: now,
    };
    let before = mission.clone();
    mission.agents.push(agent.clone());
    mission.cost.agent_spawns += 1;
    let persisted = store.with_transaction(|| {
        append_event(
            store,
            mission,
            "agent.started",
            "running",
            &instance_id,
            None,
            &format!("{} worker spawned on demand", member.role),
        )?;
        mission_failpoint("spawn_before_commit")
    });
    if let Err(error) = persisted {
        *mission = before;
        return Err(error);
    }
    Ok(instance_id)
}

fn terminate_agent(
    store: &ForgeStore,
    mission: &mut MissionRecord,
    instance_id: &str,
) -> Result<()> {
    let index = mission
        .agents
        .iter()
        .position(|agent| agent.instance_id == instance_id)
        .with_context(|| format!("mission agent not found: {instance_id}"))?;
    if mission.agents[index].status == "terminated" {
        return Ok(());
    }
    if mission.agents[index].status != "running" {
        bail!("mission agent {instance_id} is not running");
    }
    let before = mission.clone();
    mission.agents[index].status = "terminated".to_string();
    mission.agents[index].updated_at = Utc::now();
    let persisted = store.with_transaction(|| {
        append_event(
            store,
            mission,
            "agent.terminated",
            "completed",
            instance_id,
            None,
            "outputs collected and worker scaled to zero",
        )?;
        mission_failpoint("terminate_before_commit")
    });
    if let Err(error) = persisted {
        *mission = before;
        return Err(error);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn enqueue_handoff(
    store: &ForgeStore,
    mission: &mut MissionRecord,
    from_agent: &str,
    to_agent: &str,
    task_id: &str,
    idempotency_key: &str,
    summary: &str,
    delivery: StructuredAgentDelivery,
    validations: Vec<String>,
    recommended_next_action: &str,
) -> Result<String> {
    if idempotency_key.trim().is_empty() {
        bail!("handoff idempotency key cannot be empty");
    }
    if delivery.task_id != task_id {
        bail!("handoff delivery task does not match envelope task");
    }
    if !mission.tasks.iter().any(|task| task.id == task_id) {
        bail!("handoff references unknown task {task_id}");
    }
    for agent_id in [from_agent, to_agent] {
        if !mission
            .agents
            .iter()
            .any(|agent| agent.instance_id == agent_id && agent.status == "running")
        {
            bail!("handoff agent {agent_id} is not active");
        }
    }
    if from_agent == to_agent {
        bail!("handoff source and recipient must be different agents");
    }
    let handoff_id = format!("handoff_{}", Uuid::new_v4().simple());
    let created_at = Utc::now();
    let handoff = AgentHandoff {
        schema_version: AGENT_HANDOFF_SCHEMA_VERSION.to_string(),
        id: handoff_id.clone(),
        idempotency_key: idempotency_key.to_string(),
        mission_id: mission.id.clone(),
        from_agent: from_agent.to_string(),
        to_agent: to_agent.to_string(),
        task_id: task_id.to_string(),
        status: "queued".to_string(),
        summary: summary.to_string(),
        delivery,
        validations,
        unresolved_questions: Vec::new(),
        recommended_next_action: recommended_next_action.to_string(),
        created_at,
        accepted_at: None,
    };
    mission.handoffs.push(handoff.clone());
    let created_event = append_correlated_event_in_memory(
        mission,
        "agent.handoff.created",
        "ready",
        from_agent,
        Some(task_id),
        Some(&handoff_id),
        None,
        summary,
    )?;
    let created_checkpoint = mission.clone();
    let inbox_id = format!("inbox_{}", Uuid::new_v4().simple());
    let enqueued_at = Utc::now();
    mission.inbox.push(MissionInboxItem {
        id: inbox_id.clone(),
        handoff_id: handoff_id.clone(),
        recipient_agent: to_agent.to_string(),
        status: "pending".to_string(),
        enqueued_at,
        consumed_at: None,
        wakeup_event_sequence: 0,
        attempts: 0,
        max_attempts: default_inbox_max_attempts(),
        lease_owner: None,
        lease_expires_at: None,
        last_error: None,
    });
    let enqueued_event = append_correlated_event_in_memory(
        mission,
        "agent.inbox.enqueued",
        "pending",
        to_agent,
        Some(task_id),
        Some(&handoff_id),
        Some(created_event.sequence),
        "typed handoff persisted in the recipient inbox",
    )?;
    ensure_mission_runtime_schema(store)?;
    let mut connection = open_configured_connection(store.path())?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    persist_handoff_row(&transaction, &handoff)?;
    persist_mission_checkpoint(
        &transaction,
        &created_checkpoint,
        &serde_json::to_string(&created_checkpoint)?,
    )?;
    persist_local_mission_event(&transaction, &mission.workflow_id, &created_event)?;
    transaction.execute(
        r#"
        INSERT INTO mission_runtime_inbox
            (id, mission_id, handoff_id, recipient_agent, status, attempts,
             max_attempts, enqueued_at)
        VALUES (?1, ?2, ?3, ?4, 'pending', 0, ?5, ?6)
        ON CONFLICT(handoff_id) DO NOTHING
        "#,
        params![
            inbox_id,
            mission.id,
            handoff_id,
            to_agent,
            i64::try_from(default_inbox_max_attempts())
                .context("inbox retry count exceeds SQLite range")?,
            enqueued_at.to_rfc3339(),
        ],
    )?;
    transaction.execute(
        r#"
        INSERT INTO mission_handoff_processing
            (handoff_id, mission_id, phase, outcome, lease_owner, created_at, updated_at)
        VALUES (?1, ?2, ?3, NULL, NULL, ?4, ?4)
        ON CONFLICT(handoff_id) DO NOTHING
        "#,
        params![
            handoff_id,
            mission.id,
            HANDOFF_PHASE_QUEUED,
            enqueued_at.to_rfc3339(),
        ],
    )?;
    persist_local_mission_event(&transaction, &mission.workflow_id, &enqueued_event)?;
    persist_mission_row(&transaction, mission)?;
    mission_failpoint("enqueue_before_commit")?;
    transaction.commit()?;
    Ok(handoff_id)
}

#[allow(clippy::too_many_arguments)]
fn add_handoff(
    store: &ForgeStore,
    mission: &mut MissionRecord,
    from_agent: &str,
    to_agent: &str,
    task_id: &str,
    summary: &str,
    delivery: StructuredAgentDelivery,
    validations: Vec<String>,
    recommended_next_action: &str,
) -> Result<String> {
    let material = format!(
        "{}:{task_id}:{from_agent}:{to_agent}:{}",
        mission.id,
        mission.handoffs.len() + 1
    );
    let idempotency_key = format!("{:x}", Sha256::digest(material.as_bytes()));
    enqueue_handoff(
        store,
        mission,
        from_agent,
        to_agent,
        task_id,
        &idempotency_key,
        summary,
        delivery,
        validations,
        recommended_next_action,
    )
}

#[derive(Debug, Clone)]
struct ClaimedMissionInbox {
    inbox_id: String,
    handoff_id: String,
    lease_owner: String,
    lease_expires_at: DateTime<Utc>,
    attempts: usize,
    max_attempts: usize,
}

#[derive(Debug, Clone)]
struct MissionHandoffProcessing {
    phase: String,
    outcome: Option<String>,
}

fn load_handoff_processing(
    store: &ForgeStore,
    handoff_id: &str,
) -> Result<MissionHandoffProcessing> {
    ensure_mission_runtime_schema(store)?;
    let connection = open_configured_connection(store.path())?;
    connection
        .query_row(
            r#"
            SELECT phase, outcome
            FROM mission_handoff_processing
            WHERE handoff_id=?1
            "#,
            [handoff_id],
            |row| {
                Ok(MissionHandoffProcessing {
                    phase: row.get(0)?,
                    outcome: row.get(1)?,
                })
            },
        )
        .optional()?
        .with_context(|| format!("handoff processing journal is missing: {handoff_id}"))
}

fn handoff_phase_rank(phase: &str) -> Result<u8> {
    match phase {
        HANDOFF_PHASE_QUEUED => Ok(0),
        HANDOFF_PHASE_CLAIMED => Ok(1),
        HANDOFF_PHASE_WOKEN => Ok(2),
        HANDOFF_PHASE_OUTCOME_PERSISTED => Ok(3),
        HANDOFF_PHASE_FINALIZED => Ok(4),
        other => bail!("unsupported mission handoff processing phase: {other}"),
    }
}

fn advance_handoff_processing(
    store: &ForgeStore,
    mission_id: &str,
    handoff_id: &str,
    phase: &str,
    outcome: Option<&str>,
    lease_owner: Option<&str>,
) -> Result<MissionHandoffProcessing> {
    ensure_mission_runtime_schema(store)?;
    let requested_rank = handoff_phase_rank(phase)?;
    let mut connection = open_configured_connection(store.path())?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current: Option<MissionHandoffProcessing> = transaction
        .query_row(
            "SELECT phase, outcome FROM mission_handoff_processing WHERE handoff_id=?1",
            [handoff_id],
            |row| {
                Ok(MissionHandoffProcessing {
                    phase: row.get(0)?,
                    outcome: row.get(1)?,
                })
            },
        )
        .optional()?;
    if let Some(current) = current.as_ref() {
        let current_rank = handoff_phase_rank(&current.phase)?;
        if current_rank > requested_rank {
            transaction.commit()?;
            return Ok(current.clone());
        }
        if current_rank == requested_rank {
            if outcome.is_some() && current.outcome.as_deref() != outcome {
                bail!("handoff processing phase is bound to a different outcome");
            }
            transaction.commit()?;
            return Ok(current.clone());
        }
    }
    let now = Utc::now().to_rfc3339();
    transaction.execute(
        r#"
        INSERT INTO mission_handoff_processing
            (handoff_id, mission_id, phase, outcome, lease_owner, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
        ON CONFLICT(handoff_id) DO UPDATE SET
            phase=excluded.phase,
            outcome=COALESCE(excluded.outcome, mission_handoff_processing.outcome),
            lease_owner=excluded.lease_owner,
            updated_at=excluded.updated_at
        "#,
        params![handoff_id, mission_id, phase, outcome, lease_owner, now],
    )?;
    transaction.commit()?;
    load_handoff_processing(store, handoff_id)
}

fn claim_next_handoff(store: &ForgeStore, mission_id: &str) -> Result<Option<ClaimedMissionInbox>> {
    ensure_mission_runtime_schema(store)?;
    let mut connection = open_configured_connection(store.path())?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let now = Utc::now();
    let now_text = now.to_rfc3339();
    transaction.execute(
        r#"
        UPDATE mission_runtime_inbox
        SET status = 'dead_letter', lease_owner = NULL, lease_expires_at = NULL,
            last_error = COALESCE(last_error, 'retry budget exhausted')
        WHERE mission_id = ?1 AND status = 'leased'
          AND lease_expires_at <= ?2 AND attempts >= max_attempts
        "#,
        params![mission_id, now_text],
    )?;
    let candidate: Option<(String, String, i64, i64)> = transaction
        .query_row(
            r#"
            SELECT id, handoff_id, attempts, max_attempts
            FROM mission_runtime_inbox
            WHERE mission_id = ?1
              AND attempts < max_attempts
              AND (status = 'pending'
                   OR (status = 'leased' AND lease_expires_at <= ?2))
            ORDER BY enqueued_at, id
            LIMIT 1
            "#,
            params![mission_id, now_text],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((inbox_id, handoff_id, attempts, max_attempts)) = candidate else {
        transaction.commit()?;
        return Ok(None);
    };
    let lease_owner = format!("mission-drive-{}", Uuid::new_v4().simple());
    let lease_expires_at = now + Duration::seconds(MISSION_INBOX_LEASE_SECONDS);
    let changed = transaction.execute(
        r#"
        UPDATE mission_runtime_inbox
        SET status = 'leased', attempts = attempts + 1, lease_owner = ?1,
            lease_expires_at = ?2, last_error = NULL
        WHERE id = ?3 AND attempts = ?4
          AND (status = 'pending' OR (status = 'leased' AND lease_expires_at <= ?5))
        "#,
        params![
            lease_owner,
            lease_expires_at.to_rfc3339(),
            inbox_id,
            attempts,
            now_text,
        ],
    )?;
    if changed != 1 {
        bail!("mission inbox claim lost an atomic lease race");
    }
    transaction.execute(
        r#"
        INSERT INTO mission_handoff_processing
            (handoff_id, mission_id, phase, outcome, lease_owner, created_at, updated_at)
        VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?5)
        ON CONFLICT(handoff_id) DO UPDATE SET
            phase=CASE
                WHEN mission_handoff_processing.phase='queued' THEN 'claimed'
                ELSE mission_handoff_processing.phase
            END,
            lease_owner=excluded.lease_owner,
            updated_at=excluded.updated_at
        "#,
        params![
            handoff_id,
            mission_id,
            HANDOFF_PHASE_CLAIMED,
            lease_owner,
            now_text,
        ],
    )?;
    transaction.commit()?;
    Ok(Some(ClaimedMissionInbox {
        inbox_id,
        handoff_id,
        lease_owner,
        lease_expires_at,
        attempts: usize::try_from(attempts + 1).context("negative inbox attempt count")?,
        max_attempts: usize::try_from(max_attempts).context("negative inbox retry limit")?,
    }))
}

fn renew_claimed_handoff_lease(store: &ForgeStore, claim: &mut ClaimedMissionInbox) -> Result<()> {
    let mut connection = open_configured_connection(store.path())?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let expires_at = Utc::now() + Duration::seconds(MISSION_INBOX_LEASE_SECONDS);
    let changed = transaction.execute(
        r#"
        UPDATE mission_runtime_inbox
        SET lease_expires_at=?1
        WHERE id=?2 AND status='leased' AND lease_owner=?3
        "#,
        params![expires_at.to_rfc3339(), claim.inbox_id, claim.lease_owner],
    )?;
    if changed != 1 {
        bail!("mission inbox lease was lost before phase renewal");
    }
    transaction.execute(
        r#"
        UPDATE mission_handoff_processing
        SET lease_owner=?1, updated_at=?2
        WHERE handoff_id=?3 AND phase!='finalized'
        "#,
        params![claim.lease_owner, Utc::now().to_rfc3339(), claim.handoff_id],
    )?;
    transaction.commit()?;
    claim.lease_expires_at = expires_at;
    Ok(())
}

fn wake_claimed_handoff(
    store: &ForgeStore,
    mission: &mut MissionRecord,
    claim: &ClaimedMissionInbox,
) -> Result<usize> {
    let handoff = mission
        .handoffs
        .iter()
        .find(|handoff| handoff.id == claim.handoff_id)
        .cloned()
        .with_context(|| format!("claimed handoff missing from mission: {}", claim.handoff_id))?;
    let enqueued_sequence = correlated_event(mission, &claim.handoff_id, "agent.inbox.enqueued")
        .map(|event| event.sequence)
        .context("claimed handoff is missing its persisted enqueue event")?;
    let inbox = mission
        .inbox
        .iter_mut()
        .find(|item| item.id == claim.inbox_id)
        .with_context(|| {
            format!(
                "claimed inbox item missing from mission: {}",
                claim.inbox_id
            )
        })?;
    inbox.status = "leased".to_string();
    inbox.attempts = claim.attempts;
    inbox.max_attempts = claim.max_attempts;
    inbox.lease_owner = Some(claim.lease_owner.clone());
    inbox.lease_expires_at = Some(claim.lease_expires_at);
    if let Some(existing) = correlated_event(mission, &handoff.id, "agent.wakeup.triggered") {
        let wakeup_sequence = existing.sequence;
        let inbox = mission
            .inbox
            .iter_mut()
            .find(|item| item.id == claim.inbox_id)
            .context("claimed inbox item disappeared while restoring wakeup")?;
        inbox.wakeup_event_sequence = wakeup_sequence;
        mission.updated_at = Utc::now();
        save_mission(store, mission)?;
        advance_handoff_processing(
            store,
            &mission.id,
            &handoff.id,
            HANDOFF_PHASE_WOKEN,
            None,
            Some(&claim.lease_owner),
        )?;
        return Ok(wakeup_sequence);
    }
    append_correlated_event(
        store,
        mission,
        "agent.inbox.leased",
        "leased",
        &claim.lease_owner,
        Some(&handoff.task_id),
        Some(&handoff.id),
        Some(enqueued_sequence),
        "a separate mission drive atomically leased the persisted inbox item",
    )?;
    let leased_sequence = mission.events.len();
    let wakeup_sequence = append_correlated_event(
        store,
        mission,
        "agent.wakeup.triggered",
        "ready",
        &handoff.to_agent,
        Some(&handoff.task_id),
        Some(&handoff.id),
        Some(leased_sequence),
        "the inbox consumer woke the restricted orchestrator after the producer returned",
    )?;
    let inbox = mission
        .inbox
        .iter_mut()
        .find(|item| item.id == claim.inbox_id)
        .context("claimed inbox item disappeared after wakeup")?;
    inbox.wakeup_event_sequence = wakeup_sequence;
    mission.updated_at = Utc::now();
    save_mission(store, mission)?;
    advance_handoff_processing(
        store,
        &mission.id,
        &handoff.id,
        HANDOFF_PHASE_WOKEN,
        None,
        Some(&claim.lease_owner),
    )?;
    Ok(wakeup_sequence)
}

fn accept_claimed_handoff(
    store: &ForgeStore,
    mission: &mut MissionRecord,
    claim: &ClaimedMissionInbox,
    wakeup_sequence: usize,
) -> Result<()> {
    let processing = load_handoff_processing(store, &claim.handoff_id)?;
    if processing.phase == HANDOFF_PHASE_FINALIZED {
        return Ok(());
    }
    if processing.phase != HANDOFF_PHASE_OUTCOME_PERSISTED {
        bail!("mission inbox cannot be consumed before its outcome is persisted");
    }
    let accepted_at = Utc::now();
    let handoff = mission
        .handoffs
        .iter_mut()
        .find(|handoff| handoff.id == claim.handoff_id)
        .with_context(|| {
            format!(
                "claimed handoff missing before acceptance: {}",
                claim.handoff_id
            )
        })?;
    handoff.status = "accepted".to_string();
    handoff.accepted_at = Some(accepted_at);
    let handoff = handoff.clone();
    let inbox = mission
        .inbox
        .iter_mut()
        .find(|item| item.id == claim.inbox_id)
        .context("claimed inbox item disappeared before consumption")?;
    inbox.status = "consumed".to_string();
    inbox.consumed_at = Some(accepted_at);
    inbox.lease_owner = None;
    inbox.lease_expires_at = None;
    let accepted_event = append_correlated_event_in_memory(
        mission,
        "agent.handoff.accepted",
        "accepted",
        &handoff.to_agent,
        Some(&handoff.task_id),
        Some(&handoff.id),
        Some(wakeup_sequence),
        "typed handoff accepted after its outcome was durably persisted",
    )?;
    let mut connection = open_configured_connection(store.path())?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    persist_handoff_row(&transaction, &handoff)?;
    let changed = transaction.execute(
        r#"
        UPDATE mission_runtime_inbox
        SET status = 'consumed', consumed_at = ?1, lease_owner = NULL,
            lease_expires_at = NULL, last_error = NULL
        WHERE id = ?2 AND status = 'leased' AND lease_owner = ?3
        "#,
        params![accepted_at.to_rfc3339(), claim.inbox_id, claim.lease_owner],
    )?;
    if changed != 1 {
        bail!("mission inbox lease was lost before handoff acceptance");
    }
    persist_local_mission_event(&transaction, &mission.workflow_id, &accepted_event)?;
    persist_mission_row(&transaction, mission)?;
    let finalized = transaction.execute(
        r#"
        UPDATE mission_handoff_processing
        SET phase=?1, lease_owner=NULL, updated_at=?2
        WHERE handoff_id=?3 AND phase=?4 AND lease_owner=?5
        "#,
        params![
            HANDOFF_PHASE_FINALIZED,
            accepted_at.to_rfc3339(),
            handoff.id,
            HANDOFF_PHASE_OUTCOME_PERSISTED,
            claim.lease_owner,
        ],
    )?;
    if finalized != 1 {
        bail!("handoff processing journal was lost before finalization");
    }
    mission_failpoint("accept_before_commit")?;
    transaction.commit()?;
    Ok(())
}

fn consume_queued_handoff_for_simulation(
    store: &ForgeStore,
    mission: &mut MissionRecord,
    expected_handoff_id: &str,
) -> Result<()> {
    let claim = claim_next_handoff(store, &mission.id)?
        .context("simulation expected a queued handoff but the inbox was empty")?;
    if claim.handoff_id != expected_handoff_id {
        bail!(
            "simulation claimed handoff {} before expected {}",
            claim.handoff_id,
            expected_handoff_id
        );
    }
    let wakeup_sequence = wake_claimed_handoff(store, mission, &claim)?;
    advance_handoff_processing(
        store,
        &mission.id,
        &claim.handoff_id,
        HANDOFF_PHASE_OUTCOME_PERSISTED,
        Some("simulation_consumed"),
        Some(&claim.lease_owner),
    )?;
    accept_claimed_handoff(store, mission, &claim, wakeup_sequence)
}

fn harness_for(task_id: &str, member: &RosterMember) -> HarnessResolution {
    HarnessResolution {
        task_id: task_id.to_string(),
        agent_id: member.agent.id.clone(),
        role: member.role.clone(),
        runtime: member.agent.runtime.clone(),
        provider: member.agent.provider.clone(),
        model: member.agent.model.clone(),
        effort: member.agent.effort.clone(),
        skills: member.agent.skills.clone(),
        tools: member.agent.tools.clone(),
        resolved_from: format!("squad.roster.{}.agent", member.role),
        overrode: vec![
            "agent.default".to_string(),
            "provider.default_model".to_string(),
        ],
    }
}

fn record_harness_resolution(
    store: &ForgeStore,
    mission: &mut MissionRecord,
    instance_id: &str,
    resolution: HarnessResolution,
) -> Result<()> {
    let task_id = resolution.task_id.clone();
    mission.harnesses.push(resolution);
    append_event(
        store,
        mission,
        "mission.harness.resolved",
        "resolved",
        instance_id,
        Some(&task_id),
        "runtime, provider, model, effort, skills and tools resolved from the pinned squad",
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn charge_agent(
    store: &ForgeStore,
    mission: &mut MissionRecord,
    instance_id: &str,
    task_id: &str,
    usd: f64,
    tokens: u64,
    runtime_ms: u64,
    files_changed: usize,
) -> Result<()> {
    if !usd.is_finite() || usd < 0.0 {
        bail!("agent cost must be finite and non-negative");
    }
    let squad = load_squad(store, &mission.squad_id, Some(&mission.squad_version))?;
    let index = mission
        .agents
        .iter()
        .position(|agent| agent.instance_id == instance_id)
        .with_context(|| format!("mission agent not found: {instance_id}"))?;
    let member = squad
        .roster
        .iter()
        .find(|member| member.agent.id == mission.agents[index].definition_id)
        .with_context(|| {
            format!(
                "squad definition missing for agent {}",
                mission.agents[index].definition_id
            )
        })?;
    let new_agent_cost = mission.agents[index].cost_usd + usd;
    let new_agent_runtime = mission.agents[index]
        .runtime_milliseconds
        .checked_add(runtime_ms)
        .context("agent runtime overflow")?;
    let new_agent_files = mission.agents[index]
        .files_changed
        .checked_add(files_changed)
        .context("agent file count overflow")?;
    let branch_cost_limit = member
        .agent
        .limits
        .max_cost_usd
        .min(squad.cost_policy.branch_limit_usd)
        .min(squad.delegation_policy.max_branch_cost_usd);
    if new_agent_cost > branch_cost_limit {
        bail!("agent {instance_id} exceeded its branch cost limit");
    }
    if new_agent_runtime
        > member
            .agent
            .limits
            .max_runtime_seconds
            .min(squad.delegation_policy.max_branch_runtime_seconds)
            .saturating_mul(1000)
    {
        bail!("agent {instance_id} exceeded its branch runtime limit");
    }
    if new_agent_files
        > member
            .agent
            .limits
            .max_files_changed
            .min(squad.delegation_policy.max_files_per_branch)
    {
        bail!("agent {instance_id} exceeded its branch file limit");
    }
    let new_mission_cost = mission.cost.total_usd + usd;
    if new_mission_cost > mission.budget_usd.min(squad.cost_policy.mission_limit_usd) {
        bail!("mission exceeded its cost budget");
    }
    let role = mission.agents[index].role.clone();
    mission.agents[index].cost_usd = new_agent_cost;
    mission.agents[index].runtime_milliseconds = new_agent_runtime;
    mission.agents[index].files_changed = new_agent_files;
    mission.agents[index].updated_at = Utc::now();
    mission.cost.total_usd = new_mission_cost;
    mission.cost.tokens = mission
        .cost
        .tokens
        .checked_add(tokens)
        .context("mission token count overflow")?;
    mission.cost.runtime_milliseconds = mission
        .cost
        .runtime_milliseconds
        .checked_add(runtime_ms)
        .context("mission runtime overflow")?;
    mission.cost.cpu_milliseconds = mission
        .cost
        .cpu_milliseconds
        .checked_add(runtime_ms / 2)
        .context("mission CPU time overflow")?;
    mission.cost.files_changed = mission
        .cost
        .files_changed
        .checked_add(files_changed)
        .context("mission file count overflow")?;
    *mission.cost.by_role_usd.entry(role).or_default() += usd;
    let task = mission
        .tasks
        .iter_mut()
        .find(|task| task.id == task_id)
        .with_context(|| format!("mission task not found: {task_id}"))?;
    task.cost_usd += usd;
    append_event(
        store,
        mission,
        "mission.cost.recorded",
        "within_limit",
        instance_id,
        Some(task_id),
        "bounded simulated usage recorded under branch and mission limits",
    )?;
    Ok(())
}

#[derive(Debug)]
struct MissionSimulationAudit {
    mission: MissionRecord,
    orchestrator_restricted: bool,
    on_demand_spawn_proven: bool,
    event_driven_handoff_proven: bool,
    validation_before_promotion_proven: bool,
    rework_cycle_proven: bool,
    exact_composition_recorded: bool,
    incremental_persistence_proven: bool,
    hierarchy_limits_enforced: bool,
    cost_limits_enforced: bool,
    inbox_wakeup_proven: bool,
}

fn orchestrator_is_structurally_restricted(squad: &SquadDefinition) -> bool {
    let orchestrator = &squad.orchestrator;
    orchestrator.runtime == "forge-control-plane"
        && orchestrator.provider == "forge"
        && orchestrator.model == "policy-only"
        && orchestrator.permissions.network == "deny"
        && orchestrator.permissions.filesystem_allow.is_empty()
        && orchestrator.permissions.shell_allow.is_empty()
        && orchestrator.limits.max_files_changed == 0
        && ["shell", "modify_files", "commit", "deploy"]
            .iter()
            .all(|capability| {
                orchestrator.permissions.denies(capability)
                    && !orchestrator.permissions.allows(capability)
                    && !orchestrator.tools.iter().any(|tool| tool == capability)
            })
        && orchestrator
            .tools
            .iter()
            .all(|tool| orchestrator.permissions.allows(tool))
}

fn correlated_event<'a>(
    mission: &'a MissionRecord,
    handoff_id: &str,
    kind: &str,
) -> Option<&'a MissionEvent> {
    mission
        .events
        .iter()
        .find(|event| event.kind == kind && event.correlation_id.as_deref() == Some(handoff_id))
}

fn hierarchy_is_bounded(mission: &MissionRecord, squad: &SquadDefinition) -> bool {
    if mission.agents.is_empty() {
        return false;
    }
    let parent_links_valid = mission.agents.iter().all(|agent| {
        if agent.depth == 0 {
            return agent.instance_id == mission.orchestrator_instance_id
                && agent.parent_instance_id.is_none();
        }
        agent.depth <= squad.delegation_policy.max_depth
            && agent.parent_instance_id.as_ref().is_some_and(|parent_id| {
                mission.agents.iter().any(|parent| {
                    parent.instance_id == *parent_id && parent.depth + 1 == agent.depth
                })
            })
    });
    let children_bounded = mission.agents.iter().all(|parent| {
        let definition_limit = if parent.definition_id == squad.orchestrator.id {
            squad.orchestrator.limits.max_children
        } else {
            squad
                .roster
                .iter()
                .find(|member| member.agent.id == parent.definition_id)
                .map_or(0, |member| member.agent.limits.max_children)
        };
        let children = mission
            .agents
            .iter()
            .filter(|agent| agent.parent_instance_id.as_ref() == Some(&parent.instance_id))
            .count();
        children <= definition_limit.min(squad.delegation_policy.max_children_per_agent)
    });
    let mut active_agents = 1usize;
    let concurrency_bounded = mission.events.iter().all(|event| {
        match event.kind.as_str() {
            "agent.started" => active_agents = active_agents.saturating_add(1),
            "agent.terminated" => active_agents = active_agents.saturating_sub(1),
            _ => {}
        }
        active_agents <= squad.lifecycle_policy.max_concurrent_agents
    });
    parent_links_valid && children_bounded && concurrency_bounded
}

fn costs_are_bounded(mission: &MissionRecord, squad: &SquadDefinition) -> bool {
    let total_agent_cost: f64 = mission.agents.iter().map(|agent| agent.cost_usd).sum();
    let agent_limits_hold = mission.agents.iter().all(|agent| {
        if agent.definition_id == squad.orchestrator.id {
            return agent.cost_usd == 0.0
                && agent.runtime_milliseconds == 0
                && agent.files_changed == 0;
        }
        squad
            .roster
            .iter()
            .find(|member| member.agent.id == agent.definition_id)
            .is_some_and(|member| {
                agent.cost_usd
                    <= member
                        .agent
                        .limits
                        .max_cost_usd
                        .min(squad.cost_policy.branch_limit_usd)
                        .min(squad.delegation_policy.max_branch_cost_usd)
                    && agent.runtime_milliseconds
                        <= member
                            .agent
                            .limits
                            .max_runtime_seconds
                            .min(squad.delegation_policy.max_branch_runtime_seconds)
                            .saturating_mul(1000)
                    && agent.files_changed
                        <= member
                            .agent
                            .limits
                            .max_files_changed
                            .min(squad.delegation_policy.max_files_per_branch)
            })
    });
    mission.cost.total_usd <= mission.budget_usd.min(squad.cost_policy.mission_limit_usd)
        && (mission.cost.total_usd - total_agent_cost).abs() < 0.000_001
        && mission.cost.agent_spawns
            == mission
                .agents
                .iter()
                .filter(|agent| agent.role != "orchestrator")
                .count()
        && agent_limits_hold
}

fn repair_cycle_is_verifiable(mission: &MissionRecord, expected: bool) -> bool {
    let implementation_gates: Vec<&GateResult> = mission
        .gates
        .iter()
        .filter(|gate| gate.gate_id == "implementation_validated")
        .collect();
    if !expected {
        return mission.rework_cycles == 0
            && implementation_gates.len() == 1
            && implementation_gates[0].status == "passed";
    }
    let Some(failed) = implementation_gates.first() else {
        return false;
    };
    let Some(passed) = implementation_gates.get(1) else {
        return false;
    };
    let failed_event = mission.events.iter().find(|event| {
        event.kind == "mission.quality_gate.evaluated"
            && event.correlation_id.as_deref() == Some("gate:implementation_validated")
            && event.status == "failed"
    });
    let revision_event = mission
        .events
        .iter()
        .find(|event| event.kind == "agent.revision.requested");
    let revalidation_event = mission.events.iter().find(|event| {
        event.kind == "mission.quality_gate.evaluated"
            && event.correlation_id.as_deref() == Some("gate:implementation_validated")
            && event.status == "passed"
    });
    let repair_handoff = mission
        .handoffs
        .iter()
        .find(|handoff| handoff.delivery.status == "repaired");
    mission.rework_cycles == 1
        && mission.cost.retries == 1
        && failed.attempt == 1
        && failed.status == "failed"
        && failed.repair_cycle == 0
        && passed.attempt == 2
        && passed.status == "passed"
        && passed.repair_cycle == 1
        && passed.supersedes_attempt == Some(1)
        && repair_handoff.is_some_and(|handoff| {
            handoff
                .validations
                .iter()
                .any(|validation| validation == "repair_applied")
        })
        && failed_event
            .zip(revision_event)
            .zip(revalidation_event)
            .is_some_and(|((failed_event, revision_event), revalidation_event)| {
                failed_event.sequence < revision_event.sequence
                    && revision_event.sequence < revalidation_event.sequence
            })
}

fn audit_mission_simulation(
    store: &ForgeStore,
    mission_id: &str,
    squad: &SquadDefinition,
    validation: &SquadValidationReport,
    expected_rework: bool,
) -> Result<MissionSimulationAudit> {
    let mission = load_mission(store, mission_id)?;
    let connection = open_configured_connection(store.path())?;
    let persisted_agent_count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM mission_agent_instances WHERE mission_id = ?1",
        [mission_id],
        |row| row.get(0),
    )?;
    let persisted_handoff_count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM mission_handoffs WHERE mission_id = ?1 AND status = 'accepted' AND accepted_at IS NOT NULL",
        [mission_id],
        |row| row.get(0),
    )?;
    let mut event_statement = connection
        .prepare("SELECT data_json FROM events WHERE workflow_id = ?1 ORDER BY id ASC")?;
    let event_rows =
        event_statement.query_map([&mission.workflow_id], |row| row.get::<_, String>(0))?;
    let mut persisted_event_ids = BTreeSet::new();
    for row in event_rows {
        if let Ok(event) = serde_json::from_str::<MissionEvent>(&row?) {
            persisted_event_ids.insert(event.id);
        }
    }
    let event_sequences_are_contiguous = mission
        .events
        .iter()
        .enumerate()
        .all(|(index, event)| event.sequence == index + 1);
    let incremental_persistence_proven = mission.revision == mission.events.len() as u64
        && event_sequences_are_contiguous
        && persisted_event_ids.len() == mission.events.len()
        && mission
            .events
            .iter()
            .all(|event| persisted_event_ids.contains(&event.id))
        && persisted_agent_count == mission.agents.len() as i64
        && persisted_handoff_count == mission.handoffs.len() as i64;

    let inbox_wakeup_proven = !mission.handoffs.is_empty()
        && mission.handoffs.iter().all(|handoff| {
            let inbox = mission
                .inbox
                .iter()
                .find(|item| item.handoff_id == handoff.id);
            let created = correlated_event(&mission, &handoff.id, "agent.handoff.created");
            let enqueued = correlated_event(&mission, &handoff.id, "agent.inbox.enqueued");
            let leased = correlated_event(&mission, &handoff.id, "agent.inbox.leased");
            let wakeup = correlated_event(&mission, &handoff.id, "agent.wakeup.triggered");
            let accepted = correlated_event(&mission, &handoff.id, "agent.handoff.accepted");
            inbox
                .zip(created)
                .zip(enqueued)
                .zip(leased)
                .zip(wakeup)
                .zip(accepted)
                .is_some_and(
                    |(((((inbox, created), enqueued), leased), wakeup), accepted)| {
                        handoff.status == "accepted"
                            && handoff.accepted_at.is_some()
                            && inbox.status == "consumed"
                            && inbox.consumed_at.is_some()
                            && inbox.recipient_agent == handoff.to_agent
                            && inbox.wakeup_event_sequence == wakeup.sequence
                            && created.sequence < enqueued.sequence
                            && enqueued.sequence < leased.sequence
                            && leased.sequence < wakeup.sequence
                            && enqueued.sequence < wakeup.sequence
                            && wakeup.sequence < accepted.sequence
                            && enqueued.caused_by_sequence == Some(created.sequence)
                            && leased.caused_by_sequence == Some(enqueued.sequence)
                            && wakeup.caused_by_sequence == Some(leased.sequence)
                            && accepted.caused_by_sequence == Some(wakeup.sequence)
                    },
                )
        });
    let completion_sequence = mission
        .events
        .iter()
        .find(|event| event.kind == "mission.completed")
        .map(|event| event.sequence);
    let validation_before_promotion_proven = mission.status == MissionStatus::Completed
        && latest_required_gates_passed(&mission, squad)
        && completion_sequence.is_some_and(|completion_sequence| {
            squad.gates.iter().all(|definition| {
                mission
                    .gates
                    .iter()
                    .rev()
                    .find(|gate| gate.gate_id == definition.id)
                    .and_then(|gate| {
                        mission.events.iter().find(|event| {
                            event.kind == "mission.quality_gate.evaluated"
                                && event.correlation_id.as_deref()
                                    == Some(format!("gate:{}", definition.id).as_str())
                                && event.status == gate.status
                        })
                    })
                    .is_some_and(|event| event.sequence < completion_sequence)
            })
        });
    let on_demand_spawn_proven = mission
        .agents
        .iter()
        .filter(|agent| agent.role != "orchestrator")
        .all(|agent| {
            agent.spawned_on_demand
                && agent.status == "terminated"
                && agent.depth > 0
                && mission
                    .events
                    .iter()
                    .any(|event| event.kind == "agent.started" && event.actor == agent.instance_id)
        });
    let audit = MissionSimulationAudit {
        orchestrator_restricted: orchestrator_is_structurally_restricted(squad),
        on_demand_spawn_proven,
        event_driven_handoff_proven: inbox_wakeup_proven,
        validation_before_promotion_proven,
        rework_cycle_proven: repair_cycle_is_verifiable(&mission, expected_rework),
        exact_composition_recorded: squad_digest(squad)? == validation.composition_sha256
            && mission.squad_composition_sha256 == validation.composition_sha256,
        incremental_persistence_proven,
        hierarchy_limits_enforced: hierarchy_is_bounded(&mission, squad),
        cost_limits_enforced: costs_are_bounded(&mission, squad),
        inbox_wakeup_proven,
        mission,
    };
    let failures = [
        ("orchestrator_restricted", audit.orchestrator_restricted),
        ("on_demand_spawn_proven", audit.on_demand_spawn_proven),
        (
            "event_driven_handoff_proven",
            audit.event_driven_handoff_proven,
        ),
        (
            "validation_before_promotion_proven",
            audit.validation_before_promotion_proven,
        ),
        ("rework_cycle_proven", audit.rework_cycle_proven),
        (
            "exact_composition_recorded",
            audit.exact_composition_recorded,
        ),
        (
            "incremental_persistence_proven",
            audit.incremental_persistence_proven,
        ),
        ("hierarchy_limits_enforced", audit.hierarchy_limits_enforced),
        ("cost_limits_enforced", audit.cost_limits_enforced),
        ("inbox_wakeup_proven", audit.inbox_wakeup_proven),
    ]
    .into_iter()
    .filter_map(|(name, passed)| (!passed).then_some(name))
    .collect::<Vec<_>>();
    if !failures.is_empty() {
        bail!(
            "bounded mission simulation audit failed: {}",
            failures.join(", ")
        );
    }
    Ok(audit)
}

#[derive(Clone, Copy)]
enum MissionTaskPresentation {
    Operational,
    Simulation,
}

fn mission_atomic_tasks_for_squad(
    squad: &SquadDefinition,
    presentation: MissionTaskPresentation,
) -> Result<Vec<AtomicTask>> {
    if squad.roster.len() < 3 {
        bail!("operational missions require a squad roster with at least three roles");
    }
    let (titles, outputs): ([&str; 3], [&str; 3]) = match presentation {
        MissionTaskPresentation::Operational => (
            [
                "Establish mission intake and acceptance evidence",
                "Produce the bounded mission delivery",
                "Independently validate and consolidate the outcome",
            ],
            [
                "mission_intake.v1",
                "structured_agent_delivery.v1",
                "mission_consolidation.v1",
            ],
        ),
        MissionTaskPresentation::Simulation => (
            [
                "Scout objective and workspace",
                "Build bounded implementation",
                "Review, repair and promote",
            ],
            [
                "requirements_summary.v1",
                "structured_agent_delivery.v1",
                "mission_consolidation.v1",
            ],
        ),
    };

    (0..3)
        .map(|index| {
            let id = format!("mission-task-{:03}", index + 1);
            let dependencies = if index == 0 {
                Vec::new()
            } else {
                vec![format!("mission-task-{:03}", index)]
            };
            let acceptance_criteria = squad
                .gates
                .get(index)
                .map(|gate| gate.required_evidence.clone())
                .unwrap_or_else(|| vec!["structured_delivery".to_string()]);
            mission_atomic_task(
                &id,
                titles[index],
                &squad.roster[index].role,
                dependencies,
                outputs[index],
                acceptance_criteria,
            )
        })
        .collect()
}

fn mission_atomic_task(
    id: &str,
    title: &str,
    owner_role: &str,
    dependencies: Vec<String>,
    expected_output: &str,
    acceptance_criteria: Vec<String>,
) -> Result<AtomicTask> {
    let dependency_refs = dependencies.iter().map(String::as_str).collect::<Vec<_>>();
    let validation_rules = acceptance_criteria
        .iter()
        .map(|evidence| ValidationRule {
            kind: "mission_gate_evidence".to_string(),
            command: None,
            expected: evidence.clone(),
        })
        .collect();
    let mut task = graph_task(
        id,
        title,
        &dependency_refs,
        &[
            "mission objective",
            "accepted predecessor handoffs",
            "revisioned squad gate contract",
        ],
        validation_rules,
        expected_output,
        (ExecutorKind::Mixed, 0.0),
    );
    task.work_item.owner_role = owner_role.to_string();
    task.work_item.acceptance_criteria = acceptance_criteria.clone();
    task.work_item.goal_validation.evidence_required = acceptance_criteria;
    task.work_item.goal_validation.goal =
        format!("{title}: satisfy the revisioned mission gate before promotion");
    task.cost.cost_model = "mission_runtime_observation".to_string();
    Ok(task)
}

fn mission_task_from_atomic(task: &AtomicTask) -> MissionTask {
    MissionTask {
        id: task.id.clone(),
        title: task.title.clone(),
        owner_role: task.work_item.owner_role.clone(),
        status: "pending".to_string(),
        dependencies: task.dependencies.clone(),
        expected_output: task.expected_output.clone(),
        acceptance_criteria: task.work_item.acceptance_criteria.clone(),
        progress_percent: 0,
        artifacts: Vec::new(),
        cost_usd: 0.0,
        assigned_agent_id: None,
        attempt: 0,
    }
}

fn mission_atomic_tasks_from_record(mission: &MissionRecord) -> Result<Vec<AtomicTask>> {
    mission
        .tasks
        .iter()
        .map(|task| {
            mission_atomic_task(
                &task.id,
                &task.title,
                &task.owner_role,
                task.dependencies.clone(),
                &task.expected_output,
                task.acceptance_criteria.clone(),
            )
        })
        .collect()
}

fn push_mission_workflow_revision(
    workflow: &mut Workflow,
    change_type: &str,
    summary: String,
) -> u64 {
    let revision = workflow
        .revisions
        .last()
        .map_or(1, |item| item.revision.saturating_add(1));
    workflow.revisions.push(WorkflowRevision {
        revision,
        origin: "mission.runtime".to_string(),
        change_type: change_type.to_string(),
        summary,
        created_at: Utc::now(),
    });
    revision
}

pub fn start_mission(
    store: &ForgeStore,
    goal: &str,
    squad_id: &str,
    squad_version: Option<&str>,
    worktree: &Path,
) -> Result<MissionStartReport> {
    if goal.trim().is_empty() {
        bail!("mission goal cannot be empty");
    }
    install_builtin_squads(store)?;
    let squad = load_squad(store, squad_id, squad_version)?;
    let validation = validate_squad_definition(&squad)?;
    if !validation.valid {
        bail!("squad is not valid for operational mission execution");
    }
    let mut workflow = create_workflow(parse_intent(goal));
    workflow.tasks = mission_atomic_tasks_for_squad(&squad, MissionTaskPresentation::Operational)?;
    push_mission_workflow_revision(
        &mut workflow,
        "mission_graph_materialized",
        format!(
            "materialized the canonical three-task graph for squad {}@{}",
            squad.id, squad.version
        ),
    );
    let mission_tasks = workflow
        .tasks
        .iter()
        .map(mission_task_from_atomic)
        .collect();
    let mission_id = format!("mission_{}", Uuid::new_v4().simple());
    let orchestrator_instance_id = format!("agent_{}", Uuid::new_v4().simple());
    let now = Utc::now();
    let mission = store.with_transaction(|| {
        store.save_workflow(&workflow)?;
        let binding = register_worktree(
            store,
            WorktreeRegisterOptions {
                path: worktree.to_path_buf(),
                id: None,
                workflow_id: Some(workflow.id.clone()),
                task_id: None,
                origin: "mission.start".to_string(),
                created_by_forge: false,
            },
        )?;
        if binding.binding.is_none() {
            bail!("operational mission start did not produce a worktree binding receipt");
        }
        let mut mission = MissionRecord {
            schema_version: MISSION_SCHEMA_VERSION.to_string(),
            id: mission_id,
            workflow_id: workflow.id.clone(),
            tenant_id: workflow.intent.operating_context.organization.id.clone(),
            workspace_id: workflow.intent.operating_context.product.id.clone(),
            objective: goal.trim().to_string(),
            mode: MissionMode::Workflow,
            status: MissionStatus::Planning,
            squad_id: squad.id.clone(),
            squad_version: squad.version.clone(),
            squad_composition_sha256: validation.composition_sha256.clone(),
            orchestrator_instance_id: orchestrator_instance_id.clone(),
            worktree: Some(binding.worktree.worktree_root),
            budget_usd: squad.cost_policy.mission_limit_usd,
            created_at: now,
            updated_at: now,
            tasks: mission_tasks,
            agents: vec![MissionAgentInstance {
                instance_id: orchestrator_instance_id.clone(),
                definition_id: squad.orchestrator.id.clone(),
                role: "orchestrator".to_string(),
                status: "running".to_string(),
                spawned_on_demand: false,
                parent_instance_id: None,
                session_preserved: true,
                depth: 0,
                cost_usd: 0.0,
                runtime_milliseconds: 0,
                files_changed: 0,
                spawned_at: now,
                updated_at: now,
            }],
            handoffs: Vec::new(),
            inbox: Vec::new(),
            gates: Vec::new(),
            events: Vec::new(),
            harnesses: Vec::new(),
            cost: MissionCostLedger::default(),
            rework_cycles: 0,
            revision: 0,
        };
        save_mission(store, &mission)?;
        transition_mission(
            store,
            &mut mission,
            MissionStatus::Running,
            "mission.started",
            &orchestrator_instance_id,
            "operational mission persisted with a bound Git worktree and task graph",
        )?;
        mission_failpoint("start_before_commit")?;
        Ok(mission)
    })?;
    Ok(MissionStartReport {
        schema_version: "forge.mission.start.v1".to_string(),
        status: "started".to_string(),
        mission,
    })
}

fn deterministic_inbox_id(handoff_id: &str) -> String {
    let digest = format!("{:x}", Sha256::digest(handoff_id.as_bytes()));
    format!("inbox_{}", &digest[..24])
}

struct StoredMissionInboxRow {
    id: String,
    handoff_id: String,
    recipient_agent: String,
    status: String,
    attempts: i64,
    max_attempts: i64,
    lease_owner: Option<String>,
    lease_expires_at: Option<String>,
    last_error: Option<String>,
    enqueued_at: String,
    consumed_at: Option<String>,
}

fn load_runtime_inbox_by_handoff(
    connection: &rusqlite::Connection,
    handoff_id: &str,
) -> Result<Option<MissionInboxItem>> {
    let row: Option<StoredMissionInboxRow> = connection
        .query_row(
            r#"
            SELECT id, handoff_id, recipient_agent, status, attempts, max_attempts,
                   lease_owner, lease_expires_at, last_error, enqueued_at, consumed_at
            FROM mission_runtime_inbox
            WHERE handoff_id = ?1
            "#,
            [handoff_id],
            |row| {
                Ok(StoredMissionInboxRow {
                    id: row.get(0)?,
                    handoff_id: row.get(1)?,
                    recipient_agent: row.get(2)?,
                    status: row.get(3)?,
                    attempts: row.get(4)?,
                    max_attempts: row.get(5)?,
                    lease_owner: row.get(6)?,
                    lease_expires_at: row.get(7)?,
                    last_error: row.get(8)?,
                    enqueued_at: row.get(9)?,
                    consumed_at: row.get(10)?,
                })
            },
        )
        .optional()?;
    let Some(row) = row else {
        return Ok(None);
    };
    Ok(Some(MissionInboxItem {
        id: row.id,
        handoff_id: row.handoff_id,
        recipient_agent: row.recipient_agent,
        status: row.status,
        enqueued_at: DateTime::parse_from_rfc3339(&row.enqueued_at)?.with_timezone(&Utc),
        consumed_at: row
            .consumed_at
            .map(|value| DateTime::parse_from_rfc3339(&value).map(|time| time.with_timezone(&Utc)))
            .transpose()?,
        wakeup_event_sequence: 0,
        attempts: usize::try_from(row.attempts).context("negative inbox attempt count")?,
        max_attempts: usize::try_from(row.max_attempts).context("negative inbox retry limit")?,
        lease_owner: row.lease_owner,
        lease_expires_at: row
            .lease_expires_at
            .map(|value| DateTime::parse_from_rfc3339(&value).map(|time| time.with_timezone(&Utc)))
            .transpose()?,
        last_error: row.last_error,
    }))
}

fn reconcile_handoff_materialization(
    store: &ForgeStore,
    mission: &mut MissionRecord,
    persisted_handoff: &AgentHandoff,
) -> Result<String> {
    ensure_mission_runtime_schema(store)?;
    let mut connection = open_configured_connection(store.path())?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mission_handoff = mission
        .handoffs
        .iter()
        .find(|candidate| candidate.id == persisted_handoff.id)
        .cloned();
    let mission_inbox = mission
        .inbox
        .iter()
        .find(|item| item.handoff_id == persisted_handoff.id)
        .cloned();
    let runtime_inbox = load_runtime_inbox_by_handoff(&transaction, &persisted_handoff.id)?;

    let mut handoff = persisted_handoff.clone();
    if mission_handoff
        .as_ref()
        .is_some_and(|candidate| candidate.status == "accepted")
    {
        handoff = mission_handoff
            .clone()
            .context("accepted mission handoff disappeared during reconciliation")?;
    }
    if runtime_inbox
        .as_ref()
        .is_some_and(|item| item.status == "consumed")
        && handoff.status != "accepted"
    {
        handoff.status = "accepted".to_string();
        handoff.accepted_at = runtime_inbox
            .as_ref()
            .and_then(|item| item.consumed_at)
            .or_else(|| Some(Utc::now()));
    }

    let mut inbox = runtime_inbox
        .clone()
        .or_else(|| mission_inbox.clone())
        .unwrap_or_else(|| MissionInboxItem {
            id: deterministic_inbox_id(&handoff.id),
            handoff_id: handoff.id.clone(),
            recipient_agent: handoff.to_agent.clone(),
            status: "pending".to_string(),
            enqueued_at: handoff.created_at,
            consumed_at: None,
            wakeup_event_sequence: 0,
            attempts: 0,
            max_attempts: default_inbox_max_attempts(),
            lease_owner: None,
            lease_expires_at: None,
            last_error: None,
        });
    if let Some(existing) = mission_inbox.as_ref() {
        inbox.wakeup_event_sequence = existing.wakeup_event_sequence;
    }
    if inbox.recipient_agent != handoff.to_agent {
        bail!("persisted mission inbox recipient does not match handoff");
    }
    if handoff.status == "accepted" {
        inbox.status = "consumed".to_string();
        inbox.consumed_at = handoff
            .accepted_at
            .or(inbox.consumed_at)
            .or_else(|| Some(Utc::now()));
        inbox.lease_owner = None;
        inbox.lease_expires_at = None;
    }

    persist_handoff_row(&transaction, &handoff)?;
    if runtime_inbox.is_none() {
        transaction.execute(
            r#"
            INSERT INTO mission_runtime_inbox
                (id, mission_id, handoff_id, recipient_agent, status, attempts,
                 max_attempts, lease_owner, lease_expires_at, last_error,
                 enqueued_at, consumed_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
            params![
                inbox.id,
                mission.id,
                inbox.handoff_id,
                inbox.recipient_agent,
                inbox.status,
                i64::try_from(inbox.attempts)
                    .context("inbox attempt count exceeds SQLite range")?,
                i64::try_from(inbox.max_attempts)
                    .context("inbox retry limit exceeds SQLite range")?,
                inbox.lease_owner,
                inbox.lease_expires_at.map(|value| value.to_rfc3339()),
                inbox.last_error,
                inbox.enqueued_at.to_rfc3339(),
                inbox.consumed_at.map(|value| value.to_rfc3339()),
            ],
        )?;
    } else if handoff.status == "accepted" {
        transaction.execute(
            r#"
            UPDATE mission_runtime_inbox
            SET status='consumed', consumed_at=?1, lease_owner=NULL,
                lease_expires_at=NULL, last_error=NULL
            WHERE handoff_id=?2
            "#,
            params![
                inbox.consumed_at.map(|value| value.to_rfc3339()),
                handoff.id
            ],
        )?;
    }
    let processing_phase = if handoff.status == "accepted" {
        HANDOFF_PHASE_FINALIZED
    } else {
        HANDOFF_PHASE_QUEUED
    };
    transaction.execute(
        r#"
        INSERT INTO mission_handoff_processing
            (handoff_id, mission_id, phase, outcome, lease_owner, created_at, updated_at)
        VALUES (?1, ?2, ?3, NULL, NULL, ?4, ?4)
        ON CONFLICT(handoff_id) DO UPDATE SET
            phase=CASE
                WHEN excluded.phase='finalized' THEN 'finalized'
                ELSE mission_handoff_processing.phase
            END,
            updated_at=excluded.updated_at
        "#,
        params![
            handoff.id,
            mission.id,
            processing_phase,
            Utc::now().to_rfc3339(),
        ],
    )?;

    let mut changed = false;
    if let Some(index) = mission
        .handoffs
        .iter()
        .position(|candidate| candidate.id == handoff.id)
    {
        if serde_json::to_value(&mission.handoffs[index])? != serde_json::to_value(&handoff)? {
            mission.handoffs[index] = handoff.clone();
            changed = true;
        }
    } else {
        mission.handoffs.push(handoff.clone());
        changed = true;
    }
    if let Some(index) = mission
        .inbox
        .iter()
        .position(|candidate| candidate.handoff_id == handoff.id)
    {
        if serde_json::to_value(&mission.inbox[index])? != serde_json::to_value(&inbox)? {
            mission.inbox[index] = inbox.clone();
            changed = true;
        }
    } else {
        mission.inbox.push(inbox.clone());
        changed = true;
    }
    if changed {
        let event = append_correlated_event_in_memory(
            mission,
            "agent.inbox.reconciled",
            &inbox.status,
            "mission-runtime",
            Some(&handoff.task_id),
            Some(&handoff.id),
            None,
            "handoff, mission inbox and runtime inbox were reconciled atomically",
        )?;
        persist_local_mission_event(&transaction, &mission.workflow_id, &event)?;
        persist_mission_row(&transaction, mission)?;
    }
    mission_failpoint("reconcile_before_commit")?;
    transaction.commit()?;
    Ok(inbox.id)
}

fn submission_exists(store: &ForgeStore, mission_id: &str, idempotency_key: &str) -> Result<bool> {
    let connection = open_configured_connection(store.path())?;
    let exists: bool = connection.query_row(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM mission_handoffs
            WHERE mission_id = ?1 AND idempotency_key = ?2
        )
        "#,
        params![mission_id, idempotency_key],
        |row| row.get(0),
    )?;
    Ok(exists)
}

fn existing_submission(
    store: &ForgeStore,
    mission: &mut MissionRecord,
    submission: &MissionSubmission,
) -> Result<Option<MissionSubmitReport>> {
    let connection = open_configured_connection(store.path())?;
    let data_json: Option<String> = connection
        .query_row(
            "SELECT data_json FROM mission_handoffs WHERE mission_id = ?1 AND idempotency_key = ?2",
            params![mission.id, submission.idempotency_key],
            |row| row.get(0),
        )
        .optional()?;
    let Some(data_json) = data_json else {
        return Ok(None);
    };
    let handoff: AgentHandoff = serde_json::from_str(&data_json)?;
    let expected_delivery = StructuredAgentDelivery {
        task_id: submission.task_id.clone(),
        status: submission.status.clone(),
        summary: submission.summary.clone(),
        artifacts: submission.artifacts.clone(),
        tests_passed: submission.tests_passed,
        tests_failed: submission.tests_failed,
        risks: submission.risks.clone(),
        followups: submission.followups.clone(),
    };
    if handoff.mission_id != mission.id
        || handoff.task_id != submission.task_id
        || handoff.from_agent != submission.agent_id
        || serde_json::to_value(&handoff.delivery)? != serde_json::to_value(expected_delivery)?
        || handoff.validations != submission.validations
    {
        bail!("idempotency key is already bound to a different mission submission");
    }
    let inbox_id = reconcile_handoff_materialization(store, mission, &handoff)?;
    Ok(Some(MissionSubmitReport {
        schema_version: "forge.mission.submit.v1".to_string(),
        status: "deduplicated".to_string(),
        mission_id: mission.id.clone(),
        handoff_id: handoff.id,
        inbox_id,
        producer_revision: mission.revision,
        deduplicated: true,
        accepted: handoff.status == "accepted",
    }))
}

fn submit_mission_inner(
    store: &ForgeStore,
    mission_id: &str,
    submission: MissionSubmission,
) -> Result<MissionSubmitReport> {
    if submission.idempotency_key.trim().is_empty() {
        bail!("mission submit requires a non-empty idempotency key");
    }
    if submission.execution_receipt_id.trim().is_empty() {
        bail!("mission submit requires a mission execution receipt");
    }
    if submission.summary.trim().is_empty() {
        bail!("mission submit requires a non-empty delivery summary");
    }
    let mut mission = load_mission(store, mission_id)?;
    if submission_exists(store, mission_id, &submission.idempotency_key)? {
        let receipt = load_mission_execution_receipt(store, &submission.execution_receipt_id)?;
        let submission = authoritative_existing_submission(&mission, submission, &receipt)?;
        return existing_submission(store, &mut mission, &submission)?
            .context("mission submission disappeared during idempotent retry");
    }
    let expected_mission_revision = mission.revision;
    let receipt = claim_mission_execution_receipt_for_submission(
        store,
        &submission.execution_receipt_id,
        mission_id,
        expected_mission_revision,
        &submission.task_id,
        &submission.agent_id,
        &submission.idempotency_key,
    )?;
    let receipt_id = submission.execution_receipt_id.clone();
    let submission_key = submission.idempotency_key.clone();
    let result = (|| {
        let mission = load_mission(store, mission_id)?;
        let submission = authoritative_submission(&mission, submission, &receipt)?;
        submit_claimed_mission_receipt(store, mission_id, submission, &receipt)
    })();
    match result {
        Ok(report) => Ok(report),
        Err(error) => match release_mission_execution_receipt_submission_claim(
            store,
            &receipt_id,
            &submission_key,
        ) {
            Ok(()) => Err(error),
            Err(release_error) => Err(error.context(format!(
                "mission submission failed and receipt claim rollback also failed: {release_error:#}"
            ))),
        },
    }
}

fn authoritative_submission(
    mission: &MissionRecord,
    submission: MissionSubmission,
    receipt: &MissionExecutionReceipt,
) -> Result<MissionSubmission> {
    authoritative_submission_at_revision(mission, submission, receipt, mission.revision)
}

fn authoritative_existing_submission(
    mission: &MissionRecord,
    submission: MissionSubmission,
    receipt: &MissionExecutionReceipt,
) -> Result<MissionSubmission> {
    authoritative_submission_at_revision(mission, submission, receipt, receipt.mission_revision)
}

fn authoritative_submission_at_revision(
    mission: &MissionRecord,
    mut submission: MissionSubmission,
    receipt: &MissionExecutionReceipt,
    expected_mission_revision: u64,
) -> Result<MissionSubmission> {
    let task = mission
        .tasks
        .iter()
        .find(|task| task.id == submission.task_id)
        .with_context(|| format!("mission task not found: {}", submission.task_id))?;
    let producer = mission
        .agents
        .iter()
        .find(|agent| agent.instance_id == submission.agent_id)
        .context("mission submission producer is absent from the persisted mission")?;
    if producer.role != task.owner_role {
        bail!("mission submission producer role does not match the task owner role");
    }
    let verified_claims = verified_mission_execution_claims(
        receipt,
        &mission.id,
        &mission.workflow_id,
        expected_mission_revision,
        &submission.task_id,
        &submission.agent_id,
    )?;

    submission.status = "completed".to_string();
    submission.artifacts = receipt
        .evidence
        .iter()
        .map(|evidence| {
            format!(
                "{}:{}:sha256:{}",
                evidence.kind, evidence.locator, evidence.sha256
            )
        })
        .collect();
    submission.validations = verified_claims
        .claims
        .iter()
        .map(|claim| match claim {
            MissionExecutionClaimKind::ExecutionCompleted => "execution_completed".to_string(),
            MissionExecutionClaimKind::TestsPassed => "tests_passed".to_string(),
        })
        .collect();
    submission.validations.extend(
        verified_claims
            .gate_evidence
            .iter()
            .map(|claim| claim.evidence_kind.clone()),
    );
    submission
        .validations
        .push(format!("execution_receipt:{}", verified_claims.receipt_id));
    submission.validations.push(format!(
        "execution_receipt_sha256:{}",
        verified_claims.receipt_sha256
    ));
    submission.tests_passed = receipt.tests_passed;
    submission.tests_failed = receipt.tests_failed;
    Ok(submission)
}

fn apply_execution_accounting(
    store: &ForgeStore,
    mission: &mut MissionRecord,
    receipt: &MissionExecutionReceipt,
) -> Result<()> {
    if mission.revision != receipt.mission_revision {
        bail!("mission revision changed after execution; obtain a fresh assignment and receipt");
    }
    let duration_ms =
        u64::try_from(receipt.duration_ms).context("execution duration exceeds ledger range")?;
    let agent_index = mission
        .agents
        .iter()
        .position(|agent| agent.instance_id == receipt.agent_id)
        .context("execution receipt agent disappeared before accounting")?;
    let squad = load_squad(store, &mission.squad_id, Some(&mission.squad_version))?;
    let definition_id = mission.agents[agent_index].definition_id.clone();
    let member = squad
        .roster
        .iter()
        .find(|member| member.agent.id == definition_id)
        .context("execution receipt agent is absent from the pinned squad")?;
    let metrics = resolved_mission_execution_metrics(receipt)?;
    let observed_cost_usd = metrics.cost_usd.value.with_context(|| {
        format!(
            "execution cost is unknown; finite agent or mission budget blocks submission ({})",
            metrics
                .cost_usd
                .reason
                .as_deref()
                .unwrap_or("missing trusted cost observation")
        )
    })?;
    let observed_files_changed = metrics.files_changed.value.with_context(|| {
        format!(
            "execution file changes are unknown; finite file budget blocks submission ({})",
            metrics
                .files_changed
                .reason
                .as_deref()
                .unwrap_or("missing trusted file observation")
        )
    })?;
    let observed_external_calls = metrics.external_calls.value.with_context(|| {
        format!(
            "execution external calls are unknown and cannot be added to the ledger ({})",
            metrics
                .external_calls
                .reason
                .as_deref()
                .unwrap_or("missing trusted network observation")
        )
    })?;
    let next_agent_runtime = mission.agents[agent_index]
        .runtime_milliseconds
        .checked_add(duration_ms)
        .context("agent runtime ledger overflow")?;
    let runtime_limit = member
        .agent
        .limits
        .max_runtime_seconds
        .saturating_mul(1_000);
    if next_agent_runtime > runtime_limit {
        bail!("execution receipt exceeds the agent runtime budget");
    }
    let next_agent_files = mission.agents[agent_index]
        .files_changed
        .checked_add(observed_files_changed)
        .context("agent file-change ledger overflow")?;
    if next_agent_files > member.agent.limits.max_files_changed {
        bail!("execution receipt exceeds the agent file-change budget");
    }
    let next_agent_cost = mission.agents[agent_index].cost_usd + observed_cost_usd;
    if !next_agent_cost.is_finite() || next_agent_cost > member.agent.limits.max_cost_usd {
        bail!("execution receipt exceeds the agent cost budget");
    }
    let next_mission_cost = mission.cost.total_usd + observed_cost_usd;
    if !next_mission_cost.is_finite() || next_mission_cost > mission.budget_usd {
        bail!("execution receipt exceeds the mission cost budget");
    }
    mission.agents[agent_index].runtime_milliseconds = next_agent_runtime;
    mission.agents[agent_index].files_changed = next_agent_files;
    mission.agents[agent_index].cost_usd = next_agent_cost;
    mission.agents[agent_index].updated_at = Utc::now();
    mission.cost.total_usd = next_mission_cost;
    mission.cost.runtime_milliseconds = mission
        .cost
        .runtime_milliseconds
        .checked_add(duration_ms)
        .context("mission runtime ledger overflow")?;
    mission.cost.files_changed = mission
        .cost
        .files_changed
        .checked_add(observed_files_changed)
        .context("mission file-change ledger overflow")?;
    mission.cost.external_calls = mission
        .cost
        .external_calls
        .checked_add(observed_external_calls)
        .context("mission external-call ledger overflow")?;
    Ok(())
}

fn submit_claimed_mission_receipt(
    store: &ForgeStore,
    mission_id: &str,
    submission: MissionSubmission,
    receipt: &MissionExecutionReceipt,
) -> Result<MissionSubmitReport> {
    let mut mission = load_mission(store, mission_id)?;
    if let Some(existing) = existing_submission(store, &mut mission, &submission)? {
        return Ok(existing);
    }
    if mission
        .handoffs
        .iter()
        .any(|handoff| handoff.task_id == submission.task_id && handoff.status == "queued")
    {
        bail!("mission task already has a queued submission awaiting resume");
    }
    if matches!(
        mission.status,
        MissionStatus::Blocked
            | MissionStatus::Completed
            | MissionStatus::Failed
            | MissionStatus::Cancelled
            | MissionStatus::Archived
    ) {
        bail!("mission {mission_id} no longer accepts submissions");
    }
    let task = mission
        .tasks
        .iter()
        .find(|task| task.id == submission.task_id)
        .with_context(|| format!("mission task not found: {}", submission.task_id))?;
    if task.assigned_agent_id.as_deref() != Some(submission.agent_id.as_str()) {
        bail!("mission submission agent does not own the current task assignment");
    }
    if task.status != "running" && task.status != "repairing" {
        bail!("mission task {} is not accepting a delivery", task.id);
    }
    let producer = mission
        .agents
        .iter()
        .find(|agent| agent.instance_id == submission.agent_id && agent.status == "running")
        .context("mission submission producer is not an active agent")?;
    if producer.role != task.owner_role {
        bail!("mission submission producer role does not match the task owner role");
    }
    apply_execution_accounting(store, &mut mission, receipt)?;
    let delivery = StructuredAgentDelivery {
        task_id: submission.task_id.clone(),
        status: submission.status.clone(),
        summary: submission.summary.clone(),
        artifacts: submission.artifacts.clone(),
        tests_passed: submission.tests_passed,
        tests_failed: submission.tests_failed,
        risks: submission.risks.clone(),
        followups: submission.followups.clone(),
    };
    let orchestrator_id = mission.orchestrator_instance_id.clone();
    let handoff_id = enqueue_handoff(
        store,
        &mut mission,
        &submission.agent_id,
        &orchestrator_id,
        &submission.task_id,
        &submission.idempotency_key,
        &submission.summary,
        delivery,
        submission.validations,
        "resume mission inbox",
    )?;
    let inbox_id = mission
        .inbox
        .iter()
        .find(|item| item.handoff_id == handoff_id)
        .map(|item| item.id.clone())
        .context("queued mission handoff has no inbox receipt")?;
    Ok(MissionSubmitReport {
        schema_version: "forge.mission.submit.v1".to_string(),
        status: "queued".to_string(),
        mission_id: mission.id,
        handoff_id,
        inbox_id,
        producer_revision: mission.revision,
        deduplicated: false,
        accepted: false,
    })
}

pub fn submit_mission(
    store: &ForgeStore,
    mission_id: &str,
    submission: MissionSubmission,
) -> Result<MissionSubmitReport> {
    with_mission_drive_lease(store, mission_id, |lease_owner| {
        renew_mission_drive_lease(store, mission_id, lease_owner)?;
        submit_mission_inner(store, mission_id, submission)
    })
}

fn submission_passes(handoff: &AgentHandoff) -> bool {
    matches!(handoff.delivery.status.as_str(), "completed" | "repaired")
        && handoff.delivery.tests_failed == 0
}

fn evidence_passes(handoff: &AgentHandoff, gate: &QualityGateDefinition) -> bool {
    evidence_values_pass(handoff, gate, &handoff.validations)
}

fn evidence_values_pass(
    handoff: &AgentHandoff,
    gate: &QualityGateDefinition,
    validations: &[String],
) -> bool {
    submission_passes(handoff)
        && gate
            .required_evidence
            .iter()
            .all(|required| validations.iter().any(|item| item == required))
}

fn mark_task_unassigned(mission: &mut MissionRecord, task_id: &str) -> Result<()> {
    let task = mission
        .tasks
        .iter_mut()
        .find(|task| task.id == task_id)
        .with_context(|| format!("mission task not found: {task_id}"))?;
    task.assigned_agent_id = None;
    Ok(())
}

fn create_operational_repair(
    store: &ForgeStore,
    mission: &mut MissionRecord,
    failed_task_index: usize,
    repair_task_index: usize,
    actor: &str,
    reason: &str,
) -> Result<()> {
    if mission.status != MissionStatus::Repairing {
        mission.rework_cycles = mission
            .rework_cycles
            .checked_add(1)
            .context("mission repair cycle overflow")?;
        mission.cost.retries = mission
            .cost
            .retries
            .checked_add(1)
            .context("mission retry count overflow")?;
        transition_mission(
            store,
            mission,
            MissionStatus::Repairing,
            "mission.repair.started",
            &mission.orchestrator_instance_id.clone(),
            reason,
        )?;
    }
    let repair_task_id = mission.tasks[repair_task_index].id.clone();
    reopen_task_for_repair(store, mission, &repair_task_id, actor)?;
    if failed_task_index != repair_task_index {
        let failed_task_id = mission.tasks[failed_task_index].id.clone();
        let already_invalidated = {
            let failed_task = &mission.tasks[failed_task_index];
            failed_task.status == "pending"
                && failed_task.progress_percent == 0
                && failed_task.artifacts.is_empty()
                && failed_task.assigned_agent_id.is_none()
        };
        if !already_invalidated {
            let failed_task = &mut mission.tasks[failed_task_index];
            failed_task.status = "pending".to_string();
            failed_task.progress_percent = 0;
            failed_task.artifacts.clear();
            failed_task.assigned_agent_id = None;
            append_event(
                store,
                mission,
                "mission.task.invalidated",
                "pending",
                actor,
                Some(&failed_task_id),
                "downstream assurance was invalidated until repaired delivery is revalidated",
            )?;
        }
    }
    let repair_started_sequence = mission
        .events
        .iter()
        .rev()
        .find(|event| event.kind == "mission.repair.started")
        .map_or(0, |event| event.sequence);
    let revision_already_requested = mission.events.iter().any(|event| {
        event.sequence >= repair_started_sequence
            && event.kind == "agent.revision.requested"
            && event.task_id.as_deref() == Some(repair_task_id.as_str())
    });
    if !revision_already_requested {
        append_event(
            store,
            mission,
            "agent.revision.requested",
            "repair_required",
            actor,
            Some(&repair_task_id),
            reason,
        )?;
    }
    Ok(())
}

fn infer_persisted_handoff_outcome(
    mission: &MissionRecord,
    handoff: &AgentHandoff,
    task_index: usize,
    squad: &SquadDefinition,
) -> Option<String> {
    let task = mission.tasks.get(task_index)?;
    if task_index + 1 == mission.tasks.len()
        && task.status == "completed"
        && submission_passes(handoff)
        && latest_required_gates_passed(mission, squad)
    {
        return Some("mission_ready".to_string());
    }
    let producer_terminated = mission
        .agents
        .iter()
        .find(|agent| agent.instance_id == handoff.from_agent)
        .is_some_and(|agent| agent.status == "terminated");
    if producer_terminated {
        return Some(
            match mission.status {
                MissionStatus::Completed => "mission_completed",
                MissionStatus::Repairing => "repair_created",
                _ => "handoff_consumed",
            }
            .to_string(),
        );
    }
    if mission.status == MissionStatus::Repairing
        && (task.status == "repairing"
            || (task.status == "pending" && task.assigned_agent_id.is_none()))
    {
        return Some("repair_created".to_string());
    }
    None
}

fn promote_ready_mission_if_possible(
    store: &ForgeStore,
    mission: &mut MissionRecord,
) -> Result<bool> {
    if mission.status == MissionStatus::Completed {
        return Ok(true);
    }
    if mission.status != MissionStatus::Reviewing {
        return Ok(false);
    }
    let Some(final_task) = mission.tasks.last() else {
        return Ok(false);
    };
    let squad = load_squad(store, &mission.squad_id, Some(&mission.squad_version))?;
    if !mission.tasks.iter().all(|task| task.status == "completed")
        || !latest_required_gates_passed(mission, &squad)
        || !latest_task_handoff_is_authoritatively_accepted(mission, &final_task.id)
    {
        return Ok(false);
    }

    let workflow = store.load_workflow(&mission.workflow_id)?;
    let validation = validate_workflow(&workflow);
    if !validation.promotable {
        bail!(
            "mission {} cannot be promoted while workflow {} has {} failed validation rule(s)",
            mission.id,
            workflow.id,
            validation.failed_rules.len()
        );
    }

    let orchestrator = mission
        .agents
        .iter_mut()
        .find(|agent| agent.instance_id == mission.orchestrator_instance_id)
        .context("mission orchestrator disappeared before promotion")?;
    if orchestrator.status != "completed" {
        if orchestrator.status != "running" {
            bail!(
                "mission orchestrator cannot complete from status {}",
                orchestrator.status
            );
        }
        orchestrator.status = "completed".to_string();
        orchestrator.updated_at = Utc::now();
    }
    transition_mission(
        store,
        mission,
        MissionStatus::Completed,
        "mission.completed",
        &mission.orchestrator_instance_id.clone(),
        "all operational mission tasks and latest quality gates passed after accepted handoff validation",
    )?;
    Ok(true)
}

fn process_claimed_submission(
    store: &ForgeStore,
    mission: &mut MissionRecord,
    claim: &ClaimedMissionInbox,
) -> Result<String> {
    let processing = load_handoff_processing(store, &claim.handoff_id)?;
    if handoff_phase_rank(&processing.phase)?
        >= handoff_phase_rank(HANDOFF_PHASE_OUTCOME_PERSISTED)?
    {
        return processing
            .outcome
            .context("persisted handoff outcome phase has no outcome");
    }
    let handoff = mission
        .handoffs
        .iter()
        .find(|handoff| handoff.id == claim.handoff_id)
        .cloned()
        .with_context(|| format!("claimed handoff is missing: {}", claim.handoff_id))?;
    if handoff.to_agent != mission.orchestrator_instance_id || handoff.status != "queued" {
        bail!("claimed handoff is not a queued delivery to the mission orchestrator");
    }
    let task_index = mission
        .tasks
        .iter()
        .position(|task| task.id == handoff.task_id)
        .with_context(|| {
            format!(
                "claimed handoff references unknown task: {}",
                handoff.task_id
            )
        })?;
    let outcome_squad = load_squad(store, &mission.squad_id, Some(&mission.squad_version))?;
    if let Some(outcome) =
        infer_persisted_handoff_outcome(mission, &handoff, task_index, &outcome_squad)
    {
        mark_task_unassigned(mission, &handoff.task_id)?;
        terminate_agent(store, mission, &handoff.from_agent)?;
        advance_handoff_processing(
            store,
            &mission.id,
            &handoff.id,
            HANDOFF_PHASE_OUTCOME_PERSISTED,
            Some(&outcome),
            Some(&claim.lease_owner),
        )?;
        return Ok(outcome);
    }
    if mission.tasks[task_index].assigned_agent_id.as_deref() != Some(&handoff.from_agent) {
        bail!("claimed handoff producer no longer owns the task assignment");
    }
    complete_task(
        store,
        mission,
        &handoff.task_id,
        &mission.orchestrator_instance_id.clone(),
        handoff.delivery.artifacts.clone(),
    )?;

    let last_task_index = mission.tasks.len() - 1;
    let mut passed = submission_passes(&handoff);
    let mut repair_task_index = task_index;
    if task_index == 0 {
        let gate = mission_gate_definition(store, mission, 0)?;
        let gate_passed = evidence_passes(&handoff, &gate);
        record_gate_result(
            store,
            mission,
            &load_squad(store, &mission.squad_id, Some(&mission.squad_version))?,
            &gate.id,
            if gate_passed { "passed" } else { "failed" },
            handoff.validations.clone(),
        )?;
        passed &= gate_passed;
    } else if task_index == last_task_index {
        let squad = load_squad(store, &mission.squad_id, Some(&mission.squad_version))?;
        for gate_index in 1..squad.gates.len() {
            let gate = &squad.gates[gate_index];
            let gate_validations = if gate_index == 1 {
                let delivery_task_id = mission
                    .tasks
                    .get(task_index.saturating_sub(1))
                    .map(|task| task.id.as_str());
                let mut validations = delivery_task_id
                    .and_then(|task_id| {
                        mission.handoffs.iter().rev().find(|candidate| {
                            candidate.task_id == task_id && candidate.status == "accepted"
                        })
                    })
                    .map(|delivery| delivery.validations.clone())
                    .unwrap_or_default();
                validations.extend(handoff.validations.iter().cloned());
                let mut seen = BTreeSet::new();
                validations.retain(|validation| seen.insert(validation.clone()));
                validations
            } else {
                handoff.validations.clone()
            };
            let gate_passed = evidence_values_pass(&handoff, gate, &gate_validations);
            record_gate_result(
                store,
                mission,
                &squad,
                &gate.id,
                if gate_passed { "passed" } else { "failed" },
                gate_validations,
            )?;
            if !gate_passed && gate_index == 1 {
                repair_task_index = task_index.saturating_sub(1);
            }
            passed &= gate_passed;
        }
    }
    mission_failpoint("after_gate_persisted")?;

    let action = if !passed {
        create_operational_repair(
            store,
            mission,
            task_index,
            repair_task_index,
            &handoff.from_agent,
            "submitted evidence failed deterministic mission validation",
        )?;
        "repair_created".to_string()
    } else {
        if mission.status == MissionStatus::Repairing {
            let next_status = if task_index == last_task_index {
                MissionStatus::Reviewing
            } else {
                MissionStatus::Running
            };
            transition_mission(
                store,
                mission,
                next_status,
                "mission.repair.completed",
                &mission.orchestrator_instance_id.clone(),
                "repaired delivery passed the same deterministic validation contract",
            )?;
        }
        if task_index == last_task_index {
            let squad = load_squad(store, &mission.squad_id, Some(&mission.squad_version))?;
            if !latest_required_gates_passed(mission, &squad) {
                bail!("mission cannot be promoted before every latest gate result passes");
            }
            "mission_ready".to_string()
        } else {
            "handoff_consumed".to_string()
        }
    };
    mark_task_unassigned(mission, &handoff.task_id)?;
    terminate_agent(store, mission, &handoff.from_agent)?;
    advance_handoff_processing(
        store,
        &mission.id,
        &handoff.id,
        HANDOFF_PHASE_OUTCOME_PERSISTED,
        Some(&action),
        Some(&claim.lease_owner),
    )?;
    mission_failpoint("after_outcome_persisted")?;
    Ok(action)
}

fn mission_gate_definition(
    store: &ForgeStore,
    mission: &MissionRecord,
    index: usize,
) -> Result<QualityGateDefinition> {
    load_squad(store, &mission.squad_id, Some(&mission.squad_version))?
        .gates
        .get(index)
        .cloned()
        .with_context(|| format!("mission squad gate {index} is missing"))
}

fn pending_inbox_exists(store: &ForgeStore, mission_id: &str) -> Result<bool> {
    ensure_mission_runtime_schema(store)?;
    let connection = open_configured_connection(store.path())?;
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM mission_runtime_inbox WHERE mission_id = ?1 AND status IN ('pending', 'leased')",
        [mission_id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn dead_letter_exists(store: &ForgeStore, mission_id: &str) -> Result<bool> {
    ensure_mission_runtime_schema(store)?;
    let connection = open_configured_connection(store.path())?;
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM mission_runtime_inbox WHERE mission_id = ?1 AND status = 'dead_letter'",
        [mission_id],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn with_mission_drive_lease<T>(
    store: &ForgeStore,
    mission_id: &str,
    operation: impl FnOnce(&str) -> Result<T>,
) -> Result<T> {
    ensure_mission_runtime_schema(store)?;
    let owner = format!("drive-{}", Uuid::new_v4().simple());
    let now = Utc::now();
    let mut connection = open_configured_connection(store.path())?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute(
        "DELETE FROM mission_drive_leases WHERE mission_id = ?1 AND lease_expires_at <= ?2",
        params![mission_id, now.to_rfc3339()],
    )?;
    let acquired = transaction.execute(
        r#"
        INSERT INTO mission_drive_leases (mission_id, owner, lease_expires_at, acquired_at)
        VALUES (?1, ?2, ?3, ?4)
        ON CONFLICT(mission_id) DO NOTHING
        "#,
        params![
            mission_id,
            owner,
            (now + Duration::seconds(MISSION_DRIVE_LEASE_SECONDS)).to_rfc3339(),
            now.to_rfc3339(),
        ],
    )?;
    transaction.commit()?;
    if acquired != 1 {
        bail!("mission drive is already leased by another consumer");
    }
    let result = operation(&owner);
    let release_result = open_configured_connection(store.path())?.execute(
        "DELETE FROM mission_drive_leases WHERE mission_id = ?1 AND owner = ?2",
        params![mission_id, owner],
    );
    match (result, release_result) {
        (Ok(value), Ok(1)) => Ok(value),
        (Ok(_), Ok(_)) => bail!("mission drive lease disappeared before release"),
        (Ok(_), Err(error)) => Err(error.into()),
        (Err(error), _) => Err(error),
    }
}

fn renew_mission_drive_lease(store: &ForgeStore, mission_id: &str, owner: &str) -> Result<()> {
    let connection = open_configured_connection(store.path())?;
    let now = Utc::now();
    let changed = connection.execute(
        r#"
        UPDATE mission_drive_leases
        SET lease_expires_at=?1
        WHERE mission_id=?2 AND owner=?3 AND lease_expires_at>?4
        "#,
        params![
            (now + Duration::seconds(MISSION_DRIVE_LEASE_SECONDS)).to_rfc3339(),
            mission_id,
            owner,
            now.to_rfc3339(),
        ],
    )?;
    if changed != 1 {
        bail!("mission drive lease was lost or expired before phase mutation");
    }
    Ok(())
}

fn assignment_for_running_task(
    mission: &MissionRecord,
    task: &MissionTask,
) -> Option<MissionAssignment> {
    let agent_id = task.assigned_agent_id.as_deref()?;
    let agent = mission
        .agents
        .iter()
        .find(|agent| agent.instance_id == agent_id && agent.status == "running")?
        .clone();
    let harness = mission
        .harnesses
        .iter()
        .rev()
        .find(|harness| harness.task_id == task.id && harness.role == agent.role)?
        .clone();
    Some(MissionAssignment {
        task: task.clone(),
        agent,
        harness,
    })
}

fn dispatch_mission_task(
    store: &ForgeStore,
    mission: &mut MissionRecord,
) -> Result<MissionDriveReport> {
    if pending_inbox_exists(store, &mission.id)? {
        return Ok(MissionDriveReport {
            schema_version: "forge.mission.drive.v1".to_string(),
            status: "ready".to_string(),
            action: "resume_pending_inbox".to_string(),
            mission_id: mission.id.clone(),
            revision: mission.revision,
            assignment: None,
            handoff_id: None,
            mission: mission.clone(),
        });
    }
    if let Some(assignment) = mission
        .tasks
        .iter()
        .find_map(|task| assignment_for_running_task(mission, task))
    {
        return Ok(MissionDriveReport {
            schema_version: "forge.mission.drive.v1".to_string(),
            status: "waiting".to_string(),
            action: "awaiting_submission".to_string(),
            mission_id: mission.id.clone(),
            revision: mission.revision,
            assignment: Some(assignment),
            handoff_id: None,
            mission: mission.clone(),
        });
    }
    let task_index = mission.tasks.iter().position(|task| {
        (task.status == "pending" || task.status == "repairing")
            && task.dependencies.iter().all(|dependency| {
                mission
                    .tasks
                    .iter()
                    .any(|candidate| candidate.id == *dependency && candidate.status == "completed")
            })
    });
    let Some(task_index) = task_index else {
        return Ok(MissionDriveReport {
            schema_version: "forge.mission.drive.v1".to_string(),
            status: format!("{:?}", mission.status).to_lowercase(),
            action: "no_dispatchable_task".to_string(),
            mission_id: mission.id.clone(),
            revision: mission.revision,
            assignment: None,
            handoff_id: None,
            mission: mission.clone(),
        });
    };
    let squad = load_squad(store, &mission.squad_id, Some(&mission.squad_version))?;
    let role = mission.tasks[task_index].owner_role.clone();
    let member = squad
        .roster
        .iter()
        .find(|member| member.role == role)
        .with_context(|| format!("mission squad has no roster member for role {role}"))?;
    if task_index + 1 == mission.tasks.len()
        && matches!(
            mission.status,
            MissionStatus::Running | MissionStatus::Repairing
        )
    {
        transition_mission(
            store,
            mission,
            MissionStatus::Reviewing,
            "mission.review.started",
            &mission.orchestrator_instance_id.clone(),
            "independent assurance task became dispatchable",
        )?;
    }
    let orchestrator_id = mission.orchestrator_instance_id.clone();
    let agent_id = spawn_agent(store, mission, member, &orchestrator_id)?;
    let task_id = mission.tasks[task_index].id.clone();
    let harness = harness_for(&task_id, member);
    record_harness_resolution(store, mission, &agent_id, harness.clone())?;
    mission.tasks[task_index].assigned_agent_id = Some(agent_id.clone());
    mission.tasks[task_index].attempt = mission.tasks[task_index]
        .attempt
        .checked_add(1)
        .context("mission task attempt overflow")?;
    if mission.tasks[task_index].status == "pending" {
        start_task(store, mission, &task_id, &agent_id)?;
    } else {
        append_event(
            store,
            mission,
            "mission.task.repair_assigned",
            "repairing",
            &agent_id,
            Some(&task_id),
            "repair work was assigned under the same task contract",
        )?;
    }
    let agent = mission
        .agents
        .iter()
        .find(|agent| agent.instance_id == agent_id)
        .cloned()
        .context("newly spawned mission agent disappeared")?;
    let assignment = MissionAssignment {
        task: mission.tasks[task_index].clone(),
        agent,
        harness,
    };
    Ok(MissionDriveReport {
        schema_version: "forge.mission.drive.v1".to_string(),
        status: "dispatched".to_string(),
        action: "assignment_created".to_string(),
        mission_id: mission.id.clone(),
        revision: mission.revision,
        assignment: Some(assignment),
        handoff_id: None,
        mission: mission.clone(),
    })
}

fn resume_mission_inner(
    store: &ForgeStore,
    mission_id: &str,
    drive_lease_owner: &str,
) -> Result<MissionDriveReport> {
    renew_mission_drive_lease(store, mission_id, drive_lease_owner)?;
    let Some(mut claim) = claim_next_handoff(store, mission_id)? else {
        let mut mission = load_mission(store, mission_id)?;
        save_mission(store, &mission)?;
        let action = if dead_letter_exists(store, mission_id)? {
            if matches!(
                mission.status,
                MissionStatus::Running | MissionStatus::Reviewing | MissionStatus::Repairing
            ) {
                transition_mission(
                    store,
                    &mut mission,
                    MissionStatus::Blocked,
                    "mission.inbox.dead_lettered",
                    "mission-runtime",
                    "inbox retry budget was exhausted; operator repair is required",
                )?;
            }
            "dead_letter_blocked"
        } else if promote_ready_mission_if_possible(store, &mut mission)? {
            "mission_completed"
        } else {
            "no_pending_inbox"
        };
        return Ok(MissionDriveReport {
            schema_version: "forge.mission.drive.v1".to_string(),
            status: match action {
                "dead_letter_blocked" => "blocked".to_string(),
                "mission_completed" => "completed".to_string(),
                _ => "idle".to_string(),
            },
            action: action.to_string(),
            mission_id: mission.id.clone(),
            revision: mission.revision,
            assignment: None,
            handoff_id: None,
            mission,
        });
    };
    renew_mission_drive_lease(store, mission_id, drive_lease_owner)?;
    renew_claimed_handoff_lease(store, &mut claim)?;
    // This reload is intentional: the consumer never accepts from the producer's
    // in-memory mission and can run after a process/SQLite reopen.
    let mut mission = load_mission(store, mission_id)?;
    let processing = load_handoff_processing(store, &claim.handoff_id)?;
    let wakeup_sequence =
        if handoff_phase_rank(&processing.phase)? >= handoff_phase_rank(HANDOFF_PHASE_WOKEN)? {
            correlated_event(&mission, &claim.handoff_id, "agent.wakeup.triggered")
                .map(|event| event.sequence)
                .context("woken handoff is missing its durable wakeup event")?
        } else {
            wake_claimed_handoff(store, &mut mission, &claim)?
        };
    renew_mission_drive_lease(store, mission_id, drive_lease_owner)?;
    renew_claimed_handoff_lease(store, &mut claim)?;
    let persisted_action = process_claimed_submission(store, &mut mission, &claim)?;
    renew_mission_drive_lease(store, mission_id, drive_lease_owner)?;
    renew_claimed_handoff_lease(store, &mut claim)?;
    accept_claimed_handoff(store, &mut mission, &claim, wakeup_sequence)?;
    let promoted = promote_ready_mission_if_possible(store, &mut mission)?;
    let action = if persisted_action == "mission_ready" {
        if !promoted {
            bail!("accepted mission-ready handoff did not produce a promotable mission");
        }
        "mission_completed".to_string()
    } else if promoted {
        "mission_completed".to_string()
    } else {
        persisted_action
    };
    Ok(MissionDriveReport {
        schema_version: "forge.mission.drive.v1".to_string(),
        status: format!("{:?}", mission.status).to_lowercase(),
        action,
        mission_id: mission.id.clone(),
        revision: mission.revision,
        assignment: None,
        handoff_id: Some(claim.handoff_id),
        mission,
    })
}

pub fn resume_mission(store: &ForgeStore, mission_id: &str) -> Result<MissionDriveReport> {
    with_mission_drive_lease(store, mission_id, |lease_owner| {
        resume_mission_inner(store, mission_id, lease_owner)
    })
}

pub fn drive_mission(store: &ForgeStore, mission_id: &str) -> Result<MissionDriveReport> {
    with_mission_drive_lease(store, mission_id, |lease_owner| {
        renew_mission_drive_lease(store, mission_id, lease_owner)?;
        if pending_inbox_exists(store, mission_id)? || dead_letter_exists(store, mission_id)? {
            return resume_mission_inner(store, mission_id, lease_owner);
        }
        let mut mission = load_mission(store, mission_id)?;
        save_mission(store, &mission)?;
        if promote_ready_mission_if_possible(store, &mut mission)? {
            return Ok(MissionDriveReport {
                schema_version: "forge.mission.drive.v1".to_string(),
                status: "completed".to_string(),
                action: "mission_completed".to_string(),
                mission_id: mission.id.clone(),
                revision: mission.revision,
                assignment: None,
                handoff_id: None,
                mission,
            });
        }
        renew_mission_drive_lease(store, mission_id, lease_owner)?;
        dispatch_mission_task(store, &mut mission)
    })
}

pub fn simulate_mission(
    store: &ForgeStore,
    goal: &str,
    squad_id: &str,
    squad_version: Option<&str>,
    inject_rework: bool,
) -> Result<MissionSimulationReport> {
    simulate_mission_with_worktree(store, goal, squad_id, squad_version, inject_rework, None)
}

pub fn simulate_mission_with_worktree(
    store: &ForgeStore,
    goal: &str,
    squad_id: &str,
    squad_version: Option<&str>,
    inject_rework: bool,
    worktree: Option<&Path>,
) -> Result<MissionSimulationReport> {
    if goal.trim().is_empty() {
        bail!("mission goal cannot be empty");
    }
    install_builtin_squads(store)?;
    let squad = load_squad(store, squad_id, squad_version)?;
    let validation = validate_squad_definition(&squad)?;
    if !validation.valid {
        bail!("squad is not valid for mission execution");
    }

    let mut workflow = create_workflow(parse_intent(goal));
    workflow.tasks = mission_atomic_tasks_for_squad(&squad, MissionTaskPresentation::Simulation)?;
    push_mission_workflow_revision(
        &mut workflow,
        "mission_graph_materialized",
        format!(
            "materialized the canonical three-task simulation graph for squad {}@{}",
            squad.id, squad.version
        ),
    );
    let mission_tasks = workflow
        .tasks
        .iter()
        .map(mission_task_from_atomic)
        .collect();
    store.save_workflow(&workflow)?;
    let bound_worktree = if let Some(worktree) = worktree {
        let binding = register_worktree(
            store,
            WorktreeRegisterOptions {
                path: worktree.to_path_buf(),
                id: None,
                workflow_id: Some(workflow.id.clone()),
                task_id: None,
                origin: "mission.simulate".to_string(),
                created_by_forge: false,
            },
        )?;
        if binding.binding.is_none() {
            bail!("mission simulation worktree did not produce a binding receipt");
        }
        Some(binding.worktree.worktree_root)
    } else {
        None
    };
    let mission_id = format!("mission_{}", Uuid::new_v4().simple());
    let orchestrator_instance_id = format!("agent_{}", Uuid::new_v4().simple());
    let now = Utc::now();
    let mut mission = MissionRecord {
        schema_version: MISSION_SCHEMA_VERSION.to_string(),
        id: mission_id,
        workflow_id: workflow.id,
        tenant_id: workflow.intent.operating_context.organization.id.clone(),
        workspace_id: workflow.intent.operating_context.product.id.clone(),
        objective: goal.to_string(),
        mode: MissionMode::Simulation,
        status: MissionStatus::Planning,
        squad_id: squad.id.clone(),
        squad_version: squad.version.clone(),
        squad_composition_sha256: validation.composition_sha256.clone(),
        orchestrator_instance_id: orchestrator_instance_id.clone(),
        worktree: bound_worktree,
        budget_usd: squad.cost_policy.mission_limit_usd,
        created_at: now,
        updated_at: now,
        tasks: mission_tasks,
        agents: vec![MissionAgentInstance {
            instance_id: orchestrator_instance_id.clone(),
            definition_id: squad.orchestrator.id.clone(),
            role: "orchestrator".to_string(),
            status: "running".to_string(),
            spawned_on_demand: false,
            parent_instance_id: None,
            session_preserved: true,
            depth: 0,
            cost_usd: 0.0,
            runtime_milliseconds: 0,
            files_changed: 0,
            spawned_at: now,
            updated_at: now,
        }],
        handoffs: Vec::new(),
        inbox: Vec::new(),
        gates: Vec::new(),
        events: Vec::new(),
        harnesses: Vec::new(),
        cost: MissionCostLedger::default(),
        rework_cycles: 0,
        revision: 0,
    };
    save_mission(store, &mission)?;
    transition_mission(
        store,
        &mut mission,
        MissionStatus::Running,
        "mission.started",
        &orchestrator_instance_id,
        "bounded deterministic mission simulation started",
    )?;

    let member = |role: &str| -> Result<&RosterMember> {
        squad
            .roster
            .iter()
            .find(|member| member.role == role)
            .with_context(|| format!("squad role not found: {role}"))
    };

    let scout = member("scout")?;
    let scout_instance = spawn_agent(store, &mut mission, scout, &orchestrator_instance_id)?;
    record_harness_resolution(
        store,
        &mut mission,
        &scout_instance,
        harness_for("mission-task-001", scout),
    )?;
    start_task(store, &mut mission, "mission-task-001", &scout_instance)?;
    charge_agent(
        store,
        &mut mission,
        &scout_instance,
        "mission-task-001",
        0.04,
        800,
        150,
        0,
    )?;
    complete_task(
        store,
        &mut mission,
        "mission-task-001",
        &scout_instance,
        vec!["requirements-summary.json".to_string()],
    )?;
    let scout_handoff = add_handoff(
        store,
        &mut mission,
        &scout_instance,
        &orchestrator_instance_id,
        "mission-task-001",
        "scout delivered explicit requirements and acceptance criteria",
        StructuredAgentDelivery {
            task_id: "mission-task-001".to_string(),
            status: "completed".to_string(),
            summary: "objective and workspace constraints mapped".to_string(),
            artifacts: vec!["requirements-summary.json".to_string()],
            tests_passed: 1,
            tests_failed: 0,
            risks: Vec::new(),
            followups: vec!["dispatch builder".to_string()],
        },
        vec![
            "requirements_summary".to_string(),
            "acceptance_criteria".to_string(),
        ],
        "dispatch builder",
    )?;
    consume_queued_handoff_for_simulation(store, &mut mission, &scout_handoff)?;
    record_gate_result(
        store,
        &mut mission,
        &squad,
        "requirements_ready",
        "passed",
        vec![
            "requirements_summary".to_string(),
            "acceptance_criteria".to_string(),
        ],
    )?;
    terminate_agent(store, &mut mission, &scout_instance)?;

    let builder = member("builder")?;
    let builder_instance = spawn_agent(store, &mut mission, builder, &orchestrator_instance_id)?;
    record_harness_resolution(
        store,
        &mut mission,
        &builder_instance,
        harness_for("mission-task-002", builder),
    )?;
    start_task(store, &mut mission, "mission-task-002", &builder_instance)?;
    charge_agent(
        store,
        &mut mission,
        &builder_instance,
        "mission-task-002",
        0.11,
        2200,
        300,
        1,
    )?;
    complete_task(
        store,
        &mut mission,
        "mission-task-002",
        &builder_instance,
        vec!["implementation.patch".to_string()],
    )?;
    let builder_handoff = add_handoff(
        store,
        &mut mission,
        &builder_instance,
        &orchestrator_instance_id,
        "mission-task-002",
        "builder delivered bounded implementation and test evidence",
        StructuredAgentDelivery {
            task_id: "mission-task-002".to_string(),
            status: "completed".to_string(),
            summary: "bounded implementation completed".to_string(),
            artifacts: vec!["implementation.patch".to_string()],
            tests_passed: 8,
            tests_failed: 0,
            risks: Vec::new(),
            followups: vec!["independent review".to_string()],
        },
        vec!["tests_passed".to_string()],
        "dispatch independent reviewer",
    )?;
    consume_queued_handoff_for_simulation(store, &mut mission, &builder_handoff)?;
    terminate_agent(store, &mut mission, &builder_instance)?;

    transition_mission(
        store,
        &mut mission,
        MissionStatus::Reviewing,
        "mission.review.started",
        &orchestrator_instance_id,
        "implementation delivery checkpointed before independent review",
    )?;
    let reviewer = member("reviewer")?;
    let reviewer_instance = spawn_agent(store, &mut mission, reviewer, &orchestrator_instance_id)?;
    record_harness_resolution(
        store,
        &mut mission,
        &reviewer_instance,
        harness_for("mission-task-003", reviewer),
    )?;
    start_task(store, &mut mission, "mission-task-003", &reviewer_instance)?;
    charge_agent(
        store,
        &mut mission,
        &reviewer_instance,
        "mission-task-003",
        0.07,
        1300,
        220,
        0,
    )?;

    if inject_rework {
        record_gate_result(
            store,
            &mut mission,
            &squad,
            "implementation_validated",
            "failed",
            vec!["tests_passed".to_string()],
        )?;
        mission.rework_cycles = mission
            .rework_cycles
            .checked_add(1)
            .context("mission repair cycle overflow")?;
        mission.cost.retries = mission
            .cost
            .retries
            .checked_add(1)
            .context("mission retry count overflow")?;
        append_event(
            store,
            &mut mission,
            "agent.revision.requested",
            "repair_required",
            &reviewer_instance,
            Some("mission-task-002"),
            "reviewer requested one bounded repair before promotion",
        )?;
        transition_mission(
            store,
            &mut mission,
            MissionStatus::Repairing,
            "mission.repair.started",
            &orchestrator_instance_id,
            "failed gate moved the mission back to bounded work",
        )?;
        reopen_task_for_repair(store, &mut mission, "mission-task-002", &reviewer_instance)?;
        let repair_instance = spawn_agent(store, &mut mission, builder, &orchestrator_instance_id)?;
        record_harness_resolution(
            store,
            &mut mission,
            &repair_instance,
            harness_for("mission-task-002", builder),
        )?;
        charge_agent(
            store,
            &mut mission,
            &repair_instance,
            "mission-task-002",
            0.03,
            500,
            100,
            1,
        )?;
        complete_task(
            store,
            &mut mission,
            "mission-task-002",
            &repair_instance,
            vec![
                "implementation.patch".to_string(),
                "repair.patch".to_string(),
            ],
        )?;
        let repair_handoff = add_handoff(
            store,
            &mut mission,
            &repair_instance,
            &reviewer_instance,
            "mission-task-002",
            "builder repaired the reviewer finding",
            StructuredAgentDelivery {
                task_id: "mission-task-002".to_string(),
                status: "repaired".to_string(),
                summary: "review finding repaired and revalidated".to_string(),
                artifacts: vec!["repair.patch".to_string()],
                tests_passed: 9,
                tests_failed: 0,
                risks: Vec::new(),
                followups: Vec::new(),
            },
            vec!["tests_passed".to_string(), "repair_applied".to_string()],
            "revalidate implementation gate",
        )?;
        consume_queued_handoff_for_simulation(store, &mut mission, &repair_handoff)?;
        terminate_agent(store, &mut mission, &repair_instance)?;
        transition_mission(
            store,
            &mut mission,
            MissionStatus::Reviewing,
            "mission.revalidation.started",
            &orchestrator_instance_id,
            "repair evidence returned to the same independent gate",
        )?;
    }

    record_gate_result(
        store,
        &mut mission,
        &squad,
        "implementation_validated",
        "passed",
        vec!["tests_passed".to_string(), "review_passed".to_string()],
    )?;
    complete_task(
        store,
        &mut mission,
        "mission-task-003",
        &reviewer_instance,
        vec!["mission-consolidation.json".to_string()],
    )?;
    let reviewer_handoff = add_handoff(
        store,
        &mut mission,
        &reviewer_instance,
        &orchestrator_instance_id,
        "mission-task-003",
        "reviewer approved the repaired delivery with no unresolved risks",
        StructuredAgentDelivery {
            task_id: "mission-task-003".to_string(),
            status: "completed".to_string(),
            summary: "independent review passed".to_string(),
            artifacts: vec!["mission-consolidation.json".to_string()],
            tests_passed: 9,
            tests_failed: 0,
            risks: Vec::new(),
            followups: Vec::new(),
        },
        vec![
            "review_passed".to_string(),
            "no_unresolved_risks".to_string(),
        ],
        "promote mission outcome",
    )?;
    consume_queued_handoff_for_simulation(store, &mut mission, &reviewer_handoff)?;
    record_gate_result(
        store,
        &mut mission,
        &squad,
        "mission_outcome_ready",
        "passed",
        vec![
            "structured_delivery".to_string(),
            "no_unresolved_risks".to_string(),
        ],
    )?;
    terminate_agent(store, &mut mission, &reviewer_instance)?;

    if !latest_required_gates_passed(&mission, &squad) {
        bail!("mission cannot be promoted before every required gate passes");
    }
    let orchestrator = mission
        .agents
        .iter_mut()
        .find(|agent| agent.instance_id == orchestrator_instance_id)
        .context("mission orchestrator instance disappeared")?;
    orchestrator.status = "completed".to_string();
    orchestrator.updated_at = Utc::now();
    transition_mission(
        store,
        &mut mission,
        MissionStatus::Completed,
        "mission.completed",
        &orchestrator_instance_id,
        "all required gates passed after bounded validation and repair",
    )?;

    let audit = audit_mission_simulation(store, &mission.id, &squad, &validation, inject_rework)?;

    Ok(MissionSimulationReport {
        schema_version: MISSION_SIMULATION_SCHEMA_VERSION.to_string(),
        status: "passed".to_string(),
        simulation: "bounded_deterministic_no_model_execution".to_string(),
        bounded: true,
        model_execution_performed: false,
        external_mutation_performed: false,
        orchestrator_restricted: audit.orchestrator_restricted,
        on_demand_spawn_proven: audit.on_demand_spawn_proven,
        event_driven_handoff_proven: audit.event_driven_handoff_proven,
        validation_before_promotion_proven: audit.validation_before_promotion_proven,
        rework_cycle_proven: audit.rework_cycle_proven,
        exact_composition_recorded: audit.exact_composition_recorded,
        incremental_persistence_proven: audit.incremental_persistence_proven,
        hierarchy_limits_enforced: audit.hierarchy_limits_enforced,
        cost_limits_enforced: audit.cost_limits_enforced,
        inbox_wakeup_proven: audit.inbox_wakeup_proven,
        proof_scope: vec![
            "deterministic mission state transitions were checkpointed and reloaded from SQLite"
                .to_string(),
            "agent hierarchy, branch usage and mission cost stayed within the pinned squad limits"
                .to_string(),
            "typed handoffs traversed persisted inbox, correlated wakeup and acceptance states"
                .to_string(),
            "a failed gate caused one bounded repair and a later superseding revalidation"
                .to_string(),
            "promotion occurred only after the latest result for every required gate passed"
                .to_string(),
        ],
        not_proven: vec![
            "real model or provider execution".to_string(),
            "wall-clock concurrent scheduling or process isolation".to_string(),
            "external filesystem, network, deployment or MCP mutation".to_string(),
            "crash recovery, failover, load, soak or long-running durability".to_string(),
        ],
        mission: audit.mission,
        validation,
    })
}

pub fn load_mission(store: &ForgeStore, id: &str) -> Result<MissionRecord> {
    ensure_mission_runtime_schema(store)?;
    let connection = open_configured_connection(store.path())?;
    let data_json: Option<String> = connection
        .query_row(
            "SELECT data_json FROM forge_missions WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .optional()?;
    let data_json = data_json.with_context(|| format!("mission not found: {id}"))?;
    let mut mission: MissionRecord = serde_json::from_str(&data_json)?;
    let mut statement = connection.prepare(
        r#"
        SELECT handoff_id, status, attempts, max_attempts, lease_owner,
               lease_expires_at, last_error, consumed_at
        FROM mission_runtime_inbox WHERE mission_id = ?1
        "#,
    )?;
    let rows = statement.query_map([id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, Option<String>>(7)?,
        ))
    })?;
    for row in rows {
        let (
            handoff_id,
            status,
            attempts,
            max_attempts,
            lease_owner,
            lease_expires_at,
            last_error,
            consumed_at,
        ) = row?;
        let Some(item) = mission
            .inbox
            .iter_mut()
            .find(|item| item.handoff_id == handoff_id)
        else {
            continue;
        };
        item.status = status;
        item.attempts = usize::try_from(attempts).context("negative inbox attempt count")?;
        item.max_attempts = usize::try_from(max_attempts).context("negative inbox retry limit")?;
        item.lease_owner = lease_owner;
        item.lease_expires_at = lease_expires_at
            .map(|value| DateTime::parse_from_rfc3339(&value).map(|time| time.with_timezone(&Utc)))
            .transpose()
            .context("invalid persisted mission inbox lease timestamp")?;
        item.last_error = last_error;
        item.consumed_at = consumed_at
            .map(|value| DateTime::parse_from_rfc3339(&value).map(|time| time.with_timezone(&Utc)))
            .transpose()
            .context("invalid persisted mission inbox consumed timestamp")?;
    }
    Ok(mission)
}

pub fn list_missions(store: &ForgeStore) -> Result<MissionListReport> {
    let connection = open_configured_connection(store.path())?;
    let mut statement =
        connection.prepare("SELECT data_json FROM forge_missions ORDER BY created_at DESC")?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut missions = Vec::new();
    for row in rows {
        missions.push(serde_json::from_str(&row?)?);
    }
    Ok(MissionListReport {
        schema_version: "forge.mission.list.v1".to_string(),
        status: "ready".to_string(),
        missions,
    })
}

pub fn ensure_builtin_squad(store: &ForgeStore, id: &str) -> Result<SquadDefinition> {
    install_builtin_squads(store)?;
    load_squad(store, id, None).map_err(|_| anyhow!("unknown built-in squad: {id}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::tempdir;

    fn projection_fixture(squad: &SquadDefinition) -> (Workflow, MissionRecord) {
        let tasks =
            mission_atomic_tasks_for_squad(squad, MissionTaskPresentation::Operational).unwrap();
        let mut workflow = create_workflow(parse_intent("Prove mission workflow projection"));
        workflow.tasks = tasks.clone();
        let now = Utc::now();
        let mission = MissionRecord {
            schema_version: MISSION_SCHEMA_VERSION.to_string(),
            id: "mission_projection_test".to_string(),
            workflow_id: workflow.id.clone(),
            tenant_id: "test-tenant".to_string(),
            workspace_id: "test-workspace".to_string(),
            objective: "Prove mission workflow projection".to_string(),
            mode: MissionMode::Workflow,
            status: MissionStatus::Reviewing,
            squad_id: squad.id.clone(),
            squad_version: squad.version.clone(),
            squad_composition_sha256: squad_digest(squad).unwrap(),
            orchestrator_instance_id: "agent_orchestrator".to_string(),
            worktree: None,
            budget_usd: squad.cost_policy.mission_limit_usd,
            created_at: now,
            updated_at: now,
            tasks: tasks.iter().map(mission_task_from_atomic).collect(),
            agents: Vec::new(),
            handoffs: Vec::new(),
            inbox: Vec::new(),
            gates: Vec::new(),
            events: Vec::new(),
            harnesses: Vec::new(),
            cost: MissionCostLedger::default(),
            rework_cycles: 0,
            revision: 1,
        };
        (workflow, mission)
    }

    fn initialize_git_repository(path: &Path) {
        fs::create_dir_all(path).unwrap();
        let initialized = Command::new("git")
            .args(["init", "--initial-branch=main"])
            .arg(path)
            .output()
            .unwrap();
        assert!(
            initialized.status.success(),
            "{}",
            String::from_utf8_lossy(&initialized.stderr)
        );
        let committed = Command::new("git")
            .args([
                "-C",
                path.to_str().unwrap(),
                "-c",
                "user.name=Forge Mission Atomicity",
                "-c",
                "user.email=forge-mission-atomicity@example.invalid",
                "commit",
                "--allow-empty",
                "-m",
                "initial",
            ])
            .output()
            .unwrap();
        assert!(
            committed.status.success(),
            "{}",
            String::from_utf8_lossy(&committed.stderr)
        );
    }

    fn table_count(store: &ForgeStore, table: &str) -> i64 {
        let connection = open_configured_connection(store.path()).unwrap();
        connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap()
    }

    fn persisted_projection_mission(
        store: &ForgeStore,
        squad: &SquadDefinition,
        with_orchestrator: bool,
    ) -> MissionRecord {
        install_squad(store, squad).unwrap();
        let (workflow, mut mission) = projection_fixture(squad);
        if with_orchestrator {
            let now = Utc::now();
            mission.agents.push(MissionAgentInstance {
                instance_id: mission.orchestrator_instance_id.clone(),
                definition_id: squad.orchestrator.id.clone(),
                role: "orchestrator".to_string(),
                status: "running".to_string(),
                spawned_on_demand: false,
                parent_instance_id: None,
                session_preserved: true,
                depth: 0,
                cost_usd: 0.0,
                runtime_milliseconds: 0,
                files_changed: 0,
                spawned_at: now,
                updated_at: now,
            });
        }
        store.save_workflow(&workflow).unwrap();
        save_mission(store, &mission).unwrap();
        mission
    }

    fn completed_event(task_id: &str, sequence: usize, occurred_at: DateTime<Utc>) -> MissionEvent {
        MissionEvent {
            schema_version: MISSION_EVENT_SCHEMA_VERSION.to_string(),
            id: format!("event_{task_id}_{sequence}"),
            sequence,
            kind: "mission.task.completed".to_string(),
            status: "completed".to_string(),
            actor: format!("agent_{task_id}"),
            task_id: Some(task_id.to_string()),
            correlation_id: None,
            caused_by_sequence: None,
            summary: format!("{task_id} completed"),
            occurred_at,
        }
    }

    fn accept_completed_task(
        mission: &mut MissionRecord,
        task_index: usize,
        completed_at: DateTime<Utc>,
        accepted_at: DateTime<Utc>,
    ) {
        let task = &mut mission.tasks[task_index];
        task.status = "completed".to_string();
        task.progress_percent = 100;
        let task_id = task.id.clone();
        let agent_id = format!("agent_{task_id}");
        mission.events.push(completed_event(
            &task_id,
            mission.events.len() + 1,
            completed_at,
        ));
        mission.handoffs.push(AgentHandoff {
            schema_version: AGENT_HANDOFF_SCHEMA_VERSION.to_string(),
            id: format!("handoff_{task_id}"),
            idempotency_key: format!("submission_{task_id}"),
            mission_id: mission.id.clone(),
            from_agent: agent_id,
            to_agent: mission.orchestrator_instance_id.clone(),
            task_id: task_id.clone(),
            status: "accepted".to_string(),
            summary: format!("{task_id} accepted"),
            delivery: StructuredAgentDelivery {
                task_id,
                status: "completed".to_string(),
                summary: "validated delivery".to_string(),
                artifacts: Vec::new(),
                tests_passed: 1,
                tests_failed: 0,
                risks: Vec::new(),
                followups: Vec::new(),
            },
            validations: Vec::new(),
            unresolved_questions: Vec::new(),
            recommended_next_action: "continue".to_string(),
            created_at: completed_at,
            accepted_at: Some(accepted_at),
        });
    }

    fn pass_gate(
        mission: &mut MissionRecord,
        squad: &SquadDefinition,
        gate_index: usize,
        evaluated_at: DateTime<Utc>,
    ) {
        let definition = &squad.gates[gate_index];
        mission.gates.push(GateResult {
            gate_id: definition.id.clone(),
            attempt: 1,
            status: "passed".to_string(),
            validator: definition.validator.clone(),
            evidence: definition.required_evidence.clone(),
            failure_action: definition.failure_action.clone(),
            repair_cycle: 0,
            supersedes_attempt: None,
            evaluated_at,
        });
    }

    #[test]
    fn projection_reviewing_mission_is_promotion_ready_before_completed() {
        let squad = software_factory_squad();
        let (mut workflow, mut mission) = projection_fixture(&squad);
        let base = Utc::now();
        for task_index in 0..3 {
            let completed_at = base + Duration::seconds(task_index as i64);
            accept_completed_task(
                &mut mission,
                task_index,
                completed_at,
                completed_at + Duration::milliseconds(1),
            );
            pass_gate(&mut mission, &squad, task_index, completed_at);
        }

        assert!(project_mission_state_onto_workflow(&mut workflow, &mission, &squad).unwrap());
        assert!(validate_workflow(&workflow).promotable);
        assert_eq!(workflow.status, "promotion_ready");

        mission.status = MissionStatus::Completed;
        assert!(project_mission_state_onto_workflow(&mut workflow, &mission, &squad).unwrap());
        assert!(validate_workflow(&workflow).promotable);
        assert_eq!(workflow.status, "completed");
    }

    #[test]
    fn projection_reconciled_accepted_at_follows_latest_task_completion() {
        let squad = software_factory_squad();
        let (_, mut mission) = projection_fixture(&squad);
        let task_id = mission.tasks[2].id.clone();
        let initial_completion = Utc::now();
        let repair_completion = initial_completion + Duration::seconds(2);
        mission
            .events
            .push(completed_event(&task_id, 1, initial_completion));
        accept_completed_task(
            &mut mission,
            2,
            repair_completion,
            repair_completion + Duration::milliseconds(1),
        );
        assert!(!mission
            .events
            .iter()
            .any(|event| event.kind == "agent.handoff.accepted"));
        assert!(latest_task_handoff_is_authoritatively_accepted(
            &mission, &task_id
        ));

        mission.handoffs.last_mut().unwrap().accepted_at =
            Some(initial_completion + Duration::seconds(1));
        assert!(!latest_task_handoff_is_authoritatively_accepted(
            &mission, &task_id
        ));
    }

    #[test]
    fn projection_valid_single_gate_squad_can_make_second_task_ready() {
        let mut squad = software_factory_squad();
        squad.gates.truncate(1);
        let validation = validate_squad_definition(&squad).unwrap();
        assert!(validation.valid, "{:?}", validation.errors);
        let (mut workflow, mut mission) = projection_fixture(&squad);
        let base = Utc::now();
        accept_completed_task(&mut mission, 0, base, base + Duration::milliseconds(1));
        pass_gate(&mut mission, &squad, 0, base);
        accept_completed_task(
            &mut mission,
            1,
            base + Duration::seconds(1),
            base + Duration::seconds(1) + Duration::milliseconds(1),
        );

        project_mission_state_onto_workflow(&mut workflow, &mission, &squad).unwrap();
        assert_eq!(workflow.tasks[1].status, TaskStatus::Completed);
        assert!(
            workflow.tasks[1]
                .work_item
                .goal_validation
                .definitively_ready
        );
    }

    #[test]
    fn atomic_event_cas_rejects_divergence_without_ghost_event_or_projection() {
        let temp = tempdir().unwrap();
        let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();
        let squad = software_factory_squad();
        let base = persisted_projection_mission(&store, &squad, false);
        let mut accepted = base.clone();
        let mut stale = base;

        append_event(
            &store,
            &mut accepted,
            "mission.cas.accepted",
            "recorded",
            "test",
            None,
            "accepted branch",
        )
        .unwrap();
        stale.tasks[0].status = "completed".to_string();
        let error = append_event(
            &store,
            &mut stale,
            "mission.cas.stale",
            "recorded",
            "test",
            None,
            "stale branch",
        )
        .unwrap_err();
        assert!(error.to_string().contains("stale mission"));
        let persisted = load_mission(&store, &accepted.id).unwrap();
        assert_eq!(
            persisted.events.last().unwrap().kind,
            "mission.cas.accepted"
        );
        assert_eq!(persisted.events.len(), 1);
        assert!(stale.events.is_empty());

        let workflow = store.load_workflow(&persisted.workflow_id).unwrap();
        assert_eq!(workflow.tasks[0].status, TaskStatus::Pending);
        let connection = open_configured_connection(store.path()).unwrap();
        let stored_events: Vec<String> = connection
            .prepare("SELECT kind FROM events WHERE workflow_id = ?1 ORDER BY id")
            .unwrap()
            .query_map([&persisted.workflow_id], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(stored_events, vec!["mission.cas.accepted".to_string()]);
        assert_eq!(table_count(&store, "global_events"), 1);
    }

    #[test]
    fn atomic_start_rolls_back_invalid_worktree_and_injected_commit_failure() {
        let temp = tempdir().unwrap();
        let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();
        let missing = temp.path().join("missing-worktree");
        let _invalid = start_mission(
            &store,
            "Reject an invalid worktree without an orphan workflow",
            "software-factory",
            None,
            &missing,
        )
        .unwrap_err();
        assert_eq!(table_count(&store, "workflows"), 0);
        assert_eq!(table_count(&store, "worktree_states"), 0);
        assert_eq!(table_count(&store, "forge_missions"), 0);

        let repository = temp.path().join("repository");
        initialize_git_repository(&repository);
        set_mission_failpoint(Some("start_before_commit"));
        let injected = start_mission(
            &store,
            "Roll back every start record before commit",
            "software-factory",
            None,
            &repository,
        )
        .unwrap_err();
        set_mission_failpoint(None);
        assert!(injected
            .to_string()
            .contains("injected mission failpoint: start_before_commit"));
        for table in [
            "workflows",
            "worktree_states",
            "forge_missions",
            "mission_agent_instances",
            "mission_runtime_checkpoints",
            "events",
            "global_events",
            "tenant_index",
        ] {
            assert_eq!(table_count(&store, table), 0, "orphan row in {table}");
        }
    }

    #[test]
    fn atomic_agent_lifecycle_rolls_back_spawn_and_terminate_failures() {
        let temp = tempdir().unwrap();
        let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();
        let squad = software_factory_squad();
        let mut mission = persisted_projection_mission(&store, &squad, true);
        let orchestrator_id = mission.orchestrator_instance_id.clone();
        let scout = &squad.roster[0];

        set_mission_failpoint(Some("spawn_before_commit"));
        let spawn_error = spawn_agent(&store, &mut mission, scout, &orchestrator_id).unwrap_err();
        set_mission_failpoint(None);
        assert!(spawn_error
            .to_string()
            .contains("injected mission failpoint: spawn_before_commit"));
        assert_eq!(mission.agents.len(), 1);
        assert_eq!(mission.cost.agent_spawns, 0);
        assert_eq!(table_count(&store, "mission_agent_instances"), 1);
        assert_eq!(table_count(&store, "events"), 0);

        let worker_id = spawn_agent(&store, &mut mission, scout, &orchestrator_id).unwrap();
        assert_eq!(mission.agents.len(), 2);
        assert_eq!(table_count(&store, "mission_agent_instances"), 2);
        assert_eq!(table_count(&store, "events"), 1);

        set_mission_failpoint(Some("terminate_before_commit"));
        let terminate_error = terminate_agent(&store, &mut mission, &worker_id).unwrap_err();
        set_mission_failpoint(None);
        assert!(terminate_error
            .to_string()
            .contains("injected mission failpoint: terminate_before_commit"));
        assert_eq!(
            mission
                .agents
                .iter()
                .find(|agent| agent.instance_id == worker_id)
                .unwrap()
                .status,
            "running"
        );
        let persisted = load_mission(&store, &mission.id).unwrap();
        assert_eq!(
            persisted
                .agents
                .iter()
                .find(|agent| agent.instance_id == worker_id)
                .unwrap()
                .status,
            "running"
        );
        let connection = open_configured_connection(store.path()).unwrap();
        let stored_status: String = connection
            .query_row(
                "SELECT status FROM mission_agent_instances WHERE id = ?1",
                [&worker_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored_status, "running");
        assert_eq!(table_count(&store, "events"), 1);
    }

    #[test]
    fn built_in_squad_enforces_restricted_orchestrator() {
        let squad = software_factory_squad();
        let report = validate_squad_definition(&squad).unwrap();
        assert!(report.valid, "{:?}", report.errors);
        assert!(squad.orchestrator.permissions.denies("shell"));
        assert!(squad.orchestrator.permissions.denies("modify_files"));
        assert!(squad.orchestrator.permissions.allows("spawn_agent"));
    }

    #[test]
    fn installed_squad_versions_are_immutable() {
        let temp = tempdir().unwrap();
        let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();
        let squad = software_factory_squad();
        assert_eq!(install_squad(&store, &squad).unwrap().status, "installed");
        assert_eq!(
            install_squad(&store, &squad).unwrap().status,
            "already_installed"
        );
        let mut changed = squad.clone();
        changed.name = "Mutated bytes".to_string();
        assert!(install_squad(&store, &changed).is_err());
    }

    #[test]
    fn bounded_simulation_proves_handoff_gates_and_repair() {
        let temp = tempdir().unwrap();
        let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();
        let report = simulate_mission(
            &store,
            "Implement a production-safe Rust API",
            "software-factory",
            None,
            true,
        )
        .unwrap();
        assert_eq!(report.status, "passed");
        assert!(report.orchestrator_restricted);
        assert!(report.on_demand_spawn_proven);
        assert!(report.event_driven_handoff_proven);
        assert!(report.validation_before_promotion_proven);
        assert!(report.rework_cycle_proven);
        assert_eq!(report.mission.status, MissionStatus::Completed);
        assert_eq!(report.mission.rework_cycles, 1);
        assert!(report
            .mission
            .agents
            .iter()
            .filter(|agent| agent.role != "orchestrator")
            .all(|agent| agent.status == "terminated"));
    }
}
