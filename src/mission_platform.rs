use crate::mission::{
    builtin_squad_catalog, validate_squad_definition, MissionSimulationReport,
    StructuredAgentDelivery,
};
use crate::storage::ForgeStore;
use crate::worktree::list_registered_worktrees;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const MISSION_PLATFORM_CATALOG_SCHEMA_VERSION: &str = "forge.mission_platform.catalog.v1";
pub const MISSION_PLATFORM_SIMULATION_SCHEMA_VERSION: &str = "forge.mission_platform.simulation.v1";
pub const MISSION_PLATFORM_CAPABILITY_COUNT: usize = 40;
pub const MISSION_PLATFORM_RUNTIME_REAL: &str = "runtime_real";
pub const MISSION_PLATFORM_BOUNDED_SIMULATION: &str = "bounded_simulation";
pub const MISSION_PLATFORM_CONTRACT_ONLY: &str = "contract_only";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MissionPlatformCapability {
    pub number: u8,
    pub id: String,
    pub title: String,
    pub owner: String,
    pub proof_kind: String,
    pub production_ready: bool,
    pub production_gap: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionPlatformCatalog {
    pub schema_version: String,
    pub status: String,
    pub capability_count: usize,
    pub inventory_sha256: String,
    pub proof_kind_counts: BTreeMap<String, usize>,
    pub production_ready: bool,
    pub capabilities: Vec<MissionPlatformCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapabilityEffectReceipt {
    pub schema_version: String,
    pub id: String,
    pub capability_id: String,
    pub adapter: String,
    pub input_sha256: String,
    pub result_sha256: String,
    pub result: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapabilityProbeEvidence {
    pub execution_class: String,
    pub receipt: Option<CapabilityEffectReceipt>,
    pub input_sha256: String,
    pub result_sha256: String,
    pub result: Value,
    pub verification: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityProbe {
    pub number: u8,
    pub capability_id: String,
    pub passed: bool,
    pub proof_scope: String,
    pub evidence: CapabilityProbeEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapabilityEffectFixture {
    pub dependency: Value,
    pub receipt: CapabilityEffectReceipt,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct MissionPlatformProbeEnvironment {
    pub fixtures: BTreeMap<String, CapabilityEffectFixture>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionPlatformSimulationReport {
    pub schema_version: String,
    pub status: String,
    pub evidence_scope: String,
    pub bounded: bool,
    pub model_execution_performed: bool,
    pub external_mutation_performed: bool,
    pub production_ready: bool,
    pub capability_count: usize,
    pub inventory_sha256: String,
    pub proof_kind_counts: BTreeMap<String, usize>,
    pub passed_count: usize,
    pub failed_count: usize,
    pub mission_id: String,
    pub mission_simulation_schema_version: String,
    pub probes: Vec<CapabilityProbe>,
    pub not_proven: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CanonicalAgentRole {
    Orchestrator,
    Planner,
    Controller,
    Scout,
    Researcher,
    Executor,
    Builder,
    Reviewer,
    Tester,
    SecurityReviewer,
    Auditor,
    Observer,
    Operator,
    Deployer,
    IncidentResponder,
    DataEngineer,
    HumanLiaison,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum UnifiedCatalogKind {
    Agent,
    Skill,
    Workflow,
    Squad,
    Addon,
    Tool,
    McpServer,
    Cli,
    Model,
    Provider,
    Recipe,
    Policy,
    Guardrail,
    Template,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillLayer {
    pub layer: String,
    pub allowed: Vec<String>,
    pub denied: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedSkillSet {
    pub skills: Vec<String>,
    pub denied: Vec<String>,
    pub precedence: Vec<String>,
    pub resolution_trace: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EffortLevel {
    Minimal,
    Low,
    Medium,
    High,
    Maximum,
    Adaptive,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EffortBudget {
    pub level: EffortLevel,
    pub reasoning_effort: String,
    pub max_attempts: usize,
    pub context_fraction_percent: u8,
    pub reviewer_count: usize,
    pub timeout_seconds: u64,
    pub max_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdaptiveIntakeReport {
    pub required_fields: Vec<String>,
    pub already_resolved: Vec<String>,
    pub remaining_questions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleTargetKind {
    Workflow,
    Squad,
    Mission,
    Agent,
    Skill,
    Script,
    Command,
    SyntheticEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleTriggerKind {
    Cron,
    Interval,
    OneShot,
    Calendar,
    Webhook,
    Event,
    Condition,
    DependencyCompletion,
    ManualApproval,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CostDimension {
    Tenant,
    Workspace,
    Workflow,
    Mission,
    Squad,
    Agent,
    Task,
    Provider,
    Model,
    Tool,
    Addon,
    Branch,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSandboxContract {
    pub virtual_home: bool,
    pub isolated_config_directory: bool,
    pub isolated_environment: bool,
    pub credential_references_only: bool,
    pub tool_allowlist: bool,
    pub filesystem_mount_policy: bool,
    pub network_policy: bool,
    pub resource_limits: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum MarketplacePackageKind {
    Addon,
    Agent,
    Skill,
    Workflow,
    Squad,
    Recipe,
    Tool,
    McpServer,
    Provider,
    Policy,
    Guardrail,
    UiModule,
    Connector,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum MissionMemoryScope {
    Agent,
    Task,
    Mission,
    Squad,
    Workflow,
    Workspace,
    Project,
    Tenant,
    Organization,
    Global,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionTarget {
    Local,
    Ssh,
    Docker,
    Kubernetes,
    VirtualMachine,
    Wasm,
    Serverless,
    Edge,
    Browser,
    MobileDevice,
    RemoteForgeWorker,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum MediaOperation {
    ImageGenerate,
    ImageEdit,
    VideoGenerate,
    VideoCompose,
    AudioGenerate,
    AudioTranscribe,
    VoiceSynthesize,
    MediaInspect,
    MediaConvert,
    MediaPublish,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VoiceControlLevel {
    ActiveAgent,
    MissionOperations,
    ForgeSupervisor,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceCommandRoute {
    pub level: VoiceControlLevel,
    pub action: String,
    pub target: String,
    pub requires_approval: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LanguageGatewayPlan {
    pub source_locale: String,
    pub agent_locale: String,
    pub output_locale: String,
    pub translation_required: bool,
    pub protected_segments: Vec<String>,
    pub preservation_rules: Vec<String>,
    pub input_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceService {
    VirtualFilesystem,
    TerminalSession,
    ProcessLifecycle,
    BrowserSessionRegistry,
    FileWatcher,
    WorkspaceState,
    SessionRestore,
    RecentWorkspaceIndex,
    RemoteFilesystem,
    ArtifactPreview,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConsolidationReport {
    pub status: String,
    pub delivery_count: usize,
    pub contradiction_count: usize,
    pub contradictions: Vec<String>,
    pub artifact_union: Vec<String>,
    pub promotion_allowed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepairPlan {
    pub status: String,
    pub failed_gates: Vec<String>,
    pub repair_tasks: Vec<String>,
    pub revalidation_required: bool,
    pub promotion_blocked: bool,
}

fn capability_proof_kind(number: u8) -> &'static str {
    match number {
        1 | 2 | 3 | 5 | 8 | 10 | 12 | 13 | 18 | 19 | 21 | 24 | 26 | 28 | 29 | 31 | 36 | 37 | 38
        | 40 => MISSION_PLATFORM_RUNTIME_REAL,
        4 | 11 | 14 | 15 | 17 | 20 | 22 | 25 | 30 | 32 | 33 | 34 | 35 | 39 => {
            MISSION_PLATFORM_BOUNDED_SIMULATION
        }
        6 | 7 | 9 | 16 | 23 | 27 => MISSION_PLATFORM_CONTRACT_ONLY,
        _ => MISSION_PLATFORM_CONTRACT_ONLY,
    }
}

fn capability_production_gap(proof_kind: &str) -> &'static str {
    match proof_kind {
        MISSION_PLATFORM_RUNTIME_REAL => {
            "partial runtime path exists; production promotion still requires the operational execute-submit-resume receipt"
        }
        MISSION_PLATFORM_BOUNDED_SIMULATION => {
            "only bounded deterministic simulation is proven; no production effect is claimed"
        }
        _ => "only the static catalog, schema or adapter contract is proven",
    }
}

fn capability_inventory_sha256(capabilities: &[MissionPlatformCapability]) -> String {
    let bytes = serde_json::to_vec(capabilities).unwrap_or_default();
    format!("{:x}", Sha256::digest(bytes))
}

pub fn mission_platform_inventory_sha256() -> String {
    mission_platform_catalog().inventory_sha256
}

pub fn mission_platform_catalog() -> MissionPlatformCatalog {
    let rows = [
        (
            1,
            "first_class_squads",
            "Squads como entidade de primeira classe",
            "mission",
        ),
        (
            2,
            "restricted_orchestrator",
            "Maestro ou orquestrador restrito",
            "mission",
        ),
        (3, "agent_hierarchy", "Hierarquia de agentes", "mission"),
        (
            4,
            "canonical_agent_roles",
            "Papéis canônicos de agentes",
            "mission_platform",
        ),
        (
            5,
            "persistent_mission",
            "Missão como unidade isolada de execução",
            "mission",
        ),
        (6, "mission_modes", "Diferentes modos de missão", "mission"),
        (
            7,
            "orchestration_topologies",
            "Estilos de orquestração",
            "mission",
        ),
        (
            8,
            "complete_agent_definitions",
            "Agentes como definições operacionais completas",
            "mission",
        ),
        (
            9,
            "unified_multi_runtime_catalog",
            "Catálogo multi-runtime e multi-CLI",
            "mission_platform",
        ),
        (
            10,
            "controlled_originals",
            "Agentes built-in com atualização controlada",
            "mission",
        ),
        (
            11,
            "role_skill_resolution",
            "Skills com auto-deploy por papel",
            "mission_platform",
        ),
        (12, "mission_skill_gate", "Skill gate por missão", "mission"),
        (
            13,
            "task_execution_harness",
            "Harness por tarefa",
            "mission",
        ),
        (
            14,
            "deterministic_precedence",
            "Cadeia determinística de precedência",
            "mission_platform",
        ),
        (
            15,
            "invocation_effort",
            "Effort por invocação",
            "mission_platform",
        ),
        (16, "recipes", "Receitas", "mission"),
        (
            17,
            "adaptive_intake",
            "Intake adaptativo",
            "mission_platform",
        ),
        (
            18,
            "event_driven_handoff",
            "Handoff event-driven",
            "mission",
        ),
        (19, "on_demand_agents", "Agentes sob demanda", "mission"),
        (
            20,
            "agent_aware_scheduling",
            "Agendamento e cron associado a agentes",
            "schedule",
        ),
        (
            21,
            "mission_task_telemetry",
            "Tasks como telemetria pública da missão",
            "mission",
        ),
        (
            22,
            "squad_cost_aggregation",
            "Custo agregado por squad",
            "mission",
        ),
        (
            23,
            "per_agent_providers",
            "Providers independentes por agente",
            "executor",
        ),
        (
            24,
            "runtime_discovery",
            "Detecção automática de CLIs e runtimes",
            "executor",
        ),
        (
            25,
            "agent_config_isolation",
            "Isolamento de configuração por agente",
            "mission_platform",
        ),
        (
            26,
            "embedded_mcp_gateway",
            "MCP embutido e configuração automática",
            "mcp",
        ),
        (27, "unified_marketplace", "Marketplace unificado", "addon"),
        (
            28,
            "workspace_memory",
            "Memória compartilhada por workspace",
            "memory",
        ),
        (
            29,
            "session_restoration",
            "Persistência e restauração completa de sessão",
            "mission",
        ),
        (30, "remote_workspace", "Workspace remoto", "cluster"),
        (31, "mission_worktree", "Worktree por missão", "worktree"),
        (
            32,
            "native_media_tools",
            "Geração multimídia como ferramentas nativas",
            "multimodal",
        ),
        (
            33,
            "two_level_voice",
            "Interface de voz em dois níveis",
            "mission_platform",
        ),
        (
            34,
            "operational_translation",
            "Tradução operacional automática",
            "mission_platform",
        ),
        (
            35,
            "integrated_workspace",
            "Workspace integrado",
            "interactive",
        ),
        (
            36,
            "official_squad_catalog",
            "Catálogo de squads prontos",
            "mission_platform",
        ),
        (
            37,
            "phase_quality_gates",
            "Gates de qualidade entre fases",
            "mission",
        ),
        (38, "dynamic_roster", "Roster dinâmico", "mission"),
        (
            39,
            "result_consolidation",
            "Consolidação de resultados",
            "mission_platform",
        ),
        (
            40,
            "formal_review_repair",
            "Ciclo formal de revisão e reparação",
            "mission_platform",
        ),
    ];
    let capabilities = rows
        .into_iter()
        .map(|(number, id, title, owner)| {
            let proof_kind = capability_proof_kind(number);
            MissionPlatformCapability {
                number,
                id: id.to_string(),
                title: title.to_string(),
                owner: owner.to_string(),
                proof_kind: proof_kind.to_string(),
                production_ready: false,
                production_gap: capability_production_gap(proof_kind).to_string(),
            }
        })
        .collect::<Vec<_>>();
    let inventory_sha256 = capability_inventory_sha256(&capabilities);
    let mut proof_kind_counts = BTreeMap::new();
    for capability in &capabilities {
        *proof_kind_counts
            .entry(capability.proof_kind.clone())
            .or_insert(0) += 1;
    }
    debug_assert_eq!(capabilities.len(), MISSION_PLATFORM_CAPABILITY_COUNT);
    MissionPlatformCatalog {
        schema_version: MISSION_PLATFORM_CATALOG_SCHEMA_VERSION.to_string(),
        status: "classified_not_production_ready".to_string(),
        capability_count: capabilities.len(),
        inventory_sha256,
        proof_kind_counts,
        production_ready: false,
        capabilities,
    }
}

pub fn canonical_agent_roles() -> Vec<CanonicalAgentRole> {
    vec![
        CanonicalAgentRole::Orchestrator,
        CanonicalAgentRole::Planner,
        CanonicalAgentRole::Controller,
        CanonicalAgentRole::Scout,
        CanonicalAgentRole::Researcher,
        CanonicalAgentRole::Executor,
        CanonicalAgentRole::Builder,
        CanonicalAgentRole::Reviewer,
        CanonicalAgentRole::Tester,
        CanonicalAgentRole::SecurityReviewer,
        CanonicalAgentRole::Auditor,
        CanonicalAgentRole::Observer,
        CanonicalAgentRole::Operator,
        CanonicalAgentRole::Deployer,
        CanonicalAgentRole::IncidentResponder,
        CanonicalAgentRole::DataEngineer,
        CanonicalAgentRole::HumanLiaison,
    ]
}

pub fn unified_catalog_kinds() -> Vec<UnifiedCatalogKind> {
    vec![
        UnifiedCatalogKind::Agent,
        UnifiedCatalogKind::Skill,
        UnifiedCatalogKind::Workflow,
        UnifiedCatalogKind::Squad,
        UnifiedCatalogKind::Addon,
        UnifiedCatalogKind::Tool,
        UnifiedCatalogKind::McpServer,
        UnifiedCatalogKind::Cli,
        UnifiedCatalogKind::Model,
        UnifiedCatalogKind::Provider,
        UnifiedCatalogKind::Recipe,
        UnifiedCatalogKind::Policy,
        UnifiedCatalogKind::Guardrail,
        UnifiedCatalogKind::Template,
    ]
}

pub fn resolve_layered_skills(layers: &[SkillLayer]) -> ResolvedSkillSet {
    let precedence = [
        "system",
        "tenant",
        "workspace",
        "mission",
        "squad",
        "role",
        "agent",
        "task",
    ];
    let rank = precedence
        .iter()
        .enumerate()
        .map(|(index, layer)| ((*layer).to_string(), index))
        .collect::<BTreeMap<_, _>>();
    let mut ordered = layers.to_vec();
    ordered.sort_by_key(|layer| rank.get(&layer.layer).copied().unwrap_or(usize::MAX));
    let mut skills = BTreeSet::new();
    let mut denied = BTreeSet::new();
    let mut trace = Vec::new();
    for layer in ordered {
        for skill in layer.denied {
            denied.insert(skill.clone());
            skills.remove(&skill);
            trace.push(format!("{}.deny:{skill}", layer.layer));
        }
        for skill in layer.allowed {
            if denied.contains(&skill) {
                trace.push(format!("{}.blocked:{skill}", layer.layer));
            } else {
                skills.insert(skill.clone());
                trace.push(format!("{}.allow:{skill}", layer.layer));
            }
        }
    }
    ResolvedSkillSet {
        skills: skills.into_iter().collect(),
        denied: denied.into_iter().collect(),
        precedence: precedence
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        resolution_trace: trace,
    }
}

pub fn resolve_effort(level: EffortLevel) -> EffortBudget {
    match level {
        EffortLevel::Minimal => effort_budget(level, "minimal", 1, 20, 0, 300, 0.25),
        EffortLevel::Low => effort_budget(level, "low", 1, 35, 0, 600, 0.75),
        EffortLevel::Medium => effort_budget(level, "medium", 2, 55, 1, 1_200, 2.0),
        EffortLevel::High => effort_budget(level, "high", 3, 75, 1, 2_700, 5.0),
        EffortLevel::Maximum => effort_budget(level, "maximum", 4, 95, 2, 3_600, 10.0),
        EffortLevel::Adaptive => effort_budget(level, "adaptive", 3, 70, 1, 2_700, 5.0),
    }
}

fn effort_budget(
    level: EffortLevel,
    reasoning_effort: &str,
    max_attempts: usize,
    context_fraction_percent: u8,
    reviewer_count: usize,
    timeout_seconds: u64,
    max_cost_usd: f64,
) -> EffortBudget {
    EffortBudget {
        level,
        reasoning_effort: reasoning_effort.to_string(),
        max_attempts,
        context_fraction_percent,
        reviewer_count,
        timeout_seconds,
        max_cost_usd,
    }
}

pub fn derive_adaptive_intake(
    required_fields: &[String],
    recipe_values: &BTreeMap<String, String>,
    workspace_values: &BTreeMap<String, String>,
    context_values: &BTreeMap<String, String>,
) -> AdaptiveIntakeReport {
    let mut resolved = BTreeSet::new();
    for field in required_fields {
        if recipe_values.contains_key(field)
            || workspace_values.contains_key(field)
            || context_values.contains_key(field)
        {
            resolved.insert(field.clone());
        }
    }
    AdaptiveIntakeReport {
        required_fields: required_fields.to_vec(),
        already_resolved: resolved.iter().cloned().collect(),
        remaining_questions: required_fields
            .iter()
            .filter(|field| !resolved.contains(*field))
            .cloned()
            .collect(),
    }
}

pub fn schedule_target_kinds() -> Vec<ScheduleTargetKind> {
    vec![
        ScheduleTargetKind::Workflow,
        ScheduleTargetKind::Squad,
        ScheduleTargetKind::Mission,
        ScheduleTargetKind::Agent,
        ScheduleTargetKind::Skill,
        ScheduleTargetKind::Script,
        ScheduleTargetKind::Command,
        ScheduleTargetKind::SyntheticEvent,
    ]
}

pub fn schedule_trigger_kinds() -> Vec<ScheduleTriggerKind> {
    vec![
        ScheduleTriggerKind::Cron,
        ScheduleTriggerKind::Interval,
        ScheduleTriggerKind::OneShot,
        ScheduleTriggerKind::Calendar,
        ScheduleTriggerKind::Webhook,
        ScheduleTriggerKind::Event,
        ScheduleTriggerKind::Condition,
        ScheduleTriggerKind::DependencyCompletion,
        ScheduleTriggerKind::ManualApproval,
    ]
}

pub fn cost_dimensions() -> Vec<CostDimension> {
    vec![
        CostDimension::Tenant,
        CostDimension::Workspace,
        CostDimension::Workflow,
        CostDimension::Mission,
        CostDimension::Squad,
        CostDimension::Agent,
        CostDimension::Task,
        CostDimension::Provider,
        CostDimension::Model,
        CostDimension::Tool,
        CostDimension::Addon,
        CostDimension::Branch,
    ]
}

pub fn marketplace_package_kinds() -> Vec<MarketplacePackageKind> {
    vec![
        MarketplacePackageKind::Addon,
        MarketplacePackageKind::Agent,
        MarketplacePackageKind::Skill,
        MarketplacePackageKind::Workflow,
        MarketplacePackageKind::Squad,
        MarketplacePackageKind::Recipe,
        MarketplacePackageKind::Tool,
        MarketplacePackageKind::McpServer,
        MarketplacePackageKind::Provider,
        MarketplacePackageKind::Policy,
        MarketplacePackageKind::Guardrail,
        MarketplacePackageKind::UiModule,
        MarketplacePackageKind::Connector,
    ]
}

pub fn memory_scopes() -> Vec<MissionMemoryScope> {
    vec![
        MissionMemoryScope::Agent,
        MissionMemoryScope::Task,
        MissionMemoryScope::Mission,
        MissionMemoryScope::Squad,
        MissionMemoryScope::Workflow,
        MissionMemoryScope::Workspace,
        MissionMemoryScope::Project,
        MissionMemoryScope::Tenant,
        MissionMemoryScope::Organization,
        MissionMemoryScope::Global,
    ]
}

pub fn execution_targets() -> Vec<ExecutionTarget> {
    vec![
        ExecutionTarget::Local,
        ExecutionTarget::Ssh,
        ExecutionTarget::Docker,
        ExecutionTarget::Kubernetes,
        ExecutionTarget::VirtualMachine,
        ExecutionTarget::Wasm,
        ExecutionTarget::Serverless,
        ExecutionTarget::Edge,
        ExecutionTarget::Browser,
        ExecutionTarget::MobileDevice,
        ExecutionTarget::RemoteForgeWorker,
    ]
}

pub fn media_operations() -> Vec<MediaOperation> {
    vec![
        MediaOperation::ImageGenerate,
        MediaOperation::ImageEdit,
        MediaOperation::VideoGenerate,
        MediaOperation::VideoCompose,
        MediaOperation::AudioGenerate,
        MediaOperation::AudioTranscribe,
        MediaOperation::VoiceSynthesize,
        MediaOperation::MediaInspect,
        MediaOperation::MediaConvert,
        MediaOperation::MediaPublish,
    ]
}

pub fn route_voice_command(input: &str) -> Option<VoiceCommandRoute> {
    let normalized = input.trim().to_lowercase();
    let route = if normalized.contains("iniciar missão") || normalized.contains("start mission") {
        (
            VoiceControlLevel::MissionOperations,
            "mission.start",
            "mission",
            true,
        )
    } else if normalized.contains("chamar squad") || normalized.contains("call squad") {
        (
            VoiceControlLevel::MissionOperations,
            "squad.invoke",
            "squad",
            true,
        )
    } else if normalized.contains("pedir status") || normalized == "status" {
        (
            VoiceControlLevel::ActiveAgent,
            "status.read",
            "active_context",
            false,
        )
    } else if normalized.contains("pausar") || normalized.contains("pause") {
        (
            VoiceControlLevel::MissionOperations,
            "mission.pause",
            "mission",
            true,
        )
    } else if normalized.contains("aprovar gate") || normalized.contains("approve gate") {
        (
            VoiceControlLevel::MissionOperations,
            "gate.approve",
            "quality_gate",
            true,
        )
    } else if normalized.contains("solicitar revisão") || normalized.contains("request review") {
        (
            VoiceControlLevel::MissionOperations,
            "revision.request",
            "task",
            false,
        )
    } else if normalized.contains("cancelar agente") || normalized.contains("cancel agent") {
        (
            VoiceControlLevel::ForgeSupervisor,
            "agent.cancel",
            "agent",
            true,
        )
    } else if normalized.contains("resumir resultados") || normalized.contains("summarize results")
    {
        (
            VoiceControlLevel::ActiveAgent,
            "results.summarize",
            "mission",
            false,
        )
    } else {
        return None;
    };
    Some(VoiceCommandRoute {
        level: route.0,
        action: route.1.to_string(),
        target: route.2.to_string(),
        requires_approval: route.3,
    })
}

pub fn build_language_gateway_plan(
    input: &str,
    source_locale: &str,
    agent_locale: &str,
    output_locale: &str,
) -> LanguageGatewayPlan {
    let mut protected = BTreeSet::new();
    let mut in_code = false;
    let mut code = String::new();
    for character in input.chars() {
        if character == '`' {
            if in_code && !code.is_empty() {
                protected.insert(code.clone());
                code.clear();
            }
            in_code = !in_code;
        } else if in_code {
            code.push(character);
        }
    }
    for token in input.split_whitespace() {
        let trimmed = token.trim_matches(|character: char| {
            matches!(
                character,
                ',' | ';' | ':' | '.' | '(' | ')' | '[' | ']' | '"' | '\''
            )
        });
        if trimmed.contains('/')
            || trimmed.contains("::")
            || trimmed.ends_with(".rs")
            || trimmed.starts_with("forge.")
            || (trimmed.starts_with('E')
                && trimmed[1..].chars().all(|value| value.is_ascii_digit()))
        {
            protected.insert(trimmed.to_string());
        }
    }
    LanguageGatewayPlan {
        source_locale: source_locale.to_string(),
        agent_locale: agent_locale.to_string(),
        output_locale: output_locale.to_string(),
        translation_required: source_locale != agent_locale || agent_locale != output_locale,
        protected_segments: protected.into_iter().collect(),
        preservation_rules: vec![
            "code".to_string(),
            "identifiers".to_string(),
            "file_names".to_string(),
            "contracts".to_string(),
            "logs".to_string(),
            "error_messages".to_string(),
            "technical_terms".to_string(),
            "citations".to_string(),
        ],
        input_sha256: format!("{:x}", Sha256::digest(input.as_bytes())),
    }
}

pub fn workspace_services() -> Vec<WorkspaceService> {
    vec![
        WorkspaceService::VirtualFilesystem,
        WorkspaceService::TerminalSession,
        WorkspaceService::ProcessLifecycle,
        WorkspaceService::BrowserSessionRegistry,
        WorkspaceService::FileWatcher,
        WorkspaceService::WorkspaceState,
        WorkspaceService::SessionRestore,
        WorkspaceService::RecentWorkspaceIndex,
        WorkspaceService::RemoteFilesystem,
        WorkspaceService::ArtifactPreview,
    ]
}

pub fn official_squad_ids() -> Vec<&'static str> {
    vec![
        "software-factory",
        "bug-triage",
        "security-audit",
        "architecture-review",
        "migration-squad",
        "incident-response",
        "research-squad",
        "content-studio",
        "crm-operations",
        "sales-squad",
        "customer-support",
        "data-analysis",
        "infrastructure-operations",
        "product-discovery",
        "qa-factory",
        "release-squad",
    ]
}

pub fn consolidate_deliveries(deliveries: &[StructuredAgentDelivery]) -> ConsolidationReport {
    let mut by_task = BTreeMap::<String, BTreeSet<String>>::new();
    let mut artifacts = BTreeSet::new();
    for delivery in deliveries {
        by_task
            .entry(delivery.task_id.clone())
            .or_default()
            .insert(delivery.status.clone());
        artifacts.extend(delivery.artifacts.iter().cloned());
    }
    let contradictions = by_task
        .into_iter()
        .filter(|(_, statuses)| {
            let all_success = statuses
                .iter()
                .all(|status| matches!(status.as_str(), "completed" | "repaired"));
            statuses.len() > 1 && !all_success
        })
        .map(|(task, statuses)| {
            format!(
                "task {task} has conflicting statuses: {}",
                statuses.into_iter().collect::<Vec<_>>().join(",")
            )
        })
        .collect::<Vec<_>>();
    let tests_failed = deliveries
        .iter()
        .map(|item| item.tests_failed)
        .sum::<usize>();
    let promotion_allowed = !deliveries.is_empty()
        && contradictions.is_empty()
        && tests_failed == 0
        && deliveries
            .iter()
            .all(|item| matches!(item.status.as_str(), "completed" | "repaired"));
    ConsolidationReport {
        status: if promotion_allowed {
            "ready"
        } else {
            "blocked"
        }
        .to_string(),
        delivery_count: deliveries.len(),
        contradiction_count: contradictions.len(),
        contradictions,
        artifact_union: artifacts.into_iter().collect(),
        promotion_allowed,
    }
}

pub fn build_repair_plan(failed_gates: &[String]) -> RepairPlan {
    let repair_tasks = failed_gates
        .iter()
        .map(|gate| format!("repair:{gate}"))
        .collect::<Vec<_>>();
    RepairPlan {
        status: if failed_gates.is_empty() {
            "not_required"
        } else {
            "repairing"
        }
        .to_string(),
        failed_gates: failed_gates.to_vec(),
        repair_tasks,
        revalidation_required: !failed_gates.is_empty(),
        promotion_blocked: !failed_gates.is_empty(),
    }
}

fn json_sha256(value: &Value) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    format!("{:x}", Sha256::digest(bytes))
}

fn effect_receipt(
    capability_id: &str,
    adapter: &str,
    input: &Value,
    result: Value,
) -> CapabilityEffectReceipt {
    let input_sha256 = json_sha256(input);
    let result_sha256 = json_sha256(&result);
    let id_material = json!({
        "adapter": adapter,
        "capability_id": capability_id,
        "input_sha256": input_sha256,
        "result_sha256": result_sha256,
    });
    CapabilityEffectReceipt {
        schema_version: "forge.mission_platform.effect_receipt.v1".to_string(),
        id: format!("effect_{}", &json_sha256(&id_material)[..24]),
        capability_id: capability_id.to_string(),
        adapter: adapter.to_string(),
        input_sha256,
        result_sha256,
        result,
    }
}

fn value_strings(value: &Value) -> Result<Vec<String>, String> {
    value
        .as_array()
        .ok_or_else(|| "expected an array".to_string())?
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_string)
                .ok_or_else(|| "expected an array of strings".to_string())
        })
        .collect()
}

fn required_str<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value[field]
        .as_str()
        .filter(|item| !item.trim().is_empty())
        .ok_or_else(|| format!("missing non-empty field {field}"))
}

fn execute_effect_adapter(capability_id: &str, dependency: &Value) -> Result<Value, String> {
    match capability_id {
        "canonical_agent_roles" => {
            let registered = dependency["registered"]
                .as_object()
                .ok_or_else(|| "role registry is missing".to_string())?;
            let requested = value_strings(&dependency["requested"])?;
            let mut resolved = Vec::new();
            for role in &requested {
                let definition = registered
                    .get(role)
                    .ok_or_else(|| format!("canonical role is not registered: {role}"))?;
                if required_str(definition, "contract")? != "forge.agent.v1" {
                    return Err(format!("canonical role has an invalid contract: {role}"));
                }
                resolved.push(json!({"role": role, "definition": definition}));
            }
            Ok(json!({"requested_count": requested.len(), "resolved": resolved}))
        }
        "mission_modes" => {
            let mode = required_str(dependency, "mode")?;
            let started = dependency["events"].as_array().is_some_and(|events| {
                events
                    .iter()
                    .any(|event| event["kind"] == "mission.started")
            });
            let completed = dependency["events"].as_array().is_some_and(|events| {
                events
                    .iter()
                    .any(|event| event["kind"] == "mission.completed")
            });
            if !started || !completed || mode != "simulation" {
                return Err(
                    "mode handler did not execute the bounded simulation lifecycle".to_string(),
                );
            }
            Ok(json!({"handler": "bounded_simulation", "started": started, "completed": completed}))
        }
        "unified_multi_runtime_catalog" => {
            let entries = dependency["entries"]
                .as_array()
                .ok_or_else(|| "catalog entries are missing".to_string())?;
            let requested = value_strings(&dependency["requested"])?;
            let mut resolved = BTreeMap::new();
            for kind in requested {
                let entry = entries
                    .iter()
                    .find(|entry| entry["kind"] == kind)
                    .ok_or_else(|| format!("catalog kind cannot be resolved: {kind}"))?;
                resolved.insert(kind, required_str(entry, "locator")?.to_string());
            }
            Ok(json!({"resolved": resolved}))
        }
        "controlled_originals" => {
            let packages = dependency["packages"]
                .as_array()
                .ok_or_else(|| "controlled originals are missing".to_string())?;
            let squad_id = required_str(dependency, "squad_id")?;
            let version = required_str(dependency, "version")?;
            let composition = required_str(dependency, "composition_sha256")?;
            let package = packages
                .iter()
                .find(|package| package["id"] == squad_id && package["version"] == version)
                .ok_or_else(|| "selected original is not versioned in the catalog".to_string())?;
            if package["composition_sha256"] != composition
                || package["trusted"] != true
                || package["auto_update"] != false
            {
                return Err("selected original does not match its controlled package".to_string());
            }
            Ok(
                json!({"resolved": true, "id": squad_id, "version": version, "composition_sha256": composition}),
            )
        }
        "role_skill_resolution" => {
            let layers: Vec<SkillLayer> = serde_json::from_value(dependency["layers"].clone())
                .map_err(|error| format!("invalid skill layers: {error}"))?;
            let resolution = resolve_layered_skills(&layers);
            let task_id = required_str(&dependency["harness"], "task_id")?;
            Ok(json!({
                "task_id": task_id,
                "harness": {"skills": resolution.skills},
                "denied": resolution.denied,
                "trace": resolution.resolution_trace,
            }))
        }
        "deterministic_precedence" => {
            let layers: Vec<SkillLayer> = serde_json::from_value(dependency["layers"].clone())
                .map_err(|error| format!("invalid precedence layers: {error}"))?;
            let resolution = resolve_layered_skills(&layers);
            Ok(json!({
                "selected_skills": resolution.skills,
                "denied": resolution.denied,
                "trace": resolution.resolution_trace,
            }))
        }
        "invocation_effort" => {
            let level = match required_str(dependency, "level")? {
                "minimal" => EffortLevel::Minimal,
                "low" => EffortLevel::Low,
                "medium" => EffortLevel::Medium,
                "high" => EffortLevel::High,
                "maximum" => EffortLevel::Maximum,
                "adaptive" => EffortLevel::Adaptive,
                other => return Err(format!("unsupported effort level: {other}")),
            };
            let budget = resolve_effort(level);
            Ok(
                json!({"task_id": required_str(dependency, "task_id")?, "applied_level": dependency["level"], "budget": budget}),
            )
        }
        "recipes" => {
            let recipes = dependency["recipes"]
                .as_array()
                .ok_or_else(|| "recipe catalog is missing".to_string())?;
            let objective_type = required_str(dependency, "objective_type")?;
            let recipe = recipes
                .iter()
                .find(|recipe| recipe["objective_type"] == objective_type)
                .ok_or_else(|| format!("no recipe handles objective type {objective_type}"))?;
            Ok(
                json!({"resolved": true, "recipe_id": required_str(recipe, "id")?, "defaults": recipe["defaults"]}),
            )
        }
        "adaptive_intake" => {
            let required = value_strings(&dependency["required_fields"])?;
            let recipe: BTreeMap<String, String> =
                serde_json::from_value(dependency["recipe_values"].clone())
                    .map_err(|error| format!("invalid recipe values: {error}"))?;
            let workspace: BTreeMap<String, String> =
                serde_json::from_value(dependency["workspace_values"].clone())
                    .map_err(|error| format!("invalid workspace values: {error}"))?;
            let context: BTreeMap<String, String> =
                serde_json::from_value(dependency["context_values"].clone())
                    .map_err(|error| format!("invalid context values: {error}"))?;
            Ok(serde_json::to_value(derive_adaptive_intake(
                &required, &recipe, &workspace, &context,
            ))
            .map_err(|error| error.to_string())?)
        }
        "agent_aware_scheduling" => {
            let target_id = required_str(dependency, "target_id")?;
            let trigger = required_str(dependency, "trigger")?;
            let targets = dependency["targets"]
                .as_array()
                .ok_or_else(|| "schedule target registry is missing".to_string())?;
            let target = targets
                .iter()
                .find(|target| target["id"] == target_id)
                .ok_or_else(|| format!("schedule target is not registered: {target_id}"))?;
            let target_kind = required_str(target, "kind")?;
            if ![
                "agent",
                "mission",
                "squad",
                "workflow",
                "skill",
                "script",
                "command",
                "synthetic_event",
            ]
            .contains(&target_kind)
            {
                return Err(format!("unsupported schedule target kind: {target_kind}"));
            }
            Ok(
                json!({"dispatched": true, "dispatch_id": format!("schedule_{}", &json_sha256(dependency)[..16]), "target_id": target_id, "target_kind": target_kind, "trigger": trigger}),
            )
        }
        "agent_config_isolation" => {
            let virtual_home = required_str(dependency, "virtual_home")?;
            let config_dir = required_str(dependency, "config_dir")?;
            let host_home = required_str(dependency, "host_home")?;
            let credential = required_str(dependency, "credential")?;
            let command = required_str(dependency, "command")?;
            let allowlist = value_strings(&dependency["tool_allowlist"])?;
            if virtual_home == host_home
                || config_dir.starts_with(host_home)
                || !credential.starts_with("vault://")
                || !allowlist.iter().any(|allowed| allowed == command)
                || dependency["network"] != "deny"
                || dependency["max_memory_mb"].as_u64().unwrap_or(0) == 0
            {
                return Err("sandbox launch policy is not isolated and fail-closed".to_string());
            }
            Ok(
                json!({"launched": true, "virtual_home": virtual_home, "config_dir": config_dir, "credential_reference": credential, "network": "deny", "command": command}),
            )
        }
        "unified_marketplace" => {
            let requested = required_str(dependency, "requested")?;
            let package = dependency["packages"]
                .as_array()
                .and_then(|packages| packages.iter().find(|package| package["id"] == requested))
                .ok_or_else(|| format!("marketplace package is missing: {requested}"))?;
            if package["signed"] != true || package["trusted"] != true {
                return Err("marketplace package is not signed and trusted".to_string());
            }
            Ok(
                json!({"installed": true, "package": requested, "kind": package["kind"], "digest": json_sha256(package)}),
            )
        }
        "workspace_memory" => {
            let key = required_str(&dependency["write"], "key")?;
            let value = required_str(&dependency["write"], "value")?;
            let write_scope = required_str(&dependency["write"], "scope")?;
            let read_scope = required_str(dependency, "read_scope")?;
            let readable = value_strings(&dependency["readable_scopes"])?;
            if !readable.iter().any(|scope| scope == write_scope)
                || !readable.iter().any(|scope| scope == read_scope)
            {
                return Err("memory scope cannot share this record".to_string());
            }
            Ok(
                json!({"read_back": true, "key": key, "value_sha256": json_sha256(&json!(value)), "write_scope": write_scope, "read_scope": read_scope}),
            )
        }
        "session_restoration" => {
            let checkpoint = &dependency["checkpoint"];
            let checkpoint_id = required_str(checkpoint, "id")?;
            let state = checkpoint["state"].clone();
            let state_sha256 = json_sha256(&state);
            if checkpoint["state_sha256"] != state_sha256 {
                return Err("session checkpoint hash is corrupt".to_string());
            }
            Ok(
                json!({"restored": true, "checkpoint_id": checkpoint_id, "source_sha256": state_sha256, "restored_sha256": json_sha256(&state), "state": state}),
            )
        }
        "remote_workspace" => {
            let target_id = required_str(dependency, "target_id")?;
            let target = dependency["targets"]
                .as_array()
                .and_then(|targets| targets.iter().find(|target| target["id"] == target_id))
                .ok_or_else(|| format!("remote target is unavailable: {target_id}"))?;
            let kind = required_str(target, "kind")?;
            if kind == "local" || !required_str(target, "credential")?.starts_with("vault://") {
                return Err("remote target is not safely configured".to_string());
            }
            Ok(
                json!({"dispatched": true, "dispatch_id": format!("remote_{}", &json_sha256(dependency)[..16]), "target_id": target_id, "target_kind": kind, "external_execution_performed": false}),
            )
        }
        "mission_worktree" => {
            let mission_worktree = required_str(dependency, "mission_worktree")?;
            let record_root = required_str(dependency, "record_root")?;
            let workflow_id = required_str(dependency, "workflow_id")?;
            let binding_workflow_id = required_str(&dependency["binding"], "workflow_id")?;
            let identity = required_str(dependency, "record_identity_sha256")?;
            let binding_identity =
                required_str(&dependency["binding"], "worktree_identity_sha256")?;
            if mission_worktree != record_root
                || workflow_id != binding_workflow_id
                || identity != binding_identity
                || dependency["binding"]["schema_version"] != "forge.worktree.binding.v1"
            {
                return Err("mission worktree has no matching durable binding receipt".to_string());
            }
            Ok(
                json!({"bound": true, "worktree": record_root, "workflow_id": workflow_id, "binding_fingerprint": json_sha256(&dependency["binding"])}),
            )
        }
        "native_media_tools" => {
            let operation = required_str(dependency, "operation")?;
            let tool = dependency["tools"]
                .as_object()
                .and_then(|tools| tools.get(operation))
                .and_then(Value::as_str)
                .ok_or_else(|| format!("no native media tool handles {operation}"))?;
            Ok(
                json!({"invoked": true, "operation": operation, "tool": tool, "invocation_id": format!("media_{}", &json_sha256(dependency)[..16]), "external_execution_performed": false}),
            )
        }
        "two_level_voice" => {
            let command = required_str(dependency, "command")?;
            let route = route_voice_command(command)
                .ok_or_else(|| "voice command is not routable".to_string())?;
            let approved = dependency["approved"].as_bool().unwrap_or(false);
            if route.requires_approval && !approved {
                return Err("voice route requires an approval receipt".to_string());
            }
            Ok(
                json!({"dispatched": true, "level": route.level, "action": route.action, "target": route.target, "approval_consumed": route.requires_approval, "external_execution_performed": false}),
            )
        }
        "operational_translation" => {
            let source = required_str(dependency, "source")?;
            let plan = build_language_gateway_plan(
                source,
                required_str(dependency, "source_locale")?,
                required_str(dependency, "agent_locale")?,
                required_str(dependency, "output_locale")?,
            );
            let mut translated = source.to_string();
            let dictionary = dependency["dictionary"]
                .as_array()
                .ok_or_else(|| "translation dictionary is missing".to_string())?;
            for replacement in dictionary {
                let from = required_str(replacement, "from")?;
                let to = required_str(replacement, "to")?;
                translated = translated.replace(from, to);
            }
            let protected_preserved = plan
                .protected_segments
                .iter()
                .all(|segment| translated.contains(segment));
            if translated == source || !protected_preserved {
                return Err(
                    "translation did not transform prose while preserving protected segments"
                        .to_string(),
                );
            }
            Ok(
                json!({"translated": true, "output": translated, "source_sha256": plan.input_sha256, "output_sha256": json_sha256(&json!(translated)), "protected_segments": plan.protected_segments, "protected_preserved": protected_preserved, "external_execution_performed": false}),
            )
        }
        "integrated_workspace" => {
            let session = required_str(dependency, "session_id")?;
            let path = required_str(&dependency["file"], "path")?;
            let contents = required_str(&dependency["file"], "contents")?;
            if dependency["terminal"]["session_id"] != session
                || dependency["restore"]["session_id"] != session
            {
                return Err("workspace services do not share a restorable session".to_string());
            }
            Ok(
                json!({"operations_applied": 4, "session_id": session, "file_path": path, "file_sha256": json_sha256(&json!(contents)), "terminal_attached": true, "restored": true}),
            )
        }
        "official_squad_catalog" => {
            let squads = dependency["squads"]
                .as_array()
                .ok_or_else(|| "official squad definitions are missing".to_string())?;
            let mut ids = BTreeSet::new();
            let mut definition_hashes = BTreeMap::new();
            for squad in squads {
                let id = required_str(squad, "id")?;
                required_str(squad, "version")?;
                if squad["roster"].as_array().is_none_or(Vec::is_empty) {
                    return Err(format!("official squad is not installable: {id}"));
                }
                if !ids.insert(id.to_string()) {
                    return Err(format!("duplicate official squad id: {id}"));
                }
                definition_hashes.insert(id.to_string(), json_sha256(squad));
            }
            Ok(
                json!({"installable_count": ids.len(), "ids": ids, "definition_hashes": definition_hashes}),
            )
        }
        "result_consolidation" => {
            let deliveries: Vec<StructuredAgentDelivery> =
                serde_json::from_value(dependency["deliveries"].clone())
                    .map_err(|error| format!("invalid structured deliveries: {error}"))?;
            Ok(serde_json::to_value(consolidate_deliveries(&deliveries))
                .map_err(|error| error.to_string())?)
        }
        "formal_review_repair" => {
            let gates = dependency["gates"]
                .as_array()
                .ok_or_else(|| "gate history is missing".to_string())?;
            let failed = gates
                .iter()
                .filter(|gate| gate["status"] == "failed")
                .filter_map(|gate| gate["gate_id"].as_str().map(str::to_string))
                .collect::<Vec<_>>();
            if failed.is_empty() {
                return Err("no failed gate triggered repair".to_string());
            }
            let revalidated = failed.iter().all(|failed_id| {
                gates.iter().any(|gate| {
                    gate["gate_id"] == *failed_id
                        && gate["status"] == "passed"
                        && gate["attempt"].as_u64().unwrap_or(0) > 1
                })
            });
            let plan = build_repair_plan(&failed);
            Ok(
                json!({"repair_plan": plan, "revalidated": revalidated, "promotion_after_revalidation": revalidated}),
            )
        }
        other => Err(format!(
            "no deterministic effect adapter registered for {other}"
        )),
    }
}

fn effect_satisfies(capability_id: &str, result: &Value) -> bool {
    match capability_id {
        "canonical_agent_roles" => {
            result["requested_count"]
                .as_u64()
                .is_some_and(|count| count >= 4)
                && result["resolved"]
                    .as_array()
                    .is_some_and(|items| items.len() >= 4)
        }
        "mission_modes" => {
            result["handler"] == "bounded_simulation"
                && result["started"] == true
                && result["completed"] == true
        }
        "unified_multi_runtime_catalog" => result["resolved"].as_object().is_some_and(|resolved| {
            ["agent", "skill", "mcp_server", "cli", "provider"]
                .iter()
                .all(|kind| resolved.get(*kind).is_some_and(Value::is_string))
        }),
        "controlled_originals" => {
            result["resolved"] == true
                && result["composition_sha256"]
                    .as_str()
                    .is_some_and(|hash| hash.len() == 64)
        }
        "role_skill_resolution" => result["harness"]["skills"]
            .as_array()
            .is_some_and(|skills| {
                skills.iter().any(|skill| skill == "safe-execution")
                    && !skills.iter().any(|skill| skill == "unreviewed-deployment")
            }),
        "deterministic_precedence" => {
            result["trace"]
                .as_array()
                .is_some_and(|trace| trace.iter().any(|item| item == "task.blocked:deploy"))
                && !result["selected_skills"]
                    .as_array()
                    .is_some_and(|skills| skills.iter().any(|skill| skill == "deploy"))
        }
        "invocation_effort" => {
            result["task_id"].is_string()
                && result["applied_level"] == result["budget"]["level"]
                && result["budget"]["max_attempts"].as_u64().unwrap_or(0) > 0
                && result["budget"]["timeout_seconds"].as_u64().unwrap_or(0) > 0
        }
        "recipes" => result["resolved"] == true && result["recipe_id"].is_string(),
        "adaptive_intake" => {
            result["already_resolved"]
                .as_array()
                .is_some_and(|items| items.len() == 2)
                && result["remaining_questions"] == json!(["feature_description"])
        }
        "agent_aware_scheduling" => {
            result["dispatched"] == true
                && result["dispatch_id"].is_string()
                && result["target_id"] == "reviewer-primary"
        }
        "agent_config_isolation" => {
            result["launched"] == true
                && result["virtual_home"] == "/sandbox/agent-reviewer/home"
                && result["credential_reference"]
                    .as_str()
                    .is_some_and(|value| value.starts_with("vault://"))
        }
        "unified_marketplace" => {
            result["installed"] == true
                && result["digest"]
                    .as_str()
                    .is_some_and(|hash| hash.len() == 64)
        }
        "workspace_memory" => {
            result["read_back"] == true
                && result["write_scope"] == "task"
                && result["read_scope"] == "workspace"
        }
        "session_restoration" => {
            result["restored"] == true && result["source_sha256"] == result["restored_sha256"]
        }
        "remote_workspace" => {
            result["dispatched"] == true
                && result["target_kind"] == "ssh"
                && result["external_execution_performed"] == false
        }
        "mission_worktree" => {
            result["bound"] == true
                && result["binding_fingerprint"]
                    .as_str()
                    .is_some_and(|hash| hash.len() == 64)
        }
        "native_media_tools" => {
            result["invoked"] == true
                && result["operation"] == "media_inspect"
                && result["external_execution_performed"] == false
        }
        "two_level_voice" => {
            result["dispatched"] == true
                && result["action"] == "gate.approve"
                && result["approval_consumed"] == true
        }
        "operational_translation" => {
            result["translated"] == true
                && result["protected_preserved"] == true
                && result["source_sha256"] != result["output_sha256"]
        }
        "integrated_workspace" => {
            result["operations_applied"]
                .as_u64()
                .is_some_and(|count| count >= 4)
                && result["terminal_attached"] == true
                && result["restored"] == true
        }
        "official_squad_catalog" => {
            result["installable_count"] == 16
                && result["definition_hashes"]
                    .as_object()
                    .is_some_and(|items| items.len() == 16)
        }
        "result_consolidation" => {
            result["promotion_allowed"] == true
                && result["delivery_count"].as_u64().unwrap_or(0) >= 3
                && result["artifact_union"]
                    .as_array()
                    .is_some_and(|items| !items.is_empty())
        }
        "formal_review_repair" => {
            result["repair_plan"]["revalidation_required"] == true
                && result["repair_plan"]["promotion_blocked"] == true
                && result["revalidated"] == true
                && result["promotion_after_revalidation"] == true
        }
        _ => false,
    }
}

fn insert_effect_fixture(
    environment: &mut MissionPlatformProbeEnvironment,
    capability_id: &str,
    adapter: &str,
    dependency: Value,
) {
    if let Ok(result) = execute_effect_adapter(capability_id, &dependency) {
        let receipt = effect_receipt(capability_id, adapter, &dependency, result);
        environment.fixtures.insert(
            capability_id.to_string(),
            CapabilityEffectFixture {
                dependency,
                receipt,
            },
        );
    }
}

impl MissionPlatformProbeEnvironment {
    pub fn for_mission(mission: &MissionSimulationReport) -> Self {
        let mut environment = Self::default();
        let role_registry = canonical_agent_roles()
            .into_iter()
            .filter_map(|role| {
                let value = serde_json::to_value(role).ok()?;
                let id = value.as_str()?.to_string();
                Some((
                    id.clone(),
                    json!({"id": format!("builtin-role/{id}"), "contract": "forge.agent.v1"}),
                ))
            })
            .collect::<BTreeMap<_, _>>();
        insert_effect_fixture(
            &mut environment,
            "canonical_agent_roles",
            "canonical_role_registry.resolve",
            json!({"registered": role_registry, "requested": ["orchestrator", "builder", "reviewer", "tester"]}),
        );

        insert_effect_fixture(
            &mut environment,
            "mission_modes",
            "mission_mode.dispatch",
            json!({"mode": mission.mission.mode, "events": mission.mission.events}),
        );

        let catalog_entries = unified_catalog_kinds()
            .into_iter()
            .filter_map(|kind| {
                let value = serde_json::to_value(kind).ok()?;
                let name = value.as_str()?;
                Some(json!({"kind": name, "locator": format!("forge://catalog/{name}/v1")}))
            })
            .collect::<Vec<_>>();
        insert_effect_fixture(
            &mut environment,
            "unified_multi_runtime_catalog",
            "unified_catalog.resolve",
            json!({"entries": catalog_entries, "requested": ["agent", "skill", "mcp_server", "cli", "provider"]}),
        );

        let official_catalog = builtin_squad_catalog();
        let packages = official_catalog
            .squads
            .iter()
            .filter_map(|squad| {
                let validation = validate_squad_definition(squad).ok()?;
                validation.valid.then(|| {
                    json!({
                        "id": squad.id,
                        "version": squad.version,
                        "composition_sha256": validation.composition_sha256,
                        "trusted": squad.distribution.trusted,
                        "auto_update": squad.distribution.auto_update,
                    })
                })
            })
            .collect::<Vec<_>>();
        insert_effect_fixture(
            &mut environment,
            "controlled_originals",
            "controlled_originals.resolve_exact",
            json!({
                "packages": packages,
                "squad_id": mission.mission.squad_id,
                "version": mission.mission.squad_version,
                "composition_sha256": mission.mission.squad_composition_sha256,
            }),
        );

        let harness = mission.mission.harnesses.first();
        let harness_task = harness.map_or("mission-task-001", |item| item.task_id.as_str());
        let harness_skills = harness.map_or_else(Vec::new, |item| item.skills.clone());
        insert_effect_fixture(
            &mut environment,
            "role_skill_resolution",
            "skill_resolver.apply_to_harness",
            json!({
                "layers": [
                    SkillLayer { layer: "system".to_string(), allowed: vec!["safe-execution".to_string()], denied: vec!["unreviewed-deployment".to_string()] },
                    SkillLayer { layer: "role".to_string(), allowed: harness_skills, denied: Vec::new() },
                    SkillLayer { layer: "task".to_string(), allowed: vec!["unit-testing".to_string(), "unreviewed-deployment".to_string()], denied: Vec::new() },
                ],
                "harness": {"task_id": harness_task},
            }),
        );
        let precedence_layers = json!({"layers": [
            SkillLayer { layer: "task".to_string(), allowed: vec!["deploy".to_string(), "test".to_string()], denied: Vec::new() },
            SkillLayer { layer: "system".to_string(), allowed: vec!["read".to_string()], denied: vec!["deploy".to_string()] },
        ]});
        insert_effect_fixture(
            &mut environment,
            "deterministic_precedence",
            "precedence.resolve_conflict",
            precedence_layers,
        );
        let effort = harness.map_or("high", |item| item.effort.as_str());
        insert_effect_fixture(
            &mut environment,
            "invocation_effort",
            "effort.apply_to_invocation",
            json!({"task_id": harness_task, "level": effort}),
        );

        let recipes = official_catalog
            .squads
            .iter()
            .flat_map(|squad| squad.recipes.iter())
            .collect::<Vec<_>>();
        let objective_type = recipes
            .first()
            .map_or("software_delivery", |recipe| recipe.objective_type.as_str());
        insert_effect_fixture(
            &mut environment,
            "recipes",
            "recipe.resolve_objective",
            json!({"recipes": recipes, "objective_type": objective_type}),
        );
        insert_effect_fixture(
            &mut environment,
            "adaptive_intake",
            "adaptive_intake.resolve_sources",
            json!({
                "required_fields": ["feature_description", "acceptance_criteria", "related_issue"],
                "recipe_values": {"acceptance_criteria": "required"},
                "workspace_values": {"related_issue": "FORGE-1"},
                "context_values": {},
            }),
        );
        insert_effect_fixture(
            &mut environment,
            "agent_aware_scheduling",
            "schedule.dispatch_target",
            json!({
                "target_id": "reviewer-primary",
                "trigger": "dependency_completion:mission-task-002",
                "targets": [{"id": "reviewer-primary", "kind": "agent"}],
            }),
        );
        insert_effect_fixture(
            &mut environment,
            "agent_config_isolation",
            "sandbox.launch",
            json!({
                "virtual_home": "/sandbox/agent-reviewer/home",
                "config_dir": "/sandbox/agent-reviewer/config",
                "host_home": "/home/operator",
                "credential": "vault://providers/openai/reviewer",
                "command": "cargo",
                "tool_allowlist": ["cargo", "git"],
                "network": "deny",
                "max_memory_mb": 512,
            }),
        );
        insert_effect_fixture(
            &mut environment,
            "unified_marketplace",
            "marketplace.install",
            json!({
                "requested": "security-audit@1.0.0",
                "packages": [{"id": "security-audit@1.0.0", "kind": "squad", "signed": true, "trusted": true}],
            }),
        );
        insert_effect_fixture(
            &mut environment,
            "workspace_memory",
            "memory.write_then_read",
            json!({
                "write": {"scope": "task", "key": "decision/validator", "value": "clippy"},
                "read_scope": "workspace",
                "readable_scopes": ["task", "mission", "workspace"],
            }),
        );
        let session_state = json!({
            "mission_id": mission.mission.id,
            "revision": mission.mission.revision,
            "status": mission.mission.status,
            "agents": mission.mission.agents.iter().map(|agent| agent.instance_id.as_str()).collect::<Vec<_>>(),
        });
        insert_effect_fixture(
            &mut environment,
            "session_restoration",
            "session.checkpoint_restore",
            json!({"checkpoint": {"id": format!("checkpoint-{}", mission.mission.revision), "state_sha256": json_sha256(&session_state), "state": session_state}}),
        );
        insert_effect_fixture(
            &mut environment,
            "remote_workspace",
            "execution_target.dispatch",
            json!({
                "target_id": "staging-ssh",
                "targets": [{"id": "staging-ssh", "kind": "ssh", "credential": "vault://ssh/staging"}],
                "command": ["forge", "validate"],
            }),
        );
        insert_effect_fixture(
            &mut environment,
            "native_media_tools",
            "media_tool.invoke",
            json!({"operation": "media_inspect", "tools": {"media_inspect": "forge.media.inspect.v1"}, "input": "artifact://preview.png"}),
        );
        insert_effect_fixture(
            &mut environment,
            "two_level_voice",
            "voice.route_and_dispatch",
            json!({"command": "aprovar gate", "approved": true, "approval_receipt": "approval-local-simulation"}),
        );
        insert_effect_fixture(
            &mut environment,
            "operational_translation",
            "translation.transform_preserving_contracts",
            json!({
                "source": "Corrija `UserId` em src/api.rs sem alterar forge.agent.v1 ou E0425.",
                "source_locale": "pt-BR",
                "agent_locale": "en",
                "output_locale": "en",
                "dictionary": [
                    {"from": "Corrija", "to": "Fix"},
                    {"from": " em ", "to": " in "},
                    {"from": " sem alterar ", "to": " without changing "},
                    {"from": " ou ", "to": " or "},
                ],
            }),
        );
        insert_effect_fixture(
            &mut environment,
            "integrated_workspace",
            "workspace.apply_operations",
            json!({
                "session_id": "workspace-session-1",
                "file": {"path": "src/lib.rs", "contents": "pub mod mission;"},
                "terminal": {"session_id": "workspace-session-1", "command": "cargo test"},
                "restore": {"session_id": "workspace-session-1"},
            }),
        );
        insert_effect_fixture(
            &mut environment,
            "official_squad_catalog",
            "official_squad_catalog.validate_installable",
            json!({"squads": official_catalog.squads}),
        );
        let deliveries = mission
            .mission
            .handoffs
            .iter()
            .map(|handoff| handoff.delivery.clone())
            .collect::<Vec<_>>();
        insert_effect_fixture(
            &mut environment,
            "result_consolidation",
            "delivery.consolidate",
            json!({"deliveries": deliveries}),
        );
        insert_effect_fixture(
            &mut environment,
            "formal_review_repair",
            "quality_gate.repair_and_revalidate",
            json!({"gates": mission.mission.gates}),
        );
        environment
    }

    pub fn with_store(store: &ForgeStore, mission: &MissionSimulationReport) -> Self {
        let mut environment = Self::for_mission(mission);
        let Some(selected) = mission.mission.worktree.as_deref() else {
            return environment;
        };
        let Ok(records) =
            list_registered_worktrees(store, None, Some(&mission.mission.workflow_id))
        else {
            return environment;
        };
        let Some(record) = records
            .worktrees
            .iter()
            .find(|record| record.worktree_root == selected || record.id == selected)
        else {
            return environment;
        };
        let Some(binding) = record.bindings.iter().find(|binding| {
            binding.workflow_id == mission.mission.workflow_id && binding.task_id.is_none()
        }) else {
            return environment;
        };
        insert_effect_fixture(
            &mut environment,
            "mission_worktree",
            "registered_worktree.verify_binding",
            json!({
                "mission_worktree": record.worktree_root,
                "record_root": record.worktree_root,
                "record_identity_sha256": record.identity_sha256,
                "workflow_id": mission.mission.workflow_id,
                "binding": binding,
            }),
        );
        environment
    }
}

fn runtime_probe(
    capability: &MissionPlatformCapability,
    passed: bool,
    result: Value,
) -> CapabilityProbe {
    let receipt = effect_receipt(
        &capability.id,
        "mission_runtime.observe_effect",
        &json!({"mission_runtime": result}),
        result.clone(),
    );
    CapabilityProbe {
        number: capability.number,
        capability_id: capability.id.clone(),
        passed,
        proof_scope: MISSION_PLATFORM_BOUNDED_SIMULATION.to_string(),
        evidence: CapabilityProbeEvidence {
            execution_class: MISSION_PLATFORM_BOUNDED_SIMULATION.to_string(),
            input_sha256: receipt.input_sha256.clone(),
            result_sha256: receipt.result_sha256.clone(),
            result,
            verification: if passed {
                "runtime_effect_observed"
            } else {
                "runtime_effect_missing"
            }
            .to_string(),
            receipt: Some(receipt),
        },
    }
}

fn deterministic_probe(
    capability: &MissionPlatformCapability,
    environment: &MissionPlatformProbeEnvironment,
) -> CapabilityProbe {
    let execution_class = MISSION_PLATFORM_BOUNDED_SIMULATION;
    let Some(fixture) = environment.fixtures.get(&capability.id) else {
        return CapabilityProbe {
            number: capability.number,
            capability_id: capability.id.clone(),
            passed: false,
            proof_scope: execution_class.to_string(),
            evidence: CapabilityProbeEvidence {
                execution_class: execution_class.to_string(),
                receipt: None,
                input_sha256: String::new(),
                result_sha256: String::new(),
                result: Value::Null,
                verification: "effect_dependency_or_receipt_missing".to_string(),
            },
        };
    };
    let input_sha256 = json_sha256(&fixture.dependency);
    let actual = execute_effect_adapter(&capability.id, &fixture.dependency);
    let receipt_shape_valid = fixture.receipt.schema_version
        == "forge.mission_platform.effect_receipt.v1"
        && fixture.receipt.capability_id == capability.id
        && fixture.receipt.input_sha256 == input_sha256
        && fixture.receipt.result_sha256 == json_sha256(&fixture.receipt.result)
        && fixture.receipt.id
            == effect_receipt(
                &fixture.receipt.capability_id,
                &fixture.receipt.adapter,
                &fixture.dependency,
                fixture.receipt.result.clone(),
            )
            .id;
    let (actual_matches_receipt, result, verification) = match actual {
        Ok(actual) => {
            let matches = actual == fixture.receipt.result
                && json_sha256(&actual) == fixture.receipt.result_sha256;
            (
                matches,
                actual,
                if matches {
                    "effect_replayed_and_receipt_verified"
                } else {
                    "effect_result_mismatch"
                },
            )
        }
        Err(error) => (false, json!({"error": error}), "effect_replay_failed"),
    };
    let passed =
        receipt_shape_valid && actual_matches_receipt && effect_satisfies(&capability.id, &result);
    CapabilityProbe {
        number: capability.number,
        capability_id: capability.id.clone(),
        passed,
        proof_scope: execution_class.to_string(),
        evidence: CapabilityProbeEvidence {
            execution_class: execution_class.to_string(),
            receipt: Some(fixture.receipt.clone()),
            input_sha256,
            result_sha256: json_sha256(&result),
            result,
            verification: if passed {
                verification
            } else if !receipt_shape_valid {
                "receipt_integrity_failed"
            } else {
                verification
            }
            .to_string(),
        },
    }
}

fn runtime_capability_probe(
    capability: &MissionPlatformCapability,
    mission: &MissionSimulationReport,
) -> CapabilityProbe {
    let record = &mission.mission;
    let agent_ids = record
        .agents
        .iter()
        .map(|agent| agent.instance_id.as_str())
        .collect::<BTreeSet<_>>();
    let (passed, result) = match capability.number {
        1 => (
            mission.exact_composition_recorded
                && !record.squad_id.is_empty()
                && !record.squad_version.is_empty()
                && record.squad_composition_sha256.len() == 64,
            json!({"squad_id": record.squad_id, "version": record.squad_version, "composition_sha256": record.squad_composition_sha256}),
        ),
        2 => (
            mission.orchestrator_restricted && !record.orchestrator_instance_id.is_empty(),
            json!({"restricted": mission.orchestrator_restricted, "orchestrator_instance_id": record.orchestrator_instance_id}),
        ),
        3 => {
            let parent_links_valid = record.agents.iter().all(|agent| {
                agent
                    .parent_instance_id
                    .as_ref()
                    .is_none_or(|parent| agent_ids.contains(parent.as_str()))
            });
            (
                mission.hierarchy_limits_enforced && parent_links_valid,
                json!({"limits_enforced": mission.hierarchy_limits_enforced, "parent_links_valid": parent_links_valid, "agents": record.agents.len()}),
            )
        }
        5 => (
            mission.incremental_persistence_proven && record.revision > 1,
            json!({"revision": record.revision, "incremental_persistence": mission.incremental_persistence_proven, "event_count": record.events.len()}),
        ),
        7 => {
            let sequences = record
                .events
                .iter()
                .map(|event| event.sequence)
                .collect::<BTreeSet<_>>();
            let causal = record.events.iter().all(|event| {
                event
                    .caused_by_sequence
                    .is_none_or(|sequence| sequences.contains(&sequence))
            });
            (
                mission.validation.valid && causal,
                json!({"squad_valid": mission.validation.valid, "causal_event_graph": causal}),
            )
        }
        8 => {
            let complete = record.harnesses.iter().all(|harness| {
                !harness.task_id.is_empty()
                    && !harness.agent_id.is_empty()
                    && !harness.runtime.is_empty()
                    && !harness.provider.is_empty()
                    && !harness.model.is_empty()
                    && !harness.effort.is_empty()
            });
            (
                complete && !record.harnesses.is_empty(),
                json!({"complete_harnesses": complete, "harness_count": record.harnesses.len()}),
            )
        }
        12 => {
            let fail_closed = record.harnesses.iter().all(|harness| {
                !harness
                    .skills
                    .iter()
                    .any(|skill| skill == "unreviewed-deployment")
            });
            (
                fail_closed && !record.harnesses.is_empty(),
                json!({"fail_closed": fail_closed, "harness_count": record.harnesses.len()}),
            )
        }
        13 => {
            let task_ids = record
                .harnesses
                .iter()
                .map(|harness| harness.task_id.as_str())
                .collect::<BTreeSet<_>>();
            let covered = record
                .tasks
                .iter()
                .all(|task| task_ids.contains(task.id.as_str()));
            (
                covered && !record.tasks.is_empty(),
                json!({"tasks": record.tasks.len(), "harnessed_tasks": task_ids.len(), "all_tasks_harnessed": covered}),
            )
        }
        18 => {
            let inbox_handoffs = record
                .inbox
                .iter()
                .map(|item| item.handoff_id.as_str())
                .collect::<BTreeSet<_>>();
            let routed = record
                .handoffs
                .iter()
                .all(|handoff| inbox_handoffs.contains(handoff.id.as_str()));
            (
                mission.event_driven_handoff_proven && mission.inbox_wakeup_proven && routed,
                json!({"handoffs": record.handoffs.len(), "inbox": record.inbox.len(), "routed": routed}),
            )
        }
        19 => {
            let spawned = record
                .agents
                .iter()
                .filter(|agent| agent.spawned_on_demand)
                .count();
            (
                mission.on_demand_spawn_proven && spawned > 0,
                json!({"spawned_on_demand": spawned, "proven": mission.on_demand_spawn_proven}),
            )
        }
        21 => {
            let task_events = record
                .events
                .iter()
                .filter(|event| event.task_id.is_some())
                .count();
            (
                record.tasks.len() >= 3 && task_events >= record.tasks.len(),
                json!({"tasks": record.tasks, "task_event_count": task_events}),
            )
        }
        22 => {
            let role_sum = record.cost.by_role_usd.values().sum::<f64>();
            let aggregates = !record.cost.by_role_usd.is_empty()
                && role_sum <= record.cost.total_usd + f64::EPSILON;
            (
                mission.cost_limits_enforced && aggregates,
                json!({"total_usd": record.cost.total_usd, "by_role_usd": record.cost.by_role_usd, "aggregates": aggregates}),
            )
        }
        23 => {
            let assignments = record.harnesses.iter().map(|harness| json!({"agent": harness.agent_id, "provider": harness.provider, "model": harness.model})).collect::<Vec<_>>();
            let complete = assignments.iter().all(|assignment| {
                assignment["provider"]
                    .as_str()
                    .is_some_and(|value| !value.is_empty())
            });
            (
                complete && !assignments.is_empty(),
                json!({"independent_assignments": assignments}),
            )
        }
        24 => {
            let assignments = record
                .harnesses
                .iter()
                .map(|harness| json!({"agent": harness.agent_id, "runtime": harness.runtime}))
                .collect::<Vec<_>>();
            let complete = assignments.iter().all(|assignment| {
                assignment["runtime"]
                    .as_str()
                    .is_some_and(|value| !value.is_empty())
            });
            (
                complete && !assignments.is_empty(),
                json!({"runtime_assignments": assignments}),
            )
        }
        26 => {
            let typed = record
                .handoffs
                .iter()
                .all(|handoff| handoff.schema_version == "forge.agent_handoff.v1");
            (
                typed && !record.handoffs.is_empty(),
                json!({"typed_gateway_handoffs": typed, "handoff_count": record.handoffs.len()}),
            )
        }
        37 => {
            let passed_gates = record
                .gates
                .iter()
                .filter(|gate| gate.status == "passed")
                .count();
            (
                mission.validation_before_promotion_proven && passed_gates >= 3,
                json!({"passed_gates": passed_gates, "validation_before_promotion": mission.validation_before_promotion_proven}),
            )
        }
        38 => {
            let spawned = record
                .agents
                .iter()
                .filter(|agent| agent.spawned_on_demand)
                .count();
            let terminated = record
                .agents
                .iter()
                .filter(|agent| agent.status == "terminated")
                .count();
            (
                mission.on_demand_spawn_proven && spawned > 0 && terminated > 0,
                json!({"spawned": spawned, "terminated": terminated}),
            )
        }
        _ => (false, json!({"error": "runtime probe is not implemented"})),
    };
    runtime_probe(capability, passed, result)
}

pub fn simulate_mission_platform_with_environment(
    mission: &MissionSimulationReport,
    environment: &MissionPlatformProbeEnvironment,
) -> MissionPlatformSimulationReport {
    let catalog = mission_platform_catalog();
    let deterministic_capabilities = BTreeSet::from([
        4_u8, 6, 9, 10, 11, 14, 15, 16, 17, 20, 25, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 39, 40,
    ]);
    let probes = catalog
        .capabilities
        .iter()
        .map(|capability| {
            if deterministic_capabilities.contains(&capability.number) {
                deterministic_probe(capability, environment)
            } else {
                runtime_capability_probe(capability, mission)
            }
        })
        .collect::<Vec<_>>();
    let passed_count = probes.iter().filter(|probe| probe.passed).count();
    let failed = probes
        .iter()
        .filter(|probe| !probe.passed)
        .map(|probe| probe.capability_id.clone())
        .collect::<Vec<_>>();
    MissionPlatformSimulationReport {
        schema_version: MISSION_PLATFORM_SIMULATION_SCHEMA_VERSION.to_string(),
        status: if failed.is_empty() {
            "passed"
        } else {
            "failed"
        }
        .to_string(),
        evidence_scope: MISSION_PLATFORM_BOUNDED_SIMULATION.to_string(),
        bounded: true,
        model_execution_performed: false,
        external_mutation_performed: false,
        production_ready: false,
        capability_count: probes.len(),
        inventory_sha256: catalog.inventory_sha256,
        proof_kind_counts: catalog.proof_kind_counts,
        passed_count,
        failed_count: failed.len(),
        mission_id: mission.mission.id.clone(),
        mission_simulation_schema_version: mission.schema_version.clone(),
        probes,
        not_proven: vec![
            "real external model/provider execution".to_string(),
            "real SSH, Docker, Kubernetes or remote-worker execution".to_string(),
            "real microphone, speech, translation or media provider execution".to_string(),
            "high availability, multi-tenant isolation or multi-day soak".to_string(),
            "operational production evidence required by forge.milestone.production_readiness.v1"
                .to_string(),
        ],
    }
}

pub fn simulate_mission_platform(
    mission: &MissionSimulationReport,
) -> MissionPlatformSimulationReport {
    let environment = MissionPlatformProbeEnvironment::for_mission(mission);
    simulate_mission_platform_with_environment(mission, &environment)
}

pub fn simulate_mission_platform_with_store(
    store: &ForgeStore,
    mission: &MissionSimulationReport,
) -> MissionPlatformSimulationReport {
    let environment = MissionPlatformProbeEnvironment::with_store(store, mission);
    simulate_mission_platform_with_environment(mission, &environment)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission::simulate_mission;
    use tempfile::tempdir;

    fn bounded_mission() -> MissionSimulationReport {
        let temp = tempdir().unwrap();
        let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();
        simulate_mission(
            &store,
            "Verify adversarial mission platform effects",
            "software-factory",
            None,
            true,
        )
        .unwrap()
    }

    fn probe<'a>(
        report: &'a MissionPlatformSimulationReport,
        capability_id: &str,
    ) -> &'a CapabilityProbe {
        report
            .probes
            .iter()
            .find(|probe| probe.capability_id == capability_id)
            .unwrap()
    }

    #[test]
    fn catalog_is_exactly_the_numbered_forty_capabilities() {
        let catalog = mission_platform_catalog();
        assert_eq!(catalog.capability_count, MISSION_PLATFORM_CAPABILITY_COUNT);
        assert_eq!(catalog.status, "classified_not_production_ready");
        assert!(!catalog.production_ready);
        assert_eq!(catalog.inventory_sha256.len(), 64);
        assert_eq!(catalog.proof_kind_counts[MISSION_PLATFORM_RUNTIME_REAL], 20);
        assert_eq!(
            catalog.proof_kind_counts[MISSION_PLATFORM_BOUNDED_SIMULATION],
            14
        );
        assert_eq!(catalog.proof_kind_counts[MISSION_PLATFORM_CONTRACT_ONLY], 6);
        assert_eq!(catalog.capabilities.first().unwrap().number, 1);
        assert_eq!(catalog.capabilities.last().unwrap().number, 40);
        assert!(catalog
            .capabilities
            .iter()
            .all(|capability| !capability.production_ready));
        let ids = catalog
            .capabilities
            .iter()
            .map(|capability| capability.id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), 40);
    }

    #[test]
    fn layered_skill_resolution_is_fail_closed() {
        let resolved = resolve_layered_skills(&[
            SkillLayer {
                layer: "system".to_string(),
                allowed: vec!["read".to_string()],
                denied: vec!["deploy".to_string()],
            },
            SkillLayer {
                layer: "task".to_string(),
                allowed: vec!["deploy".to_string(), "test".to_string()],
                denied: Vec::new(),
            },
        ]);
        assert_eq!(resolved.skills, vec!["read", "test"]);
        assert_eq!(resolved.denied, vec!["deploy"]);
        assert!(resolved
            .resolution_trace
            .contains(&"task.blocked:deploy".to_string()));
    }

    #[test]
    fn language_gateway_protects_operational_tokens() {
        let plan = build_language_gateway_plan(
            "Corrija `UserId` em src/api.rs; preserve forge.agent.v1 e E0425.",
            "pt-BR",
            "en",
            "pt-BR",
        );
        for expected in ["UserId", "src/api.rs", "forge.agent.v1", "E0425"] {
            assert!(plan.protected_segments.contains(&expected.to_string()));
        }
        assert_eq!(plan.preservation_rules.len(), 8);
    }

    #[test]
    fn consolidation_blocks_conflicting_deliveries() {
        let delivery = |status: &str| StructuredAgentDelivery {
            task_id: "task-1".to_string(),
            status: status.to_string(),
            summary: status.to_string(),
            artifacts: Vec::new(),
            tests_passed: 1,
            tests_failed: 0,
            risks: Vec::new(),
            followups: Vec::new(),
        };
        let report = consolidate_deliveries(&[delivery("completed"), delivery("blocked")]);
        assert!(!report.promotion_allowed);
        assert_eq!(report.contradiction_count, 1);
    }

    #[test]
    fn every_probe_carries_a_structured_integrity_receipt_or_an_explicit_missing_result() {
        let mission = bounded_mission();
        let environment = MissionPlatformProbeEnvironment::for_mission(&mission);
        let report = simulate_mission_platform_with_environment(&mission, &environment);
        assert_eq!(report.probes.len(), 40);
        for item in &report.probes {
            assert!(!item.evidence.execution_class.is_empty());
            if item.capability_id == "mission_worktree" {
                assert!(!item.passed);
                assert!(item.evidence.receipt.is_none());
                assert_eq!(
                    item.evidence.verification,
                    "effect_dependency_or_receipt_missing"
                );
            } else {
                let receipt = item
                    .evidence
                    .receipt
                    .as_ref()
                    .expect("observed effects must carry a receipt");
                assert_eq!(receipt.capability_id, item.capability_id);
                assert_eq!(receipt.input_sha256.len(), 64);
                assert_eq!(receipt.result_sha256.len(), 64);
                assert!(!receipt.result.is_null());
            }
        }
    }

    #[test]
    fn every_deterministic_adapter_fails_when_its_effect_fixture_is_removed() {
        let mission = bounded_mission();
        let baseline = MissionPlatformProbeEnvironment::for_mission(&mission);
        assert!(baseline.fixtures.len() >= 22);
        for capability_id in baseline.fixtures.keys() {
            let mut adversarial = baseline.clone();
            adversarial.fixtures.remove(capability_id);
            let report = simulate_mission_platform_with_environment(&mission, &adversarial);
            let item = probe(&report, capability_id);
            assert!(
                !item.passed,
                "{capability_id} passed after its effect fixture was removed"
            );
            assert_eq!(
                item.evidence.verification,
                "effect_dependency_or_receipt_missing"
            );
        }
    }

    #[test]
    fn sandbox_probe_fails_when_credential_is_not_a_reference() {
        let mission = bounded_mission();
        let mut environment = MissionPlatformProbeEnvironment::for_mission(&mission);
        environment
            .fixtures
            .get_mut("agent_config_isolation")
            .unwrap()
            .dependency["credential"] = json!("plaintext-secret");
        let report = simulate_mission_platform_with_environment(&mission, &environment);
        let item = probe(&report, "agent_config_isolation");
        assert!(!item.passed);
        assert!(matches!(
            item.evidence.verification.as_str(),
            "effect_replay_failed" | "receipt_integrity_failed"
        ));
    }

    #[test]
    fn worktree_probe_fails_without_a_registered_binding_receipt() {
        let mission = bounded_mission();
        let environment = MissionPlatformProbeEnvironment::for_mission(&mission);
        let report = simulate_mission_platform_with_environment(&mission, &environment);
        let item = probe(&report, "mission_worktree");
        assert!(!item.passed);
        assert_eq!(
            item.evidence.execution_class,
            MISSION_PLATFORM_BOUNDED_SIMULATION
        );
        assert!(item.evidence.receipt.is_none());
    }

    #[test]
    fn voice_probe_fails_when_required_approval_is_removed() {
        let mission = bounded_mission();
        let mut environment = MissionPlatformProbeEnvironment::for_mission(&mission);
        environment
            .fixtures
            .get_mut("two_level_voice")
            .unwrap()
            .dependency["approved"] = json!(false);
        let report = simulate_mission_platform_with_environment(&mission, &environment);
        let item = probe(&report, "two_level_voice");
        assert!(!item.passed);
        assert!(matches!(
            item.evidence.verification.as_str(),
            "effect_replay_failed" | "receipt_integrity_failed"
        ));
    }

    #[test]
    fn translation_probe_fails_when_transformation_dependency_is_removed() {
        let mission = bounded_mission();
        let mut environment = MissionPlatformProbeEnvironment::for_mission(&mission);
        environment
            .fixtures
            .get_mut("operational_translation")
            .unwrap()
            .dependency["dictionary"] = json!([]);
        let report = simulate_mission_platform_with_environment(&mission, &environment);
        let item = probe(&report, "operational_translation");
        assert!(!item.passed);
        assert!(matches!(
            item.evidence.verification.as_str(),
            "effect_replay_failed" | "receipt_integrity_failed"
        ));
    }

    #[test]
    fn consolidation_probe_fails_when_a_delivery_is_corrupted() {
        let mission = bounded_mission();
        let mut environment = MissionPlatformProbeEnvironment::for_mission(&mission);
        let dependency = &mut environment
            .fixtures
            .get_mut("result_consolidation")
            .unwrap()
            .dependency;
        dependency["deliveries"][0]["status"] = json!("blocked");
        let report = simulate_mission_platform_with_environment(&mission, &environment);
        let item = probe(&report, "result_consolidation");
        assert!(!item.passed);
        assert_ne!(
            item.evidence.input_sha256,
            item.evidence.receipt.as_ref().unwrap().input_sha256
        );
    }

    #[test]
    fn receipt_hash_corruption_is_rejected_even_when_effect_replays() {
        let mission = bounded_mission();
        let mut environment = MissionPlatformProbeEnvironment::for_mission(&mission);
        environment
            .fixtures
            .get_mut("agent_aware_scheduling")
            .unwrap()
            .receipt
            .result_sha256 = "0".repeat(64);
        let report = simulate_mission_platform_with_environment(&mission, &environment);
        let item = probe(&report, "agent_aware_scheduling");
        assert!(!item.passed);
        assert_eq!(item.evidence.verification, "receipt_integrity_failed");
    }
}
