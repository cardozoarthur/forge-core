use crate::addon::{
    addon_observability_report, builtin_addon_catalog, default_addon_dirs,
    list_addon_capability_index, list_addon_event_adapters, list_addon_permission_authorizations,
    list_addon_views, load_addon_catalog_from_store, AddonCatalog, AddonEventExtensionRegistry,
    AddonManifest, AddonViewAction, AddonViewEntry, ADDON_EVENT_EXTENSIONS_SCHEMA_VERSION,
    CAP_MULTIMODAL_RUNTIME, CAP_SOURCE_CODE_PATCH_LIFECYCLE,
};
use crate::artifact::list_workflow_artifacts;
use crate::checkpoint::TaskCheckpoint;
use crate::context::{build_context_package_with_checkpoint_and_project, DEFAULT_CONTEXT_BUDGET};
use crate::cost::{build_cost_ledger, CostLedgerReport};
use crate::event::{
    build_global_event_timeline, list_event_services, list_inbound_event_inbox_for_context,
    GlobalEventTimelineReport, WorkflowEventEnvelope,
};
use crate::executor::{
    build_brain_sessions_report_with_options, load_executors, BrainSessionOperationPlan,
    BrainSessionState, BrainSessionsReport, BrainSessionsReportOptions,
};
use crate::graph::{AtomicTask, ExecutorKind, TaskStatus};
use crate::harness::{
    analyze_token_headroom, build_harness_adoption_plan, build_harness_bootstrap_report,
    build_harness_doctor_report, build_harness_executor_compatibility_report,
    build_harness_headroom_plan, build_harness_mode_report, build_headroom_stats_report,
    inspect_cli_harness_shim_status, install_cli_harness_shim, persist_token_headroom_report,
    resolve_harness_forge_first_source_for_project, resolve_harness_runtime_policy,
    run_cli_harness_exec, CliHarnessExecOptions, CliShimInstallOptions, CliShimInstallReport,
    CliShimStatusOptions, CliShimStatusReport, CliWrapperPlanReport, HarnessAdoptionPlanOptions,
    HarnessAdoptionPlanReport, HarnessBootstrapOptions, HarnessBootstrapReport,
    HarnessDoctorOptions, HarnessDoctorReport, HarnessExecutorCompatibilityReport,
    HarnessHeadroomPlanOptions, HarnessHeadroomPlanReport, HarnessModeOptions, HarnessModeReport,
    HarnessRuntimePolicyOptions, HarnessSessionLifecyclePlan, HeadroomStatsContentKindBucket,
    HeadroomStatsOptions, HeadroomStatsReport, HeadroomStatsSourceBucket, TokenHeadroomReport,
};
use crate::identity::{
    audit_tenant_index, inspect_project_operating_context, list_identity_links,
    list_identity_memberships, list_identity_registry, load_project_operating_context,
};
use crate::improve::{
    rank_improvement_candidates, OrchestratorImprovementCandidate,
    OrchestratorImprovementCandidatesReport,
};
use crate::intent::OperatingContextSpec;
use crate::interaction::{
    create_choice_interaction, list_human_interactions, CreateChoiceInteractionRequest,
};
use crate::memory::{
    memory_policy_report_for_project, MemoryEffectiveDefaults, MemoryPolicyReport,
};
use crate::milestone::{
    build_milestone_evidence_plan, build_milestone_manifest_with_store, build_milestone_status,
    collect_ready_milestone_evidence, milestone_required_attached_evidence_kinds,
    MilestoneAttachedEvidence, MilestoneCollectReadyEvidenceOptions,
    MilestoneCollectReadyEvidenceReport, MilestoneEvidencePlanConfigCheck,
    MilestoneEvidencePlanManifestTemplate, MilestoneEvidencePlanOptions,
    MilestoneEvidenceProviderCandidate, MilestonePromotionDecision, MilestonePromotionGateTemplate,
    MilestoneStatusSummary,
};
use crate::multimodal::{
    build_multimodal_benchmark_template, build_multimodal_demo_plan, build_multimodal_install_plan,
    build_multimodal_readiness, build_multimodal_status_with_feature_flag,
    evaluate_multimodal_guard, resolve_multimodal_feature_flag, MultimodalReadinessOptions,
};
use crate::ops::{
    build_addon_view_renderer_report, build_operational_digital_twin, create_modifier_proposal,
    load_modifier_lane, OpsAddonViewRendererReport, OpsModifierLane, OpsModifierProposal,
    OpsModifierProposalInput, OpsOperationalDigitalTwin,
};
use crate::registry::{
    list_workflows_with_filters, RegistryContextActionRef, RegistryContextActionSummary,
    RegistryContextQualitySummary, WorkflowLifecycleFilter, WorkflowRegistryFilters,
    WorkflowRegistryRow,
};
use crate::request::start_async_request;
use crate::runtime::load_runtimes;
use crate::schedule::{
    build_schedule_worker_status, create_daily_goal_research_workflow, ScheduleWorkerStatusReport,
};
use crate::storage::{ForgeStore, GlobalEventWrite, StoreEvent};
use crate::workflow::{record_product_decision, ProductDecisionInput};
use anyhow::Result;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const INTERACTIVE_HOME_SCHEMA_VERSION: &str = "forge.interactive.home.v1";
const INTERACTIVE_WORKFLOW_SIDEBAR_SCHEMA_VERSION: &str = "forge.interactive.workflow_sidebar.v1";
const INTERACTIVE_REPLACEMENT_CLI_SCHEMA_VERSION: &str = "forge.interactive.replacement_cli.v1";
const INTERACTIVE_MULTIMODAL_RUNTIME_SCHEMA_VERSION: &str =
    "forge.interactive.multimodal_runtime.v1";
const INTERACTIVE_TASK_BOARD_SCHEMA_VERSION: &str = "forge.interactive.task_board.v1";
const INTERACTIVE_ARTIFACTS_SCHEMA_VERSION: &str = "forge.interactive.artifacts.v1";
const INTERACTIVE_WORKFLOW_DAG_SCHEMA_VERSION: &str = "forge.interactive.workflow_dag.v1";
const INTERACTIVE_SCHEDULES_SCHEMA_VERSION: &str = "forge.interactive.schedules.v1";
const INTERACTIVE_CONTEXT_MEMORY_SCHEMA_VERSION: &str = "forge.interactive.context_memory.v1";
const INTERACTIVE_OPERATING_CONTEXT_SCHEMA_VERSION: &str = "forge.interactive.operating_context.v1";
const INTERACTIVE_IMPROVEMENT_LOOP_SCHEMA_VERSION: &str = "forge.interactive.improvement_loop.v1";
const INTERACTIVE_READINESS_SCHEMA_VERSION: &str = "forge.interactive.readiness.v1";
const INTERACTIVE_RELEASE_GATES_SCHEMA_VERSION: &str = "forge.interactive.release_gates.v1";
const INTERACTIVE_HARNESS_SCHEMA_VERSION: &str = "forge.interactive.harness.v1";
const INTERACTIVE_TOKEN_USAGE_SCHEMA_VERSION: &str = "forge.interactive.token_usage.v1";
const INTERACTIVE_SESSIONS_SCHEMA_VERSION: &str = "forge.interactive.sessions.v1";
const INTERACTIVE_COMMAND_PALETTE_SCHEMA_VERSION: &str = "forge.interactive.command_palette.v1";
const INTERACTIVE_COMMAND_PALETTE_ACTION_PLAN_SCHEMA_VERSION: &str =
    "forge.interactive.command_palette_action_plan.v1";
const INTERACTIVE_ACTION_REGISTRY_SCHEMA_VERSION: &str = "forge.interactive.action_registry.v1";
const INTERACTIVE_ACTION_INVOCATION_SCHEMA_VERSION: &str = "forge.interactive.action_invocation.v1";
const INTERACTIVE_AUTOCOMPLETE_SCHEMA_VERSION: &str = "forge.interactive.autocomplete.v1";
const INTERACTIVE_PATCH_WORKBENCH_SCHEMA_VERSION: &str = "forge.interactive.patch_workbench.v1";
const INTERACTIVE_ADDON_ACTION_CONTRACT_SCHEMA_VERSION: &str =
    "forge.interactive.addon_action_contract.v1";
const INTERACTIVE_PATCH_ADDON_CONTRACT_SCHEMA_VERSION: &str =
    "forge.interactive.patch_addon_contract.v1";
const INTERACTIVE_PATCH_EDIT_INTAKE_SCHEMA_VERSION: &str = "forge.interactive.patch_edit_intake.v1";
const INTERACTIVE_PATCH_FILE_ACTION_HINT_SCHEMA_VERSION: &str =
    "forge.interactive.patch_file_action_hint.v1";
const INTERACTIVE_PERMISSIONS_SCHEMA_VERSION: &str = "forge.interactive.permissions.v1";
const INTERACTIVE_IDENTITY_SCHEMA_VERSION: &str = "forge.interactive.identity.v1";
const INTERACTIVE_NAVIGATION_SCHEMA_VERSION: &str = "forge.interactive.navigation.v1";
const INTERACTIVE_OPERATIONAL_COCKPIT_SCHEMA_VERSION: &str =
    "forge.interactive.operational_cockpit.v1";
const INTERACTIVE_OPERATIONAL_MODIFIER_LANE_SCHEMA_VERSION: &str =
    "forge.interactive.operational_modifier_lane.v1";
const INTERACTIVE_WORKFLOW_MUTATION_SCHEMA_VERSION: &str = "forge.interactive.workflow_mutation.v1";
const INTERACTIVE_GUIDED_COCKPIT_SCHEMA_VERSION: &str = "forge.interactive.guided_cockpit.v1";
const INTERACTIVE_ADDON_CAPABILITY_SCHEMA_VERSION: &str = "forge.interactive.addon_capability.v1";
const INTERACTIVE_CORE_BOUNDARY_SCHEMA_VERSION: &str = "forge.interactive.core_boundary.v1";
const INTERACTIVE_ARCHITECTURE_COMPASS_SCHEMA_VERSION: &str =
    "forge.interactive.architecture_compass.v1";
const OPERATIONAL_TUI_SMOKE_SCHEMA_VERSION: &str = "forge.smoke.operational_tui.v1";
const FORGE_FIRST_HARNESS_SMOKE_SCHEMA_VERSION: &str = "forge.smoke.forge_first_harness.v1";
const REPLACEMENT_CLI_EVIDENCE_SMOKE_SCHEMA_VERSION: &str =
    "forge.smoke.replacement_cli_evidence.v1";
const INTERACTIVE_UI_COMPOSITION_SCHEMA_VERSION: &str = "forge.interactive.ui_composition.v1";
const INTERACTIVE_STRUCTURED_LOGS_SCHEMA_VERSION: &str = "forge.interactive.structured_logs.v1";
const INTERACTIVE_EVENT_RUNTIME_SCHEMA_VERSION: &str = "forge.interactive.event_runtime.v1";
const INTERACTIVE_EVENT_WORKFLOW_LIFECYCLE_SCHEMA_VERSION: &str =
    "forge.interactive.event_workflow_lifecycle.v1";
const SLASH_COMMANDS_SCHEMA_VERSION: &str = "forge.interactive.slash_commands.v1";
const INTERACTIVE_ROUTE_SCHEMA_VERSION: &str = "forge.interactive.route.v1";

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveHomeReport {
    pub status: String,
    pub schema_version: String,
    pub banner: InteractiveBanner,
    pub dashboard: InteractiveDashboard,
    pub slash_commands: Vec<SlashCommandSpec>,
}

#[derive(Debug, Clone, Default)]
pub struct InteractiveHomeOptions {
    pub project_root: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveBanner {
    pub mark: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveDashboard {
    pub active_runs: usize,
    pub active_run_ids: Vec<String>,
    pub runs_needing_attention: usize,
    pub scheduled_workflows: usize,
    pub looping_workflows: usize,
    pub paused_idle_workflows: usize,
    pub recent_artifacts: usize,
    pub product_decisions: usize,
    pub pending_approvals: usize,
    pub validation_failures: usize,
    pub executor_availability: String,
    pub brain_router: String,
    pub forge_controlled_surfaces: Vec<String>,
    pub shell_entrypoints: Vec<String>,
    pub harness_panel: InteractiveHarnessPanel,
    pub token_usage_panel: InteractiveTokenUsagePanel,
    pub sessions_panel: InteractiveSessionsPanel,
    pub command_palette_panel: InteractiveCommandPalettePanel,
    pub action_registry_panel: InteractiveActionRegistryPanel,
    pub autocomplete_panel: InteractiveAutocompletePanel,
    pub release_gates_panel: InteractiveReleaseGatesPanel,
    pub harness_mode_panel: HarnessModeReport,
    pub harness_doctor_panel: HarnessDoctorReport,
    pub runtime_node_status: String,
    pub repository_context: String,
    pub estimated_costs: String,
    pub scheduler_worker_status: String,
    pub workflow_focus: Vec<InteractiveWorkflowCard>,
    pub workflow_sidebar_panel: InteractiveWorkflowSidebarPanel,
    pub replacement_cli_panel: InteractiveReplacementCliPanel,
    pub multimodal_runtime_panel: InteractiveMultimodalRuntimePanel,
    pub navigation_panel: InteractiveNavigationPanel,
    pub ui_composition_panel: InteractiveUiCompositionPanel,
    pub patch_workbench_panel: InteractivePatchWorkbenchPanel,
    pub permissions_panel: InteractivePermissionsPanel,
    pub identity_panel: InteractiveIdentityPanel,
    pub dag_panel: InteractiveWorkflowDagPanel,
    pub task_board_panel: InteractiveTaskBoardPanel,
    pub workflow_mutation_panel: InteractiveWorkflowMutationPanel,
    pub guided_cockpit_panel: InteractiveGuidedCockpitPanel,
    pub artifact_panel: InteractiveArtifactPanel,
    pub schedule_panel: InteractiveSchedulePanel,
    pub event_panel: InteractiveEventPanel,
    pub event_runtime_panel: InteractiveEventRuntimePanel,
    pub structured_logs_panel: InteractiveStructuredLogsPanel,
    pub cost_panel: InteractiveCostPanel,
    pub improvement_loop_panel: InteractiveImprovementLoopPanel,
    pub context_memory_panel: InteractiveContextMemoryPanel,
    pub operating_context_panel: InteractiveOperatingContextPanel,
    pub digital_twin_panel: OpsOperationalDigitalTwin,
    pub operational_cockpit_panel: InteractiveOperationalCockpitPanel,
    pub architecture_compass_panel: InteractiveArchitectureCompassPanel,
    pub core_boundary_panel: InteractiveCoreBoundaryPanel,
    pub addon_capability_panel: InteractiveAddonCapabilityPanel,
    pub addon_renderer_panel: InteractiveAddonRendererPanel,
    pub attention_actions: Vec<String>,
    pub useful_next_commands: Vec<String>,
    pub quick_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveGuidedCockpitPanel {
    pub schema_version: String,
    pub status: String,
    pub title: String,
    pub visual_mode: String,
    pub completed_step_count: usize,
    pub total_step_count: usize,
    pub blocked_step_count: usize,
    pub confirmation_step_count: usize,
    pub current_step_id: String,
    pub layout_panes: Vec<InteractiveGuidedCockpitPane>,
    pub steps: Vec<InteractiveGuidedCockpitStep>,
    pub safe_action_policy: Vec<String>,
    pub next_command: String,
    pub next_commands: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveGuidedCockpitPane {
    pub pane_id: String,
    pub title: String,
    pub role: String,
    pub source_panel: String,
    pub focus_key: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveGuidedCockpitStep {
    pub step_id: String,
    pub order: usize,
    pub title: String,
    pub status: String,
    pub evidence: String,
    pub primary_panel: String,
    pub primary_command: String,
    pub preview_command: String,
    pub risk_level: String,
    pub requires_confirmation: bool,
    pub mutates_workflow: bool,
    pub can_apply_now: bool,
    pub rollback_command: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveArchitectureCompassPanel {
    pub schema_version: String,
    pub status: String,
    pub source_documents: Vec<InteractiveArchitectureSourceDocument>,
    pub operating_context: InteractiveArchitectureOperatingContextSummary,
    pub tracks: Vec<InteractiveArchitectureTrack>,
    pub benchmark_sources: Vec<InteractiveArchitectureBenchmarkSource>,
    pub execution_plan: InteractiveArchitectureExecutionPlan,
    pub dependencies: Vec<InteractiveArchitectureDependency>,
    pub conflicts: Vec<String>,
    pub reuse_opportunities: Vec<String>,
    pub next_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveArchitectureOperatingContextSummary {
    pub schema_version: String,
    pub project_root: String,
    pub status: String,
    pub context_status: String,
    pub tenant_path: String,
    pub organization_id: String,
    pub organization_label: String,
    pub brand_id: String,
    pub brand_label: String,
    pub product_id: String,
    pub product_label: String,
    pub user_id: String,
    pub channel_id: String,
    pub memory_scope: String,
    pub memory_level: String,
    pub memory_scopes: Vec<String>,
    pub memory_audience: String,
    pub personality_scope: String,
    pub personality_status: String,
    pub brand_voice: String,
    pub brand_tone: String,
    pub design_token_source: String,
    pub component_source: String,
    pub prompt_packet_gates: Vec<String>,
    pub company_work_departments: Vec<String>,
    pub tenant_policy_status: String,
    pub memory_policy_status: String,
    pub evidence_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveArchitectureSourceDocument {
    pub document_id: String,
    pub role: String,
    pub line_count: usize,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveArchitectureTrack {
    pub track_id: String,
    pub title: String,
    pub source_refs: Vec<String>,
    pub status: String,
    pub evidence_refs: Vec<String>,
    pub gaps: Vec<String>,
    pub next_increment: String,
    pub core_boundary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveArchitectureBenchmarkSource {
    pub source: String,
    pub absorbed_concept: String,
    pub forge_boundary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveArchitectureDependency {
    pub item: String,
    pub depends_on: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveArchitectureExecutionPlan {
    pub schema_version: String,
    pub status: String,
    pub strategy: String,
    pub selection_rule: String,
    pub increments: Vec<InteractiveArchitectureExecutionIncrement>,
    pub acceptance_policy: String,
    pub next_command: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveArchitectureExecutionIncrement {
    pub increment_id: String,
    pub priority: usize,
    pub title: String,
    pub status: String,
    pub source_refs: Vec<String>,
    pub depends_on: Vec<String>,
    pub unlocks: Vec<String>,
    pub core_boundary: String,
    pub addon_boundary: String,
    pub acceptance_gates: Vec<String>,
    pub evidence_commands: Vec<String>,
    pub risk_controls: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveOperationalCockpitPanel {
    pub schema_version: String,
    pub status: String,
    pub attention_level: String,
    pub priority_summary: String,
    pub active_work_count: usize,
    pub needs_attention_count: usize,
    pub ready_handoff_count: usize,
    pub pending_human_wait_count: usize,
    pub pending_approval_count: usize,
    pub pending_modifier_proposal_count: usize,
    pub pending_event_count: usize,
    pub validation_failure_count: usize,
    pub due_workflow_count: usize,
    pub selected_brain: String,
    pub ready_session_count: usize,
    pub forge_first_ready: bool,
    pub headroom_operational_status: String,
    pub event_count: usize,
    pub estimated_cost_total_usd: f64,
    pub sections: Vec<InteractiveOperationalCockpitSection>,
    pub modifier_lane: InteractiveOperationalModifierLanePanel,
    pub event_runtime: InteractiveEventRuntimePanel,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveOperationalCockpitSection {
    pub section_id: String,
    pub title: String,
    pub status: String,
    pub signal_count: usize,
    pub summary: String,
    pub primary_command: String,
    pub secondary_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveOperationalModifierLanePanel {
    pub schema_version: String,
    pub status: String,
    pub operation_mode: String,
    pub purpose: String,
    pub pending_count: usize,
    pub applied_count: usize,
    pub proposal_cards: Vec<InteractiveOperationalModifierProposalCard>,
    pub commands: InteractiveOperationalModifierLaneCommands,
    pub next_actions: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveOperationalModifierProposalCard {
    pub proposal_id: String,
    pub workflow_id: String,
    pub target_kind: String,
    pub task_id: Option<String>,
    pub title: String,
    pub summary: String,
    pub rationale: String,
    pub author: String,
    pub status: String,
    pub created_at: String,
    pub applied_at: Option<String>,
    pub applied_revision: Option<u64>,
    pub apply_route: String,
    pub inspect_command: Vec<String>,
    pub apply_payload_hint: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveOperationalModifierLaneCommands {
    pub serve_console: Vec<String>,
    pub refresh_cockpit: Vec<String>,
    pub snapshot_route: String,
    pub propose_goal_route: String,
    pub propose_task_route: String,
    pub apply_proposal_route: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveEventRuntimePanel {
    pub schema_version: String,
    pub status: String,
    pub project_root: String,
    pub pending_event_count: usize,
    pub sampled_event_count: usize,
    pub service_count: usize,
    pub running_service_count: usize,
    pub persistent_workflow_count: usize,
    pub wakeable_workflow_count: usize,
    pub action_required: bool,
    pub recommended_action: String,
    pub recommendation_reason: String,
    pub workflow_lifecycle: InteractiveEventWorkflowLifecyclePanel,
    pub event_cards: Vec<InteractiveEventRuntimeEventCard>,
    pub service_cards: Vec<InteractiveEventRuntimeServiceCard>,
    pub commands: InteractiveEventRuntimeCommands,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveEventRuntimeEventCard {
    pub event_id: String,
    pub origin: String,
    pub action: String,
    pub status: String,
    pub workflow_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveEventRuntimeServiceCard {
    pub service_id: String,
    pub service_kind: String,
    pub status: String,
    pub lease_owner: String,
    pub lease_expires_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveEventRuntimeCommands {
    pub inbox: Vec<String>,
    pub runtime_reconcile: Vec<String>,
    pub service_supervise: Vec<String>,
    pub webhook_ingress: Vec<String>,
    pub services: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveEventWorkflowLifecyclePanel {
    pub schema_version: String,
    pub status: String,
    pub action_count: usize,
    pub validated_action_count: usize,
    pub needs_attention_count: usize,
    pub core_owned_actions: Vec<String>,
    pub addon_owned_channels: Vec<String>,
    pub actions: Vec<InteractiveEventWorkflowLifecycleAction>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveEventWorkflowLifecycleAction {
    pub action: String,
    pub normalized_route: String,
    pub status: String,
    pub purpose: String,
    pub required_payload_fields: Vec<String>,
    pub core_boundary: String,
    pub addon_boundary: String,
    pub primary_command: Vec<String>,
    pub evidence_commands: Vec<String>,
    pub acceptance_gates: Vec<String>,
    pub risk_controls: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveAddonCapabilityPanel {
    pub schema_version: String,
    pub status: String,
    pub project_root: String,
    pub addon_count: usize,
    pub enabled_addon_count: usize,
    pub unauthorized_addon_count: usize,
    pub capability_count: usize,
    pub enabled_capability_count: usize,
    pub disabled_capability_count: usize,
    pub permission_count: usize,
    pub runtime_contract_count: usize,
    pub view_count: usize,
    pub dispatch_count: usize,
    pub queued_dispatch_count: usize,
    pub event_type_count: usize,
    pub event_channel_count: usize,
    pub event_trigger_count: usize,
    pub event_listener_count: usize,
    pub event_adapter_count: usize,
    pub event_extensions: Vec<String>,
    pub event_extension_registry: AddonEventExtensionRegistry,
    pub capabilities: Vec<String>,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveCoreBoundaryPanel {
    pub schema_version: String,
    pub status: String,
    pub project_root: String,
    pub core_addon_id: String,
    pub core_capability_count: usize,
    pub addon_count: usize,
    pub domain_addon_count: usize,
    pub addon_owned_capability_count: usize,
    pub domain_specific_core_leak_count: usize,
    pub compatibility_boundary_count: usize,
    pub core_allowed_responsibilities: Vec<String>,
    pub core_kernel_capabilities: Vec<InteractiveCoreCapabilityBoundary>,
    pub addon_boundaries: Vec<InteractiveAddonBoundaryCard>,
    pub compatibility_boundaries: Vec<InteractiveCompatibilityBoundary>,
    pub acceptance_gates: Vec<InteractiveCoreBoundaryGate>,
    pub notes: Vec<String>,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveCoreCapabilityBoundary {
    pub capability_id: String,
    pub title: String,
    pub domains: Vec<String>,
    pub boundary_status: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveAddonBoundaryCard {
    pub addon_id: String,
    pub source: String,
    pub lifecycle: String,
    pub capability_count: usize,
    pub runtime_contract_count: usize,
    pub view_count: usize,
    pub domains: Vec<String>,
    pub sample_capabilities: Vec<String>,
    pub boundary_summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveCompatibilityBoundary {
    pub addon_id: String,
    pub contract_id: String,
    pub capability_id: String,
    pub compatibility_executor: String,
    pub target_boundary: String,
    pub migration_state: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveCoreBoundaryGate {
    pub gate_id: String,
    pub title: String,
    pub passed: bool,
    pub evidence: String,
    pub evidence_command: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveWorkflowCard {
    pub workflow_id: String,
    pub goal: String,
    pub lifecycle_state: String,
    pub operator_action: String,
    pub context_action: String,
    pub quality_action: String,
    pub tasks: String,
    pub schedule: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveNavigationPanel {
    pub schema_version: String,
    pub status: String,
    pub default_display_mode: String,
    pub display_modes: Vec<String>,
    pub active_theme: String,
    pub themes: Vec<String>,
    pub keybindings: Vec<InteractiveKeyBinding>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveKeyBinding {
    pub key: String,
    pub action: String,
    pub target: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveUiCompositionPanel {
    pub schema_version: String,
    pub status: String,
    pub layout_kind: String,
    pub region_count: usize,
    pub widget_count: usize,
    pub core_widget_count: usize,
    pub addon_widget_count: usize,
    pub addon_renderer_families: Vec<String>,
    pub regions: Vec<InteractiveUiRegion>,
    pub commands: InteractiveUiCompositionCommands,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveUiRegion {
    pub region_id: String,
    pub title: String,
    pub role: String,
    pub order: i64,
    pub widget_count: usize,
    pub widgets: Vec<InteractiveUiWidget>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveUiWidget {
    pub widget_id: String,
    pub title: String,
    pub source: String,
    pub panel: String,
    pub renderer_family: String,
    pub safe_renderer: bool,
    pub layout_density: String,
    pub layout_width: String,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveUiCompositionCommands {
    pub refresh: Vec<String>,
    pub inspect_addons: Vec<String>,
    pub open_task_board: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveWorkflowSidebarPanel {
    pub schema_version: String,
    pub status: String,
    pub workflow_count: usize,
    pub group_count: usize,
    pub selected_workflow_id: String,
    pub selected_group_id: String,
    pub selected_index: usize,
    pub active_count: usize,
    pub attention_count: usize,
    pub event_driven_count: usize,
    pub scheduled_count: usize,
    pub completed_count: usize,
    pub groups: Vec<InteractiveWorkflowSidebarGroup>,
    pub keyboard_hints: Vec<String>,
    pub commands: InteractiveWorkflowSidebarCommands,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveWorkflowSidebarGroup {
    pub group_id: String,
    pub title: String,
    pub item_count: usize,
    pub items: Vec<InteractiveWorkflowSidebarItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveWorkflowSidebarItem {
    pub workflow_id: String,
    pub selected: bool,
    pub title: String,
    pub lifecycle_state: String,
    pub current_goal: String,
    pub active_run_count: usize,
    pub ready_handoff_count: usize,
    pub pending_human_interaction_count: usize,
    pub due_schedule_count: usize,
    pub artifact_count: usize,
    pub runtime: crate::registry::RegistryWorkflowRuntimeState,
    pub schedule_summary: crate::schedule::ScheduleSummary,
    pub commands: InteractiveWorkflowSidebarItemCommands,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveWorkflowSidebarItemCommands {
    pub inspect: Vec<String>,
    pub task_board: Vec<String>,
    pub workflow_dag: Vec<String>,
    pub events: Vec<String>,
    pub validate: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveWorkflowSidebarCommands {
    pub refresh: Vec<String>,
    pub list: Vec<String>,
    pub task_board: Vec<String>,
    pub workflow_dag: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveReplacementCliPanel {
    pub schema_version: String,
    pub status: String,
    pub project_root: String,
    pub milestone: String,
    pub capability_id: String,
    pub promotion_ready: bool,
    pub required_attached_evidence_kinds: Vec<String>,
    pub attached_evidence_kinds: Vec<String>,
    pub missing_attached_evidence_kinds: Vec<String>,
    pub surface_count: usize,
    pub ready_surface_count: usize,
    pub blocked_surface_count: usize,
    pub readiness_percent: u64,
    pub surfaces: Vec<InteractiveReplacementCliSurface>,
    pub external_brain_evidence_plan: InteractiveReleaseGateEvidencePlan,
    pub provider_readiness_count: usize,
    pub installed_provider_count: usize,
    pub wrapper_required_provider_count: usize,
    pub provider_readiness: Vec<InteractiveReplacementCliProviderReadiness>,
    pub provider_wrapper_plan_count: usize,
    pub provider_wrapper_plans: Vec<InteractiveReplacementCliProviderWrapperPlan>,
    pub provider_wrapper_manifest_audit: InteractiveReplacementCliProviderWrapperManifestAudit,
    pub blockers: Vec<String>,
    pub next_actions: Vec<String>,
    pub commands: InteractiveReplacementCliCommands,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveReplacementCliSurface {
    pub surface_id: String,
    pub title: String,
    pub status: String,
    pub ready: bool,
    pub source_panels: Vec<String>,
    pub evidence: Vec<String>,
    pub blockers: Vec<String>,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveReplacementCliProviderReadiness {
    pub provider_id: String,
    pub brain_id: String,
    pub binary: String,
    pub installed: bool,
    pub detected_path: Option<String>,
    pub readiness: String,
    pub version_status: String,
    pub wrapper_required: bool,
    pub required_output_schema: String,
    pub manifest_provider_template: serde_json::Value,
    pub evidence_blocker: String,
    pub next_action: String,
    pub collect_evidence_command: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveReplacementCliProviderWrapperPlan {
    pub schema_version: String,
    pub provider_id: String,
    pub brain_id: String,
    pub binary: String,
    pub installed: bool,
    pub detected_path: Option<String>,
    pub readiness: String,
    pub wrapper_required: bool,
    pub wrapper_manifest_path: String,
    pub required_output_schema: String,
    pub manifest_provider_template: serde_json::Value,
    pub recommended_wrapper_command: Vec<String>,
    pub evidence_plan_command: Vec<String>,
    pub prepare_evidence_inputs_command: Vec<String>,
    pub collect_evidence_command: Vec<String>,
    pub counts_as_release_evidence: bool,
    pub model_execution_allowed: bool,
    pub mutates_project: bool,
    pub safety_requirements: Vec<String>,
    pub next_action: String,
    pub promotion_impact: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveReplacementCliProviderWrapperManifestAudit {
    pub schema_version: String,
    pub status: String,
    pub manifest_path: String,
    pub manifest_present: bool,
    pub manifest_parseable: bool,
    pub provider_count: usize,
    pub selected_provider_id: Option<String>,
    pub selected_brain_id: Option<String>,
    pub capability_declared: bool,
    pub command_declared: bool,
    pub command_placeholders_absent: bool,
    pub command_first_binary: Option<String>,
    pub command_path_status: String,
    pub command_executable: bool,
    pub approval_ready: bool,
    pub model_ready: bool,
    pub allow_model_execution: bool,
    pub network_access_blocked: bool,
    pub device_access_blocked: bool,
    pub external_resources_untouched: bool,
    pub evidence_plan_ready: bool,
    pub ready_to_collect_evidence: bool,
    pub counts_as_release_evidence: bool,
    pub model_execution_performed: bool,
    pub blockers: Vec<String>,
    pub safety_requirements: Vec<String>,
    pub evidence_plan_command: Vec<String>,
    pub prepare_evidence_inputs_command: Vec<String>,
    pub collect_evidence_command: Vec<String>,
    pub next_action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveReplacementCliCommands {
    pub refresh: Vec<String>,
    pub home: Vec<String>,
    pub command_palette: Vec<String>,
    pub action_registry: Vec<String>,
    pub autocomplete: Vec<String>,
    pub patch_workbench: Vec<String>,
    pub harness: Vec<String>,
    pub sessions: Vec<String>,
    pub release_gates: Vec<String>,
    pub cli_demo: Vec<String>,
    pub evidence_plan: Vec<String>,
    pub prepare_evidence_inputs: Vec<String>,
    pub collect_external_brain_evidence: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct InteractiveReplacementCliOptions {
    pub project_root: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveMultimodalRuntimePanel {
    pub schema_version: String,
    pub status: String,
    pub project_root: String,
    pub capability_id: String,
    pub addon_id: String,
    pub addon_view_id: String,
    pub feature_flag_enabled: bool,
    pub feature_flag_source: String,
    pub feature_flag_status: String,
    pub promotion_ready: bool,
    pub required_attached_evidence_kinds: Vec<String>,
    pub attached_evidence_kinds: Vec<String>,
    pub missing_attached_evidence_kinds: Vec<String>,
    pub evidence_plan_status: String,
    pub ready_to_collect_evidence: bool,
    pub missing_config_check_count: usize,
    pub config_checks: Vec<MilestoneEvidencePlanConfigCheck>,
    pub manifest_template_ids: Vec<String>,
    pub production_runtime_evidence_plan: InteractiveReleaseGateEvidencePlan,
    pub installs_performed: bool,
    pub model_execution_performed: bool,
    pub device_access_performed: bool,
    pub network_access_performed: bool,
    pub capability_count: usize,
    pub available_count: usize,
    pub missing_count: usize,
    pub guard_status: String,
    pub guard_allowed: bool,
    pub surface_count: usize,
    pub ready_surface_count: usize,
    pub blocked_surface_count: usize,
    pub readiness_percent: u64,
    pub surfaces: Vec<InteractiveMultimodalRuntimeSurface>,
    pub blockers: Vec<String>,
    pub next_actions: Vec<String>,
    pub commands: InteractiveMultimodalRuntimeCommands,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveMultimodalRuntimeSurface {
    pub surface_id: String,
    pub title: String,
    pub status: String,
    pub ready: bool,
    pub source_panels: Vec<String>,
    pub evidence: Vec<String>,
    pub blockers: Vec<String>,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveMultimodalRuntimeCommands {
    pub refresh: Vec<String>,
    pub status: Vec<String>,
    pub readiness: Vec<String>,
    pub install_plan: Vec<String>,
    pub benchmark_template: Vec<String>,
    pub runtime_benchmark: Vec<String>,
    pub demo_plan: Vec<String>,
    pub guard: Vec<String>,
    pub evidence_plan: Vec<String>,
    pub collect_evidence: Vec<String>,
    pub addon_capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveReadinessPanel {
    pub schema_version: String,
    pub status: String,
    pub executor_count: usize,
    pub usable_executor_count: usize,
    pub usable_executors: Vec<String>,
    pub needs_executor_approval: bool,
    pub runtime_count: usize,
    pub usable_runtime_count: usize,
    pub usable_runtimes: Vec<String>,
    pub needs_runtime_approval: bool,
    pub brain_count: usize,
    pub selected_brain: String,
    pub forge_controlled_surface_count: usize,
    pub forge_controlled_surfaces: Vec<String>,
    pub shell_count: usize,
    pub forge_first_shell_count: usize,
    pub shell_entrypoints: Vec<String>,
    pub harness_mode: HarnessModeReport,
    pub harness_doctor: HarnessDoctorReport,
    pub harness_adoption_plan: HarnessAdoptionPlanReport,
    pub headroom_stats: HeadroomStatsReport,
    pub headroom_operational_status: String,
    pub headroom_recommended_action: String,
    pub next_actions: Vec<String>,
    pub commands: InteractiveReadinessCommands,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveReadinessCommands {
    pub sync: Vec<String>,
    pub brains: Vec<String>,
    pub sessions: Vec<String>,
    pub shells: Vec<String>,
    pub harness_mode: Vec<String>,
    pub harness_doctor: Vec<String>,
    pub harness_adoption_plan: Vec<String>,
    pub bootstrap_project_harness: Vec<String>,
    pub headroom_plan: Vec<String>,
    pub headroom_stats: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveReleaseGatesPanel {
    pub schema_version: String,
    pub status: String,
    pub milestone: String,
    pub promotion_ready: bool,
    pub promotion_decision: MilestonePromotionDecision,
    pub blocked_by: Vec<String>,
    pub summary: MilestoneStatusSummary,
    pub gate_count: usize,
    pub blocked_gate_count: usize,
    pub attached_evidence_count: usize,
    pub attached_evidence: Vec<MilestoneAttachedEvidence>,
    pub gate_cards: Vec<InteractiveReleaseGateCard>,
    pub commands: InteractiveReleaseGateCommands,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveReleaseGateCard {
    pub capability_id: String,
    pub title: String,
    pub status: String,
    pub promotion_ready: bool,
    pub required_evidence: String,
    pub evidence: String,
    pub attached_evidence_state: String,
    pub required_attached_evidence_kinds: Vec<String>,
    pub attached_evidence_kinds: Vec<String>,
    pub missing_attached_evidence_kinds: Vec<String>,
    pub attached_evidence_count: usize,
    pub attached_evidence: Vec<MilestoneAttachedEvidence>,
    pub evidence_plan: InteractiveReleaseGateEvidencePlan,
    pub gap_before_promotion: String,
    pub next_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveReleaseGateEvidencePlan {
    pub schema_version: String,
    pub status: String,
    pub ready_to_collect_evidence: bool,
    pub project_root: Option<String>,
    pub config_check_count: usize,
    pub missing_config_check_count: usize,
    pub config_checks: Vec<MilestoneEvidencePlanConfigCheck>,
    pub manifest_template_count: usize,
    pub manifest_template_ids: Vec<String>,
    pub manifest_template_paths: Vec<String>,
    pub manifest_templates: Vec<MilestoneEvidencePlanManifestTemplate>,
    pub provider_candidate_count: usize,
    pub provider_candidates: Vec<MilestoneEvidenceProviderCandidate>,
    pub promotion_gate_templates: Vec<MilestonePromotionGateTemplate>,
    pub evidence_collection_commands: Vec<String>,
    pub next_action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveReleaseGateCommands {
    pub refresh: Vec<String>,
    pub status: Vec<String>,
    pub manifest: Vec<String>,
    pub cli_demo: Vec<String>,
    pub multimodal_readiness: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveCommandPalettePanel {
    pub schema_version: String,
    pub status: String,
    pub query: String,
    pub group_count: usize,
    pub entry_count: usize,
    pub groups: Vec<InteractiveCommandPaletteGroup>,
    pub entries: Vec<InteractiveCommandPaletteEntry>,
    pub navigation: Vec<InteractiveKeyBinding>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveCommandPaletteGroup {
    pub group_id: String,
    pub title: String,
    pub entry_count: usize,
    pub entries: Vec<InteractiveCommandPaletteEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveCommandPaletteEntry {
    pub action_id: String,
    pub group_id: String,
    pub title: String,
    pub description: String,
    pub source_panel: String,
    pub enabled: bool,
    pub blocked_reason: String,
    pub operation_plan: InteractiveCommandPaletteActionPlan,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addon_contract: Option<InteractiveAddonActionContract>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addon_view_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addon_view_action_id: Option<String>,
    pub workflow_id: Option<String>,
    pub commands: Vec<String>,
    pub mutates_workflow: bool,
    pub requires_approval: bool,
    pub risk_level: String,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveCommandPaletteActionPlan {
    pub schema_version: String,
    pub status: String,
    pub recommended_action: String,
    pub diagnostic_only: bool,
    pub blocked_reason: String,
    pub next_commands: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveActionRegistryPanel {
    pub schema_version: String,
    pub status: String,
    pub query: String,
    pub action_count: usize,
    pub enabled_action_count: usize,
    pub blocked_action_count: usize,
    pub diagnostic_action_count: usize,
    pub mutation_action_count: usize,
    pub approval_action_count: usize,
    pub group_count: usize,
    pub groups: Vec<InteractiveActionRegistryGroup>,
    pub actions: Vec<InteractiveCommandPaletteEntry>,
    pub commands: InteractiveActionRegistryCommands,
    pub navigation: Vec<InteractiveKeyBinding>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveActionRegistryGroup {
    pub group_id: String,
    pub title: String,
    pub action_count: usize,
    pub enabled_action_count: usize,
    pub blocked_action_count: usize,
    pub diagnostic_action_count: usize,
    pub mutation_action_count: usize,
    pub approval_action_count: usize,
    pub actions: Vec<InteractiveCommandPaletteEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveActionRegistryCommands {
    pub action_registry: Vec<String>,
    pub command_palette: Vec<String>,
    pub autocomplete: Vec<String>,
    pub inspect_addons: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveActionInvocationReport {
    pub schema_version: String,
    pub status: String,
    pub requested_action_id: String,
    pub match_count: usize,
    pub can_execute: bool,
    pub diagnostic_only: bool,
    pub not_executed: bool,
    pub selected_command: Vec<String>,
    pub selected_command_text: String,
    pub source_panel: String,
    pub risk_level: String,
    pub mutates_workflow: bool,
    pub requires_approval: bool,
    pub execution_boundary: String,
    pub blocked_reason: String,
    pub recommended_action: String,
    pub next_commands: Vec<Vec<String>>,
    pub operation_plan: InteractiveCommandPaletteActionPlan,
    pub action: Option<InteractiveCommandPaletteEntry>,
    pub commands: InteractiveActionInvocationCommands,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveActionInvocationCommands {
    pub action_invocation: Vec<String>,
    pub action_registry: Vec<String>,
    pub command_palette: Vec<String>,
    pub autocomplete: Vec<String>,
}

#[derive(Debug, Default)]
struct InteractiveActionRegistryCounts {
    action_count: usize,
    enabled_action_count: usize,
    blocked_action_count: usize,
    diagnostic_action_count: usize,
    mutation_action_count: usize,
    approval_action_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveAutocompletePanel {
    pub schema_version: String,
    pub status: String,
    pub input: String,
    pub normalized_query: String,
    pub suggestion_count: usize,
    pub suggestions: Vec<InteractiveAutocompleteSuggestion>,
    pub navigation: Vec<InteractiveKeyBinding>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveAutocompleteSuggestion {
    pub suggestion_id: String,
    pub kind: String,
    pub label: String,
    pub insert_text: String,
    pub description: String,
    pub source: String,
    pub source_panel: String,
    pub enabled: bool,
    pub blocked_reason: String,
    pub operation_plan: InteractiveCommandPaletteActionPlan,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addon_contract: Option<InteractiveAddonActionContract>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addon_view_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addon_view_action_id: Option<String>,
    pub workflow_id: Option<String>,
    pub equivalent_command: Vec<String>,
    pub mutates_workflow: bool,
    pub requires_approval: bool,
    pub risk_level: String,
    pub score: i64,
}

#[derive(Debug, Clone)]
pub struct InteractiveHarnessOptions {
    pub executor: String,
    pub shim_dir: PathBuf,
    pub project_root: Option<PathBuf>,
    pub forge_first: bool,
    pub observe_only: bool,
    pub workflow_id: Option<String>,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub context_budget: Option<usize>,
    pub token_headroom: Option<bool>,
}

impl InteractiveHarnessOptions {
    pub fn default_for_current_dir() -> Self {
        Self {
            executor: "codex".to_string(),
            shim_dir: default_interactive_harness_shim_dir(),
            project_root: Some(env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
            forge_first: false,
            observe_only: false,
            workflow_id: None,
            task_id: None,
            run_id: None,
            context_budget: None,
            token_headroom: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveHarnessPanel {
    pub schema_version: String,
    pub status: String,
    pub executor: String,
    pub project_root: String,
    pub shim_dir: String,
    pub forge_first_ready: bool,
    pub token_headroom_ready: bool,
    pub shim_ready: bool,
    pub lineage_policy_ready: bool,
    pub lineage_context_ready: bool,
    pub mode: HarnessModeReport,
    pub doctor: HarnessDoctorReport,
    pub shim_status: CliShimStatusReport,
    pub wrapper_plan: CliWrapperPlanReport,
    pub headroom_plan: HarnessHeadroomPlanReport,
    pub adoption_plan: HarnessAdoptionPlanReport,
    pub forge_first_adoption_readiness: InteractiveHarnessForgeFirstAdoptionReadiness,
    pub headroom_stats: HeadroomStatsReport,
    pub headroom_operational_status: String,
    pub headroom_recommended_action: String,
    pub session_lifecycle_plan: HarnessSessionLifecyclePlan,
    pub executor_compatibility: HarnessExecutorCompatibilityReport,
    pub headroom_preview: TokenHeadroomReport,
    pub next_actions: Vec<String>,
    pub notes: Vec<String>,
    pub commands: InteractiveHarnessCommands,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveHarnessForgeFirstAdoptionReadiness {
    pub schema_version: String,
    pub status: String,
    pub executor: String,
    pub forge_first_default_active: bool,
    pub ready_to_use_as_default: bool,
    pub token_headroom_ready: bool,
    pub token_headroom_required: bool,
    pub shim_ready: bool,
    pub activation_status: String,
    pub activation_required: bool,
    pub activation_possible: bool,
    pub activation_reason: String,
    pub activation_command: String,
    pub activation_profile_command: Vec<String>,
    pub lineage_policy_ready: bool,
    pub lineage_context_ready: bool,
    pub execution_guard_status: String,
    pub wrapper_strategy: String,
    pub wrapper_interception_points: Vec<String>,
    pub controlled_routes: Vec<String>,
    pub readiness_gates: Vec<String>,
    pub blocked_reasons: Vec<String>,
    pub next_commands: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveHarnessCommands {
    pub refresh: Vec<String>,
    pub mode: Vec<String>,
    pub doctor: Vec<String>,
    pub shim_status: Vec<String>,
    pub wrap_plan: Vec<String>,
    pub headroom_plan: Vec<String>,
    pub headroom_stats: Vec<String>,
    pub adoption_plan: Vec<String>,
    pub lineage_plan: Vec<String>,
    pub lineage_exec_dry_run: Vec<String>,
    pub activation_profile: Vec<String>,
    pub bootstrap_project_harness: Vec<String>,
    pub install_shims: Vec<String>,
    pub exec: Vec<String>,
    pub sessions: Vec<String>,
    pub sync: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveTokenUsagePanel {
    pub schema_version: String,
    pub status: String,
    pub operational_status: String,
    pub recommended_action: String,
    pub total_headroom_blobs: usize,
    pub total_original_tokens: i64,
    pub total_compressed_tokens: i64,
    pub estimated_saved_tokens: i64,
    pub average_savings_percent: f64,
    pub over_budget_after_headroom_count: usize,
    pub primary_source: String,
    pub primary_content_kind: String,
    pub retrieve_commands: Vec<String>,
    pub source_buckets: Vec<HeadroomStatsSourceBucket>,
    pub content_kind_buckets: Vec<HeadroomStatsContentKindBucket>,
    pub headroom_stats: HeadroomStatsReport,
    pub commands: InteractiveTokenUsageCommands,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveTokenUsageCommands {
    pub refresh: Vec<String>,
    pub headroom_stats: Vec<String>,
    pub analyze_payload: Vec<String>,
    pub retrieve_top: Vec<String>,
    pub harness: Vec<String>,
    pub cost_ledger: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct InteractiveSessionsOptions {
    pub provider_id: Option<String>,
    pub lifecycle_state: Option<String>,
    pub readiness: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveSessionsPanel {
    pub schema_version: String,
    pub status: String,
    pub controller: String,
    pub selected_provider_id: Option<String>,
    pub provider_count: usize,
    pub session_count: usize,
    pub ready_session_count: usize,
    pub planned_event_count: usize,
    pub lifecycle_event_count: usize,
    pub session_report: BrainSessionsReport,
    pub session_cards: Vec<InteractiveSessionCard>,
    pub commands: InteractiveSessionsCommands,
    pub next_actions: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveSessionCard {
    pub session_id: String,
    pub provider_id: String,
    pub provider_kind: String,
    pub readiness: String,
    pub launch_mode: String,
    pub forge_first_ready: bool,
    pub lifecycle_state: String,
    pub recorded_plan_count: usize,
    pub lifecycle_event_count: usize,
    pub last_origin: Option<String>,
    pub last_workflow_id: Option<String>,
    pub last_task_id: Option<String>,
    pub last_run_id: Option<String>,
    pub operation_plan: BrainSessionOperationPlan,
    pub commands: InteractiveSessionCardCommands,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveSessionCardCommands {
    pub history: Vec<String>,
    pub lifecycle: Vec<String>,
    pub launch_plan: Vec<String>,
    pub record_plan: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveSessionsCommands {
    pub refresh: Vec<String>,
    pub list: Vec<String>,
    pub brains: Vec<String>,
    pub shells: Vec<String>,
    pub lifecycle: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractivePatchWorkbenchPanel {
    pub schema_version: String,
    pub addon_contract: InteractivePatchAddonContract,
    pub status: String,
    pub repository_path: String,
    pub clean: bool,
    pub changed_path_count: usize,
    pub staged_path_count: usize,
    pub unstaged_path_count: usize,
    pub untracked_path_count: usize,
    pub diff_present: bool,
    pub diff_check_status: String,
    pub diff_stat: String,
    pub diff_preview: InteractivePatchDiffPreview,
    pub diff_review_queue: InteractivePatchDiffReviewQueue,
    pub edit_intake: InteractivePatchEditIntake,
    pub operation_plan: InteractivePatchOperationPlan,
    pub files: Vec<InteractivePatchWorkbenchFile>,
    pub approval_flow: InteractivePatchApprovalFlow,
    pub commands: InteractivePatchWorkbenchCommands,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractivePatchAddonContract {
    pub schema_version: String,
    pub source_addon: String,
    pub capability_id: String,
    pub permission_id: String,
    pub view_id: String,
    pub runtime_contract_id: String,
    pub runtime: String,
    pub entrypoint: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveAddonActionContract {
    pub schema_version: String,
    pub source_addon: String,
    pub addon_name: String,
    pub addon_version: String,
    pub addon_lifecycle: String,
    pub capability_id: String,
    pub permission_id: String,
    pub permission_gate_status: String,
    pub view_id: String,
    pub action_id: String,
    pub action_type: String,
    pub method: String,
    pub target: String,
}

#[derive(Debug, Clone)]
struct InteractiveAddonActionReadiness {
    enabled: bool,
    blocked_reason: String,
    permission_gate_status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractivePatchDiffPreview {
    pub schema_version: String,
    pub status: String,
    pub selected_path: Option<String>,
    pub line_count: usize,
    pub truncated: bool,
    pub command: Vec<String>,
    pub lines: Vec<InteractivePatchDiffPreviewLine>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractivePatchDiffPreviewLine {
    pub line_kind: String,
    pub old_line: Option<usize>,
    pub new_line: Option<usize>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractivePatchDiffReviewQueue {
    pub schema_version: String,
    pub status: String,
    pub selected_path: Option<String>,
    pub file_count: usize,
    pub pending_review_count: usize,
    pub total_hunk_count: usize,
    pub total_addition_count: usize,
    pub total_deletion_count: usize,
    pub files: Vec<InteractivePatchDiffReviewQueueFile>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractivePatchDiffReviewQueueFile {
    pub path: String,
    pub review_status: String,
    pub action_hint: InteractivePatchFileActionHint,
    pub selected: bool,
    pub staged: bool,
    pub unstaged: bool,
    pub hunk_count: usize,
    pub addition_count: usize,
    pub deletion_count: usize,
    pub line_count: usize,
    pub commands: InteractivePatchWorkbenchFileCommands,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractivePatchEditIntake {
    pub schema_version: String,
    pub status: String,
    pub default_action: String,
    pub required_input_count: usize,
    pub missing_required_input_count: usize,
    pub inferred_path_count: usize,
    pub required_inputs: Vec<InteractivePatchEditInput>,
    pub forms: Vec<InteractivePatchEditForm>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractivePatchEditInput {
    pub input_id: String,
    pub label: String,
    pub input_kind: String,
    pub required: bool,
    pub missing: bool,
    pub source: String,
    pub example: String,
    pub command_flag: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractivePatchEditForm {
    pub action_id: String,
    pub title: String,
    pub ready: bool,
    pub requires_human_approval: bool,
    pub required_input_ids: Vec<String>,
    pub missing_input_ids: Vec<String>,
    pub command_template: Vec<String>,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractivePatchOperationPlan {
    pub schema_version: String,
    pub status: String,
    pub current_step: String,
    pub step_count: usize,
    pub ready_step_count: usize,
    pub blocked_step_count: usize,
    pub requires_human_approval_count: usize,
    pub steps: Vec<InteractivePatchOperationStep>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractivePatchOperationStep {
    pub step_id: String,
    pub title: String,
    pub status: String,
    pub action_id: String,
    pub command: Vec<String>,
    pub mutates_workflow: bool,
    pub requires_human_approval: bool,
    pub depends_on: Vec<String>,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractivePatchApprovalFlow {
    pub schema_version: String,
    pub status: String,
    pub current_gate: String,
    pub requires_human_approval: bool,
    pub apply_ready: bool,
    pub gates: Vec<InteractivePatchApprovalGate>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractivePatchApprovalGate {
    pub gate_id: String,
    pub title: String,
    pub status: String,
    pub command: Vec<String>,
    pub mutates_workflow: bool,
    pub requires_human_approval: bool,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractivePatchWorkbenchFile {
    pub path: String,
    pub index_status: String,
    pub worktree_status: String,
    pub status_label: String,
    pub action_hint: InteractivePatchFileActionHint,
    pub staged: bool,
    pub unstaged: bool,
    pub untracked: bool,
    pub commands: InteractivePatchWorkbenchFileCommands,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractivePatchFileActionHint {
    pub schema_version: String,
    pub suggested_next_action: String,
    pub review_required: bool,
    pub apply_blocked_until_review: bool,
    pub primary_command: Vec<String>,
    pub blocked_reason: String,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractivePatchWorkbenchFileCommands {
    pub plan: Vec<String>,
    pub review: Vec<String>,
    pub diff: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractivePatchWorkbenchCommands {
    pub refresh: Vec<String>,
    pub status: Vec<String>,
    pub plan: Vec<String>,
    pub review: Vec<String>,
    pub diff: Vec<String>,
    pub apply: Vec<String>,
    pub revert: Vec<String>,
    pub restore: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractivePermissionsPanel {
    pub schema_version: String,
    pub status: String,
    pub membership_count: usize,
    pub active_membership_count: usize,
    pub expired_membership_count: usize,
    pub not_yet_valid_membership_count: usize,
    pub addon_authorization_count: usize,
    pub approved_addon_permission_count: usize,
    pub revoked_addon_permission_count: usize,
    pub pending_human_approval_count: usize,
    pub timed_out_human_approval_count: usize,
    pub memberships: Vec<InteractivePermissionMembership>,
    pub addon_permissions: Vec<InteractiveAddonPermissionAuthorization>,
    pub approval_items: Vec<InteractiveApprovalItem>,
    pub commands: InteractivePermissionsCommands,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractivePermissionMembership {
    pub subject_scope: String,
    pub subject_id: String,
    pub tenant_path: String,
    pub organization_id: String,
    pub brand_id: String,
    pub product_id: String,
    pub role: String,
    pub status: String,
    pub permission_count: usize,
    pub permissions: Vec<String>,
    pub permission_grants: Vec<String>,
    pub permission_denies: Vec<String>,
    pub expired: bool,
    pub not_yet_valid: bool,
    pub commands: InteractivePermissionMembershipCommands,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractivePermissionMembershipCommands {
    pub list: Vec<String>,
    pub update: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveAddonPermissionAuthorization {
    pub addon_id: String,
    pub permission_id: String,
    pub status: String,
    pub risk: String,
    pub approved_by: String,
    pub source: String,
    pub granted_at: String,
    pub commands: InteractiveAddonPermissionCommands,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveAddonPermissionCommands {
    pub list: Vec<String>,
    pub authorize: Vec<String>,
    pub revoke: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveApprovalItem {
    pub source: String,
    pub workflow_id: String,
    pub task_id: String,
    pub task_title: String,
    pub interaction_id: String,
    pub kind: String,
    pub state: String,
    pub prompt: String,
    pub required: bool,
    pub commands: InteractiveApprovalCommands,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveApprovalCommands {
    pub list: Vec<String>,
    pub answer: Vec<String>,
    pub expire: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractivePermissionsCommands {
    pub refresh: Vec<String>,
    pub list_memberships: Vec<String>,
    pub update_membership: Vec<String>,
    pub list_addon_permissions: Vec<String>,
    pub authorize_addon_permission: Vec<String>,
    pub revoke_addon_permission: Vec<String>,
    pub list_interactions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveIdentityPanel {
    pub schema_version: String,
    pub status: String,
    pub project_root: String,
    pub context_status: String,
    pub identity_count: usize,
    pub link_count: usize,
    pub channel_alias_count: usize,
    pub membership_count: usize,
    pub active_membership_count: usize,
    pub tenant_audit_missing_count: usize,
    pub current_context: InteractiveIdentityCurrentContext,
    pub identities: Vec<crate::identity::IdentityRegistryView>,
    pub channel_aliases: Vec<InteractiveIdentityChannelAlias>,
    pub memberships: Vec<InteractiveIdentityMembership>,
    pub tenant_audit: crate::identity::TenantAuditReport,
    pub commands: InteractiveIdentityCommands,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveIdentityCurrentContext {
    pub organization_id: String,
    pub organization_label: String,
    pub brand_id: String,
    pub brand_label: String,
    pub product_id: String,
    pub product_label: String,
    pub user_id: String,
    pub user_label: String,
    pub channel_id: String,
    pub channel_label: String,
    pub memory_scope: String,
    pub personality_scope: String,
    pub tenant_policy_mode: String,
    pub source: String,
    pub warning_count: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveIdentityChannelAlias {
    pub alias_path: String,
    pub left_scope: String,
    pub left_id: String,
    pub right_scope: String,
    pub right_id: String,
    pub link_type: String,
    pub status: String,
    pub source: String,
    pub commands: InteractiveIdentityAliasCommands,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveIdentityAliasCommands {
    pub resolve: Vec<String>,
    pub unlink: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveIdentityMembership {
    pub subject_scope: String,
    pub subject_id: String,
    pub tenant_path: String,
    pub organization_id: String,
    pub brand_id: String,
    pub product_id: String,
    pub role: String,
    pub status: String,
    pub permission_count: usize,
    pub expired: bool,
    pub not_yet_valid: bool,
    pub commands: InteractiveIdentityMembershipCommands,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveIdentityMembershipCommands {
    pub list: Vec<String>,
    pub update: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveIdentityCommands {
    pub refresh: Vec<String>,
    pub context: Vec<String>,
    pub sync: Vec<String>,
    pub registry: Vec<String>,
    pub link: Vec<String>,
    pub links: Vec<String>,
    pub resolve: Vec<String>,
    pub memberships: Vec<String>,
    pub tenant_audit: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveWorkflowDagPanel {
    pub schema_version: String,
    pub status: String,
    pub workflow_count: usize,
    pub node_count: usize,
    pub edge_count: usize,
    pub running_node_count: usize,
    pub blocked_node_count: usize,
    pub wait_node_count: usize,
    pub human_wait_count: usize,
    pub workflows: Vec<InteractiveWorkflowDag>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveWorkflowDag {
    pub workflow_id: String,
    pub lifecycle_state: String,
    pub goal: String,
    pub node_count: usize,
    pub edge_count: usize,
    pub ready_root_count: usize,
    pub blocked_node_count: usize,
    pub human_wait_count: usize,
    pub nodes: Vec<InteractiveWorkflowDagNode>,
    pub edges: Vec<InteractiveWorkflowDagEdge>,
    pub commands: InteractiveWorkflowDagCommands,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveWorkflowDagNode {
    pub task_id: String,
    pub title: String,
    pub status: String,
    pub executor: String,
    pub dependency_count: usize,
    pub dependent_count: usize,
    pub ready_for_execution: bool,
    pub human_required: bool,
    pub human_interaction_state: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveWorkflowDagEdge {
    pub from_task_id: String,
    pub to_task_id: String,
    pub edge_kind: String,
    pub dependency_status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveWorkflowDagCommands {
    pub inspect: Vec<String>,
    pub task_board: Vec<String>,
    pub validate: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveTaskBoardPanel {
    pub schema_version: String,
    pub status: String,
    pub workflow_count: usize,
    pub task_count: usize,
    pub ready_handoffs: usize,
    pub blocked_tasks: usize,
    pub failed_tasks: usize,
    pub running_tasks: usize,
    pub checkpoint_resume_candidates: usize,
    pub pending_human_interactions: usize,
    pub artifact_count: usize,
    pub lanes: Vec<InteractiveTaskBoardLane>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveTaskBoardLane {
    pub workflow_id: String,
    pub lifecycle_state: String,
    pub goal: String,
    pub total_tasks: usize,
    pub pending_tasks: usize,
    pub running_tasks: usize,
    pub completed_tasks: usize,
    pub blocked_tasks: usize,
    pub failed_tasks: usize,
    pub ready_handoffs: usize,
    pub checkpoint_resume_candidates: usize,
    pub pending_human_interactions: usize,
    pub artifact_count: usize,
    pub next_actions: Vec<String>,
    pub task_cards: Vec<InteractiveTaskBoardTaskCard>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveTaskBoardTaskCard {
    pub task_id: String,
    pub title: String,
    pub status: String,
    pub executor: String,
    pub dependency_count: usize,
    pub dependent_count: usize,
    pub context_requirement_count: usize,
    pub validation_rule_count: usize,
    pub estimated_cost_usd: f64,
    pub cost_model: String,
    pub workflow_artifact_count: usize,
    pub history_event_count: usize,
    pub latest_history_event: Option<InteractiveTaskHistoryEvent>,
    pub human_required: bool,
    pub human_interaction_state: String,
    pub ready_for_handoff: bool,
    pub context_action: String,
    pub checkpoint_id: Option<String>,
    pub checkpoint_state: Option<String>,
    pub next_action: String,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveWorkflowMutationPanel {
    pub schema_version: String,
    pub status: String,
    pub operation_mode: String,
    pub workflow_count: usize,
    pub active_workflow_count: usize,
    pub mutable_workflow_count: usize,
    pub task_count: usize,
    pub ready_handoff_count: usize,
    pub human_wait_count: usize,
    pub checkpoint_resume_candidate_count: usize,
    pub artifact_count: usize,
    pub pending_modifier_proposal_count: usize,
    pub applied_modifier_proposal_count: usize,
    pub event_count: usize,
    pub estimated_cost_total_usd: f64,
    pub workflow_cards: Vec<InteractiveWorkflowMutationCard>,
    pub proposal_cards: Vec<InteractiveOperationalModifierProposalCard>,
    pub commands: InteractiveWorkflowMutationCommands,
    pub next_actions: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveWorkflowMutationCard {
    pub workflow_id: String,
    pub lifecycle_state: String,
    pub goal: String,
    pub active: bool,
    pub task_count: usize,
    pub ready_handoffs: usize,
    pub human_waits: usize,
    pub checkpoint_resume_candidates: usize,
    pub artifact_count: usize,
    pub dag_node_count: usize,
    pub dag_edge_count: usize,
    pub mutable_targets: Vec<String>,
    pub recommended_action: String,
    pub commands: InteractiveWorkflowMutationWorkflowCommands,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveWorkflowMutationWorkflowCommands {
    pub inspect: Vec<String>,
    pub task_board: Vec<String>,
    pub workflow_dag: Vec<String>,
    pub validate: Vec<String>,
    pub update_goal: Vec<String>,
    pub update_node_brain: Vec<String>,
    pub attach_artifact: Vec<String>,
    pub context: Vec<String>,
    pub handoff: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveWorkflowMutationCommands {
    pub refresh: Vec<String>,
    pub task_board: Vec<String>,
    pub workflow_dag: Vec<String>,
    pub operational_cockpit: Vec<String>,
    pub action_registry: Vec<String>,
    pub ops_console: Vec<String>,
    pub propose_goal_route: String,
    pub propose_task_route: String,
    pub apply_proposal_route: String,
    pub update_goal: Vec<String>,
    pub update_node_brain: Vec<String>,
    pub attach_artifact: Vec<String>,
    pub validate: Vec<String>,
    pub context: Vec<String>,
    pub handoff: Vec<String>,
    pub structured_logs: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveArtifactPanel {
    pub schema_version: String,
    pub status: String,
    pub workflow_count: usize,
    pub artifact_count: usize,
    pub total_bytes: u64,
    pub workflows: Vec<InteractiveArtifactWorkflow>,
    pub commands: InteractiveArtifactPanelCommands,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveArtifactWorkflow {
    pub workflow_id: String,
    pub lifecycle_state: String,
    pub goal: String,
    pub artifact_count: usize,
    pub total_bytes: u64,
    pub artifacts: Vec<InteractiveArtifactEntry>,
    pub commands: InteractiveArtifactWorkflowCommands,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveArtifactEntry {
    pub artifact_id: String,
    pub kind: String,
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    pub created_at: String,
    pub lineage_summary: String,
    pub commands: InteractiveArtifactEntryCommands,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveArtifactPanelCommands {
    pub refresh: Vec<String>,
    pub task_board: Vec<String>,
    pub workflow_list: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveArtifactWorkflowCommands {
    pub list: Vec<String>,
    pub inspect: Vec<String>,
    pub task_board: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveArtifactEntryCommands {
    pub open: Vec<String>,
    pub inspect_workflow: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveTaskHistoryEvent {
    pub event_id: i64,
    pub kind: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveSchedulePanel {
    pub schema_version: String,
    pub status: String,
    pub executor: String,
    pub observed_at: String,
    pub ttl_seconds: u64,
    pub scanned_workflows: usize,
    pub due_workflows: usize,
    pub runnable_due_workflows: usize,
    pub blocked_due_workflows: usize,
    pub idle_workflows: usize,
    pub paused_or_stopped_loop_workflows: usize,
    pub scheduled_nodes: usize,
    pub cron_nodes: usize,
    pub wait_until_nodes: usize,
    pub delay_nodes: usize,
    pub scale_to_zero_workflows: usize,
    pub worker_pool: InteractiveScheduleWorkerPool,
    pub assignment_plan: InteractiveScheduleAssignmentPlan,
    pub assigned_workflows: Vec<InteractiveScheduleAssignment>,
    pub queued_workflows: Vec<InteractiveScheduleAssignment>,
    pub sleep_until_next_wakeup: bool,
    pub next_wakeup_at: Option<String>,
    pub sleep_seconds: u64,
    pub sleep_mode: String,
    pub sleep_reason: String,
    pub backpressure_active: bool,
    pub queued_due_workflows: usize,
    pub backpressure_reason: String,
    pub cancellation_supported: bool,
    pub lease_ttl_seconds: u64,
    pub cancellation_safe_points: Vec<String>,
    pub workflows: Vec<InteractiveScheduleWorkflow>,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveScheduleWorkerPool {
    pub max_workers: usize,
    pub available_workers: usize,
    pub assignable_due_workflows: usize,
    pub worker_kind: String,
    pub deterministic: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveScheduleAssignmentPlan {
    pub schema_version: String,
    pub max_workers: usize,
    pub assigned_count: usize,
    pub queued_count: usize,
    pub deterministic_ordering: bool,
    pub ordering_key: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveScheduleAssignment {
    pub workflow_id: String,
    pub goal: String,
    pub schedule_task_id: String,
    pub due_nodes: usize,
    pub next_run_at: Option<String>,
    pub lease_scope: String,
    pub wave: usize,
    pub queue_position: usize,
    pub executor: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveScheduleWorkflow {
    pub workflow_id: String,
    pub goal: String,
    pub status: String,
    pub due_nodes: usize,
    pub next_wakeup_at: Option<String>,
    pub scale_to_zero_eligible: bool,
    pub blocked_loop_task_id: Option<String>,
    pub blocked_loop_state: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveEventPanel {
    pub status: String,
    pub total_event_count: usize,
    pub visible_event_count: usize,
    pub latest_events: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveStructuredLogsPanel {
    pub schema_version: String,
    pub status: String,
    pub total_event_count: usize,
    pub log_count: usize,
    pub next_cursor: Option<i64>,
    pub has_more: bool,
    pub logs: Vec<InteractiveStructuredLogEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveStructuredLogEntry {
    pub event_id: String,
    pub store_sequence: i64,
    pub workflow_id: String,
    pub kind: String,
    pub category: String,
    pub severity: String,
    pub origin: String,
    pub source: String,
    pub occurred_at: String,
    pub correlation: serde_json::Value,
    pub observability: serde_json::Value,
    pub payload_preview: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveCostPanel {
    pub status: String,
    pub workflow_count: usize,
    pub node_count: usize,
    pub ai_node_count: usize,
    pub deterministic_node_count: usize,
    pub model_call_avoided_node_count: usize,
    pub estimated_task_cost_total_usd: f64,
    pub observed_event_cost_total_usd: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveImprovementLoopPanel {
    pub schema_version: String,
    pub status: String,
    pub candidate_count: usize,
    pub total_workflows: usize,
    pub matched_workflows: usize,
    pub critical_candidate_count: usize,
    pub high_candidate_count: usize,
    pub parallel_ready_candidate_count: usize,
    pub avoidable_ai_candidate_count: usize,
    pub final_outcome_candidate_count: usize,
    pub stale_or_attention_candidate_count: usize,
    pub event_count: usize,
    pub structured_log_count: usize,
    pub cost_status: String,
    pub estimated_cost_total_usd: f64,
    pub observed_cost_total_usd: f64,
    pub ai_node_count: usize,
    pub model_call_avoided_node_count: usize,
    pub validation_failure_count: usize,
    pub context_quality_status: String,
    pub top_candidates: Vec<InteractiveImprovementCandidateCard>,
    pub commands: InteractiveImprovementLoopCommands,
    pub next_actions: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveImprovementCandidateCard {
    pub workflow_id: String,
    pub goal: String,
    pub priority: String,
    pub score: i64,
    pub recommended_action: String,
    pub reason_codes: Vec<String>,
    pub ready_parallel_task_count: usize,
    pub avoidable_estimated_cost_usd: f64,
    pub avoidable_observed_cost_usd: Option<f64>,
    pub outcome_status: String,
    pub event_count: usize,
    pub active_run_count: usize,
    pub suggested_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveImprovementLoopCommands {
    pub refresh: Vec<String>,
    pub candidates: Vec<String>,
    pub cost_ledger: Vec<String>,
    pub structured_logs: Vec<String>,
    pub task_board: Vec<String>,
    pub validate: Vec<String>,
    pub apply_event_policy: Vec<String>,
    pub benchmark_event_policy: Vec<String>,
    pub promote_event_policy: Vec<String>,
}

pub fn build_interactive_improvement_loop(
    store: &ForgeStore,
) -> Result<InteractiveImprovementLoopPanel> {
    let workflows = list_workflows_with_filters(
        store,
        WorkflowRegistryFilters::new(WorkflowLifecycleFilter::All),
    )?;
    let structured_logs_panel = build_interactive_structured_logs(store)?;
    let cost_panel = build_cost_ledger(store, None, None, None, None)
        .ok()
        .map(interactive_cost_panel_from_ledger)
        .unwrap_or_else(|| InteractiveCostPanel {
            status: "cost_ledger_unavailable".to_string(),
            workflow_count: 0,
            node_count: 0,
            ai_node_count: 0,
            deterministic_node_count: 0,
            model_call_avoided_node_count: 0,
            estimated_task_cost_total_usd: 0.0,
            observed_event_cost_total_usd: 0.0,
        });
    let validation_failure_count = workflows
        .workflows
        .iter()
        .map(|workflow| workflow.task_summary.failed + workflow.task_summary.blocked)
        .sum();
    let candidates = rank_improvement_candidates(store, 10)?;

    Ok(build_improvement_loop_panel(
        &candidates,
        &structured_logs_panel,
        &cost_panel,
        validation_failure_count,
        &workflows.summary.context_quality,
    ))
}

fn interactive_cost_panel_from_ledger(ledger: CostLedgerReport) -> InteractiveCostPanel {
    let summary = ledger.summary;
    InteractiveCostPanel {
        status: ledger.status,
        workflow_count: summary.workflow_count,
        node_count: summary.node_count,
        ai_node_count: summary.ai_node_count,
        deterministic_node_count: summary.deterministic_node_count,
        model_call_avoided_node_count: summary.model_call_avoided_node_count,
        estimated_task_cost_total_usd: summary.estimated_task_cost_total_usd,
        observed_event_cost_total_usd: summary.observed_event_cost_total_usd,
    }
}

fn build_improvement_loop_panel(
    candidates: &OrchestratorImprovementCandidatesReport,
    structured_logs: &InteractiveStructuredLogsPanel,
    cost: &InteractiveCostPanel,
    validation_failure_count: usize,
    context_quality: &RegistryContextQualitySummary,
) -> InteractiveImprovementLoopPanel {
    let top_candidates = candidates
        .candidates
        .iter()
        .take(8)
        .map(improvement_loop_candidate_card)
        .collect::<Vec<_>>();
    let critical_candidate_count = candidates
        .candidates
        .iter()
        .filter(|candidate| candidate.priority == "critical")
        .count();
    let high_candidate_count = candidates
        .candidates
        .iter()
        .filter(|candidate| candidate.priority == "high")
        .count();
    let parallel_ready_candidate_count = candidates
        .candidates
        .iter()
        .filter(|candidate| candidate.parallelization.ready_parallel_task_count > 0)
        .count();
    let avoidable_ai_candidate_count = candidates
        .candidates
        .iter()
        .filter(|candidate| {
            candidate.cost_efficiency.avoidable_estimated_cost_usd > 0.0
                || candidate
                    .cost_efficiency
                    .avoidable_observed_cost_total_usd
                    .unwrap_or(0.0)
                    > 0.0
                || candidate
                    .reasons
                    .iter()
                    .any(|reason| reason.code == "avoidable_ai_cost")
        })
        .count();
    let final_outcome_candidate_count = candidates
        .candidates
        .iter()
        .filter(|candidate| {
            candidate.recommended_action.contains("final")
                || candidate.reasons.iter().any(|reason| {
                    matches!(
                        reason.code.as_str(),
                        "missing_final_outcome_audit"
                            | "missing_user_delivery_evidence"
                            | "completed_without_final_package"
                            | "verified_without_final_package"
                    )
                })
        })
        .count();
    let stale_or_attention_candidate_count = candidates
        .candidates
        .iter()
        .filter(|candidate| {
            candidate.reasons.iter().any(|reason| {
                matches!(
                    reason.code.as_str(),
                    "stale_running_run" | "missing_runtime_heartbeat" | "run_needs_attention"
                )
            })
        })
        .count();
    let context_quality_status = improvement_loop_context_quality_status(context_quality);
    let status = improvement_loop_status(
        candidates.candidate_count,
        structured_logs.total_event_count,
        structured_logs.log_count,
        cost.node_count,
        validation_failure_count,
    );

    InteractiveImprovementLoopPanel {
        schema_version: INTERACTIVE_IMPROVEMENT_LOOP_SCHEMA_VERSION.to_string(),
        status: status.clone(),
        candidate_count: candidates.candidate_count,
        total_workflows: candidates.total_workflows,
        matched_workflows: candidates.matched_workflows,
        critical_candidate_count,
        high_candidate_count,
        parallel_ready_candidate_count,
        avoidable_ai_candidate_count,
        final_outcome_candidate_count,
        stale_or_attention_candidate_count,
        event_count: structured_logs.total_event_count,
        structured_log_count: structured_logs.log_count,
        cost_status: cost.status.clone(),
        estimated_cost_total_usd: cost.estimated_task_cost_total_usd,
        observed_cost_total_usd: cost.observed_event_cost_total_usd,
        ai_node_count: cost.ai_node_count,
        model_call_avoided_node_count: cost.model_call_avoided_node_count,
        validation_failure_count,
        context_quality_status,
        top_candidates,
        commands: improvement_loop_commands(),
        next_actions: improvement_loop_next_actions(&status, candidates.candidate_count),
        notes: vec![
            "This panel is read-only; mutations still go through explicit improve, request, workflow or event-policy commands.".to_string(),
            "Use it before self-improvement so Forge ranks work from logs, cost, validation, context quality and outcome evidence.".to_string(),
        ],
    }
}

fn improvement_loop_candidate_card(
    candidate: &OrchestratorImprovementCandidate,
) -> InteractiveImprovementCandidateCard {
    InteractiveImprovementCandidateCard {
        workflow_id: candidate.workflow_id.clone(),
        goal: truncate_display(&candidate.goal, 120),
        priority: candidate.priority.clone(),
        score: candidate.score,
        recommended_action: candidate.recommended_action.clone(),
        reason_codes: candidate
            .reasons
            .iter()
            .map(|reason| reason.code.clone())
            .collect(),
        ready_parallel_task_count: candidate.parallelization.ready_parallel_task_count,
        avoidable_estimated_cost_usd: candidate.cost_efficiency.avoidable_estimated_cost_usd,
        avoidable_observed_cost_usd: candidate.cost_efficiency.avoidable_observed_cost_total_usd,
        outcome_status: candidate.outcome_status.status.clone(),
        event_count: candidate.evidence.event_count,
        active_run_count: candidate.evidence.active_run_count,
        suggested_commands: candidate
            .suggested_commands
            .iter()
            .take(4)
            .map(|command| command.join(" "))
            .collect(),
    }
}

fn improvement_loop_context_quality_status(summary: &RegistryContextQualitySummary) -> String {
    if summary.blocked > 0 || summary.blocking_warnings > 0 || summary.required_context_missing > 0
    {
        "context_quality_blocked"
    } else if summary.total_warnings > 0 || summary.budget_pressure > 0 || summary.warning > 0 {
        "context_quality_warn"
    } else if summary.total_tasks > 0 {
        "context_quality_ready"
    } else {
        "context_quality_idle"
    }
    .to_string()
}

fn improvement_loop_status(
    candidate_count: usize,
    event_count: usize,
    structured_log_count: usize,
    cost_node_count: usize,
    validation_failure_count: usize,
) -> String {
    if candidate_count > 0 {
        "improvement_loop_actionable"
    } else if event_count > 0
        || structured_log_count > 0
        || cost_node_count > 0
        || validation_failure_count > 0
    {
        "improvement_loop_observing"
    } else {
        "improvement_loop_idle"
    }
    .to_string()
}

fn improvement_loop_commands() -> InteractiveImprovementLoopCommands {
    InteractiveImprovementLoopCommands {
        refresh: vec![
            "forge".to_string(),
            "interactive".to_string(),
            "improvement-loop".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        candidates: vec![
            "forge".to_string(),
            "improve".to_string(),
            "candidates".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        cost_ledger: vec![
            "forge".to_string(),
            "cost".to_string(),
            "ledger".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        structured_logs: vec![
            "forge".to_string(),
            "interactive".to_string(),
            "structured-logs".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        task_board: vec![
            "forge".to_string(),
            "interactive".to_string(),
            "task-board".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        validate: vec![
            "forge".to_string(),
            "validate".to_string(),
            "--workflow".to_string(),
            "<workflow-id>".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        apply_event_policy: vec![
            "forge".to_string(),
            "improve".to_string(),
            "apply-event-policy".to_string(),
            "--workflow".to_string(),
            "<workflow-id>".to_string(),
            "--recommendation".to_string(),
            "<recommendation-id>".to_string(),
            "--apply".to_string(),
            "--approved-by".to_string(),
            "<operator>".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        benchmark_event_policy: vec![
            "forge".to_string(),
            "improve".to_string(),
            "benchmark-event-policy".to_string(),
            "--workflow".to_string(),
            "<workflow-id>".to_string(),
            "--policy".to_string(),
            "<policy>".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        promote_event_policy: vec![
            "forge".to_string(),
            "improve".to_string(),
            "promote-event-policy".to_string(),
            "--workflow".to_string(),
            "<workflow-id>".to_string(),
            "--policy".to_string(),
            "<policy>".to_string(),
            "--approved-by".to_string(),
            "<operator>".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
    }
}

fn improvement_loop_next_actions(status: &str, candidate_count: usize) -> Vec<String> {
    let mut actions = vec![
        "forge interactive improvement-loop --output json".to_string(),
        "forge improve candidates --output json".to_string(),
        "forge interactive structured-logs --output json".to_string(),
        "forge cost ledger --output json".to_string(),
    ];
    if status == "improvement_loop_actionable" && candidate_count > 0 {
        actions.push(
            "inspect the top candidate and run only the suggested governed command for its evidence"
                .to_string(),
        );
        actions.push(
            "benchmark and explicitly approve event-policy changes before promotion".to_string(),
        );
    } else {
        actions.push(
            "start or route a workflow, then rerun the improvement loop to collect evidence"
                .to_string(),
        );
    }
    actions
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveContextMemoryPanel {
    pub schema_version: String,
    pub status: String,
    pub project_root: String,
    pub ready_for_handoff: usize,
    pub blocked_tasks: usize,
    pub context_budget_pressure: usize,
    pub memory_policy_status: String,
    pub memory_level_count: usize,
    pub temporary_memory_rule: String,
    pub memory_policy: MemoryPolicyReport,
    pub context_actions: RegistryContextActionSummary,
    pub context_quality: RegistryContextQualitySummary,
    pub memory_commands: BTreeMap<String, Vec<String>>,
    pub context_commands: BTreeMap<String, Vec<String>>,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveOperatingContextPanel {
    pub schema_version: String,
    pub status: String,
    pub project_root: String,
    pub context_status: String,
    pub tenant_path: String,
    pub organization_id: String,
    pub organization_label: String,
    pub brand_id: String,
    pub brand_label: String,
    pub product_id: String,
    pub product_label: String,
    pub user_id: String,
    pub channel_id: String,
    pub tenant_policy_mode: String,
    pub tenant_policy_status: String,
    pub memory_scope: String,
    pub memory_policy_status: String,
    pub memory_level: String,
    pub memory_scopes: Vec<String>,
    pub memory_audience: String,
    pub memory_governance_status: String,
    pub personality_scope: String,
    pub personality_status: String,
    pub brand_voice: String,
    pub brand_tone: String,
    pub brand_value_count: usize,
    pub design_token_source: String,
    pub component_source: String,
    pub prompt_packet_contract: InteractiveOperatingPromptPacketContract,
    pub company_work_contract: InteractiveOperatingCompanyWorkContract,
    pub prompt_packet_sample: InteractiveOperatingPromptPacketSample,
    pub memory_isolation_evidence: InteractiveOperatingMemoryIsolationEvidence,
    pub identity_summary: InteractiveOperatingIdentitySummary,
    pub handoff_context_summary: InteractiveOperatingHandoffContextSummary,
    pub commands: InteractiveOperatingContextCommands,
    pub next_actions: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveOperatingPromptPacketContract {
    pub schema_version: String,
    pub status: String,
    pub required_gates: Vec<String>,
    pub organization_context_required: bool,
    pub personality_decision_required: bool,
    pub company_work_decision_required: bool,
    pub evidence_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveOperatingCompanyWorkContract {
    pub schema_version: String,
    pub status: String,
    pub operating_depth: String,
    pub departments: Vec<String>,
    pub required_decisions: Vec<String>,
    pub sensitive_action_rule: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveOperatingPromptPacketSample {
    pub schema_version: String,
    pub status: String,
    pub source: String,
    pub workflow_id: Option<String>,
    pub task_id: Option<String>,
    pub task_title: Option<String>,
    pub task_executor: Option<String>,
    pub tenant_path: String,
    pub persona_mode: Option<String>,
    pub persona_profile_id: Option<String>,
    pub selected_voice: Option<String>,
    pub selected_tone: Option<String>,
    pub validation_gates: Vec<String>,
    pub organization_context_sha256: Option<String>,
    pub personality_decision_sha256: Option<String>,
    pub company_work_decision_sha256: Option<String>,
    pub packet_sha256: Option<String>,
    pub handoff_status: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveOperatingMemoryIsolationEvidence {
    pub schema_version: String,
    pub status: String,
    pub tenant_path: String,
    pub organization_id: String,
    pub brand_id: String,
    pub product_id: String,
    pub memory_scope: String,
    pub allowed_scopes: Vec<String>,
    pub default_audience: String,
    pub project_governance_status: String,
    pub isolation_keys: Vec<String>,
    pub governed_search_command: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveOperatingIdentitySummary {
    pub identity_count: usize,
    pub channel_alias_count: usize,
    pub membership_count: usize,
    pub active_membership_count: usize,
    pub tenant_audit_missing_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveOperatingHandoffContextSummary {
    pub ready_for_handoff: usize,
    pub blocked_tasks: usize,
    pub context_budget_pressure: usize,
    pub context_quality_status: String,
    pub required_context_missing: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveOperatingContextCommands {
    pub refresh: Vec<String>,
    pub identity: Vec<String>,
    pub context_memory: Vec<String>,
    pub memory_policy: Vec<String>,
    pub context_packet: Vec<String>,
    pub task_handoff: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveAddonRendererPanel {
    pub status: String,
    pub renderer_count: usize,
    pub safe_renderer_count: usize,
    pub family_count: usize,
    pub families: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SlashCommandCatalogReport {
    pub status: String,
    pub schema_version: String,
    pub commands: Vec<SlashCommandSpec>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SlashCommandSpec {
    pub name: String,
    pub title: String,
    pub description: String,
    pub equivalent_command: Vec<String>,
    pub scriptable: bool,
    pub mutates_workflow: bool,
    pub risk_level: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveRouteReport {
    pub status: String,
    pub schema_version: String,
    pub input_kind: String,
    pub routing_decision: String,
    pub routing_explanation: String,
    pub workflow_created: bool,
    pub run_id: Option<String>,
    pub workflow_id: Option<String>,
    pub answer: Option<String>,
    pub slash_command: Option<SlashCommandRoute>,
    pub product_decision_id: Option<String>,
    pub product_decision_revision: Option<u64>,
    pub retention_decision: RetentionDecision,
}

#[derive(Debug, Clone, Serialize)]
pub struct SlashCommandRoute {
    pub name: String,
    pub recognized: bool,
    pub input_arguments: Vec<String>,
    pub input_argument_text: String,
    pub equivalent_command: Vec<String>,
    pub mutates_workflow: bool,
    pub risk_level: String,
    pub execution_boundary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RetentionDecision {
    pub schema_version: String,
    pub action: String,
    pub reason: String,
    pub confidence: f32,
    pub requires_human_approval: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperationalTuiSmokeReport {
    pub schema_version: String,
    pub status: String,
    pub workflow_id: String,
    pub run_id: String,
    pub scheduled_workflow_id: String,
    pub event_id: i64,
    pub dashboard: OperationalTuiSmokeDashboard,
    pub checks: Vec<OperationalTuiSmokeCheck>,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperationalTuiSmokeDashboard {
    pub active_runs: usize,
    pub workflow_count: usize,
    pub event_count: usize,
    pub schedule_workflow_count: usize,
    pub addon_count: usize,
    pub capability_count: usize,
    pub core_boundary_status: String,
    pub domain_specific_core_leak_count: usize,
    pub cost_estimated_usd: f64,
    pub improvement_candidate_count: usize,
    pub structured_log_count: usize,
    pub workflow_mutation_workflow_count: usize,
    pub pending_mutation_proposal_count: usize,
    pub ready_handoff_count: usize,
    pub pending_approval_count: usize,
    pub guided_cockpit_step_count: usize,
    pub guided_cockpit_completed_step_count: usize,
    pub guided_cockpit_current_step: String,
    pub guided_cockpit_confirmation_step_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperationalTuiSmokeCheck {
    pub check_id: String,
    pub title: String,
    pub passed: bool,
    pub evidence: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForgeFirstHarnessSmokeReport {
    pub schema_version: String,
    pub status: String,
    pub executor: String,
    pub project_root: String,
    pub shim_dir: String,
    pub real_cmd: String,
    pub mutates_external_cli: bool,
    pub executes_external_cli: bool,
    pub headroom: TokenHeadroomReport,
    pub adoption_plan: HarnessAdoptionPlanReport,
    pub bootstrap_plan: HarnessBootstrapReport,
    pub shim_install: CliShimInstallReport,
    pub shim_status: CliShimStatusReport,
    pub exec_receipt: crate::harness::CliHarnessExecReceipt,
    pub checks: Vec<OperationalTuiSmokeCheck>,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReplacementCliEvidenceSmokeReport {
    pub schema_version: String,
    pub status: String,
    pub project_root: String,
    pub collect_ready: MilestoneCollectReadyEvidenceReport,
    pub release_gates: InteractiveReleaseGatesPanel,
    pub checks: Vec<OperationalTuiSmokeCheck>,
    pub commands: Vec<String>,
}

pub fn build_interactive_home(store: &ForgeStore) -> Result<InteractiveHomeReport> {
    build_interactive_home_with_options(store, InteractiveHomeOptions::default())
}

pub fn build_interactive_operational_cockpit(
    store: &ForgeStore,
) -> Result<InteractiveOperationalCockpitPanel> {
    let report = build_interactive_home(store)?;
    Ok(report.dashboard.operational_cockpit_panel)
}

pub fn build_interactive_ui_composition(
    store: &ForgeStore,
    project_root: Option<PathBuf>,
) -> Result<InteractiveUiCompositionPanel> {
    let report =
        build_interactive_home_with_options(store, InteractiveHomeOptions { project_root })?;
    Ok(report.dashboard.ui_composition_panel)
}

pub fn build_interactive_event_runtime(
    store: &ForgeStore,
    project_root: &Path,
) -> Result<InteractiveEventRuntimePanel> {
    let workflows = list_workflows_with_filters(
        store,
        WorkflowRegistryFilters::new(WorkflowLifecycleFilter::All),
    )?;
    Ok(build_event_runtime_panel(
        store,
        project_root,
        &workflows.workflows,
    ))
}

pub fn build_interactive_architecture_compass(
    store: &ForgeStore,
    project_root: Option<PathBuf>,
) -> Result<InteractiveArchitectureCompassPanel> {
    let report =
        build_interactive_home_with_options(store, InteractiveHomeOptions { project_root })?;
    Ok(report.dashboard.architecture_compass_panel)
}

pub fn build_interactive_operating_context(
    store: &ForgeStore,
    project_root: &Path,
) -> Result<InteractiveOperatingContextPanel> {
    let identity_panel = build_interactive_identity(store, project_root)?;
    let context_memory_panel = build_interactive_context_memory(store, project_root)?;
    build_operating_context_panel(store, project_root, &identity_panel, &context_memory_panel)
}

pub fn build_interactive_home_with_options(
    store: &ForgeStore,
    options: InteractiveHomeOptions,
) -> Result<InteractiveHomeReport> {
    let workflows = list_workflows_with_filters(
        store,
        WorkflowRegistryFilters::new(WorkflowLifecycleFilter::All),
    )?;
    let requests = crate::request::list_requests(store, None)?;
    let executors = load_executors(store)?;
    let runtimes = load_runtimes(store)?;

    let active_runs_list: Vec<&crate::request::RequestListRow> = requests
        .runs
        .iter()
        .filter(|run| run.activity.active || matches!(run.status.as_str(), "accepted" | "resumed"))
        .collect();
    let active_run_ids: Vec<String> = active_runs_list
        .iter()
        .take(5)
        .map(|run| run.run_id.clone())
        .collect();
    let active_runs = active_runs_list.len();
    let attention_runs = requests
        .runs
        .iter()
        .filter(|run| run.status == "needs_attention" || run.activity.heartbeat_status == "stale")
        .collect::<Vec<_>>();
    let runs_needing_attention = attention_runs.len();
    let attention_actions = build_attention_actions(&attention_runs);
    let scheduled_workflows = workflows
        .workflows
        .iter()
        .filter(|workflow| workflow.schedule_summary.scheduled_nodes > 0)
        .count();
    let looping_workflows = workflows
        .workflows
        .iter()
        .filter(|workflow| workflow.loop_summary.loop_nodes > 0)
        .count();
    let recent_artifacts = workflows
        .workflows
        .iter()
        .map(|workflow| workflow.artifact_count)
        .sum();
    let product_decisions = workflows
        .workflows
        .iter()
        .map(|workflow| workflow.product_decision_count)
        .sum();
    let validation_failures = workflows
        .workflows
        .iter()
        .map(|workflow| workflow.task_summary.failed + workflow.task_summary.blocked)
        .sum();
    let pending_human_interactions: usize = workflows
        .workflows
        .iter()
        .map(|workflow| workflow.human_interaction_summary.pending_required)
        .sum();
    let pending_approvals = usize::from(executors.needs_human_approval)
        + usize::from(runtimes.needs_human_approval)
        + pending_human_interactions;
    let executor_availability = if executors.usable.is_empty() {
        "no allowed executors; run /sync before executor handoff".to_string()
    } else {
        format!("usable executors: {}", executors.usable.join(", "))
    };
    let brain_router = format!(
        "{} controls {} surface(s) across {} brain adapter(s); selected brain: {}",
        executors.brain_router.controller,
        executors.brain_router.forge_controlled_surfaces.len(),
        executors.brain_router.brains.len(),
        executors
            .brain_router
            .selected_brain
            .as_deref()
            .unwrap_or("none")
    );
    let forge_controlled_surfaces = executors
        .brain_router
        .forge_controlled_surfaces
        .iter()
        .take(8)
        .cloned()
        .collect::<Vec<_>>();
    let shell_entrypoints = executors
        .brain_router
        .shell_sessions
        .iter()
        .map(|session| {
            format!(
                "{}: {}",
                session.id,
                if session.entry_command.is_empty() {
                    "<none>".to_string()
                } else {
                    session.entry_command.join(" ")
                }
            )
        })
        .collect::<Vec<_>>();
    let repository_context_path = options
        .project_root
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let repository_context_display = repository_context_path.display().to_string();
    let harness_shim_dir = default_interactive_harness_shim_dir();
    let harness_panel = build_interactive_harness(
        store,
        InteractiveHarnessOptions {
            executor: "codex".to_string(),
            shim_dir: harness_shim_dir,
            project_root: Some(repository_context_path.clone()),
            forge_first: false,
            observe_only: false,
            workflow_id: None,
            task_id: None,
            run_id: None,
            context_budget: None,
            token_headroom: None,
        },
    )?;
    let token_usage_panel = build_token_usage_panel(store)?;
    let sessions_panel = build_interactive_sessions(store, InteractiveSessionsOptions::default())?;
    let command_palette_panel = build_interactive_command_palette(store, None)?;
    let action_registry_panel = build_action_registry_from_palette(&command_palette_panel);
    let autocomplete_panel = build_interactive_autocomplete(store, "")?;
    let release_gates_panel =
        build_interactive_release_gates(store, "0.5", Some(&repository_context_path))?;
    let harness_mode_panel = harness_panel.mode.clone();
    let harness_doctor_panel = harness_panel.doctor.clone();
    let runtime_node_status = if runtimes.usable.is_empty() {
        "no allowed async run substrates".to_string()
    } else {
        format!("usable runtimes: {}", runtimes.usable.join(", "))
    };

    let schedule_panel = build_interactive_schedules(store);
    let scheduler_worker_status = if schedule_panel.scanned_workflows > 0
        || schedule_panel.due_workflows > 0
        || schedule_panel.next_wakeup_at.is_some()
    {
        let due = schedule_panel.runnable_due_workflows;
        let idle = schedule_panel.idle_workflows;
        let capacity = schedule_panel.worker_pool.available_workers;
        let sleep = if schedule_panel.sleep_until_next_wakeup {
            schedule_panel
                .next_wakeup_at
                .as_deref()
                .unwrap_or("now")
                .to_string()
        } else {
            "immediate".to_string()
        };
        format!("{due} due, {idle} idle, capacity {capacity}, next {sleep}")
    } else {
        "no scheduled workflows".to_string()
    };
    let workflow_focus = workflows
        .workflows
        .iter()
        .take(8)
        .map(|workflow| InteractiveWorkflowCard {
            workflow_id: workflow.workflow_id.clone(),
            goal: truncate_display(&workflow.current_goal, 96),
            lifecycle_state: workflow.lifecycle_state.clone(),
            operator_action: workflow.runtime.operator_action.clone(),
            context_action: workflow
                .context_action_refs
                .first()
                .map(|action| action.action.clone())
                .unwrap_or_else(|| "none".to_string()),
            quality_action: workflow.quality_action.action.clone(),
            tasks: format!(
                "{} total, {} pending, {} blocked, {} failed",
                workflow.task_summary.total,
                workflow.task_summary.pending,
                workflow.task_summary.blocked,
                workflow.task_summary.failed
            ),
            schedule: format!(
                "{} scheduled, {} due, next {}",
                workflow.schedule_summary.scheduled_nodes,
                workflow.schedule_summary.due_nodes,
                workflow
                    .schedule_summary
                    .next_run_at
                    .as_deref()
                    .unwrap_or("none")
            ),
        })
        .collect::<Vec<_>>();
    let workflow_sidebar_panel = build_workflow_sidebar_panel(&workflows.workflows);
    let dag_panel = build_workflow_dag_panel(store, &workflows.workflows)?;
    let task_board_panel = build_task_board_panel(store, &workflows.workflows)?;
    let artifact_panel = build_artifact_panel(store, &workflows.workflows)?;
    let modifier_lane = load_modifier_lane(store)?;
    let digital_twin_panel = build_operational_digital_twin(store, &modifier_lane)?;
    let timeline = build_global_event_timeline(store, None, None, None, None, Some(20), None).ok();
    let event_panel = timeline
        .as_ref()
        .map(build_interactive_event_panel)
        .unwrap_or_else(|| InteractiveEventPanel {
            status: "event_timeline_unavailable".to_string(),
            total_event_count: 0,
            visible_event_count: 0,
            latest_events: Vec::new(),
        });
    let event_runtime_panel =
        build_event_runtime_panel(store, &repository_context_path, &workflows.workflows);
    let structured_logs_panel = timeline
        .as_ref()
        .map(build_structured_logs_panel)
        .unwrap_or_else(|| InteractiveStructuredLogsPanel {
            schema_version: INTERACTIVE_STRUCTURED_LOGS_SCHEMA_VERSION.to_string(),
            status: "structured_logs_unavailable".to_string(),
            total_event_count: 0,
            log_count: 0,
            next_cursor: None,
            has_more: false,
            logs: Vec::new(),
        });
    let cost_panel = build_cost_ledger(store, None, None, None, None)
        .ok()
        .map(interactive_cost_panel_from_ledger)
        .unwrap_or_else(|| InteractiveCostPanel {
            status: "cost_ledger_unavailable".to_string(),
            workflow_count: 0,
            node_count: 0,
            ai_node_count: 0,
            deterministic_node_count: 0,
            model_call_avoided_node_count: 0,
            estimated_task_cost_total_usd: 0.0,
            observed_event_cost_total_usd: 0.0,
        });
    let improvement_candidates = rank_improvement_candidates(store, 10)?;
    let improvement_loop_panel = build_improvement_loop_panel(
        &improvement_candidates,
        &structured_logs_panel,
        &cost_panel,
        validation_failures,
        &workflows.summary.context_quality,
    );
    let context_memory_panel = build_context_memory_panel_from_summary(
        store,
        &repository_context_path,
        &workflows.summary.context_actions,
        &workflows.summary.context_quality,
    );
    let addon_dirs = addon_dirs_for_project(Some(&repository_context_path));
    let addon_catalog = load_addon_catalog_from_store(store, &addon_dirs).ok();
    let addon_renderer_report = addon_catalog
        .as_ref()
        .map(|catalog| {
            let addon_views = list_addon_views(catalog, None, None, Some("enabled"));
            build_addon_view_renderer_report(&addon_views)
        })
        .unwrap_or_else(|| OpsAddonViewRendererReport {
            schema_version: "forge.ops.addon_view_renderers.v1".to_string(),
            status: "addon_renderers_unavailable".to_string(),
            renderer_count: 0,
            safe_renderer_count: 0,
            family_count: 0,
            families: Vec::new(),
            renderers: Vec::new(),
        });
    let addon_capability_panel = build_interactive_addon_capabilities_with_project(
        store,
        addon_catalog.as_ref(),
        Some(&repository_context_path),
    );
    let core_boundary_panel =
        build_interactive_core_boundary_for_project(store, Some(&repository_context_path));
    let addon_renderer_panel = build_interactive_addon_renderer_panel(&addon_renderer_report);
    let patch_workbench_panel = build_interactive_patch_workbench(store)?;
    let permissions_panel = build_interactive_permissions(store)?;
    let identity_panel = build_interactive_identity(store, &repository_context_path)?;
    let operating_context_panel = build_operating_context_panel(
        store,
        &repository_context_path,
        &identity_panel,
        &context_memory_panel,
    )?;
    let replacement_cli_panel = build_interactive_replacement_cli_with_options(
        store,
        InteractiveReplacementCliOptions {
            project_root: Some(repository_context_path.clone()),
        },
    )?;
    let multimodal_runtime_panel =
        build_interactive_multimodal_runtime(store, &repository_context_path, false)?;
    let operational_cockpit_panel = build_operational_cockpit_panel(
        active_runs,
        runs_needing_attention,
        scheduled_workflows,
        looping_workflows,
        workflows.summary.non_running,
        pending_approvals,
        validation_failures,
        &task_board_panel,
        &schedule_panel,
        &sessions_panel,
        &harness_panel,
        &structured_logs_panel,
        &cost_panel,
        &context_memory_panel,
        &modifier_lane,
        &event_runtime_panel,
    );
    let workflow_mutation_panel = build_workflow_mutation_panel(
        &workflows.workflows,
        &task_board_panel,
        &dag_panel,
        &operational_cockpit_panel.modifier_lane,
        &event_panel,
        &cost_panel,
    );
    let guided_cockpit_panel = build_guided_cockpit_panel(GuidedCockpitInputs {
        active_runs,
        pending_approvals,
        validation_failures,
        task_board_panel: &task_board_panel,
        dag_panel: &dag_panel,
        workflow_mutation_panel: &workflow_mutation_panel,
        artifact_panel: &artifact_panel,
        event_panel: &event_panel,
        cost_panel: &cost_panel,
        improvement_loop_panel: &improvement_loop_panel,
    });
    let ui_composition_panel =
        build_ui_composition_panel(&addon_renderer_report, &repository_context_path);
    let architecture_compass_panel = build_architecture_compass_panel(ArchitectureCompassInputs {
        workflows: &workflows,
        schedule_panel: &schedule_panel,
        event_panel: &event_panel,
        event_runtime_panel: &event_runtime_panel,
        cost_panel: &cost_panel,
        improvement_loop_panel: &improvement_loop_panel,
        context_memory_panel: &context_memory_panel,
        addon_capability_panel: &addon_capability_panel,
        ui_composition_panel: &ui_composition_panel,
        task_board_panel: &task_board_panel,
        dag_panel: &dag_panel,
        workflow_mutation_panel: &workflow_mutation_panel,
        artifact_panel: &artifact_panel,
        operational_cockpit_panel: &operational_cockpit_panel,
        harness_panel: &harness_panel,
        release_gates_panel: &release_gates_panel,
        operating_context_panel: &operating_context_panel,
        digital_twin_panel: &digital_twin_panel,
        replacement_cli_panel: &replacement_cli_panel,
        multimodal_runtime_panel: &multimodal_runtime_panel,
    });

    Ok(InteractiveHomeReport {
        status: "interactive_home_ready".to_string(),
        schema_version: INTERACTIVE_HOME_SCHEMA_VERSION.to_string(),
        banner: InteractiveBanner {
            mark: anvil_mark().to_string(),
            name: "forge".to_string(),
        },
        dashboard: InteractiveDashboard {
            active_runs,
            active_run_ids,
            runs_needing_attention,
            scheduled_workflows,
            looping_workflows,
            paused_idle_workflows: workflows.summary.non_running,
            recent_artifacts,
            product_decisions,
            pending_approvals,
            validation_failures,
            executor_availability,
            brain_router,
            forge_controlled_surfaces,
            shell_entrypoints,
            harness_panel,
            token_usage_panel,
            sessions_panel,
            command_palette_panel,
            action_registry_panel,
            autocomplete_panel,
            release_gates_panel,
            harness_mode_panel,
            harness_doctor_panel,
            runtime_node_status,
            repository_context: repository_context_path.display().to_string(),
            estimated_costs: "available per workflow via /costs or forge run --simulate"
                .to_string(),
            scheduler_worker_status,
            workflow_focus,
            workflow_sidebar_panel,
            replacement_cli_panel,
            multimodal_runtime_panel,
            navigation_panel: build_navigation_panel(),
            ui_composition_panel,
            patch_workbench_panel,
            permissions_panel,
            identity_panel,
            operating_context_panel,
            dag_panel,
            task_board_panel,
            workflow_mutation_panel,
            guided_cockpit_panel,
            artifact_panel,
            schedule_panel,
            event_panel,
            event_runtime_panel,
            structured_logs_panel,
            cost_panel,
            improvement_loop_panel,
            context_memory_panel,
            digital_twin_panel,
            operational_cockpit_panel,
            architecture_compass_panel,
            core_boundary_panel,
            addon_capability_panel,
            addon_renderer_panel,
            attention_actions,
            useful_next_commands: vec![
                "forge".to_string(),
                "forge interactive guided-cockpit --output json".to_string(),
                "forge list".to_string(),
                "forge inspect <workflow-id>".to_string(),
                "forge request list".to_string(),
                "forge interactive workflow-sidebar --output json".to_string(),
                format!(
                    "forge interactive architecture --project-root {repository_context_display} --output json"
                ),
                "forge interactive core-boundary --output json".to_string(),
                format!(
                    "forge interactive replacement-cli --project-root {repository_context_display} --output json"
                ),
                format!(
                    "forge interactive multimodal-runtime --project-root {repository_context_display} --output json"
                ),
                "forge schedule list".to_string(),
                "forge interactive schedules --output json".to_string(),
                "forge schedule worker-status".to_string(),
                format!(
                    "forge interactive harness --project-root {repository_context_display} --output json"
                ),
                "forge interactive token-usage --output json".to_string(),
                format!(
                    "forge harness headroom-plan --executor codex --project-root {repository_context_display} --output json"
                ),
                "forge harness headroom-stats --output json".to_string(),
                format!(
                    "forge harness adoption-plan --executor codex --shim-dir $HOME/.forge/bin --project-root {repository_context_display} --output json"
                ),
                format!(
                    "forge harness bootstrap --executor codex --shim-dir $HOME/.forge/bin --project-root {repository_context_display} --apply --approved-by <operator> --output json"
                ),
                "forge interactive sessions --output json".to_string(),
                "forge interactive action-registry --output json".to_string(),
                "forge interactive artifacts --output json".to_string(),
                "forge interactive workflow-mutation --output json".to_string(),
                "forge interactive improvement-loop --output json".to_string(),
                "forge improve candidates --output json".to_string(),
                "forge interactive addon-capabilities --output json".to_string(),
                "forge interactive context-memory --output json".to_string(),
                "forge addons observability --output json".to_string(),
                "forge interactive release-gates --output json".to_string(),
                "forge interactive patch-workbench --output json".to_string(),
                "forge interactive permissions --output json".to_string(),
                "forge interactive identity --output json".to_string(),
            ],
            quick_actions: vec![
                "/guided-cockpit".to_string(),
                "/guide".to_string(),
                "/cockpit".to_string(),
                "/ui-composition".to_string(),
                "/status".to_string(),
                "/workflows".to_string(),
                "/workflow-sidebar".to_string(),
                "/architecture".to_string(),
                "/core-boundary".to_string(),
                "/replacement-cli".to_string(),
                "/multimodal-runtime".to_string(),
                "/runs".to_string(),
                "/artifacts".to_string(),
                "/task-board".to_string(),
                "/workflow-mutation".to_string(),
                "/readiness".to_string(),
                "/schedules".to_string(),
                "/addons".to_string(),
                "/milestone".to_string(),
                "/sync".to_string(),
                "/brains".to_string(),
                "/sessions".to_string(),
                "/shells".to_string(),
                "/harness".to_string(),
                "/tokens".to_string(),
                "/harness doctor".to_string(),
                "/harness headroom-plan".to_string(),
                "/harness headroom-stats".to_string(),
                "/harness adoption-plan".to_string(),
                "/harness bootstrap".to_string(),
                "/validate".to_string(),
                "/logs".to_string(),
                "/improvement-loop".to_string(),
                "/workers".to_string(),
                "/operating-context".to_string(),
                "/context-memory".to_string(),
                "/context".to_string(),
                "/handoff".to_string(),
                "/pm".to_string(),
                "/decision".to_string(),
            ],
        },
        slash_commands: slash_commands(),
    })
}

struct ArchitectureCompassInputs<'a> {
    workflows: &'a crate::registry::WorkflowRegistryReport,
    schedule_panel: &'a InteractiveSchedulePanel,
    event_panel: &'a InteractiveEventPanel,
    event_runtime_panel: &'a InteractiveEventRuntimePanel,
    cost_panel: &'a InteractiveCostPanel,
    improvement_loop_panel: &'a InteractiveImprovementLoopPanel,
    context_memory_panel: &'a InteractiveContextMemoryPanel,
    addon_capability_panel: &'a InteractiveAddonCapabilityPanel,
    ui_composition_panel: &'a InteractiveUiCompositionPanel,
    task_board_panel: &'a InteractiveTaskBoardPanel,
    dag_panel: &'a InteractiveWorkflowDagPanel,
    workflow_mutation_panel: &'a InteractiveWorkflowMutationPanel,
    artifact_panel: &'a InteractiveArtifactPanel,
    operational_cockpit_panel: &'a InteractiveOperationalCockpitPanel,
    harness_panel: &'a InteractiveHarnessPanel,
    release_gates_panel: &'a InteractiveReleaseGatesPanel,
    operating_context_panel: &'a InteractiveOperatingContextPanel,
    digital_twin_panel: &'a OpsOperationalDigitalTwin,
    replacement_cli_panel: &'a InteractiveReplacementCliPanel,
    multimodal_runtime_panel: &'a InteractiveMultimodalRuntimePanel,
}

fn build_architecture_compass_panel(
    inputs: ArchitectureCompassInputs<'_>,
) -> InteractiveArchitectureCompassPanel {
    let operating_context = architecture_operating_context_summary(inputs.operating_context_panel);
    let tenant_prompt_sample_ready = inputs
        .operating_context_panel
        .prompt_packet_sample
        .status
        .starts_with("prompt_packet_sample_ready");
    let tenant_memory_isolation_ready = inputs
        .operating_context_panel
        .memory_isolation_evidence
        .status
        == "tenant_memory_isolation_ready";
    let tenant_context_ready = inputs.operating_context_panel.status == "operating_context_ready"
        && inputs
            .operating_context_panel
            .prompt_packet_contract
            .organization_context_required
        && inputs
            .operating_context_panel
            .prompt_packet_contract
            .personality_decision_required
        && inputs
            .operating_context_panel
            .prompt_packet_contract
            .company_work_decision_required
        && tenant_prompt_sample_ready
        && tenant_memory_isolation_ready;
    let mut tenant_gaps = Vec::new();
    if !tenant_prompt_sample_ready {
        tenant_gaps.push(
            "Evidenciar personalidade por node com amostras de context packet reais no painel."
                .to_string(),
        );
    }
    if !tenant_memory_isolation_ready {
        tenant_gaps.push(
            "Adicionar mais evidência de memória organizacional isolada por empresa/marca/produto em stores com contexto explícito."
                .to_string(),
        );
    }

    let mut tracks = vec![
        architecture_track(
            "world_class_tui",
            "TUI operacional de classe mundial",
            &["goal1:Fase 2", "goal1:linhas 85-122"],
            architecture_status(
                inputs.operational_cockpit_panel.status == "operational_cockpit_ready"
                    && inputs.task_board_panel.workflow_count > 0
                    && inputs.workflow_mutation_panel.workflow_count > 0
                    && inputs.ui_composition_panel.core_widget_count > 0,
                inputs.ui_composition_panel.widget_count > 0,
            ),
            vec![
                format!(
                    "operational_cockpit:{} sections={}",
                    inputs.operational_cockpit_panel.status,
                    inputs.operational_cockpit_panel.sections.len()
                ),
                format!(
                    "task_board:workflows={} ready_handoffs={}",
                    inputs.task_board_panel.workflow_count, inputs.task_board_panel.ready_handoffs
                ),
                format!(
                    "workflow_mutation:{} workflows={} mutable={} proposals={}",
                    inputs.workflow_mutation_panel.status,
                    inputs.workflow_mutation_panel.workflow_count,
                    inputs.workflow_mutation_panel.mutable_workflow_count,
                    inputs.workflow_mutation_panel.pending_modifier_proposal_count
                ),
                format!(
                    "ui_composition:widgets={} addon_widgets={}",
                    inputs.ui_composition_panel.widget_count,
                    inputs.ui_composition_panel.addon_widget_count
                ),
            ],
            vec![
                "Adicionar mais interação direta nos painéis sem transformar a TUI em lógica de domínio."
                    .to_string(),
                "Fortalecer visualização de edição/repriorização de workflows em execução.".to_string(),
            ],
            "Expandir affordances operacionais por action registry e widgets compostos.",
            "Core entrega navegação, painéis, ações e composição; UIs especializadas vêm de Addons.",
        ),
        architecture_track(
            "dynamic_workflow_engine",
            "Dynamic workflow engine",
            &["goal1:Fase 3", "goal1:Fase 7"],
            architecture_status(
                inputs.workflow_mutation_panel.workflow_count > 0
                    && inputs.workflow_mutation_panel.mutable_workflow_count > 0
                    && inputs.dag_panel.node_count > 0
                    && (inputs.task_board_panel.checkpoint_resume_candidates > 0
                        || inputs.workflow_mutation_panel.pending_modifier_proposal_count
                            + inputs.workflow_mutation_panel.applied_modifier_proposal_count
                            > 0)
                    && inputs.workflows.summary.total > 0,
                inputs.workflow_mutation_panel.status != "workflow_mutation_idle"
                    || inputs.workflows.summary.total > 0,
            ),
            vec![
                format!(
                    "registry:workflows={} running={} non_running={}",
                    inputs.workflows.summary.total,
                    inputs.workflows.summary.running,
                    inputs.workflows.summary.non_running
                ),
                format!(
                    "dag:nodes={} edges={} waits={}",
                    inputs.dag_panel.node_count,
                    inputs.dag_panel.edge_count,
                    inputs.dag_panel.wait_node_count
                ),
                format!(
                    "task_board:checkpoints={} human_waits={}",
                    inputs.task_board_panel.checkpoint_resume_candidates,
                    inputs.task_board_panel.pending_human_interactions
                ),
                format!(
                    "workflow_mutation:{} mutable={} handoffs={} proposals={}",
                    inputs.workflow_mutation_panel.status,
                    inputs.workflow_mutation_panel.mutable_workflow_count,
                    inputs.workflow_mutation_panel.ready_handoff_count,
                    inputs.workflow_mutation_panel.pending_modifier_proposal_count
                ),
            ],
            vec![
                "Aumentar cobertura de mutação visual de node/goal durante execução pela própria TUI."
                    .to_string(),
                "Evidenciar replanejamento e subworkflows dinâmicos como fluxo operacional principal."
                    .to_string(),
            ],
            "Aproximar DAG, task-board e modifier lane em uma rotina de replanejamento assistido.",
            "Core mantém grafo, estado e gates; estratégias especializadas de planejamento são Addons.",
        ),
        architecture_track(
            "event_persistent_runtime",
            "Workflows event-driven, persistentes e efêmeros",
            &["goal1:Fase 3.6", "goal1:Fase 3.7", "goal3:Event Engine"],
            architecture_status(
                !inputs.schedule_panel.workflows.is_empty()
                    && inputs.event_runtime_panel.status != "event_runtime_unavailable"
                    && inputs.event_panel.total_event_count > 0,
                inputs.event_runtime_panel.status != "event_runtime_unavailable"
                    || !inputs.schedule_panel.workflows.is_empty(),
            ),
            vec![
                format!(
                    "schedules:scheduled={} due={} runnable={}",
                    inputs.schedule_panel.workflows.len(),
                    inputs.schedule_panel.due_workflows,
                    inputs.schedule_panel.runnable_due_workflows
                ),
                format!(
                    "events:visible={} total={}",
                    inputs.event_panel.visible_event_count, inputs.event_panel.total_event_count
                ),
                format!(
                    "event_runtime:{} pending={}",
                    inputs.event_runtime_panel.status, inputs.event_runtime_panel.pending_event_count
                ),
            ],
            vec![
                "Consolidar runtime contínuo supervisionado como caminho de produção, não só CLI bounded."
                    .to_string(),
                "Ampliar adapters declarativos via Addons para canais externos sem alterar Core."
                    .to_string(),
            ],
            "Usar event runtime + scheduler como base para workers persistentes e scale-to-zero.",
            "Core só normaliza eventos/schedules; transportes e canais específicos pertencem a Addons.",
        ),
        architecture_track(
            "core_addons_domain_agnostic",
            "Core mínimo + Addons domain-agnostic",
            &["goal1:Fase 3.5", "goal3:Addons", "goal3:linhas 358-365"],
            architecture_status(
                inputs.addon_capability_panel.enabled_capability_count > 0
                    && inputs.addon_capability_panel.runtime_contract_count > 0
                    && inputs.addon_capability_panel.view_count > 0,
                inputs.addon_capability_panel.capability_count > 0,
            ),
            vec![
                format!(
                    "addons={} enabled={} capabilities={} contracts={} views={}",
                    inputs.addon_capability_panel.addon_count,
                    inputs.addon_capability_panel.enabled_addon_count,
                    inputs.addon_capability_panel.capability_count,
                    inputs.addon_capability_panel.runtime_contract_count,
                    inputs.addon_capability_panel.view_count
                ),
                format!(
                    "permissions={} dispatches={} queued={}",
                    inputs.addon_capability_panel.permission_count,
                    inputs.addon_capability_panel.dispatch_count,
                    inputs.addon_capability_panel.queued_dispatch_count
                ),
            ],
            vec![
                "Continuar movendo capacidades específicas de domínio para manifests/workers Addon."
                    .to_string(),
                "Fortalecer marketplace/compatibilidade como rotina de instalação sem recompilar Core."
                    .to_string(),
            ],
            "Priorizar novos domínios por capability registry e contratos Addon.",
            "Core fornece registry, permissões, eventos e dispatch; domínio fica fora do binário base.",
        ),
        architecture_track(
            "tenant_identity_personality_context",
            "Multi-tenant, identidade, memória e personality/context routing",
            &["goal1:Fase 5", "goal1:Fase 5.5", "goal2:Fase 5.7", "goal2:Fase 5.8"],
            architecture_status(
                tenant_context_ready,
                inputs.operating_context_panel.status != "operating_context_needs_attention",
            ),
            vec![
                format!(
                    "operating_context:{} tenant={} policy={}",
                    inputs.operating_context_panel.status,
                    inputs.operating_context_panel.tenant_path,
                    inputs.operating_context_panel.tenant_policy_status
                ),
                format!(
                    "memory:{} level={} scopes={}",
                    inputs.operating_context_panel.memory_policy_status,
                    inputs.operating_context_panel.memory_level,
                    inputs.operating_context_panel.memory_scopes.join("+")
                ),
                format!(
                    "prompt_packet:{} gates={}",
                    inputs.operating_context_panel.prompt_packet_contract.status,
                    inputs
                        .operating_context_panel
                        .prompt_packet_contract
                        .required_gates
                        .join("+")
                ),
                format!(
                    "prompt_packet_sample:{} task={} persona={} packet={}",
                    inputs.operating_context_panel.prompt_packet_sample.status,
                    inputs
                        .operating_context_panel
                        .prompt_packet_sample
                        .task_id
                        .as_deref()
                        .unwrap_or("none"),
                    inputs
                        .operating_context_panel
                        .prompt_packet_sample
                        .persona_mode
                        .as_deref()
                        .unwrap_or("brand_default"),
                    inputs
                        .operating_context_panel
                        .prompt_packet_sample
                        .packet_sha256
                        .as_deref()
                        .unwrap_or("missing")
                ),
                format!(
                    "memory_isolation:{} keys={}",
                    inputs
                        .operating_context_panel
                        .memory_isolation_evidence
                        .status,
                    inputs
                        .operating_context_panel
                        .memory_isolation_evidence
                        .isolation_keys
                        .join("+")
                ),
            ],
            tenant_gaps,
            "Usar forge interactive operating-context para decidir identity, memory policy, prompt-packet gates e handoff readiness antes de executar brains.",
            "Core decide escopo/isolamento; personas, brand assets e dados de domínio ficam em contexto/Addons.",
        ),
        architecture_track(
            "harness_headroom_cli_brains",
            "Harness, headroom e CLIs como brains substituíveis",
            &["goal1:Fase 1", "goal1:Fase 2", "headroom benchmark"],
            architecture_status(
                inputs.harness_panel.token_headroom_ready
                    && inputs.harness_panel.forge_first_adoption_readiness.ready_to_use_as_default,
                inputs.harness_panel.token_headroom_ready
                    || inputs.replacement_cli_panel.ready_surface_count > 0,
            ),
            vec![
                format!(
                    "harness:{} headroom={} forge_first_status={}",
                    inputs.harness_panel.status,
                    inputs.harness_panel.token_headroom_ready,
                    inputs.harness_panel.forge_first_adoption_readiness.status
                ),
                format!(
                    "replacement_cli:ready={}/{} readiness={}%",
                    inputs.replacement_cli_panel.ready_surface_count,
                    inputs.replacement_cli_panel.surface_count,
                    inputs.replacement_cli_panel.readiness_percent
                ),
                format!(
                    "release_gates:{} blocked={}",
                    inputs.release_gates_panel.status, inputs.release_gates_panel.blocked_gate_count
                ),
            ],
            inputs
                .harness_panel
                .forge_first_adoption_readiness
                .blocked_reasons
                .iter()
                .map(|reason| format!("Forge-first default blocked by {reason}."))
                .chain(std::iter::once(
                    "Provider/model execution evidence must remain real and approved before promotion."
                        .to_string(),
                ))
                .collect(),
            "Instalar shims aprovados e continuar usando headroom reversível sem executar CLIs externas em smokes.",
            "Forge controla contexto, memória, permissões, custos e sessões; CLIs executam como brains substituíveis.",
        ),
        architecture_track(
            "human_ai_visual_copilot",
            "Human + AI visual copilot",
            &["goal1:Fase 7", "goal2:Fase 7.8", "goal3:UI Composition"],
            architecture_status(
                inputs.workflow_mutation_panel.workflow_count > 0
                    && inputs.digital_twin_panel.workflow_count > 0
                    && inputs.operational_cockpit_panel.modifier_lane.pending_count
                        + inputs.operational_cockpit_panel.modifier_lane.applied_count
                        > 0,
                inputs.digital_twin_panel.workflow_count > 0
                    || inputs.artifact_panel.artifact_count > 0
                    || inputs.workflow_mutation_panel.status != "workflow_mutation_idle",
            ),
            vec![
                format!(
                    "digital_twin:workflows={} remaining={} approvals={}",
                    inputs.digital_twin_panel.workflow_count,
                    inputs.digital_twin_panel.global_counts.remaining_count,
                    inputs
                        .digital_twin_panel
                        .global_counts
                        .awaiting_approval_count
                ),
                format!(
                    "artifacts:workflows={} artifacts={}",
                    inputs.artifact_panel.workflow_count, inputs.artifact_panel.artifact_count
                ),
                format!(
                    "modifier_lane:pending={} applied={}",
                    inputs.operational_cockpit_panel.modifier_lane.pending_count,
                    inputs.operational_cockpit_panel.modifier_lane.applied_count
                ),
                format!(
                    "workflow_mutation:{} cards={} next_actions={}",
                    inputs.workflow_mutation_panel.status,
                    inputs.workflow_mutation_panel.workflow_cards.len(),
                    inputs.workflow_mutation_panel.next_actions.len()
                ),
            ],
            vec![
                "Aproximar whiteboard/design surface do fluxo principal da TUI terminal.".to_string(),
                "Evidenciar colaboração humano+IA em tempo real nos workflows longos.".to_string(),
            ],
            "Conectar artifacts, digital twin e modifier lane como uma operação assistida contínua.",
            "Core guarda artefatos e eventos; editores especializados podem ser Addons ou superfícies Ops.",
        ),
        architecture_track(
            "observability_cost_validation",
            "Observabilidade, custos e validação",
            &["goal1:Fase 6", "goal1:Fase 4"],
            architecture_status(
                inputs.improvement_loop_panel.candidate_count > 0
                    && inputs.improvement_loop_panel.structured_log_count > 0
                    && inputs.improvement_loop_panel.ai_node_count
                        + inputs.improvement_loop_panel.model_call_avoided_node_count
                        > 0,
                inputs.improvement_loop_panel.status != "improvement_loop_idle",
            ),
            vec![
                format!(
                    "improvement_loop:{} candidates={} critical={} high={}",
                    inputs.improvement_loop_panel.status,
                    inputs.improvement_loop_panel.candidate_count,
                    inputs.improvement_loop_panel.critical_candidate_count,
                    inputs.improvement_loop_panel.high_candidate_count
                ),
                format!(
                    "cost_observability:nodes={} ai={} deterministic={} avoided={} estimated=${:.4} observed=${:.4}",
                    inputs.cost_panel.node_count,
                    inputs.cost_panel.ai_node_count,
                    inputs.cost_panel.deterministic_node_count,
                    inputs.cost_panel.model_call_avoided_node_count,
                    inputs.improvement_loop_panel.estimated_cost_total_usd,
                    inputs.improvement_loop_panel.observed_cost_total_usd
                ),
                format!(
                    "validation_quality:failures={} context={} logs={}/{}",
                    inputs.improvement_loop_panel.validation_failure_count,
                    inputs.improvement_loop_panel.context_quality_status,
                    inputs.improvement_loop_panel.structured_log_count,
                    inputs.improvement_loop_panel.event_count
                ),
            ],
            vec![
                "Materializar séries históricas de custo/latência com maintenance daemon como rotina de produção."
                    .to_string(),
                "Ligar validação de outcome final a gates de promoção automatizados no painel."
                    .to_string(),
            ],
            "Usar forge interactive improvement-loop para decidir recover, parallelize, normalize AI cost ou event-policy experiment antes de mutações.",
            "Core mede e valida; políticas de otimização específicas devem ser configuráveis.",
        ),
    ];

    tracks.sort_by(|left, right| left.track_id.cmp(&right.track_id));
    let open_gap_count = tracks.iter().map(|track| track.gaps.len()).sum::<usize>();
    let validated_count = tracks
        .iter()
        .filter(|track| track.status == "validated")
        .count();
    let status = if open_gap_count == 0 && validated_count == tracks.len() {
        "architecture_compass_validated"
    } else {
        "architecture_compass_actionable"
    };
    let operating_context_project_root = operating_context.project_root.clone();
    let execution_plan = architecture_execution_plan(&tracks, &operating_context_project_root);

    InteractiveArchitectureCompassPanel {
        schema_version: INTERACTIVE_ARCHITECTURE_COMPASS_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        source_documents: vec![
            architecture_source_document(
                "goal1",
                "base AI-native workflow OS vision",
                487,
                "8393a777f02baf5145cee3be6bd764d8b9cebb5c6a5eaa6c0b968f0e46144eea",
            ),
            architecture_source_document(
                "goal2",
                "expanded benchmark, multi-tenant and personality routing vision",
                298,
                "3a0a4f0d9468b8030f62137f7fd855c18443fb1ada5e9937e7e9ab898f46055c",
            ),
            architecture_source_document(
                "goal3",
                "Core + Addons domain-agnostic architecture",
                378,
                "f66292a967fcf8db55a19f26ec28da7c7d207f4f35940d7ff0410599521ace1f",
            ),
        ],
        operating_context,
        tracks,
        benchmark_sources: architecture_benchmark_sources(inputs),
        execution_plan,
        dependencies: vec![
            architecture_dependency(
                "Forge-first CLI adoption",
                &["harness_headroom_cli_brains", "tenant_identity_personality_context"],
                "CLI shims must preserve Forge-owned lineage, memory, permissions and token headroom before becoming a default entrypoint.",
            ),
            architecture_dependency(
                "Persistent event runtime",
                &["event_persistent_runtime", "core_addons_domain_agnostic"],
                "Long-running workflows need event adapters and workers without baking channel-specific code into Core.",
            ),
            architecture_dependency(
                "Human + AI visual operation",
                &["world_class_tui", "human_ai_visual_copilot", "dynamic_workflow_engine"],
                "Live mutation, artifacts and task state must share the same workflow graph and audit trail.",
            ),
            architecture_dependency(
                "Organizational copilots",
                &["tenant_identity_personality_context", "core_addons_domain_agnostic"],
                "Company, brand, product and persona context must route through identity/memory before domain Addons execute.",
            ),
        ],
        conflicts: vec![
            "Do not move domain-specific behavior into Core; route it through capabilities, Addons, manifests and workers."
                .to_string(),
            "Do not treat Codex, Gemini, Claude or OpenCode as the orchestrator; they are replaceable execution brains."
                .to_string(),
            "Do not claim provider/model evidence without approved manifests and real execution receipts."
                .to_string(),
        ],
        reuse_opportunities: vec![
            "Reuse interactive home, task board, DAG, event runtime, Addon capability and cost panels as the TUI data plane."
                .to_string(),
            "Reuse action registry/autocomplete for all new operator actions instead of creating one-off terminal commands."
                .to_string(),
            "Reuse Addon runtime contracts for domain capabilities and external workers.".to_string(),
            "Reuse harness headroom receipts for CLI wrappers and future brain output compression.".to_string(),
        ],
        next_commands: vec![
            format!(
                "forge interactive architecture --project-root {} --output json",
                operating_context_project_root
            ),
            format!(
                "forge interactive home --project-root {} --output json",
                operating_context_project_root
            ),
            "forge interactive operational-cockpit --output json".to_string(),
            "forge interactive addon-capabilities --output json".to_string(),
            "forge interactive improvement-loop --output json".to_string(),
            "forge interactive harness --output json".to_string(),
            "forge smoke operational-tui --output json".to_string(),
            "forge smoke forge-first-harness --output json".to_string(),
        ],
    }
}

fn architecture_operating_context_summary(
    panel: &InteractiveOperatingContextPanel,
) -> InteractiveArchitectureOperatingContextSummary {
    InteractiveArchitectureOperatingContextSummary {
        schema_version: "forge.interactive.architecture_operating_context.v1".to_string(),
        project_root: panel.project_root.clone(),
        status: panel.status.clone(),
        context_status: panel.context_status.clone(),
        tenant_path: panel.tenant_path.clone(),
        organization_id: panel.organization_id.clone(),
        organization_label: panel.organization_label.clone(),
        brand_id: panel.brand_id.clone(),
        brand_label: panel.brand_label.clone(),
        product_id: panel.product_id.clone(),
        product_label: panel.product_label.clone(),
        user_id: panel.user_id.clone(),
        channel_id: panel.channel_id.clone(),
        memory_scope: panel.memory_scope.clone(),
        memory_level: panel.memory_level.clone(),
        memory_scopes: panel.memory_scopes.clone(),
        memory_audience: panel.memory_audience.clone(),
        personality_scope: panel.personality_scope.clone(),
        personality_status: panel.personality_status.clone(),
        brand_voice: panel.brand_voice.clone(),
        brand_tone: panel.brand_tone.clone(),
        design_token_source: panel.design_token_source.clone(),
        component_source: panel.component_source.clone(),
        prompt_packet_gates: panel.prompt_packet_contract.required_gates.clone(),
        company_work_departments: panel.company_work_contract.departments.clone(),
        tenant_policy_status: panel.tenant_policy_status.clone(),
        memory_policy_status: panel.memory_policy_status.clone(),
        evidence_commands: vec![
            format!(
                "forge interactive operating-context --project-root {} --output json",
                panel.project_root
            ),
            format!(
                "forge interactive identity --project-root {} --output json",
                panel.project_root
            ),
            format!(
                "forge interactive context-memory --project-root {} --output json",
                panel.project_root
            ),
        ],
    }
}

fn architecture_execution_plan(
    tracks: &[InteractiveArchitectureTrack],
    project_root: &str,
) -> InteractiveArchitectureExecutionPlan {
    let increments = vec![
        architecture_increment(
            "stabilize_operational_tui",
            1,
            "TUI operacional como entrada padrão do Forge",
            architecture_increment_status(tracks, &["world_class_tui", "observability_cost_validation"]),
            &["goal1:Fase 2", "goal1:Fase 6", "goal1:Critério 10"],
            &["core_addons_domain_agnostic"],
            &[
                "Operadores enxergam workflows, eventos, schedules, Addons, custos e aprovações sem abrir painéis separados.",
                "Home, REPL, MCP e smoke compartilham o mesmo contrato de dados.",
            ],
            "Core pode renderizar cockpit, navegação, task-board, DAG, logs, custos e composição; não pode embutir UX específica de domínio.",
            "Addons fornecem widgets especializados e ações de domínio através de views/capabilities, mantendo o Core genérico.",
            &[
                "`forge` abre uma superfície operacional útil em TTY e em snapshot sem TTY.",
                "`forge smoke operational-tui --output json` passa todos os checks obrigatórios.",
                "A home mostra Architecture Compass e execução incremental sem mutar workflow.",
            ],
            &[
                "forge",
                "forge interactive home --output json",
                "forge smoke operational-tui --output json",
            ],
            &[
                "Não criar atalhos de domínio no Core.",
                "Manter o snapshot sem TTY scriptável para CI e agentes.",
            ],
        ),
        architecture_increment(
            "activate_dynamic_event_runtime",
            2,
            "Runtime dinâmico, event-driven, persistente e efêmero",
            architecture_increment_status(
                tracks,
                &["dynamic_workflow_engine", "event_persistent_runtime"],
            ),
            &[
                "goal1:Fase 3",
                "goal1:Fase 3.6",
                "goal1:Fase 3.7",
                "goal3:Event Extensions",
            ],
            &["stabilize_operational_tui", "core_addons_domain_agnostic"],
            &[
                "Workflows podem iniciar, continuar, pausar, retomar e evoluir por eventos.",
                "Schedulers, waits, checkpoints e resume viram rotina operacional visível.",
            ],
            "Core mantém grafo, eventos normalizados, leases, estado, checkpoints, waits e validação.",
            "Transportes como Telegram, WhatsApp, Kafka, MQTT, sensores e CRMs entram como Addons/adapters.",
            &[
                "Eventos inbound aparecem no event runtime e podem acionar workflows sem código de canal no Core.",
                "Workflows persistentes e efêmeros aparecem no task-board, DAG e timeline.",
                "Replanejamento preserva revisions, checkpoints e auditoria.",
            ],
            &[
                "forge interactive workflow-mutation --output json",
                "forge interactive workflow-dag --output json",
                "forge interactive schedules --output json",
                "forge interactive structured-logs --output json",
                "forge events runtime-reconcile --project-root . --output json",
            ],
            &[
                "Não confundir agente permanente com primitive separada de workflow.",
                "Não deixar adapters externos pularem permissões, tenant policy ou evento global.",
            ],
        ),
        architecture_increment(
            "harden_core_addon_kernel",
            3,
            "Kernel Core + Addons domain-agnostic",
            architecture_increment_status(tracks, &["core_addons_domain_agnostic"]),
            &[
                "goal1:Fase 3.5",
                "goal3:Core Responsibilities",
                "goal3:Capability Discovery",
                "goal3:Addon Lifecycle",
            ],
            &["activate_dynamic_event_runtime"],
            &[
                "Novos domínios entram por capabilities, manifests, workers, views e validators.",
                "Core continua pequeno, universal e capaz de operar sem Addon específico de domínio.",
            ],
            "Core conhece goals, workflows, events, context, memory, identity, permissions, artifacts, observability, scheduling, runtime, UI composition e registries.",
            "Domínios como software, SDR, logística, saúde, jurídico e RH ficam em Addons instaláveis/removíveis.",
            &[
                "Capability discovery sugere Addons ausentes sem templates fixos.",
                "Install/enable/disable/upgrade/downgrade preservam compatibilidade e permissões.",
                "Addon views compõem UI sem recompilar Core.",
            ],
            &[
                "forge interactive addon-capabilities --output json",
                "forge addons catalog --output json",
                "forge addons resolve --goal \"operate a logistics workflow\" --output json",
            ],
            &[
                "Rejeitar novas features de domínio no Core quando puderem ser Addon.",
                "Manter permissões granulares e manifests auditáveis.",
            ],
        ),
        architecture_increment(
            "tenant_personality_memory_os",
            4,
            "Sistema operacional multi-tenant com memória e personalidade",
            architecture_increment_status(tracks, &["tenant_identity_personality_context"]),
            &[
                "goal1:Fase 5",
                "goal1:Fase 5.5",
                "goal2:Fase 5.7",
                "goal2:Fase 5.8",
                "goal2:Fase 7.8",
            ],
            &["harden_core_addon_kernel"],
            &[
                "Uma instalação opera múltiplas organizações, marcas, produtos, usuários e canais.",
                "Cada workflow e node pode carregar personalidade, contexto organizacional e memória governada.",
            ],
            "Core aplica isolamento, identity links, membership, memory policy, prompt packets e validation gates.",
            "Brand assets, design systems, personas especializadas e dados de negócio vivem em contexto organizacional ou Addons.",
            &[
                "Context/handoff inclui organization_context, personality_decision e company_work_decision.",
                "Memórias respeitam escopo global, organização, projeto e processamento com audiência/visibilidade.",
                "Tenant policy bloqueia leitura/mutação fora da organização ativa.",
            ],
            &[
                "forge interactive identity --output json",
                "forge interactive context-memory --output json",
                "forge memory policy --project-root . --output json",
                "forge context --workflow <id> --task <task-id> --project-root . --strict --output json",
            ],
            &[
                "Não vazar memória privada para contexto global ou público.",
                "Não permitir que brain externo ignore personalidade ou contexto organizacional.",
            ],
        ),
        architecture_increment(
            "forge_first_harness_headroom",
            5,
            "Harness Forge-first com headroom, wrappers e brains substituíveis",
            architecture_increment_status(tracks, &["harness_headroom_cli_brains"]),
            &["goal1:Fase 1", "goal1:Fase 4", "headroom benchmark"],
            &["tenant_personality_memory_os"],
            &[
                "Codex, Gemini, Claude, OpenCode e futuros CLIs operam como brains, não como orquestradores.",
                "Wrappers e shims preservam lineage, permissões, contexto, custo e compressão reversível.",
            ],
            "Core controla routing, sessions, shell lifecycle, harness policy, headroom receipts e retrieval refs.",
            "Brains e CLIs continuam externos e substituíveis; integrações específicas entram por manifestos ou Addons.",
            &[
                "`forge smoke forge-first-harness --output json` prova headroom, shim, bootstrap dry-run e exec dry-run.",
                "Wrapper nunca executa CLI externa em smoke sem aprovação explícita.",
                "Compressão preserva original recuperável por retrieval ref e audit trail.",
            ],
            &[
                "forge interactive harness --output json",
                "forge harness headroom-plan --executor codex --project-root . --output json",
                "forge harness wrap-plan --executor codex --cmd codex --project-root . --output json",
                "forge smoke forge-first-harness --output json",
            ],
            &[
                "Não esconder perda de contexto por sumarização opaca.",
                "Não promover provider/model execution sem manifesto aprovado e recibo real.",
            ],
        ),
        architecture_increment(
            "visual_human_ai_workspace",
            6,
            "Workspace visual Humano + IA e gêmeo digital operacional",
            architecture_increment_status(tracks, &["human_ai_visual_copilot", "world_class_tui"]),
            &["goal1:Fase 7", "goal2:Fase 7.8", "goal3:UI Composition Engine"],
            &["activate_dynamic_event_runtime", "tenant_personality_memory_os"],
            &[
                "Operador e Forge co-criam artefatos, whiteboards, DAGs, wireframes, flows, docs e backlogs.",
                "Mudanças humanas podem virar eventos que atualizam workflows e artefatos relacionados.",
            ],
            "Core guarda artifacts, digital twin, modifier lane, UI composition e eventos de colaboração.",
            "Editores especializados, design tools, multimídia e painéis de domínio entram como Addons ou surfaces Ops.",
            &[
                "Digital twin mostra o que acontece, feito, faltante, validado, rejeitado e aguardando aprovação.",
                "Artifact panel e UI composition expõem widgets Core/Addons com permissões.",
                "Modifier lane registra propostas e aplicação com auditoria.",
            ],
            &[
                "forge interactive operational-cockpit --output json",
                "forge interactive workflow-mutation --output json",
                "forge interactive task-board --output json",
                "forge interactive artifacts --output json",
            ],
            &[
                "Não fixar UI por domínio.",
                "Não permitir edição visual sem evento, revisão ou vínculo ao workflow.",
            ],
        ),
        architecture_increment(
            "observability_cost_improvement_loop",
            7,
            "Observabilidade, custos e auto-melhoria controlada",
            architecture_increment_status(tracks, &["observability_cost_validation"]),
            &["goal1:Fase 4", "goal1:Fase 6"],
            &["activate_dynamic_event_runtime", "forge_first_harness_headroom"],
            &[
                "Forge mede eventos, contexto, memória, custo, tempo, tokens, validação e outcomes.",
                "Auto-melhoria escolhe candidatos por evidência, custo e repetição, não por intuição.",
            ],
            "Core mede, rankeia, valida, gera experimento e exige benchmark/aprovação antes de promoção.",
            "Políticas de otimização específicas podem ser configuráveis por Addon, organização ou workflow.",
            &[
                "Structured logs e cost ledger materializam eventos recentes e custo por workflow/node.",
                "Improve candidates identifica tarefas AI evitáveis e handoffs paralelizáveis.",
                "Improve promote exige benchmark, validação e aprovação explícita.",
            ],
            &[
                "forge interactive improvement-loop --output json",
                "forge interactive structured-logs --output json",
                "forge cost ledger --project-root . --output json",
                "forge improve candidates --output json",
            ],
            &[
                "Não auto-promover mudanças de política sem benchmark.",
                "Não usar métrica estreita para declarar aceite amplo.",
            ],
        ),
    ];

    let status = if increments
        .iter()
        .all(|increment| increment.status == "validated")
    {
        "incremental_plan_validated"
    } else {
        "incremental_plan_actionable"
    };

    InteractiveArchitectureExecutionPlan {
        schema_version: "forge.interactive.architecture_execution_plan.v1".to_string(),
        status: status.to_string(),
        strategy: "architecture_correctness_first".to_string(),
        selection_rule: "Ship the smallest increment that strengthens a universal Core primitive or an Addon escape hatch, preserves workflow/event/tenant boundaries, and has executable evidence gates.".to_string(),
        increments,
        acceptance_policy: "An increment is accepted only when its evidence commands prove the stated gates without adding domain-specific behavior to Core.".to_string(),
        next_command: format!(
            "forge interactive architecture --project-root {project_root} --output json"
        ),
    }
}

fn architecture_increment_status(
    tracks: &[InteractiveArchitectureTrack],
    track_ids: &[&str],
) -> String {
    let statuses = track_ids
        .iter()
        .filter_map(|track_id| {
            tracks
                .iter()
                .find(|track| track.track_id == *track_id)
                .map(|track| track.status.as_str())
        })
        .collect::<Vec<_>>();

    if !statuses.is_empty() && statuses.iter().all(|status| *status == "validated") {
        "validated".to_string()
    } else if statuses
        .iter()
        .any(|status| matches!(*status, "validated" | "in_progress"))
    {
        "in_progress".to_string()
    } else {
        "planned".to_string()
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "static architecture-plan metadata stays more auditable as positional fixture data"
)]
fn architecture_increment(
    increment_id: &str,
    priority: usize,
    title: &str,
    status: String,
    source_refs: &[&str],
    depends_on: &[&str],
    unlocks: &[&str],
    core_boundary: &str,
    addon_boundary: &str,
    acceptance_gates: &[&str],
    evidence_commands: &[&str],
    risk_controls: &[&str],
) -> InteractiveArchitectureExecutionIncrement {
    InteractiveArchitectureExecutionIncrement {
        increment_id: increment_id.to_string(),
        priority,
        title: title.to_string(),
        status,
        source_refs: source_refs.iter().map(|value| value.to_string()).collect(),
        depends_on: depends_on.iter().map(|value| value.to_string()).collect(),
        unlocks: unlocks.iter().map(|value| value.to_string()).collect(),
        core_boundary: core_boundary.to_string(),
        addon_boundary: addon_boundary.to_string(),
        acceptance_gates: acceptance_gates
            .iter()
            .map(|value| value.to_string())
            .collect(),
        evidence_commands: evidence_commands
            .iter()
            .map(|value| value.to_string())
            .collect(),
        risk_controls: risk_controls
            .iter()
            .map(|value| value.to_string())
            .collect(),
    }
}

fn architecture_status(validated: bool, in_progress: bool) -> String {
    if validated {
        "validated".to_string()
    } else if in_progress {
        "in_progress".to_string()
    } else {
        "planned".to_string()
    }
}

fn architecture_source_document(
    document_id: &str,
    role: &str,
    line_count: usize,
    sha256: &str,
) -> InteractiveArchitectureSourceDocument {
    InteractiveArchitectureSourceDocument {
        document_id: document_id.to_string(),
        role: role.to_string(),
        line_count,
        sha256: sha256.to_string(),
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "static architecture track metadata stays more auditable as positional fixture data"
)]
fn architecture_track(
    track_id: &str,
    title: &str,
    source_refs: &[&str],
    status: String,
    evidence_refs: Vec<String>,
    gaps: Vec<String>,
    next_increment: &str,
    core_boundary: &str,
) -> InteractiveArchitectureTrack {
    InteractiveArchitectureTrack {
        track_id: track_id.to_string(),
        title: title.to_string(),
        source_refs: source_refs.iter().map(|value| value.to_string()).collect(),
        status,
        evidence_refs,
        gaps,
        next_increment: next_increment.to_string(),
        core_boundary: core_boundary.to_string(),
    }
}

fn architecture_dependency(
    item: &str,
    depends_on: &[&str],
    reason: &str,
) -> InteractiveArchitectureDependency {
    InteractiveArchitectureDependency {
        item: item.to_string(),
        depends_on: depends_on.iter().map(|value| value.to_string()).collect(),
        reason: reason.to_string(),
    }
}

fn architecture_benchmark_sources(
    inputs: ArchitectureCompassInputs<'_>,
) -> Vec<InteractiveArchitectureBenchmarkSource> {
    vec![
        architecture_benchmark_source(
            "Gemini CLI / Codex CLI / Claude CLI / OpenCode",
            format!(
                "Keyboard-first operator shell, command palette, session lifecycle and replacement-grade readiness; current CLI readiness {}%.",
                inputs.replacement_cli_panel.readiness_percent
            ),
            "Forge owns workflow, context, memory, permissions, costs and sessions; CLIs stay replaceable brains.",
        ),
        architecture_benchmark_source(
            "OpenClaw",
            format!(
                "Async interface separation and operator-visible work state; current cockpit status {}.",
                inputs.operational_cockpit_panel.status
            ),
            "Forge exposes durable panels and MCP contracts instead of coupling orchestration to one UI.",
        ),
        architecture_benchmark_source(
            "Hermes Agents",
            format!(
                "File-first scoped memory and semantic retrieval boundaries; current memory policy {}.",
                inputs.context_memory_panel.memory_policy_status
            ),
            "Forge routes memory by tenant, audience and workflow rather than giving brains broad history.",
        ),
        architecture_benchmark_source(
            "OpenSquad",
            "Multiple specialized agents mapped to node-brain routing, session records and parallel-ready handoffs."
                .to_string(),
            "Forge coordinates agents through workflows and leases, not through a fixed agent team model.",
        ),
        architecture_benchmark_source(
            "Open Design / Penpot",
            format!(
                "Design tokens, components, wireframes and UI composition; current renderer families {}.",
                inputs.ui_composition_panel.addon_renderer_families.join("+")
            ),
            "Forge stores creative artifacts and tokens; specialized editors can attach through Addon/UI surfaces.",
        ),
        architecture_benchmark_source(
            "Paperclip",
            "Company-work framing: product, technical, finance, admin, marketing, communication and delivery decisions in prompt packets."
                .to_string(),
            "Forge keeps the operating checklist domain-agnostic and tenant-aware.",
        ),
        architecture_benchmark_source(
            "Remotion",
            format!(
                "Programmatic media pipeline as an Addon-owned multimodal runtime; current readiness {}%.",
                inputs.multimodal_runtime_panel.readiness_percent
            ),
            "Media generation belongs to Addons and guarded runtime contracts, not universal Core execution.",
        ),
        architecture_benchmark_source(
            "n8n",
            format!(
                "Triggers, webhooks, schedules and node marketplace concepts; current adapters {} and schedules {}.",
                inputs.addon_capability_panel.event_adapter_count,
                inputs.schedule_panel.workflows.len()
            ),
            "Forge keeps workflow state, policy and validation central while Addons provide nodes/adapters.",
        ),
        architecture_benchmark_source(
            "headroom",
            format!(
                "Token headroom, reversible compression and wrapper interception; current headroom status {}.",
                inputs.harness_panel.headroom_operational_status
            ),
            "Forge compresses context/output through auditable receipts and retrieval refs, not lossy hidden summarization.",
        ),
    ]
}

fn architecture_benchmark_source(
    source: &str,
    absorbed_concept: String,
    forge_boundary: &str,
) -> InteractiveArchitectureBenchmarkSource {
    InteractiveArchitectureBenchmarkSource {
        source: source.to_string(),
        absorbed_concept,
        forge_boundary: forge_boundary.to_string(),
    }
}

pub fn build_operational_tui_smoke(
    store: &ForgeStore,
    project_root: Option<&Path>,
    origin: &str,
) -> Result<OperationalTuiSmokeReport> {
    let request = start_async_request(
        store,
        "Demonstrate Forge operational TUI with active workflows, events, schedules, Addons/capabilities, costs, handoffs and approvals",
        origin,
    )?;
    let scheduled = create_daily_goal_research_workflow(
        store,
        vec!["operational-tui-smoke".to_string()],
        "UTC",
        "*/15 * * * *",
        origin,
    )?;
    let workflow = store.load_workflow(&request.workflow_id)?;
    if let Some(task) = workflow.tasks.first() {
        let choices = vec![
            "approve=Approve operational TUI smoke".to_string(),
            "refine=Refine operational TUI smoke".to_string(),
        ];
        create_choice_interaction(
            store,
            CreateChoiceInteractionRequest {
                workflow_id: &request.workflow_id,
                task_id: &task.id,
                kind: "approve_reject_refine_combine",
                prompt: "Confirm the operational TUI smoke demo before handoff.",
                choices: &choices,
                timeout_seconds: Some(3600),
                origin,
            },
        )?;
        create_modifier_proposal(
            store,
            OpsModifierProposalInput {
                workflow_id: &request.workflow_id,
                target_kind: "workflow_goal",
                task_id: None,
                title: "Refine operational TUI smoke goal",
                summary: "Create a pending workflow mutation proposal for the replanning panel.",
                rationale:
                    "The smoke must prove human+AI assisted runtime mutation is visible in the TUI.",
                proposed_goal: Some(
                    "Demonstrate Forge operational TUI with visible workflow mutation evidence",
                ),
                proposed_title: None,
                proposed_expected_output: None,
                author: origin,
            },
        )?;
    }

    let event_data = serde_json::json!({
        "schema_version": OPERATIONAL_TUI_SMOKE_SCHEMA_VERSION,
        "run_id": request.run_id.clone(),
        "workflow_id": request.workflow_id.clone(),
        "scheduled_workflow_id": scheduled.workflow_id.clone(),
        "summary": "Operational TUI smoke created workflow, schedule, event, approval and dashboard evidence."
    });
    let tenant_context = serde_json::json!({
        "organization_id": "forge",
        "brand_id": "forge",
        "product_id": "forge-core"
    });
    let event_id = store.record_global_event(GlobalEventWrite {
        source: "forge.smoke.operational_tui",
        source_id: &request.run_id,
        workflow_id: Some(&request.workflow_id),
        kind: "operational_tui_smoke",
        origin,
        status: "passed",
        data: &event_data,
        tenant_context: &tenant_context,
    })?;

    let home = build_interactive_home_with_options(
        store,
        InteractiveHomeOptions {
            project_root: project_root.map(Path::to_path_buf),
        },
    )?;
    let d = &home.dashboard;
    let readme = include_str!("../README.md");
    let default_tui_preview = render_interactive_home(&home);
    let dashboard = OperationalTuiSmokeDashboard {
        active_runs: d.active_runs,
        workflow_count: d.task_board_panel.workflow_count,
        event_count: d.event_panel.total_event_count,
        schedule_workflow_count: d.scheduled_workflows,
        addon_count: d.addon_capability_panel.addon_count,
        capability_count: d.addon_capability_panel.capability_count,
        core_boundary_status: d.core_boundary_panel.status.clone(),
        domain_specific_core_leak_count: d.core_boundary_panel.domain_specific_core_leak_count,
        cost_estimated_usd: d.cost_panel.estimated_task_cost_total_usd,
        improvement_candidate_count: d.improvement_loop_panel.candidate_count,
        structured_log_count: d.improvement_loop_panel.structured_log_count,
        workflow_mutation_workflow_count: d.workflow_mutation_panel.workflow_count,
        pending_mutation_proposal_count: d.workflow_mutation_panel.pending_modifier_proposal_count,
        ready_handoff_count: d.task_board_panel.ready_handoffs,
        pending_approval_count: d.pending_approvals,
        guided_cockpit_step_count: d.guided_cockpit_panel.total_step_count,
        guided_cockpit_completed_step_count: d.guided_cockpit_panel.completed_step_count,
        guided_cockpit_current_step: d.guided_cockpit_panel.current_step_id.clone(),
        guided_cockpit_confirmation_step_count: d.guided_cockpit_panel.confirmation_step_count,
    };
    let checks = vec![
        operational_tui_smoke_check(
            "opens_useful_tui",
            "forge opens the operational TUI",
            home.status == "interactive_home_ready"
                && default_tui_preview.contains("Forge operational TUI")
                && default_tui_preview.contains("Active workflows:")
                && default_tui_preview.contains("Events/schedules:")
                && default_tui_preview.contains("Addons/capabilities:")
                && default_tui_preview.contains("Costs:")
                && default_tui_preview.contains("Improvement loop:")
                && default_tui_preview.contains("Handoffs/approvals:"),
            format!(
                "{}; cockpit {}; focus panels {}; default render {} bytes",
                home.status,
                d.operational_cockpit_panel.status,
                d.navigation_panel.keybindings.len(),
                default_tui_preview.len()
            ),
            "forge",
        ),
        operational_tui_smoke_check(
            "opens_guided_cockpit_by_default",
            "forge opens the advanced guided cockpit by default",
            d.guided_cockpit_panel.schema_version == INTERACTIVE_GUIDED_COCKPIT_SCHEMA_VERSION
                && dashboard.guided_cockpit_step_count == 8
                && default_tui_preview.contains("Guided cockpit:")
                && default_tui_preview.contains("Guided steps:")
                && default_tui_preview.contains("Safe actions:")
                && default_tui_preview.contains("create_workflow")
                && default_tui_preview.contains("close_outcome"),
            format!(
                "{}; steps {}/{}; current {}; confirmations {}; default render {} bytes",
                d.guided_cockpit_panel.status,
                dashboard.guided_cockpit_completed_step_count,
                dashboard.guided_cockpit_step_count,
                dashboard.guided_cockpit_current_step,
                dashboard.guided_cockpit_confirmation_step_count,
                default_tui_preview.len()
            ),
            "forge",
        ),
        operational_tui_smoke_check(
            "shows_active_workflows",
            "TUI shows active workflows",
            dashboard.active_runs > 0 && dashboard.workflow_count > 0,
            format!(
                "{} active runs; {} workflow lanes",
                dashboard.active_runs, dashboard.workflow_count
            ),
            "forge interactive task-board --output json",
        ),
        operational_tui_smoke_check(
            "shows_events_and_schedules",
            "TUI shows events and schedules",
            dashboard.event_count > 0 && dashboard.schedule_workflow_count > 0,
            format!(
                "{} events; {} scheduled workflows",
                dashboard.event_count, dashboard.schedule_workflow_count
            ),
            "forge interactive schedules --output json",
        ),
        operational_tui_smoke_check(
            "shows_event_workflow_lifecycle",
            "TUI shows event-driven workflow lifecycle actions",
            d.event_runtime_panel.workflow_lifecycle.action_count == 6
                && d.event_runtime_panel
                    .workflow_lifecycle
                    .validated_action_count
                    == 6
                && d.event_runtime_panel
                    .workflow_lifecycle
                    .actions
                    .iter()
                    .any(|action| {
                        action.action == "end_workflow"
                            && action.normalized_route == "complete_workflow"
                    }),
            format!(
                "{}; {}/{} validated lifecycle actions",
                d.event_runtime_panel.workflow_lifecycle.status,
                d.event_runtime_panel
                    .workflow_lifecycle
                    .validated_action_count,
                d.event_runtime_panel.workflow_lifecycle.action_count
            ),
            "forge interactive event-runtime --output json",
        ),
        operational_tui_smoke_check(
            "shows_addons_and_capabilities",
            "TUI shows Addons and capabilities",
            !d.addon_capability_panel.status.is_empty(),
            format!(
                "{}; {} addons; {} capabilities",
                d.addon_capability_panel.status, dashboard.addon_count, dashboard.capability_count
            ),
            "forge interactive addon-capabilities --output json",
        ),
        operational_tui_smoke_check(
            "shows_core_boundary_audit",
            "TUI shows Core boundary and Addon ownership audit",
            d.core_boundary_panel.schema_version == INTERACTIVE_CORE_BOUNDARY_SCHEMA_VERSION
                && dashboard.core_boundary_status == "core_boundary_clean"
                && dashboard.domain_specific_core_leak_count == 0
                && default_tui_preview.contains("Core boundary:"),
            format!(
                "{}; leaks {}; compatibility {}",
                d.core_boundary_panel.status,
                d.core_boundary_panel.domain_specific_core_leak_count,
                d.core_boundary_panel.compatibility_boundary_count
            ),
            "forge interactive core-boundary --output json",
        ),
        operational_tui_smoke_check(
            "shows_costs",
            "TUI shows costs",
            !d.cost_panel.status.is_empty(),
            format!(
                "{}; estimated ${:.4}; observed ${:.4}",
                d.cost_panel.status,
                d.cost_panel.estimated_task_cost_total_usd,
                d.cost_panel.observed_event_cost_total_usd
            ),
            "forge cost ledger --output json",
        ),
        operational_tui_smoke_check(
            "shows_improvement_loop",
            "TUI shows improvement loop candidates from logs, costs and validation",
            d.improvement_loop_panel.schema_version == INTERACTIVE_IMPROVEMENT_LOOP_SCHEMA_VERSION
                && !d.improvement_loop_panel.status.is_empty()
                && dashboard.improvement_candidate_count > 0
                && dashboard.structured_log_count > 0,
            format!(
                "{}; {} candidates; {} logs; {} validation failures",
                d.improvement_loop_panel.status,
                dashboard.improvement_candidate_count,
                dashboard.structured_log_count,
                d.improvement_loop_panel.validation_failure_count
            ),
            "forge interactive improvement-loop --output json",
        ),
        operational_tui_smoke_check(
            "shows_workflow_mutation_replanning",
            "TUI shows workflow mutation and replanning surface",
            d.workflow_mutation_panel.schema_version
                == INTERACTIVE_WORKFLOW_MUTATION_SCHEMA_VERSION
                && dashboard.workflow_mutation_workflow_count > 0
                && dashboard.pending_mutation_proposal_count > 0
                && d.workflow_mutation_panel
                    .workflow_cards
                    .iter()
                    .any(|card| !card.mutable_targets.is_empty()),
            format!(
                "{}; {} workflows; {} pending proposals; {} mutable workflows",
                d.workflow_mutation_panel.status,
                dashboard.workflow_mutation_workflow_count,
                dashboard.pending_mutation_proposal_count,
                d.workflow_mutation_panel.mutable_workflow_count
            ),
            "forge interactive workflow-mutation --output json",
        ),
        operational_tui_smoke_check(
            "shows_handoffs_and_approvals",
            "TUI shows handoffs and approvals",
            dashboard.ready_handoff_count > 0 || dashboard.pending_approval_count > 0,
            format!(
                "{} ready handoffs; {} pending approvals",
                dashboard.ready_handoff_count, dashboard.pending_approval_count
            ),
            "forge interactive permissions --output json",
        ),
        operational_tui_smoke_check(
            "shows_operating_context",
            "TUI shows operating context, memory, personality and prompt gates",
            d.operating_context_panel.schema_version
                == INTERACTIVE_OPERATING_CONTEXT_SCHEMA_VERSION
                && d.operating_context_panel
                    .prompt_packet_contract
                    .organization_context_required
                && d.operating_context_panel
                    .prompt_packet_contract
                    .personality_decision_required
                && d.operating_context_panel
                    .prompt_packet_contract
                    .company_work_decision_required
                && d.operating_context_panel
                    .company_work_contract
                    .departments
                    .iter()
                    .any(|department| department == "product"),
            format!(
                "{}; tenant {}; memory {}; personality {}; gates {}",
                d.operating_context_panel.status,
                d.operating_context_panel.tenant_path,
                d.operating_context_panel.memory_policy_status,
                d.operating_context_panel.personality_status,
                d.operating_context_panel
                    .prompt_packet_contract
                    .required_gates
                    .join(",")
            ),
            "forge interactive operating-context --output json",
        ),
        operational_tui_smoke_check(
            "runs_end_to_end_demo_flow",
            "Smoke runs an end-to-end demo flow",
            !request.workflow_id.is_empty()
                && !request.run_id.is_empty()
                && !scheduled.workflow_id.is_empty()
                && event_id > 0,
            format!(
                "workflow {}; run {}; scheduled {}; event {}",
                request.workflow_id, request.run_id, scheduled.workflow_id, event_id
            ),
            "forge smoke operational-tui --output json",
        ),
        operational_tui_smoke_check(
            "readme_five_minute_intro",
            "README explains Forge in five minutes",
            readme.contains("## Forge em 5 minutos")
                && readme.contains("forge smoke operational-tui"),
            "README contains the five-minute intro and operational smoke command".to_string(),
            "README.md",
        ),
    ];
    let status = if checks.iter().all(|check| check.passed) {
        "operational_tui_smoke_passed"
    } else {
        "operational_tui_smoke_failed"
    };

    Ok(OperationalTuiSmokeReport {
        schema_version: OPERATIONAL_TUI_SMOKE_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        workflow_id: request.workflow_id,
        run_id: request.run_id,
        scheduled_workflow_id: scheduled.workflow_id,
        event_id,
        dashboard,
        checks,
        commands: vec![
            "forge".to_string(),
            "forge interactive guided-cockpit --output json".to_string(),
            "forge interactive home --output json".to_string(),
            "forge interactive operational-cockpit --output json".to_string(),
            "forge interactive task-board --output json".to_string(),
            "forge interactive schedules --output json".to_string(),
            "forge interactive event-runtime --output json".to_string(),
            "forge interactive structured-logs --output json".to_string(),
            "forge interactive improvement-loop --output json".to_string(),
            "forge interactive workflow-mutation --output json".to_string(),
            "forge interactive addon-capabilities --output json".to_string(),
            "forge interactive operating-context --output json".to_string(),
            "forge cost ledger --output json".to_string(),
            "forge smoke operational-tui --output json".to_string(),
        ],
    })
}

pub fn build_forge_first_harness_smoke(
    store: &ForgeStore,
    project_root: Option<&Path>,
    executor: &str,
    real_cmd: Option<&str>,
) -> Result<ForgeFirstHarnessSmokeReport> {
    let executor = executor.trim();
    let executor = if executor.is_empty() {
        "codex"
    } else {
        executor
    };
    let smoke_root = forge_harness_smoke_root();
    let shim_dir = smoke_root.join("bin");
    let smoke_project_root = project_root
        .map(Path::to_path_buf)
        .unwrap_or_else(|| smoke_root.join("project"));
    std::fs::create_dir_all(&shim_dir)?;
    std::fs::create_dir_all(&smoke_project_root)?;
    let real_cmd = real_cmd
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(default_forge_harness_smoke_real_cmd);
    let context_budget = 120usize;
    let context_budget_source = "smoke_default";
    let token_headroom_source = "smoke_default";
    let require_token_headroom_for_forge_first = true;
    let headroom_content = forge_harness_smoke_headroom_content();
    let headroom = analyze_token_headroom(
        &headroom_content,
        Some("log"),
        context_budget,
        "forge_first_harness_smoke",
        true,
    );
    let headroom = persist_token_headroom_report(store, headroom, &headroom_content)?;

    let adoption_plan = build_harness_adoption_plan(HarnessAdoptionPlanOptions {
        shim_dir: &shim_dir,
        executor,
        forge_first: true,
        observe_only: false,
        project_root: Some(&smoke_project_root),
        workflow_id: None,
        task_id: None,
        run_id: None,
        context_budget,
        context_budget_source,
        token_headroom: true,
        token_headroom_source,
        require_token_headroom_for_forge_first,
    })?;
    let bootstrap_plan = build_harness_bootstrap_report(HarnessBootstrapOptions {
        shim_dir: &shim_dir,
        executor,
        project_root: &smoke_project_root,
        store_path: None,
        context_budget,
        context_budget_source,
        token_headroom: true,
        token_headroom_source,
        apply: false,
        approved_by: None,
        force: true,
    })?;
    let shim_install = install_cli_harness_shim(CliShimInstallOptions {
        shim_dir: &shim_dir,
        executor,
        real_cmd: Some(&real_cmd),
        store_path: None,
        forge_first: true,
        forge_first_source: "smoke_explicit",
        workflow_id: None,
        task_id: None,
        run_id: None,
        context_budget,
        token_headroom: true,
        force: true,
    })?;
    let shim_status = inspect_cli_harness_shim_status(CliShimStatusOptions {
        shim_dir: &shim_dir,
        executor,
    })?;
    let exec_command = vec![real_cmd.clone(), "forge-first-harness-smoke".to_string()];
    let exec_receipt = run_cli_harness_exec(CliHarnessExecOptions {
        store: Some(store),
        executor,
        command: &exec_command,
        forge_first: true,
        forge_first_source: "smoke_explicit",
        workflow_id: None,
        task_id: None,
        run_id: None,
        context_budget,
        context_budget_source,
        token_headroom: true,
        token_headroom_source,
        require_token_headroom_for_forge_first,
        dry_run: true,
        allow_exec: false,
        project_root: Some(&smoke_project_root),
        cwd: Some(&smoke_project_root),
    })?;

    let checks = vec![
        operational_tui_smoke_check(
            "headroom_persisted",
            "Token headroom is reversible and persisted",
            headroom.persisted
                && headroom.retrieval_available
                && headroom.estimated_saved_tokens > 0,
            format!(
                "{} saved {} tokens; persisted {}; retrieval {}",
                headroom.status,
                headroom.estimated_saved_tokens,
                headroom.persisted,
                headroom.retrieval_available
            ),
            "forge harness token-headroom --persist --output json",
        ),
        operational_tui_smoke_check(
            "adoption_plan_ready",
            "Forge-first adoption plan is read-only and complete",
            adoption_plan.status == "harness_adoption_plan_ready"
                && !adoption_plan.mutates_state
                && !adoption_plan.executes_child
                && adoption_plan.recommended_project_config.default_mode == "forge_first",
            format!(
                "{}; mutates {}; executes {}; next {}",
                adoption_plan.status,
                adoption_plan.mutates_state,
                adoption_plan.executes_child,
                adoption_plan.next_action
            ),
            "forge harness adoption-plan --forge-first --token-headroom --output json",
        ),
        operational_tui_smoke_check(
            "bootstrap_dry_run",
            "Bootstrap stays dry-run without approval",
            bootstrap_plan.status == "harness_bootstrap_planned"
                && !bootstrap_plan.applied
                && !bootstrap_plan.mutates_state
                && bootstrap_plan.would_mutate_state,
            format!(
                "{}; config {}; applied {}",
                bootstrap_plan.status, bootstrap_plan.config_write.status, bootstrap_plan.applied
            ),
            "forge harness bootstrap --output json",
        ),
        operational_tui_smoke_check(
            "shim_installed_in_smoke_dir",
            "Forge-owned shim can be installed in an isolated directory",
            shim_install.status == "shim_install_ready"
                && shim_install.blocked_count == 0
                && shim_install.forge_first
                && shim_install.token_headroom
                && shim_install
                    .shims
                    .first()
                    .map(|shim| shim.real_command_resolution_status == "explicit_real_command")
                    .unwrap_or(false),
            format!(
                "{}; installed {}; updated {}; blocked {}",
                shim_install.status,
                shim_install.installed_count,
                shim_install.updated_count,
                shim_install.blocked_count
            ),
            "forge harness install-shims --real-cmd <safe-command> --output json",
        ),
        operational_tui_smoke_check(
            "shim_audit_safe",
            "Shim audit proves ownership and no recursion without changing PATH",
            shim_status.shim_exists
                && shim_status.forge_owned
                && shim_status.executable
                && !shim_status.would_recurse,
            format!(
                "{}; path {}; exists {}; forge_owned {}; executable {}; recurse {}",
                shim_status.status,
                shim_status.path_precedence,
                shim_status.shim_exists,
                shim_status.forge_owned,
                shim_status.executable,
                shim_status.would_recurse
            ),
            "forge harness shim-status --output json",
        ),
        operational_tui_smoke_check(
            "exec_dry_run_forge_first",
            "Harness exec remains dry-run while projecting Forge-first policy",
            exec_receipt.status == "harness_exec_dry_run"
                && exec_receipt.dry_run
                && !exec_receipt.executed
                && exec_receipt.forge_first
                && exec_receipt.output_headroom_enabled,
            format!(
                "{}; executed {}; forge_first {}; headroom {}",
                exec_receipt.status,
                exec_receipt.executed,
                exec_receipt.forge_first,
                exec_receipt.output_headroom_enabled
            ),
            "forge harness exec --dry-run --output json",
        ),
        operational_tui_smoke_check(
            "external_cli_not_executed_or_modified",
            "Smoke does not execute or mutate the external CLI",
            !exec_receipt.executed
                && shim_install
                    .shims
                    .iter()
                    .all(|shim| shim.real_command == real_cmd),
            format!(
                "real command {}; executed {}; shim dir {}",
                real_cmd,
                exec_receipt.executed,
                shim_dir.display()
            ),
            "forge smoke forge-first-harness --output json",
        ),
    ];
    let status = if checks.iter().all(|check| check.passed) {
        "forge_first_harness_smoke_passed"
    } else {
        "forge_first_harness_smoke_failed"
    };

    Ok(ForgeFirstHarnessSmokeReport {
        schema_version: FORGE_FIRST_HARNESS_SMOKE_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        executor: executor.to_string(),
        project_root: smoke_project_root.display().to_string(),
        shim_dir: shim_dir.display().to_string(),
        real_cmd,
        mutates_external_cli: false,
        executes_external_cli: false,
        headroom,
        adoption_plan,
        bootstrap_plan,
        shim_install,
        shim_status,
        exec_receipt,
        checks,
        commands: vec![
            "forge smoke forge-first-harness --output json".to_string(),
            "forge harness token-headroom --persist --output json".to_string(),
            "forge harness adoption-plan --forge-first --token-headroom --output json".to_string(),
            "forge harness bootstrap --output json".to_string(),
            "forge harness install-shims --real-cmd <safe-command> --output json".to_string(),
            "forge harness shim-status --output json".to_string(),
            "forge harness exec --dry-run --output json".to_string(),
        ],
    })
}

pub fn build_replacement_cli_evidence_smoke(
    store: &ForgeStore,
    project_root: Option<&Path>,
    approved_by: &str,
    origin: &str,
) -> Result<ReplacementCliEvidenceSmokeReport> {
    let smoke_project_root = project_root
        .map(Path::to_path_buf)
        .unwrap_or_else(replacement_cli_evidence_smoke_root);
    std::fs::create_dir_all(smoke_project_root.join(".forge"))?;
    let approved_by = approved_by.trim();
    let approved_by = if approved_by.is_empty() {
        "forge_smoke"
    } else {
        approved_by
    };
    let origin = origin.trim();
    let origin = if origin.is_empty() {
        "forge_smoke"
    } else {
        origin
    };

    let collect_ready = collect_ready_milestone_evidence(
        store,
        MilestoneCollectReadyEvidenceOptions {
            version: "0.5",
            project_root: Some(&smoke_project_root),
            connected_brain: None,
            connected_runtime: None,
            approved_by,
            origin,
        },
    )?;
    let release_gates = build_interactive_release_gates(store, "0.5", Some(&smoke_project_root))?;

    let collected_kind = |kind: &str| {
        collect_ready.collected_evidence.iter().any(|evidence| {
            evidence.capability_id == "replacement_grade_cli"
                && evidence.kind == kind
                && evidence.status == "collected_and_attached"
                && evidence.collection_promotion_ready
        })
    };
    let skipped_kind = |capability_id: &str, kind: &str| {
        collect_ready.skipped_evidence.iter().any(|evidence| {
            evidence.capability_id == capability_id
                && evidence.kind == kind
                && evidence.status == "not_ready_to_collect"
                && !evidence.evidence_plan.ready_to_collect_evidence
                && evidence
                    .evidence_plan
                    .config_checks
                    .iter()
                    .any(|check| check.status == "missing")
                && !evidence.evidence_plan.manifest_templates.is_empty()
        })
    };
    let replacement_gate = release_gates
        .gate_cards
        .iter()
        .find(|gate| gate.capability_id == "replacement_grade_cli");
    let replacement_gate_partial = replacement_gate.is_some_and(|gate| {
        gate.attached_evidence_state == "partial_required_attached_evidence"
            && gate
                .attached_evidence_kinds
                .iter()
                .any(|kind| kind == "broader_project_coding_research_workflow")
            && gate
                .attached_evidence_kinds
                .iter()
                .any(|kind| kind == "terminal_file_editing_ux")
            && gate
                .missing_attached_evidence_kinds
                .iter()
                .any(|kind| kind == "external_brain_provider_execution")
    });

    let checks = vec![
        operational_tui_smoke_check(
            "collects_broader_project_coding_research_workflow",
            "Ready collection attaches broader project coding/research evidence",
            collected_kind("broader_project_coding_research_workflow"),
            format!(
                "collected {}; skipped {}; failed {}",
                collect_ready.collected_count,
                collect_ready.skipped_count,
                collect_ready.failed_count
            ),
            "forge milestone collect-ready-evidence --version 0.5 --project-root <project-root> --output json",
        ),
        operational_tui_smoke_check(
            "collects_terminal_file_editing_ux",
            "Ready collection attaches terminal file-editing UX evidence",
            collected_kind("terminal_file_editing_ux"),
            format!(
                "collected kinds {}",
                collect_ready
                    .collected_evidence
                    .iter()
                    .map(|evidence| evidence.kind.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            "forge milestone collect-evidence --version 0.5 --capability replacement_grade_cli --kind terminal_file_editing_ux --output json",
        ),
        operational_tui_smoke_check(
            "skips_external_provider_until_manifest",
            "Provider evidence stays skipped until an approved connected-brain manifest exists",
            skipped_kind("replacement_grade_cli", "external_brain_provider_execution"),
            format!(
                "skipped {} item(s); next {}",
                collect_ready.skipped_count, collect_ready.next_action
            ),
            "forge milestone evidence-plan --version 0.5 --capability replacement_grade_cli --project-root <project-root> --output json",
        ),
        operational_tui_smoke_check(
            "skips_multimodal_until_runtime_manifest",
            "Production multimodal evidence stays skipped until approved runtime manifests exist",
            skipped_kind("experimental_multimodal_runtime", "production_runtime_benchmark"),
            format!(
                "skipped {} item(s); failed {}",
                collect_ready.skipped_count, collect_ready.failed_count
            ),
            "forge milestone evidence-plan --version 0.5 --capability experimental_multimodal_runtime --project-root <project-root> --output json",
        ),
        operational_tui_smoke_check(
            "release_gate_tracks_partial_replacement_cli_evidence",
            "Release gate shows partial replacement CLI evidence instead of overclaiming promotion",
            replacement_gate_partial,
            replacement_gate
                .map(|gate| {
                    format!(
                        "{}; attached {}; missing {}",
                        gate.attached_evidence_state,
                        gate.attached_evidence_kinds.join(", "),
                        gate.missing_attached_evidence_kinds.join(", ")
                    )
                })
                .unwrap_or_else(|| "replacement_grade_cli gate missing".to_string()),
            "forge interactive release-gates --version 0.5 --output json",
        ),
        operational_tui_smoke_check(
            "does_not_auto_promote",
            "Evidence collection does not auto-promote Forge 0.5",
            !collect_ready.promotion_ready_after_collection && !release_gates.promotion_ready,
            format!(
                "collection promotion {}; release promotion {}; decision {}",
                collect_ready.promotion_ready_after_collection,
                release_gates.promotion_ready,
                release_gates.promotion_decision.decision
            ),
            "forge milestone manifest --version 0.5 --output json",
        ),
    ];
    let status = if checks.iter().all(|check| check.passed) {
        "replacement_cli_evidence_smoke_passed"
    } else {
        "replacement_cli_evidence_smoke_failed"
    };

    Ok(ReplacementCliEvidenceSmokeReport {
        schema_version: REPLACEMENT_CLI_EVIDENCE_SMOKE_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        project_root: smoke_project_root.display().to_string(),
        collect_ready,
        release_gates,
        checks,
        commands: vec![
            "forge smoke replacement-cli-evidence --output json".to_string(),
            "forge milestone collect-ready-evidence --version 0.5 --project-root <project-root> --approved-by <operator> --origin codex --output json".to_string(),
            "forge interactive release-gates --version 0.5 --output json".to_string(),
            "forge milestone evidence-plan --version 0.5 --capability replacement_grade_cli --project-root <project-root> --output json".to_string(),
            "forge milestone evidence-plan --version 0.5 --capability experimental_multimodal_runtime --project-root <project-root> --output json".to_string(),
        ],
    })
}

fn forge_harness_smoke_root() -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    env::temp_dir().join(format!(
        "forge-first-harness-smoke-{}-{now}",
        std::process::id()
    ))
}

fn replacement_cli_evidence_smoke_root() -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    env::temp_dir().join(format!(
        "forge-replacement-cli-evidence-smoke-{}-{now}",
        std::process::id()
    ))
}

fn default_forge_harness_smoke_real_cmd() -> String {
    ["/bin/echo", "/usr/bin/printf", "/usr/bin/env"]
        .iter()
        .find(|candidate| Path::new(candidate).exists())
        .map(|candidate| (*candidate).to_string())
        .unwrap_or_else(|| {
            env::current_exe()
                .unwrap_or_else(|_| PathBuf::from("forge"))
                .display()
                .to_string()
        })
}

fn forge_harness_smoke_headroom_content() -> String {
    (0..80)
        .map(|index| {
            format!(
                "warning[{index}]: repeated executor output can be compressed by Forge headroom while original bytes remain retrievable"
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn operational_tui_smoke_check(
    check_id: &str,
    title: &str,
    passed: bool,
    evidence: String,
    command: &str,
) -> OperationalTuiSmokeCheck {
    OperationalTuiSmokeCheck {
        check_id: check_id.to_string(),
        title: title.to_string(),
        passed,
        evidence,
        command: command.to_string(),
    }
}

pub fn render_operational_tui_smoke(report: &OperationalTuiSmokeReport) -> String {
    let checks = report
        .checks
        .iter()
        .map(|check| format!("{}={} ({})", check.check_id, check.passed, check.evidence))
        .collect::<Vec<_>>()
        .join(" | ");
    format!(
        "Operational TUI smoke: {status}; workflow {workflow_id}; run {run_id}; scheduled {scheduled_workflow_id}; event {event_id}\nDashboard: active runs {active_runs}, workflows {workflow_count}, events {event_count}, schedules {schedule_workflow_count}, addons {addon_count}, capabilities {capability_count}, cost ${cost_estimated_usd:.4}, mutation workflows {workflow_mutation_workflow_count}, mutation proposals {pending_mutation_proposal_count}, handoffs {ready_handoff_count}, approvals {pending_approval_count}\nchecks: {checks}\ncommands: {commands}\n",
        status = report.status,
        workflow_id = report.workflow_id,
        run_id = report.run_id,
        scheduled_workflow_id = report.scheduled_workflow_id,
        event_id = report.event_id,
        active_runs = report.dashboard.active_runs,
        workflow_count = report.dashboard.workflow_count,
        event_count = report.dashboard.event_count,
        schedule_workflow_count = report.dashboard.schedule_workflow_count,
        addon_count = report.dashboard.addon_count,
        capability_count = report.dashboard.capability_count,
        cost_estimated_usd = report.dashboard.cost_estimated_usd,
        workflow_mutation_workflow_count = report.dashboard.workflow_mutation_workflow_count,
        pending_mutation_proposal_count = report.dashboard.pending_mutation_proposal_count,
        ready_handoff_count = report.dashboard.ready_handoff_count,
        pending_approval_count = report.dashboard.pending_approval_count,
        checks = checks,
        commands = report.commands.join(" | "),
    )
}

pub fn render_forge_first_harness_smoke(report: &ForgeFirstHarnessSmokeReport) -> String {
    let checks = report
        .checks
        .iter()
        .map(|check| format!("{}={} ({})", check.check_id, check.passed, check.evidence))
        .collect::<Vec<_>>()
        .join(" | ");
    format!(
        "Forge-first harness smoke: {status}; executor {executor}; project {project_root}; shim {shim_dir}; real_cmd {real_cmd}\nHeadroom: {headroom_status}; saved {saved_tokens} tokens; persisted {persisted}; retrieval {retrieval_available}\nAdoption: {adoption_status}; bootstrap {bootstrap_status}; shim install {shim_install_status}; shim audit {shim_status} ({shim_path_precedence}); exec {exec_status}; external mutated {mutates_external_cli}; external executed {executes_external_cli}\nchecks: {checks}\ncommands: {commands}\n",
        status = report.status,
        executor = report.executor,
        project_root = report.project_root,
        shim_dir = report.shim_dir,
        real_cmd = report.real_cmd,
        headroom_status = report.headroom.status,
        saved_tokens = report.headroom.estimated_saved_tokens,
        persisted = report.headroom.persisted,
        retrieval_available = report.headroom.retrieval_available,
        adoption_status = report.adoption_plan.status,
        bootstrap_status = report.bootstrap_plan.status,
        shim_install_status = report.shim_install.status,
        shim_status = report.shim_status.status,
        shim_path_precedence = report.shim_status.path_precedence,
        exec_status = report.exec_receipt.status,
        mutates_external_cli = report.mutates_external_cli,
        executes_external_cli = report.executes_external_cli,
        checks = checks,
        commands = report.commands.join(" | "),
    )
}

pub fn render_replacement_cli_evidence_smoke(report: &ReplacementCliEvidenceSmokeReport) -> String {
    let checks = report
        .checks
        .iter()
        .map(|check| format!("{}={} ({})", check.check_id, check.passed, check.evidence))
        .collect::<Vec<_>>()
        .join(" | ");
    let collected = report
        .collect_ready
        .collected_evidence
        .iter()
        .map(|evidence| format!("{}:{}", evidence.capability_id, evidence.kind))
        .collect::<Vec<_>>()
        .join(", ");
    let skipped = report
        .collect_ready
        .skipped_evidence
        .iter()
        .map(|evidence| format!("{}:{}", evidence.capability_id, evidence.kind))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Replacement CLI evidence smoke: {status}; project {project_root}; collected {collected_count}/{required_count}; skipped {skipped_count}; failed {failed_count}; release promotion {promotion_ready}\nCollected: {collected}\nSkipped: {skipped}\nchecks: {checks}\ncommands: {commands}\n",
        status = report.status,
        project_root = report.project_root,
        collected_count = report.collect_ready.collected_count,
        required_count = report.collect_ready.required_count,
        skipped_count = report.collect_ready.skipped_count,
        failed_count = report.collect_ready.failed_count,
        promotion_ready = report.release_gates.promotion_ready,
        collected = if collected.is_empty() { "none" } else { &collected },
        skipped = if skipped.is_empty() { "none" } else { &skipped },
        checks = checks,
        commands = report.commands.join(" | "),
    )
}

pub fn slash_command_catalog() -> SlashCommandCatalogReport {
    SlashCommandCatalogReport {
        status: "slash_commands_loaded".to_string(),
        schema_version: SLASH_COMMANDS_SCHEMA_VERSION.to_string(),
        commands: slash_commands(),
    }
}

pub fn build_interactive_harness(
    store: &ForgeStore,
    options: InteractiveHarnessOptions,
) -> Result<InteractiveHarnessPanel> {
    let project_root = options
        .project_root
        .clone()
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let (effective_forge_first, forge_first_source) =
        resolve_harness_forge_first_source_for_project(
            options.forge_first,
            options.observe_only,
            Some(&project_root),
        );
    let runtime_policy = resolve_harness_runtime_policy(HarnessRuntimePolicyOptions {
        project_root: Some(&project_root),
        context_budget: options.context_budget,
        context_budget_source: "interactive_input",
        token_headroom: options.token_headroom,
        token_headroom_source: "interactive_input",
        forge_first: effective_forge_first,
        default_context_budget: 1200,
    });
    let mode = build_harness_mode_report(HarnessModeOptions {
        forge_first: options.forge_first,
        observe_only: options.observe_only,
        project_root: Some(&project_root),
    });
    let doctor = build_harness_doctor_report(HarnessDoctorOptions {
        shim_dir: &options.shim_dir,
        executor: &options.executor,
        forge_first: options.forge_first,
        observe_only: options.observe_only,
        project_root: Some(&project_root),
        workflow_id: options.workflow_id.as_deref(),
        task_id: options.task_id.as_deref(),
        run_id: options.run_id.as_deref(),
        context_budget: runtime_policy.context_budget,
        context_budget_source: &runtime_policy.context_budget_source,
        token_headroom: runtime_policy.token_headroom,
        token_headroom_source: &runtime_policy.token_headroom_source,
        require_token_headroom_for_forge_first: runtime_policy
            .require_token_headroom_for_forge_first,
    })?;
    let shim_status = inspect_cli_harness_shim_status(CliShimStatusOptions {
        shim_dir: &options.shim_dir,
        executor: &options.executor,
    })?;
    let command = vec![options.executor.clone()];
    let headroom_plan = build_harness_headroom_plan(HarnessHeadroomPlanOptions {
        executor: &options.executor,
        command: &command,
        forge_first: mode.forge_first,
        forge_first_source: &mode.forge_first_source,
        project_root: Some(&project_root),
        workflow_id: options.workflow_id.as_deref(),
        task_id: options.task_id.as_deref(),
        run_id: options.run_id.as_deref(),
        context_budget: runtime_policy.context_budget,
        context_budget_source: &runtime_policy.context_budget_source,
        token_headroom: runtime_policy.token_headroom,
        token_headroom_source: &runtime_policy.token_headroom_source,
        require_token_headroom_for_forge_first: runtime_policy
            .require_token_headroom_for_forge_first,
    });
    let wrapper_plan = headroom_plan.wrapper_plan.clone();
    let session_lifecycle_plan = headroom_plan.session_lifecycle_plan.clone();
    let adoption_plan = build_harness_adoption_plan(HarnessAdoptionPlanOptions {
        shim_dir: &options.shim_dir,
        executor: &options.executor,
        project_root: Some(&project_root),
        workflow_id: options.workflow_id.as_deref(),
        task_id: options.task_id.as_deref(),
        run_id: options.run_id.as_deref(),
        forge_first: options.forge_first,
        observe_only: options.observe_only,
        context_budget: runtime_policy.context_budget,
        context_budget_source: &runtime_policy.context_budget_source,
        token_headroom: runtime_policy.token_headroom,
        token_headroom_source: &runtime_policy.token_headroom_source,
        require_token_headroom_for_forge_first: runtime_policy
            .require_token_headroom_for_forge_first,
    })?;
    let headroom_preview = analyze_token_headroom(
        "Forge harness preview: route bounded context, shell receipts, logs, tool output and CLI stdout through local token headroom while preserving retrieval references.",
        Some("text"),
        runtime_policy.context_budget,
        "interactive_harness_preview",
        true,
    );
    let headroom_stats = build_headroom_stats_report(
        store,
        HeadroomStatsOptions {
            source: None,
            content_kind: None,
            limit: 5,
        },
    )?;
    let commands = interactive_harness_commands(
        &options.executor,
        &options.shim_dir,
        &project_root,
        runtime_policy.context_budget,
        runtime_policy.token_headroom,
    );
    let headroom_operational_status = headroom_stats.operational_status.clone();
    let headroom_recommended_action = headroom_stats.recommended_action.clone();
    let executor_compatibility = build_harness_executor_compatibility_report(
        &options.executor,
        &project_root,
        &options.shim_dir,
        &doctor,
        &wrapper_plan,
        &session_lifecycle_plan,
    );
    let forge_first_adoption_readiness = build_interactive_harness_forge_first_adoption_readiness(
        &options.executor,
        &mode,
        &doctor,
        &shim_status,
        &wrapper_plan,
        &session_lifecycle_plan,
        &commands,
    );
    let mut next_actions = doctor.next_actions.clone();
    next_actions.push(format!(
        "headroom recommended action: {}",
        headroom_stats.recommended_action
    ));
    next_actions.push(format!(
        "harness adoption next action: {}",
        adoption_plan.next_action
    ));
    next_actions.push(format!(
        "executor compatibility next action: {}",
        executor_compatibility.next_action
    ));
    next_actions.push("forge interactive readiness --output json".to_string());
    next_actions.push("forge interactive home --output json".to_string());

    Ok(InteractiveHarnessPanel {
        schema_version: INTERACTIVE_HARNESS_SCHEMA_VERSION.to_string(),
        status: "interactive_harness_ready".to_string(),
        executor: options.executor,
        project_root: project_root.display().to_string(),
        shim_dir: options.shim_dir.display().to_string(),
        forge_first_ready: doctor.forge_first_ready,
        token_headroom_ready: doctor.token_headroom_ready,
        shim_ready: doctor.shim_ready,
        lineage_policy_ready: doctor.lineage_policy_ready,
        lineage_context_ready: doctor.lineage_context_ready,
        mode,
        doctor,
        shim_status,
        wrapper_plan,
        headroom_plan,
        adoption_plan,
        forge_first_adoption_readiness,
        headroom_stats,
        headroom_operational_status,
        headroom_recommended_action,
        session_lifecycle_plan,
        executor_compatibility,
        headroom_preview,
        next_actions,
        notes: vec![
            format!("Forge-first source: {forge_first_source}"),
            "This panel is read-only: it does not install shims or launch child CLIs."
                .to_string(),
            "Use wrap-plan before a Forge-controlled brain shell and exec only through guarded harness receipts."
                .to_string(),
        ],
        commands,
    })
}

fn build_interactive_harness_forge_first_adoption_readiness(
    executor: &str,
    mode: &HarnessModeReport,
    doctor: &HarnessDoctorReport,
    shim_status: &CliShimStatusReport,
    wrapper_plan: &CliWrapperPlanReport,
    session_lifecycle_plan: &HarnessSessionLifecyclePlan,
    commands: &InteractiveHarnessCommands,
) -> InteractiveHarnessForgeFirstAdoptionReadiness {
    let mut blocked_reasons = Vec::new();
    if !mode.forge_first {
        blocked_reasons.push("forge_first_default_not_active".to_string());
    }
    if !doctor.token_headroom_ready {
        blocked_reasons.push("token_headroom_not_ready".to_string());
    }
    if mode.require_token_headroom_for_forge_first && !wrapper_plan.token_headroom_enabled {
        blocked_reasons.push("token_headroom_required_but_disabled".to_string());
    }
    if !doctor.shim_ready {
        if harness_shim_file_ready_for_activation(doctor) {
            blocked_reasons.push("forge_shim_installed_but_path_not_active".to_string());
        } else {
            blocked_reasons.push("forge_owned_path_shim_not_ready".to_string());
        }
    }
    if !doctor.lineage_policy_ready {
        blocked_reasons.push("lineage_policy_not_ready".to_string());
    }
    let execution_guard_status =
        if mode.require_lineage_for_exec && !session_lifecycle_plan.lineage_complete {
            "guarded_until_workflow_task_run_lineage"
        } else if mode.require_lineage_for_exec {
            "lineage_satisfied_for_real_exec"
        } else {
            "lineage_not_required_for_real_exec"
        }
        .to_string();

    let mut next_commands = Vec::new();
    if !mode.forge_first {
        next_commands.push(interactive_forge_command_line(&commands.adoption_plan));
        next_commands.push(interactive_forge_command_line(
            &commands.bootstrap_project_harness,
        ));
    }
    if !doctor.shim_ready && harness_shim_file_ready_for_activation(doctor) {
        next_commands.push(interactive_forge_command_line(&commands.activation_profile));
    } else if !doctor.shim_ready {
        next_commands.push(interactive_forge_command_line(&commands.install_shims));
    }
    if mode.require_lineage_for_exec && !session_lifecycle_plan.lineage_complete {
        next_commands.push(interactive_forge_command_line(&commands.sessions));
        next_commands.push(interactive_forge_command_line(&commands.lineage_plan));
        next_commands.push(interactive_forge_command_line(
            &commands.lineage_exec_dry_run,
        ));
    }
    next_commands.push(interactive_forge_command_line(&commands.wrap_plan));
    next_commands.push(interactive_forge_command_line(&commands.headroom_plan));

    let ready_to_use_as_default = blocked_reasons.is_empty();
    let status = if ready_to_use_as_default {
        "forge_first_default_ready"
    } else {
        "forge_first_default_blocked"
    };
    let wrapper_interception_points = wrapper_plan
        .headroom_runtime_plan
        .interception_points
        .iter()
        .map(|point| format!("{}:{}", point.point_id, point.action))
        .collect::<Vec<_>>();
    let controlled_routes = wrapper_plan
        .orchestration_contract
        .routing_stages
        .iter()
        .map(|stage| format!("{}:{}->{}", stage.id, stage.owner, stage.target))
        .collect::<Vec<_>>();
    let mut readiness_gates = wrapper_plan.orchestration_contract.gates.clone();
    readiness_gates.extend(
        session_lifecycle_plan
            .gates
            .iter()
            .map(|gate| format!("{}:{}", gate.gate_id, gate.status)),
    );
    next_commands.sort();
    next_commands.dedup();

    InteractiveHarnessForgeFirstAdoptionReadiness {
        schema_version: "forge.interactive.harness_forge_first_adoption.v1".to_string(),
        status: status.to_string(),
        executor: executor.to_string(),
        forge_first_default_active: mode.forge_first,
        ready_to_use_as_default,
        token_headroom_ready: doctor.token_headroom_ready,
        token_headroom_required: mode.require_token_headroom_for_forge_first,
        shim_ready: doctor.shim_ready,
        activation_status: shim_status.activation_diagnostic.status.clone(),
        activation_required: shim_status.activation_diagnostic.activation_required,
        activation_possible: shim_status.activation_diagnostic.activation_possible,
        activation_reason: shim_status.activation_diagnostic.reason.clone(),
        activation_command: shim_status
            .activation_diagnostic
            .one_shot_activation_command
            .clone(),
        activation_profile_command: shim_status
            .activation_diagnostic
            .activation_profile_command
            .clone(),
        lineage_policy_ready: doctor.lineage_policy_ready,
        lineage_context_ready: doctor.lineage_context_ready,
        execution_guard_status,
        wrapper_strategy: wrapper_plan.wrapper_strategy.clone(),
        wrapper_interception_points,
        controlled_routes,
        readiness_gates,
        blocked_reasons,
        next_commands,
        notes: vec![
            "This contract is read-only and does not install shims, modify PATH or launch child CLIs."
                .to_string(),
            "Treat Codex, OpenCode, Gemini and Claude as replaceable execution brains; Forge remains the workflow, context, memory, permission, headroom and session control plane."
                .to_string(),
        ],
    }
}

fn interactive_forge_command_line(command: &[String]) -> String {
    let mut parts = Vec::with_capacity(command.len() + 1);
    if command.first().is_none_or(|part| part != "forge") {
        parts.push("forge");
    }
    parts.extend(command.iter().map(String::as_str));
    parts
        .into_iter()
        .map(interactive_shell_arg)
        .collect::<Vec<_>>()
        .join(" ")
}

fn interactive_shell_arg(arg: &str) -> String {
    if (arg.starts_with('<') && arg.ends_with('>')) || arg.starts_with('$') {
        return arg.to_string();
    }
    if arg
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | ':' | '='))
    {
        return arg.to_string();
    }
    format!("'{}'", arg.replace('\'', "'\"'\"'"))
}

fn harness_shim_file_ready_for_activation(doctor: &HarnessDoctorReport) -> bool {
    doctor.shim_status.shim_exists
        && doctor.shim_status.forge_owned
        && doctor.shim_status.executable
        && !doctor.shim_status.would_recurse
}

pub fn build_interactive_sessions(
    store: &ForgeStore,
    options: InteractiveSessionsOptions,
) -> Result<InteractiveSessionsPanel> {
    let executors = load_executors(store)?;
    let session_report = build_brain_sessions_report_with_options(
        store,
        &executors.brain_router,
        BrainSessionsReportOptions {
            provider_id: options.provider_id,
            lifecycle_state: options.lifecycle_state,
            readiness: options.readiness,
        },
    )?;
    let session_cards = session_report
        .sessions
        .iter()
        .map(interactive_session_card)
        .collect::<Vec<_>>();
    let commands = interactive_sessions_commands(&session_report);
    let mut next_actions = session_report.next_actions.clone();
    next_actions.push("forge interactive readiness --output json".to_string());
    next_actions.push("forge interactive home --output json".to_string());

    Ok(InteractiveSessionsPanel {
        schema_version: INTERACTIVE_SESSIONS_SCHEMA_VERSION.to_string(),
        status: "interactive_sessions_ready".to_string(),
        controller: session_report.controller.clone(),
        selected_provider_id: session_report.selected_provider_id.clone(),
        provider_count: session_report.provider_count,
        session_count: session_report.session_count,
        ready_session_count: session_report.ready_session_count,
        planned_event_count: session_report.planned_event_count,
        lifecycle_event_count: session_report.lifecycle_event_count,
        session_report,
        session_cards,
        commands,
        next_actions,
        notes: vec![
            "Session center is read-only: it does not open, attach or close shells by itself."
                .to_string(),
            "Use lifecycle commands to record human-visible shell state changes before handoff."
                .to_string(),
        ],
    })
}

pub fn build_interactive_readiness(store: &ForgeStore) -> Result<InteractiveReadinessPanel> {
    let executors = load_executors(store)?;
    let runtimes = load_runtimes(store)?;
    let repository_context_path = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let harness_shim_dir = default_interactive_harness_shim_dir();
    let harness_mode = build_harness_mode_report(HarnessModeOptions {
        forge_first: false,
        observe_only: false,
        project_root: Some(&repository_context_path),
    });
    let harness_doctor = build_harness_doctor_report(HarnessDoctorOptions {
        shim_dir: &harness_shim_dir,
        executor: "codex",
        forge_first: false,
        observe_only: false,
        project_root: Some(&repository_context_path),
        workflow_id: None,
        task_id: None,
        run_id: None,
        context_budget: 1200,
        context_budget_source: "interactive_default",
        token_headroom: true,
        token_headroom_source: "interactive_default",
        require_token_headroom_for_forge_first: false,
    })?;
    let harness_adoption_plan = build_harness_adoption_plan(HarnessAdoptionPlanOptions {
        shim_dir: &harness_shim_dir,
        executor: "codex",
        project_root: Some(&repository_context_path),
        workflow_id: None,
        task_id: None,
        run_id: None,
        forge_first: false,
        observe_only: false,
        context_budget: 1200,
        context_budget_source: "interactive_default",
        token_headroom: true,
        token_headroom_source: "interactive_default",
        require_token_headroom_for_forge_first: false,
    })?;
    let headroom_stats = build_headroom_stats_report(
        store,
        HeadroomStatsOptions {
            source: None,
            content_kind: None,
            limit: 5,
        },
    )?;
    let shell_entrypoints = executors
        .brain_router
        .shell_sessions
        .iter()
        .map(|session| {
            format!(
                "{}: {}",
                session.id,
                if session.entry_command.is_empty() {
                    "<none>".to_string()
                } else {
                    session.entry_command.join(" ")
                }
            )
        })
        .collect::<Vec<_>>();
    let forge_first_shell_count = executors
        .brain_router
        .shell_sessions
        .iter()
        .filter(|session| session.forge_first_ready)
        .count();
    let mut next_actions = readiness_next_actions(
        executors.usable.is_empty(),
        executors.needs_human_approval,
        runtimes.needs_human_approval,
        &harness_doctor,
    );
    next_actions.push(format!(
        "headroom recommended action: {}",
        headroom_stats.recommended_action
    ));
    next_actions.push(format!(
        "harness adoption next action: {}",
        harness_adoption_plan.next_action
    ));
    let headroom_operational_status = headroom_stats.operational_status.clone();
    let headroom_recommended_action = headroom_stats.recommended_action.clone();

    Ok(InteractiveReadinessPanel {
        schema_version: INTERACTIVE_READINESS_SCHEMA_VERSION.to_string(),
        status: "interactive_readiness_ready".to_string(),
        executor_count: executors.executors.len(),
        usable_executor_count: executors.usable.len(),
        usable_executors: executors.usable.clone(),
        needs_executor_approval: executors.needs_human_approval,
        runtime_count: runtimes.runtimes.len(),
        usable_runtime_count: runtimes.usable.len(),
        usable_runtimes: runtimes.usable.clone(),
        needs_runtime_approval: runtimes.needs_human_approval,
        brain_count: executors.brain_router.brains.len(),
        selected_brain: executors
            .brain_router
            .selected_brain
            .clone()
            .unwrap_or_else(|| "none".to_string()),
        forge_controlled_surface_count: executors.brain_router.forge_controlled_surfaces.len(),
        forge_controlled_surfaces: executors.brain_router.forge_controlled_surfaces.clone(),
        shell_count: executors.brain_router.shell_sessions.len(),
        forge_first_shell_count,
        shell_entrypoints,
        harness_mode,
        harness_doctor,
        harness_adoption_plan,
        headroom_stats,
        headroom_operational_status,
        headroom_recommended_action,
        next_actions,
        commands: readiness_commands(),
    })
}

pub fn build_interactive_release_gates(
    store: &ForgeStore,
    version: &str,
    project_root: Option<&Path>,
) -> Result<InteractiveReleaseGatesPanel> {
    let status = build_milestone_status(version)?;
    let manifest = build_milestone_manifest_with_store(version, Some(store))?;
    let required_evidence_by_capability = manifest
        .requirements
        .iter()
        .map(|requirement| {
            (
                requirement.capability_id.clone(),
                requirement.required_evidence.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut attached_evidence_by_capability: BTreeMap<String, Vec<MilestoneAttachedEvidence>> =
        BTreeMap::new();
    for evidence in &manifest.attached_evidence {
        attached_evidence_by_capability
            .entry(evidence.capability_id.clone())
            .or_default()
            .push(evidence.clone());
    }
    let promotion_ready_capability_ids = manifest
        .completed_capabilities
        .iter()
        .map(|capability| capability.id.clone())
        .collect::<BTreeSet<_>>();
    let gate_cards = status
        .capabilities
        .iter()
        .map(|capability| {
            let promotion_ready = promotion_ready_capability_ids.contains(&capability.id);
            let attached_evidence = attached_evidence_by_capability
                .get(&capability.id)
                .cloned()
                .unwrap_or_default();
            let attached_evidence_count = attached_evidence.len();
            let required_attached_evidence_kinds =
                milestone_required_attached_evidence_kinds(&capability.id);
            let attached_evidence_kinds = attached_evidence
                .iter()
                .map(|evidence| evidence.kind.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let attached_evidence_kind_set = attached_evidence_kinds
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>();
            let missing_attached_evidence_kinds = required_attached_evidence_kinds
                .iter()
                .filter(|kind| !attached_evidence_kind_set.contains(*kind))
                .cloned()
                .collect::<Vec<_>>();
            let attached_evidence_state = release_gate_attached_evidence_state(
                &required_attached_evidence_kinds,
                &missing_attached_evidence_kinds,
                attached_evidence_count,
                promotion_ready,
            );
            let evidence_plan = interactive_release_gate_evidence_plan(
                store,
                version,
                &capability.id,
                project_root,
            )?;
            Ok(InteractiveReleaseGateCard {
                capability_id: capability.id.clone(),
                title: capability.title.clone(),
                status: capability.status.clone(),
                promotion_ready,
                required_evidence: required_evidence_by_capability
                    .get(&capability.id)
                    .cloned()
                    .unwrap_or_else(|| "milestone evidence required".to_string()),
                evidence: capability.evidence.clone(),
                attached_evidence_state,
                required_attached_evidence_kinds,
                attached_evidence_kinds,
                missing_attached_evidence_kinds,
                attached_evidence_count,
                attached_evidence,
                evidence_plan,
                gap_before_promotion: capability.gap_before_promotion.clone(),
                next_commands: release_gate_next_commands(&capability.id),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let blocked_gate_count = gate_cards
        .iter()
        .filter(|gate| !gate.promotion_ready)
        .count();
    let commands = release_gate_commands(&status.milestone);
    let mut next_actions = manifest
        .known_gaps
        .iter()
        .map(|gap| format!("{}: {}", gap.capability_id, gap.next_action))
        .collect::<Vec<_>>();
    if next_actions.is_empty() {
        next_actions.push(manifest.promotion_decision.next_action.clone());
    }

    Ok(InteractiveReleaseGatesPanel {
        schema_version: INTERACTIVE_RELEASE_GATES_SCHEMA_VERSION.to_string(),
        status: "interactive_release_gates_ready".to_string(),
        milestone: status.milestone,
        promotion_ready: manifest.promotion_decision.promotable,
        promotion_decision: manifest.promotion_decision.clone(),
        blocked_by: manifest.promotion_decision.blocked_by.clone(),
        summary: status.summary,
        gate_count: gate_cards.len(),
        blocked_gate_count,
        attached_evidence_count: manifest.attached_evidence.len(),
        attached_evidence: manifest.attached_evidence,
        gate_cards,
        commands,
        next_actions,
    })
}

fn interactive_release_gate_evidence_plan(
    store: &ForgeStore,
    version: &str,
    capability_id: &str,
    project_root: Option<&Path>,
) -> Result<InteractiveReleaseGateEvidencePlan> {
    let plan = build_milestone_evidence_plan(
        store,
        MilestoneEvidencePlanOptions {
            version,
            capability_id,
            project_root,
            connected_brain: None,
            connected_runtime: None,
        },
    )?;
    let missing_config_check_count = plan
        .config_checks
        .iter()
        .filter(|check| matches!(check.status.as_str(), "missing" | "blocked" | "invalid"))
        .count();
    let manifest_template_ids = plan
        .manifest_templates
        .iter()
        .map(|template| template.id.clone())
        .collect::<Vec<_>>();
    let manifest_template_paths = plan
        .manifest_templates
        .iter()
        .map(|template| template.target_path.clone())
        .collect::<Vec<_>>();
    Ok(InteractiveReleaseGateEvidencePlan {
        schema_version: "forge.interactive.release_gate_evidence_plan.v1".to_string(),
        status: plan.status,
        ready_to_collect_evidence: plan.ready_to_collect_evidence,
        project_root: plan.project_root,
        config_check_count: plan.config_checks.len(),
        missing_config_check_count,
        config_checks: plan.config_checks,
        manifest_template_count: plan.manifest_templates.len(),
        manifest_template_ids,
        manifest_template_paths,
        manifest_templates: plan.manifest_templates,
        provider_candidate_count: plan.provider_candidates.len(),
        provider_candidates: plan.provider_candidates,
        promotion_gate_templates: plan.promotion_gate_templates,
        evidence_collection_commands: plan.evidence_collection_commands,
        next_action: plan.next_action,
    })
}

pub fn build_interactive_command_palette(
    store: &ForgeStore,
    query: Option<&str>,
) -> Result<InteractiveCommandPalettePanel> {
    let workflows = list_workflows_with_filters(
        store,
        WorkflowRegistryFilters::new(WorkflowLifecycleFilter::All),
    )?;
    let query = query.unwrap_or_default().trim().to_string();
    let mut entries = base_command_palette_entries();
    entries.extend(addon_command_palette_entries(store)?);
    entries.extend(workflow_command_palette_entries(&workflows.workflows));
    let entries = entries
        .into_iter()
        .filter(|entry| command_palette_entry_matches(entry, &query))
        .collect::<Vec<_>>();
    let groups = command_palette_groups(&entries);

    Ok(InteractiveCommandPalettePanel {
        schema_version: INTERACTIVE_COMMAND_PALETTE_SCHEMA_VERSION.to_string(),
        status: "command_palette_ready".to_string(),
        query,
        group_count: groups.len(),
        entry_count: entries.len(),
        groups,
        entries,
        navigation: vec![
            navigation_key(
                "/",
                "open_command_palette",
                "global",
                "Open command palette",
            ),
            navigation_key(
                "enter",
                "run_selected_action",
                "command",
                "Run selected action",
            ),
            navigation_key(
                "esc",
                "close_command_palette",
                "global",
                "Close command palette",
            ),
        ],
    })
}

pub fn build_interactive_action_registry(
    store: &ForgeStore,
    query: Option<&str>,
) -> Result<InteractiveActionRegistryPanel> {
    let palette = build_interactive_command_palette(store, None)?;
    Ok(build_action_registry_from_palette_with_query(
        &palette,
        query.unwrap_or_default(),
    ))
}

pub fn build_interactive_action_invocation(
    store: &ForgeStore,
    action_id: &str,
) -> Result<InteractiveActionInvocationReport> {
    let action_id = action_id.trim().to_string();
    let registry = build_interactive_action_registry(store, None)?;
    let matches = registry
        .actions
        .iter()
        .filter(|action| action.action_id == action_id)
        .cloned()
        .collect::<Vec<_>>();
    let action = matches.first().cloned();
    let operation_plan = action
        .as_ref()
        .map(|action| action.operation_plan.clone())
        .unwrap_or_else(|| {
            command_palette_action_plan(
                "not_found",
                "inspect_action_registry",
                true,
                "action_not_found",
                vec![vec![
                    "interactive".to_string(),
                    "action-registry".to_string(),
                    "--query".to_string(),
                    action_id.clone(),
                    "--output".to_string(),
                    "json".to_string(),
                ]],
            )
        });
    let can_execute = matches.len() == 1
        && action.as_ref().is_some_and(|action| action.enabled)
        && !operation_plan.diagnostic_only
        && action
            .as_ref()
            .is_some_and(|action| !action.commands.is_empty());
    let diagnostic_only = operation_plan.diagnostic_only || !can_execute;
    let selected_command = if can_execute {
        action
            .as_ref()
            .map(|action| action.commands.clone())
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let blocked_reason = if matches.is_empty() {
        "action_not_found".to_string()
    } else if matches.len() > 1 {
        "action_ambiguous".to_string()
    } else {
        action
            .as_ref()
            .map(|action| action.blocked_reason.clone())
            .unwrap_or_else(|| "action_not_found".to_string())
    };
    let status = if matches.is_empty() {
        "action_invocation_not_found"
    } else if matches.len() > 1 {
        "action_invocation_ambiguous"
    } else if can_execute {
        "action_invocation_ready"
    } else {
        "action_invocation_blocked"
    };
    let selected_command_text = selected_command.join(" ");
    let next_commands = operation_plan.next_commands.clone();
    let recommended_action = operation_plan.recommended_action.clone();
    let source_panel = action
        .as_ref()
        .map(|action| action.source_panel.clone())
        .unwrap_or_else(|| "none".to_string());
    let risk_level = action
        .as_ref()
        .map(|action| action.risk_level.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let mutates_workflow = action
        .as_ref()
        .is_some_and(|action| action.mutates_workflow);
    let requires_approval = action
        .as_ref()
        .is_some_and(|action| action.requires_approval);
    let execution_boundary = if can_execute {
        "external_command_not_executed"
    } else {
        "diagnostic_only_not_executed"
    }
    .to_string();

    Ok(InteractiveActionInvocationReport {
        schema_version: INTERACTIVE_ACTION_INVOCATION_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        requested_action_id: action_id.clone(),
        match_count: matches.len(),
        can_execute,
        diagnostic_only,
        not_executed: true,
        selected_command,
        selected_command_text,
        source_panel,
        risk_level,
        mutates_workflow,
        requires_approval,
        execution_boundary,
        blocked_reason,
        recommended_action,
        next_commands,
        operation_plan,
        action,
        commands: InteractiveActionInvocationCommands {
            action_invocation: vec![
                "interactive".to_string(),
                "action-invocation".to_string(),
                "--action".to_string(),
                action_id.clone(),
                "--output".to_string(),
                "json".to_string(),
            ],
            action_registry: vec![
                "interactive".to_string(),
                "action-registry".to_string(),
                "--query".to_string(),
                action_id.clone(),
                "--output".to_string(),
                "json".to_string(),
            ],
            command_palette: vec![
                "interactive".to_string(),
                "command-palette".to_string(),
                "--query".to_string(),
                action_id.clone(),
                "--output".to_string(),
                "json".to_string(),
            ],
            autocomplete: vec![
                "interactive".to_string(),
                "autocomplete".to_string(),
                "--input".to_string(),
                action_id,
                "--output".to_string(),
                "json".to_string(),
            ],
        },
    })
}

fn build_action_registry_from_palette(
    palette: &InteractiveCommandPalettePanel,
) -> InteractiveActionRegistryPanel {
    build_action_registry_from_palette_with_query(palette, &palette.query)
}

fn build_action_registry_from_palette_with_query(
    palette: &InteractiveCommandPalettePanel,
    query: &str,
) -> InteractiveActionRegistryPanel {
    let query = query.trim().to_string();
    let actions = palette
        .entries
        .iter()
        .filter(|entry| action_registry_entry_matches(entry, &query))
        .cloned()
        .collect::<Vec<_>>();
    let counts = action_registry_counts(&actions);
    let groups = command_palette_groups(&actions)
        .iter()
        .map(|group| action_registry_group(&group.group_id, &group.title, group.entries.clone()))
        .collect::<Vec<_>>();

    InteractiveActionRegistryPanel {
        schema_version: INTERACTIVE_ACTION_REGISTRY_SCHEMA_VERSION.to_string(),
        status: "action_registry_ready".to_string(),
        query,
        action_count: counts.action_count,
        enabled_action_count: counts.enabled_action_count,
        blocked_action_count: counts.blocked_action_count,
        diagnostic_action_count: counts.diagnostic_action_count,
        mutation_action_count: counts.mutation_action_count,
        approval_action_count: counts.approval_action_count,
        group_count: groups.len(),
        groups,
        actions,
        commands: InteractiveActionRegistryCommands {
            action_registry: vec![
                "interactive".to_string(),
                "action-registry".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            command_palette: vec![
                "interactive".to_string(),
                "command-palette".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            autocomplete: vec![
                "interactive".to_string(),
                "autocomplete".to_string(),
                "--input".to_string(),
                "<input>".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            inspect_addons: vec![
                "addons".to_string(),
                "views".to_string(),
                "--surface".to_string(),
                "tui".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
        },
        navigation: vec![
            navigation_key(
                "enter",
                "execute_or_inspect_action",
                "action",
                "Run ready action or inspect blocked action",
            ),
            navigation_key(
                "tab",
                "focus_next_action_group",
                "action_registry",
                "Move to the next action group",
            ),
            navigation_key(
                "/",
                "filter_action_registry",
                "action_registry",
                "Filter actions by title, command, Addon or keyword",
            ),
            navigation_key(
                "esc",
                "close_action_registry",
                "action_registry",
                "Close the action registry",
            ),
        ],
    }
}

fn action_registry_entry_matches(entry: &InteractiveCommandPaletteEntry, query: &str) -> bool {
    let terms = query
        .split_whitespace()
        .map(|term| term.trim().to_ascii_lowercase())
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>();
    if terms.is_empty() {
        return true;
    }

    let haystack = format!(
        "{} {} {} {} {} {}",
        entry.action_id,
        entry.group_id,
        entry.title,
        entry.description,
        entry.source_panel,
        entry
            .commands
            .iter()
            .chain(entry.keywords.iter())
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
    )
    .to_ascii_lowercase();
    terms.iter().all(|term| haystack.contains(term))
}

fn action_registry_group(
    group_id: &str,
    title: &str,
    actions: Vec<InteractiveCommandPaletteEntry>,
) -> InteractiveActionRegistryGroup {
    let counts = action_registry_counts(&actions);
    InteractiveActionRegistryGroup {
        group_id: group_id.to_string(),
        title: title.to_string(),
        action_count: counts.action_count,
        enabled_action_count: counts.enabled_action_count,
        blocked_action_count: counts.blocked_action_count,
        diagnostic_action_count: counts.diagnostic_action_count,
        mutation_action_count: counts.mutation_action_count,
        approval_action_count: counts.approval_action_count,
        actions,
    }
}

fn action_registry_counts(
    actions: &[InteractiveCommandPaletteEntry],
) -> InteractiveActionRegistryCounts {
    let mut counts = InteractiveActionRegistryCounts {
        action_count: actions.len(),
        ..InteractiveActionRegistryCounts::default()
    };
    for action in actions {
        if action.enabled {
            counts.enabled_action_count += 1;
        } else {
            counts.blocked_action_count += 1;
        }
        if action.operation_plan.diagnostic_only {
            counts.diagnostic_action_count += 1;
        }
        if action.mutates_workflow {
            counts.mutation_action_count += 1;
        }
        if action.requires_approval {
            counts.approval_action_count += 1;
        }
    }
    counts
}

fn base_command_palette_entries() -> Vec<InteractiveCommandPaletteEntry> {
    vec![
        command_palette_entry(
            "navigation.home",
            "navigation",
            "Open interactive home",
            "Inspect the full Forge operator dashboard.",
            "navigation_panel",
            None,
            &["interactive", "home", "--output", "json"],
            false,
            false,
            "low",
            &["home", "dashboard", "navigation", "tui"],
        ),
        command_palette_entry(
            "navigation.guided_cockpit",
            "navigation",
            "Open guided cockpit",
            "Open the Forge 0.5 guided cockpit with the end-to-end operator checklist, panes, previews and safe actions.",
            "guided_cockpit_panel",
            None,
            &["interactive", "guided-cockpit", "--output", "json"],
            false,
            false,
            "low",
            &["guide", "guided", "cockpit", "tui", "workflow"],
        ),
        command_palette_entry(
            "navigation.ui_composition",
            "navigation",
            "Open UI composition",
            "Inspect dynamic Core and Addon widget composition for TUI, web and agent dashboards.",
            "ui_composition_panel",
            None,
            &["interactive", "ui-composition", "--output", "json"],
            false,
            false,
            "low",
            &["ui", "composition", "widgets", "addons", "renderer", "dashboard"],
        ),
        command_palette_entry(
            "navigation.slash_commands",
            "navigation",
            "Open slash commands",
            "List conversational slash-command equivalents for the TUI.",
            "navigation_panel",
            None,
            &["interactive", "slash-commands", "--output", "json"],
            false,
            false,
            "low",
            &["slash", "command", "palette", "route"],
        ),
        command_palette_entry(
            "readiness.open",
            "readiness",
            "Open interactive readiness",
            "Audit executors, brains, shells, Forge-controlled surfaces and harness diagnostics.",
            "readiness_panel",
            None,
            &["interactive", "readiness", "--output", "json"],
            false,
            false,
            "low",
            &["readiness", "executor", "brain", "shell", "harness"],
        ),
        command_palette_entry(
            "readiness.replacement_cli",
            "readiness",
            "Open replacement CLI readiness",
            "Inspect replacement-grade CLI readiness across TUI, actions, patch UX, harness, sessions and milestone evidence.",
            "replacement_cli_panel",
            None,
            &["interactive", "replacement-cli", "--output", "json"],
            false,
            false,
            "low",
            &["replacement", "cli", "tui", "patch", "harness", "sessions", "milestone"],
        ),
        command_palette_entry(
            "architecture.compass",
            "architecture",
            "Open architecture compass",
            "Inspect source-of-truth tracks, evidence, gaps, dependencies, reuse and benchmark boundaries.",
            "architecture_compass_panel",
            None,
            &["interactive", "architecture", "--output", "json"],
            false,
            false,
            "low",
            &[
                "architecture",
                "compass",
                "gaps",
                "benchmark",
                "addons",
                "workflow",
                "tenant",
                "headroom",
            ],
        ),
        command_palette_entry(
            "operations.cockpit",
            "operations",
            "Open operational cockpit",
            "Inspect the unified operator focus for workflows, handoffs, human waits, brain readiness and observability.",
            "operational_cockpit_panel",
            None,
            &["interactive", "operational-cockpit", "--output", "json"],
            false,
            false,
            "low",
            &[
                "operational",
                "operations",
                "cockpit",
                "focus",
                "dashboard",
                "handoff",
                "attention",
            ],
        ),
        command_palette_entry(
            "harness.open",
            "harness",
            "Open harness center",
            "Inspect Forge-first CLI controls, wrap plans, shims and token headroom.",
            "harness_panel",
            None,
            &["interactive", "harness", "--output", "json"],
            false,
            false,
            "low",
            &["harness", "shim", "forge-first", "brain", "cli"],
        ),
        command_palette_entry(
            "harness.headroom_plan",
            "harness",
            "Inspect headroom plan",
            "Inspect token-headroom wrapper policy before opening Forge-controlled brain shells.",
            "harness_panel",
            None,
            &[
                "harness",
                "headroom-plan",
                "--executor",
                "codex",
                "--project-root",
                ".",
                "--output",
                "json",
            ],
            false,
            false,
            "low",
            &["harness", "headroom", "token", "wrapper", "budget", "plan"],
        ),
        command_palette_entry(
            "harness.adoption_plan",
            "harness",
            "Inspect harness adoption plan",
            "Inspect the governed Forge-first adoption plan before writing project policy or shims.",
            "harness_panel",
            None,
            &[
                "harness",
                "adoption-plan",
                "--executor",
                "codex",
                "--shim-dir",
                "$HOME/.forge/bin",
                "--project-root",
                ".",
                "--output",
                "json",
            ],
            false,
            false,
            "low",
            &["harness", "adoption", "bootstrap", "forge-first", "shim", "plan"],
        ),
        command_palette_entry(
            "harness.lineage_plan",
            "harness",
            "Inspect lineage handoff plan",
            "Inspect the Forge-first harness plan with workflow/task/run placeholders before a brain CLI handoff.",
            "harness_panel",
            None,
            &[
                "harness",
                "adoption-plan",
                "--executor",
                "codex",
                "--shim-dir",
                "$HOME/.forge/bin",
                "--project-root",
                ".",
                "--workflow",
                "<workflow-id>",
                "--task",
                "<task-id>",
                "--run",
                "<run-id>",
                "--output",
                "json",
            ],
            false,
            false,
            "low",
            &[
                "harness",
                "lineage",
                "workflow",
                "task",
                "run",
                "handoff",
                "brain",
            ],
        ),
        command_palette_entry(
            "harness.lineage_exec_dry_run",
            "harness",
            "Validate lineage exec dry-run",
            "Validate the guarded harness exec receipt with workflow/task/run lineage without executing the child CLI.",
            "harness_panel",
            None,
            &[
                "harness",
                "exec",
                "--executor",
                "codex",
                "--forge-first",
                "--project-root",
                ".",
                "--workflow",
                "<workflow-id>",
                "--task",
                "<task-id>",
                "--run",
                "<run-id>",
                "--output",
                "json",
                "--",
                "codex",
            ],
            true,
            true,
            "medium",
            &[
                "harness",
                "lineage",
                "exec",
                "dry-run",
                "workflow",
                "task",
                "run",
                "handoff",
                "brain",
            ],
        ),
        command_palette_entry(
            "harness.bootstrap_project_harness",
            "harness",
            "Bootstrap project harness",
            "Apply the reviewed Forge-first harness policy and Forge-owned CLI shims with operator approval.",
            "harness_panel",
            None,
            &[
                "harness",
                "bootstrap",
                "--executor",
                "codex",
                "--shim-dir",
                "$HOME/.forge/bin",
                "--project-root",
                ".",
                "--apply",
                "--approved-by",
                "<operator>",
                "--output",
                "json",
            ],
            true,
            true,
            "medium",
            &["harness", "bootstrap", "adoption", "forge-first", "shim", "apply"],
        ),
        command_palette_entry(
            "harness.headroom_stats",
            "harness",
            "Inspect headroom stats",
            "Inspect persisted token-headroom savings and retrieval evidence for CLI output.",
            "harness_panel",
            None,
            &["harness", "headroom-stats", "--output", "json"],
            false,
            false,
            "low",
            &["harness", "headroom", "token", "stats", "savings", "retrieval"],
        ),
        command_palette_entry(
            "sessions.open",
            "sessions",
            "Open session center",
            "Inspect provider sessions, lifecycle state, shell history and lifecycle controls.",
            "sessions_panel",
            None,
            &["interactive", "sessions", "--output", "json"],
            false,
            false,
            "low",
            &["session", "shell", "provider", "brain", "history"],
        ),
        command_palette_entry(
            "permissions.open",
            "permissions",
            "Open permission center",
            "Inspect tenant memberships, Addon authorizations and pending human approvals.",
            "permissions_panel",
            None,
            &["interactive", "permissions", "--output", "json"],
            false,
            false,
            "low",
            &["permissions", "approval", "tenant", "addon", "membership"],
        ),
        command_palette_entry(
            "identity.open",
            "identity",
            "Open identity center",
            "Inspect operating context, identity registry, channel aliases, memberships and tenant audit.",
            "identity_panel",
            None,
            &["interactive", "identity", "--output", "json"],
            false,
            false,
            "low",
            &["identity", "alias", "tenant", "context", "channel"],
        ),
        command_palette_entry(
            "workflow.task_board",
            "workflow",
            "Open task board",
            "Inspect operable task lanes, ready handoffs, checkpoints and artifacts.",
            "task_board_panel",
            None,
            &["interactive", "task-board", "--output", "json"],
            false,
            false,
            "low",
            &["workflow", "task", "board", "handoff", "checkpoint"],
        ),
        command_palette_entry(
            "workflow.sidebar",
            "workflow",
            "Open workflow sidebar",
            "Inspect grouped workflow navigation with selected workflow, runtime state and drill-down commands.",
            "workflow_sidebar_panel",
            None,
            &["interactive", "workflow-sidebar", "--output", "json"],
            false,
            false,
            "low",
            &["workflow", "sidebar", "navigation", "active", "event-driven"],
        ),
        command_palette_entry(
            "workflow.dag",
            "workflow",
            "Open workflow DAG",
            "Inspect workflow dependency graphs, readiness and human waits.",
            "dag_panel",
            None,
            &["interactive", "workflow-dag", "--output", "json"],
            false,
            false,
            "low",
            &["workflow", "dag", "graph", "dependency", "wait"],
        ),
        command_palette_entry(
            "workflow.mutation",
            "workflow",
            "Open workflow mutation planner",
            "Inspect DAG, task-board, modifier lane, handoffs, costs and safe commands before replanning or mutating workflows.",
            "workflow_mutation_panel",
            None,
            &["interactive", "workflow-mutation", "--output", "json"],
            false,
            false,
            "low",
            &[
                "workflow",
                "mutation",
                "replan",
                "replanning",
                "modifier",
                "goal",
                "node",
                "brain",
                "artifact",
            ],
        ),
        command_palette_entry(
            "observability.structured_logs",
            "observability",
            "Open structured logs",
            "Inspect recent Forge event logs with workflow and correlation context.",
            "structured_logs_panel",
            None,
            &["interactive", "structured-logs", "--output", "json"],
            false,
            false,
            "low",
            &["observability", "logs", "events", "timeline", "debug"],
        ),
        command_palette_entry(
            "observability.improvement_loop",
            "observability",
            "Open improvement loop",
            "Inspect self-improvement candidates with log, cost, validation and outcome evidence before governed mutations.",
            "improvement_loop_panel",
            None,
            &["interactive", "improvement-loop", "--output", "json"],
            false,
            false,
            "low",
            &[
                "improve",
                "improvement",
                "cost",
                "validation",
                "logs",
                "self-improvement",
                "candidate",
            ],
        ),
    ]
}

fn addon_command_palette_entries(
    store: &ForgeStore,
) -> Result<Vec<InteractiveCommandPaletteEntry>> {
    let addon_dirs = default_addon_dirs();
    let catalog = load_addon_catalog_from_store(store, &addon_dirs)?;
    Ok(addon_view_command_palette_entries(&catalog))
}

fn addon_view_command_palette_entries(
    catalog: &AddonCatalog,
) -> Vec<InteractiveCommandPaletteEntry> {
    let views = list_addon_views(catalog, None, Some("tui"), Some("enabled"));
    views
        .views
        .iter()
        .flat_map(|view| {
            view.view
                .actions
                .iter()
                .filter_map(|action| addon_view_command_palette_entry(view, action))
        })
        .collect()
}

fn addon_view_command_palette_entry(
    view: &AddonViewEntry,
    action: &AddonViewAction,
) -> Option<InteractiveCommandPaletteEntry> {
    if action.action_type != "command" || action.method != "CLI" || action.palette_group.is_empty()
    {
        return None;
    }
    let title = if action.label.is_empty() {
        action.id.clone()
    } else {
        action.label.clone()
    };
    let description = if action.description.is_empty() {
        format!("Run Addon action {} from {}.", action.id, view.view.id)
    } else {
        action.description.clone()
    };
    let source_panel = if action.source_panel.is_empty() {
        format!("addon:{}", view.view.id)
    } else {
        action.source_panel.clone()
    };
    let risk_level = if action.risk_level.is_empty() {
        if action.requires_confirmation {
            "high".to_string()
        } else if action.mutates_workflow {
            "medium".to_string()
        } else {
            "low".to_string()
        }
    } else {
        action.risk_level.clone()
    };
    let keywords = if action.keywords.is_empty() {
        vec![
            view.addon_id.clone(),
            view.view.id.clone(),
            action.id.clone(),
            action.permission.clone(),
        ]
    } else {
        action.keywords.clone()
    };
    let readiness = addon_action_readiness(view, action);
    let commands = if readiness.enabled {
        addon_action_command_template(action)
    } else {
        Vec::new()
    };
    let operation_plan = addon_action_operation_plan(view, &readiness, &commands);

    Some(InteractiveCommandPaletteEntry {
        action_id: action.id.clone(),
        group_id: action.palette_group.clone(),
        title,
        description,
        source_panel,
        enabled: readiness.enabled,
        blocked_reason: readiness.blocked_reason.clone(),
        operation_plan,
        addon_contract: Some(addon_action_contract(view, action, &readiness)),
        addon_view_id: Some(view.view.id.clone()),
        addon_view_action_id: Some(action.id.clone()),
        workflow_id: None,
        commands,
        mutates_workflow: action.mutates_workflow,
        requires_approval: action.requires_confirmation,
        risk_level,
        keywords,
    })
}

fn addon_action_contract(
    view: &AddonViewEntry,
    action: &AddonViewAction,
    readiness: &InteractiveAddonActionReadiness,
) -> InteractiveAddonActionContract {
    let capability_id = view
        .view
        .data_bindings
        .iter()
        .find_map(|binding| {
            if binding.required_capability.is_empty() {
                None
            } else {
                Some(binding.required_capability.clone())
            }
        })
        .unwrap_or_default();

    InteractiveAddonActionContract {
        schema_version: INTERACTIVE_ADDON_ACTION_CONTRACT_SCHEMA_VERSION.to_string(),
        source_addon: view.addon_id.clone(),
        addon_name: view.addon_name.clone(),
        addon_version: view.addon_version.clone(),
        addon_lifecycle: view.addon_lifecycle.clone(),
        capability_id,
        permission_id: action.permission.clone(),
        permission_gate_status: readiness.permission_gate_status.clone(),
        view_id: view.view.id.clone(),
        action_id: action.id.clone(),
        action_type: action.action_type.clone(),
        method: action.method.clone(),
        target: action.target.clone(),
    }
}

fn addon_action_readiness(
    view: &AddonViewEntry,
    action: &AddonViewAction,
) -> InteractiveAddonActionReadiness {
    let permission_gate_status = addon_action_permission_gate_status(view, action);
    let enabled = permission_gate_status == "allowed";
    let blocked_reason = if enabled {
        "ready".to_string()
    } else {
        format!("permission_gate_{permission_gate_status}")
    };
    InteractiveAddonActionReadiness {
        enabled,
        blocked_reason,
        permission_gate_status,
    }
}

fn addon_action_permission_gate_status(view: &AddonViewEntry, action: &AddonViewAction) -> String {
    if !action.permission.trim().is_empty()
        && !view
            .permission_gate
            .declared_permissions
            .iter()
            .any(|permission| permission == &action.permission)
    {
        return "undeclared_permission".to_string();
    }
    view.permission_gate.status.clone()
}

fn addon_action_command_template(action: &AddonViewAction) -> Vec<String> {
    if !action.command_template.is_empty() {
        return action.command_template.clone();
    }
    let mut parts = action
        .target
        .split_whitespace()
        .map(|part| part.to_string())
        .collect::<Vec<_>>();
    if parts.first().map(|part| part == "forge").unwrap_or(false) {
        parts.remove(0);
    }
    parts
}

fn addon_action_operation_plan(
    view: &AddonViewEntry,
    readiness: &InteractiveAddonActionReadiness,
    commands: &[String],
) -> InteractiveCommandPaletteActionPlan {
    if readiness.enabled {
        return ready_command_palette_action_plan(commands);
    }

    command_palette_action_plan(
        "blocked",
        "inspect_addon_permission_gate",
        true,
        &readiness.blocked_reason,
        vec![vec![
            "addons".to_string(),
            "views".to_string(),
            "--addon".to_string(),
            view.addon_id.clone(),
            "--surface".to_string(),
            view.view.surface.clone(),
            "--output".to_string(),
            "json".to_string(),
        ]],
    )
}

fn workflow_command_palette_entries(
    workflows: &[WorkflowRegistryRow],
) -> Vec<InteractiveCommandPaletteEntry> {
    let mut entries = Vec::new();
    for workflow in workflows.iter().take(24) {
        let goal = truncate_display(&workflow.current_goal, 80);
        let mut inspect_entry = command_palette_entry(
            &format!("workflow.inspect.{}", workflow.workflow_id),
            "workflow",
            &format!("Inspect {}", workflow.workflow_id),
            &format!("Inspect workflow state before patch, handoff or validation work: {goal}"),
            "task_board_panel",
            Some(workflow.workflow_id.clone()),
            &["inspect", &workflow.workflow_id, "--output", "json"],
            false,
            false,
            "low",
            &[
                "workflow",
                "inspect",
                "task-board",
                "patch",
                "handoff",
                "validation",
            ],
        );
        enrich_workflow_palette_entry(&mut inspect_entry, workflow, &goal);
        entries.push(inspect_entry);

        if entries.len() < 72 {
            entries.extend(workflow_mutation_command_palette_entries(workflow, &goal));
        }
    }
    entries
}

fn workflow_mutation_command_palette_entries(
    workflow: &WorkflowRegistryRow,
    goal: &str,
) -> Vec<InteractiveCommandPaletteEntry> {
    let task_id = workflow
        .context_action_refs
        .first()
        .map(|action| action.task_id.clone())
        .unwrap_or_else(|| "<task-id>".to_string());
    let workflow_id = workflow.workflow_id.clone();
    let mut entries = vec![
        command_palette_entry_from_commands(
            &format!("workflow.update_goal.{workflow_id}"),
            "workflow",
            &format!("Update goal {workflow_id}"),
            &format!("Preview a governed workflow goal mutation without stopping the run: {goal}"),
            "workflow_mutation_panel",
            Some(workflow_id.clone()),
            vec![
                "workflow".to_string(),
                "update-goal".to_string(),
                "--workflow".to_string(),
                workflow_id.clone(),
                "--goal".to_string(),
                "<new-goal>".to_string(),
                "--origin".to_string(),
                "forge_cli".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            true,
            true,
            "medium",
            &["workflow", "mutation", "goal", "replan", "update-goal"],
        ),
        command_palette_entry_from_commands(
            &format!("workflow.update_node_brain.{workflow_id}"),
            "workflow",
            &format!("Update node brain {workflow_id}"),
            "Preview node-level brain routing mutation while preserving workflow lineage.",
            "workflow_mutation_panel",
            Some(workflow_id.clone()),
            vec![
                "workflow".to_string(),
                "update-node-brain".to_string(),
                "--workflow".to_string(),
                workflow_id.clone(),
                "--task".to_string(),
                task_id,
                "--default-brain".to_string(),
                "<brain>".to_string(),
                "--origin".to_string(),
                "forge_cli".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            true,
            true,
            "medium",
            &[
                "workflow",
                "mutation",
                "node",
                "brain",
                "routing",
                "update-node-brain",
            ],
        ),
        command_palette_entry_from_commands(
            &format!("workflow.attach_artifact.{workflow_id}"),
            "workflow",
            &format!("Attach artifact {workflow_id}"),
            "Preview attaching an artifact to the workflow audit trail.",
            "workflow_mutation_panel",
            Some(workflow_id.clone()),
            vec![
                "workflow".to_string(),
                "attach-artifact".to_string(),
                "--workflow".to_string(),
                workflow_id,
                "--path".to_string(),
                "<artifact-path>".to_string(),
                "--kind".to_string(),
                "<kind>".to_string(),
                "--origin".to_string(),
                "forge_cli".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            true,
            true,
            "medium",
            &["workflow", "mutation", "artifact", "attach-artifact"],
        ),
    ];
    for entry in &mut entries {
        enrich_workflow_palette_entry(entry, workflow, goal);
    }
    entries
}

fn enrich_workflow_palette_entry(
    entry: &mut InteractiveCommandPaletteEntry,
    workflow: &WorkflowRegistryRow,
    goal: &str,
) {
    entry.keywords.push(workflow.workflow_id.clone());
    entry.keywords.push(workflow.lifecycle_state.clone());
    entry.keywords.push(goal.to_string());
}

#[allow(clippy::too_many_arguments)]
fn command_palette_entry(
    action_id: &str,
    group_id: &str,
    title: &str,
    description: &str,
    source_panel: &str,
    workflow_id: Option<String>,
    commands: &[&str],
    mutates_workflow: bool,
    requires_approval: bool,
    risk_level: &str,
    keywords: &[&str],
) -> InteractiveCommandPaletteEntry {
    let commands = commands
        .iter()
        .map(|command| (*command).to_string())
        .collect::<Vec<_>>();
    command_palette_entry_from_commands(
        action_id,
        group_id,
        title,
        description,
        source_panel,
        workflow_id,
        commands,
        mutates_workflow,
        requires_approval,
        risk_level,
        keywords,
    )
}

#[allow(clippy::too_many_arguments)]
fn command_palette_entry_from_commands(
    action_id: &str,
    group_id: &str,
    title: &str,
    description: &str,
    source_panel: &str,
    workflow_id: Option<String>,
    commands: Vec<String>,
    mutates_workflow: bool,
    requires_approval: bool,
    risk_level: &str,
    keywords: &[&str],
) -> InteractiveCommandPaletteEntry {
    let operation_plan = ready_command_palette_action_plan(&commands);
    InteractiveCommandPaletteEntry {
        action_id: action_id.to_string(),
        group_id: group_id.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        source_panel: source_panel.to_string(),
        enabled: true,
        blocked_reason: "ready".to_string(),
        operation_plan,
        addon_contract: None,
        addon_view_id: None,
        addon_view_action_id: None,
        workflow_id,
        commands,
        mutates_workflow,
        requires_approval,
        risk_level: risk_level.to_string(),
        keywords: keywords
            .iter()
            .map(|keyword| (*keyword).to_string())
            .collect(),
    }
}

fn ready_command_palette_action_plan(commands: &[String]) -> InteractiveCommandPaletteActionPlan {
    let next_commands = if commands.is_empty() {
        Vec::new()
    } else {
        vec![commands.to_vec()]
    };
    command_palette_action_plan("ready", "execute_command", false, "ready", next_commands)
}

fn command_palette_action_plan(
    status: &str,
    recommended_action: &str,
    diagnostic_only: bool,
    blocked_reason: &str,
    next_commands: Vec<Vec<String>>,
) -> InteractiveCommandPaletteActionPlan {
    InteractiveCommandPaletteActionPlan {
        schema_version: INTERACTIVE_COMMAND_PALETTE_ACTION_PLAN_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        recommended_action: recommended_action.to_string(),
        diagnostic_only,
        blocked_reason: blocked_reason.to_string(),
        next_commands,
    }
}

fn command_palette_entry_matches(entry: &InteractiveCommandPaletteEntry, query: &str) -> bool {
    let terms = query
        .split_whitespace()
        .map(|term| term.trim().to_ascii_lowercase())
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>();
    if terms.is_empty() || entry.group_id == "workflow" {
        return true;
    }

    let haystack = format!(
        "{} {} {} {} {} {}",
        entry.action_id,
        entry.group_id,
        entry.title,
        entry.description,
        entry.source_panel,
        entry
            .commands
            .iter()
            .chain(entry.keywords.iter())
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
    )
    .to_ascii_lowercase();
    terms.iter().all(|term| haystack.contains(term))
}

fn command_palette_groups(
    entries: &[InteractiveCommandPaletteEntry],
) -> Vec<InteractiveCommandPaletteGroup> {
    let mut grouped: BTreeMap<String, Vec<InteractiveCommandPaletteEntry>> = BTreeMap::new();
    for entry in entries {
        grouped
            .entry(entry.group_id.clone())
            .or_default()
            .push(entry.clone());
    }

    let preferred_order = [
        "navigation",
        "workflow",
        "operations",
        "patch",
        "identity",
        "permissions",
        "sessions",
        "harness",
        "readiness",
        "observability",
    ];
    let mut ordered_groups = Vec::new();
    for group_id in preferred_order {
        if let Some(entries) = grouped.remove(group_id) {
            ordered_groups.push(command_palette_group(group_id, entries));
        }
    }
    for (group_id, entries) in grouped {
        ordered_groups.push(command_palette_group(&group_id, entries));
    }
    ordered_groups
}

fn command_palette_group(
    group_id: &str,
    entries: Vec<InteractiveCommandPaletteEntry>,
) -> InteractiveCommandPaletteGroup {
    InteractiveCommandPaletteGroup {
        group_id: group_id.to_string(),
        title: command_palette_group_title(group_id).to_string(),
        entry_count: entries.len(),
        entries,
    }
}

fn command_palette_group_title(group_id: &str) -> &'static str {
    match group_id {
        "navigation" => "Navigation",
        "workflow" => "Workflows",
        "operations" => "Operations",
        "patch" => "Patch Workbench",
        "identity" => "Identity",
        "permissions" => "Permissions",
        "sessions" => "Sessions",
        "harness" => "Harness",
        "readiness" => "Readiness",
        "observability" => "Observability",
        _ => "Other",
    }
}

pub fn build_interactive_autocomplete(
    store: &ForgeStore,
    input: &str,
) -> Result<InteractiveAutocompletePanel> {
    let input = input.to_string();
    let normalized_query = normalize_autocomplete_query(&input);
    let palette_query = autocomplete_palette_query(&normalized_query);
    let palette = build_interactive_command_palette(store, Some(&palette_query))?;
    let mut suggestions = slash_autocomplete_suggestions(&normalized_query);
    suggestions.extend(action_invocation_autocomplete_suggestions(
        store,
        &input,
        &normalized_query,
    )?);
    suggestions.extend(command_palette_autocomplete_suggestions(
        &palette.entries,
        &normalized_query,
    ));
    suggestions.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.kind.cmp(&right.kind))
            .then_with(|| left.label.cmp(&right.label))
    });
    suggestions.truncate(20);

    Ok(InteractiveAutocompletePanel {
        schema_version: INTERACTIVE_AUTOCOMPLETE_SCHEMA_VERSION.to_string(),
        status: "autocomplete_ready".to_string(),
        input,
        normalized_query,
        suggestion_count: suggestions.len(),
        suggestions,
        navigation: vec![
            navigation_key(
                "tab",
                "accept_suggestion",
                "autocomplete",
                "Accept suggestion",
            ),
            navigation_key(
                "shift-tab",
                "previous_suggestion",
                "autocomplete",
                "Move to previous suggestion",
            ),
            navigation_key(
                "esc",
                "close_autocomplete",
                "autocomplete",
                "Close suggestions",
            ),
        ],
    })
}

fn normalize_autocomplete_query(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn autocomplete_palette_query(normalized_query: &str) -> String {
    normalized_query
        .trim_start_matches('/')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn slash_autocomplete_suggestions(query: &str) -> Vec<InteractiveAutocompleteSuggestion> {
    slash_commands()
        .into_iter()
        .filter_map(|command| {
            let score = autocomplete_score(
                &command.name,
                &command.description,
                &command.equivalent_command,
                query,
            )?;
            Some(InteractiveAutocompleteSuggestion {
                suggestion_id: format!("slash:{}", command.name.replace('/', "").replace(' ', "-")),
                kind: "slash_command".to_string(),
                label: command.name.clone(),
                insert_text: command.name.clone(),
                description: command.description,
                source: "slash_command_catalog".to_string(),
                source_panel: "slash_command_catalog".to_string(),
                enabled: true,
                blocked_reason: "ready".to_string(),
                operation_plan: ready_command_palette_action_plan(&command.equivalent_command),
                addon_contract: None,
                addon_view_id: None,
                addon_view_action_id: None,
                workflow_id: None,
                equivalent_command: command.equivalent_command,
                mutates_workflow: command.mutates_workflow,
                requires_approval: command.risk_level == "high",
                risk_level: command.risk_level,
                score,
            })
        })
        .collect()
}

fn action_invocation_autocomplete_suggestions(
    store: &ForgeStore,
    input: &str,
    normalized_query: &str,
) -> Result<Vec<InteractiveAutocompleteSuggestion>> {
    let action_context = input.trim_start().starts_with("/action ")
        || normalized_query.trim_start().starts_with("/action ");
    if !action_context {
        return Ok(Vec::new());
    }

    let action_query = normalized_query
        .trim_start()
        .strip_prefix("/action")
        .unwrap_or("")
        .trim()
        .strip_prefix("--action")
        .map(str::trim)
        .unwrap_or_else(|| {
            normalized_query
                .trim_start()
                .strip_prefix("/action")
                .unwrap_or("")
                .trim()
        });
    let registry = build_interactive_action_registry(
        store,
        (!action_query.is_empty()).then_some(action_query),
    )?;

    Ok(registry
        .actions
        .iter()
        .filter(|entry| action_id_autocomplete_entry_matches(entry, action_query))
        .filter_map(|entry| {
            let score = action_id_autocomplete_score(entry, action_query)?;
            Some(InteractiveAutocompleteSuggestion {
                suggestion_id: format!("action:{}", entry.action_id),
                kind: "action_id".to_string(),
                label: entry.action_id.clone(),
                insert_text: format!("/action {}", entry.action_id),
                description: format!("Plan invocation for {}", entry.description),
                source: "action_registry".to_string(),
                source_panel: entry.source_panel.clone(),
                enabled: entry.enabled,
                blocked_reason: entry.blocked_reason.clone(),
                operation_plan: entry.operation_plan.clone(),
                addon_contract: entry.addon_contract.clone(),
                addon_view_id: entry.addon_view_id.clone(),
                addon_view_action_id: entry.addon_view_action_id.clone(),
                workflow_id: entry.workflow_id.clone(),
                equivalent_command: vec![
                    "interactive".to_string(),
                    "action-invocation".to_string(),
                    "--action".to_string(),
                    entry.action_id.clone(),
                    "--output".to_string(),
                    "json".to_string(),
                ],
                mutates_workflow: entry.mutates_workflow,
                requires_approval: entry.requires_approval,
                risk_level: entry.risk_level.clone(),
                score,
            })
        })
        .collect())
}

fn action_id_autocomplete_score(
    entry: &InteractiveCommandPaletteEntry,
    query: &str,
) -> Option<i64> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return Some(30);
    }

    let action_id = entry.action_id.to_ascii_lowercase();
    if action_id == query {
        return Some(140);
    }
    if action_id.starts_with(&query) {
        return Some(120 - action_id.len().saturating_sub(query.len()) as i64);
    }
    if action_id.contains(&query) {
        return Some(100);
    }

    let haystack = entry.commands.join(" ").to_ascii_lowercase();
    let terms = query
        .split_whitespace()
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>();
    if !terms.is_empty() && terms.iter().all(|term| haystack.contains(term)) {
        return Some(80 - terms.len() as i64);
    }
    None
}

fn action_id_autocomplete_entry_matches(
    entry: &InteractiveCommandPaletteEntry,
    query: &str,
) -> bool {
    let terms = query
        .split_whitespace()
        .map(|term| term.trim().to_ascii_lowercase())
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>();
    if terms.is_empty() {
        return true;
    }

    let haystack = format!("{} {}", entry.action_id, entry.commands.join(" ")).to_ascii_lowercase();
    terms.iter().all(|term| haystack.contains(term))
}

fn command_palette_autocomplete_suggestions(
    entries: &[InteractiveCommandPaletteEntry],
    query: &str,
) -> Vec<InteractiveAutocompleteSuggestion> {
    let palette_query = autocomplete_palette_query(query);
    entries
        .iter()
        .filter_map(|entry| {
            let score = autocomplete_score(
                &entry.action_id,
                &entry.description,
                &entry.commands,
                &palette_query,
            )?;
            Some(InteractiveAutocompleteSuggestion {
                suggestion_id: format!("palette:{}", entry.action_id),
                kind: "command_palette_action".to_string(),
                label: entry.action_id.clone(),
                insert_text: if entry.enabled {
                    entry.commands.join(" ")
                } else {
                    String::new()
                },
                description: entry.description.clone(),
                source: "command_palette_panel".to_string(),
                source_panel: entry.source_panel.clone(),
                enabled: entry.enabled,
                blocked_reason: entry.blocked_reason.clone(),
                operation_plan: entry.operation_plan.clone(),
                addon_contract: entry.addon_contract.clone(),
                addon_view_id: entry.addon_view_id.clone(),
                addon_view_action_id: entry.addon_view_action_id.clone(),
                workflow_id: entry.workflow_id.clone(),
                equivalent_command: entry.commands.clone(),
                mutates_workflow: entry.mutates_workflow,
                requires_approval: entry.requires_approval,
                risk_level: entry.risk_level.clone(),
                score,
            })
        })
        .collect()
}

fn autocomplete_score(
    label: &str,
    description: &str,
    equivalent_command: &[String],
    query: &str,
) -> Option<i64> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return Some(10);
    }
    let label_lower = label.to_ascii_lowercase();
    let haystack = format!(
        "{} {} {}",
        label_lower,
        description.to_ascii_lowercase(),
        equivalent_command.join(" ").to_ascii_lowercase()
    );
    if label_lower == query {
        return Some(120);
    }
    if label_lower.starts_with(&query) {
        return Some(100 - label_lower.len().saturating_sub(query.len()) as i64);
    }
    if label_lower.contains(&query) {
        return Some(80);
    }
    let terms = query
        .split_whitespace()
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>();
    if !terms.is_empty() && terms.iter().all(|term| haystack.contains(term)) {
        return Some(60 - terms.len() as i64);
    }
    None
}

pub fn build_interactive_patch_workbench(
    store: &ForgeStore,
) -> Result<InteractivePatchWorkbenchPanel> {
    let current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repository_path = git_command(&["rev-parse", "--show-toplevel"])
        .stdout
        .lines()
        .next()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
        .unwrap_or_else(|| current_dir.display().to_string());

    let status_output = git_command(&["status", "--porcelain=v1"]);
    if !status_output.success {
        let commands = patch_workbench_commands();
        let edit_intake = build_patch_edit_intake(&[], &commands);
        let approval_flow = build_patch_approval_flow(true, false, "not_run", &commands);
        let operation_plan = build_patch_operation_plan(&edit_intake, &approval_flow);
        return Ok(InteractivePatchWorkbenchPanel {
            schema_version: INTERACTIVE_PATCH_WORKBENCH_SCHEMA_VERSION.to_string(),
            addon_contract: patch_addon_contract(),
            status: "patch_workbench_unavailable".to_string(),
            repository_path,
            clean: true,
            changed_path_count: 0,
            staged_path_count: 0,
            unstaged_path_count: 0,
            untracked_path_count: 0,
            diff_present: false,
            diff_check_status: "not_run".to_string(),
            diff_stat: status_output.stderr,
            diff_preview: build_patch_diff_preview(&[], false),
            diff_review_queue: build_patch_diff_review_queue(&[], false),
            edit_intake,
            operation_plan,
            files: Vec::new(),
            approval_flow,
            commands,
        });
    }

    let ignored_paths = patch_workbench_ignored_paths(store, &repository_path);
    let files = parse_patch_workbench_files(&status_output.stdout, &ignored_paths);
    let staged_path_count = files.iter().filter(|file| file.staged).count();
    let unstaged_path_count = files.iter().filter(|file| file.unstaged).count();
    let untracked_path_count = files.iter().filter(|file| file.untracked).count();
    let diff_stat = combined_diff_stat();
    let diff_present = !diff_stat.trim().is_empty();
    let diff_check_status = patch_workbench_diff_check_status();
    let clean = files.is_empty();
    let status = if clean {
        "patch_workbench_clean"
    } else {
        "patch_workbench_ready"
    };
    let commands = patch_workbench_commands();
    let approval_flow =
        build_patch_approval_flow(clean, diff_present, &diff_check_status, &commands);
    let diff_preview = build_patch_diff_preview(&files, diff_present);
    let diff_review_queue = build_patch_diff_review_queue(&files, diff_present);
    let edit_intake = build_patch_edit_intake(&files, &commands);
    let operation_plan = build_patch_operation_plan(&edit_intake, &approval_flow);

    Ok(InteractivePatchWorkbenchPanel {
        schema_version: INTERACTIVE_PATCH_WORKBENCH_SCHEMA_VERSION.to_string(),
        addon_contract: patch_addon_contract(),
        status: status.to_string(),
        repository_path,
        clean,
        changed_path_count: files.len(),
        staged_path_count,
        unstaged_path_count,
        untracked_path_count,
        diff_present,
        diff_check_status,
        diff_stat,
        diff_preview,
        diff_review_queue,
        edit_intake,
        operation_plan,
        files,
        approval_flow,
        commands,
    })
}

fn patch_addon_contract() -> InteractivePatchAddonContract {
    InteractivePatchAddonContract {
        schema_version: INTERACTIVE_PATCH_ADDON_CONTRACT_SCHEMA_VERSION.to_string(),
        source_addon: "forge.addon.software_development".to_string(),
        capability_id: CAP_SOURCE_CODE_PATCH_LIFECYCLE.to_string(),
        permission_id: "source_code.patch".to_string(),
        view_id: "software.patch_workbench".to_string(),
        runtime_contract_id: "source_code_patch_lifecycle.executor".to_string(),
        runtime: "forge_core_builtin".to_string(),
        entrypoint: "forge.patch.lifecycle".to_string(),
    }
}

pub fn build_interactive_permissions(store: &ForgeStore) -> Result<InteractivePermissionsPanel> {
    let memberships_report = list_identity_memberships(store, None, None, None, None, None, None)?;
    let addon_permissions_report = list_addon_permission_authorizations(store, None, None, None)?;
    let human_interactions_report = list_human_interactions(store)?;

    let memberships = memberships_report
        .memberships
        .into_iter()
        .map(|membership| {
            let tenant_path = format!(
                "{}/{}/{}",
                membership.organization_id, membership.brand_id, membership.product_id
            );
            InteractivePermissionMembership {
                commands: permission_membership_commands(&membership),
                permission_count: membership.permissions.len(),
                subject_scope: membership.subject_scope,
                subject_id: membership.subject_id,
                tenant_path,
                organization_id: membership.organization_id,
                brand_id: membership.brand_id,
                product_id: membership.product_id,
                role: membership.role,
                status: membership.status,
                permissions: membership.permissions,
                permission_grants: membership.permission_grants,
                permission_denies: membership.permission_denies,
                expired: membership.expired,
                not_yet_valid: membership.not_yet_valid,
            }
        })
        .collect::<Vec<_>>();
    let active_membership_count = memberships
        .iter()
        .filter(|membership| {
            membership.status == "active" && !membership.expired && !membership.not_yet_valid
        })
        .count();
    let expired_membership_count = memberships
        .iter()
        .filter(|membership| membership.expired)
        .count();
    let not_yet_valid_membership_count = memberships
        .iter()
        .filter(|membership| membership.not_yet_valid)
        .count();

    let addon_permissions = addon_permissions_report
        .authorizations
        .into_iter()
        .map(|authorization| InteractiveAddonPermissionAuthorization {
            commands: addon_permission_commands(
                &authorization.addon_id,
                &authorization.permission_id,
            ),
            addon_id: authorization.addon_id,
            permission_id: authorization.permission_id,
            status: authorization.status,
            risk: authorization.risk,
            approved_by: authorization.approved_by,
            source: authorization.source,
            granted_at: authorization.granted_at,
        })
        .collect::<Vec<_>>();
    let approved_addon_permission_count = addon_permissions
        .iter()
        .filter(|authorization| authorization.status == "approved")
        .count();
    let revoked_addon_permission_count = addon_permissions
        .iter()
        .filter(|authorization| authorization.status == "revoked")
        .count();

    let approval_items = human_interactions_report
        .interactions
        .into_iter()
        .filter(|item| matches!(item.interaction.state.as_str(), "pending" | "timed_out"))
        .map(|item| InteractiveApprovalItem {
            commands: approval_item_commands(&item.workflow_id, &item.task_id),
            source: "human_interaction".to_string(),
            workflow_id: item.workflow_id,
            task_id: item.task_id,
            task_title: item.task_title,
            interaction_id: item.interaction.interaction_id,
            kind: item.interaction.kind,
            state: item.interaction.state,
            prompt: item.interaction.prompt,
            required: item.interaction.required,
        })
        .collect::<Vec<_>>();
    let pending_human_approval_count = approval_items
        .iter()
        .filter(|item| item.state == "pending")
        .count();
    let timed_out_human_approval_count = approval_items
        .iter()
        .filter(|item| item.state == "timed_out")
        .count();

    Ok(InteractivePermissionsPanel {
        schema_version: INTERACTIVE_PERMISSIONS_SCHEMA_VERSION.to_string(),
        status: "interactive_permissions_ready".to_string(),
        membership_count: memberships.len(),
        active_membership_count,
        expired_membership_count,
        not_yet_valid_membership_count,
        addon_authorization_count: addon_permissions.len(),
        approved_addon_permission_count,
        revoked_addon_permission_count,
        pending_human_approval_count,
        timed_out_human_approval_count,
        memberships,
        addon_permissions,
        approval_items,
        commands: permissions_commands(),
    })
}

pub fn build_interactive_identity(
    store: &ForgeStore,
    project_root: &Path,
) -> Result<InteractiveIdentityPanel> {
    let context = inspect_project_operating_context(project_root)?;
    let registry_report = list_identity_registry(store, None, None)?;
    let links_report = list_identity_links(store, None, None, Some("active"))?;
    let link_count = links_report.link_count;
    let memberships_report = list_identity_memberships(store, None, None, None, None, None, None)?;
    let tenant_audit = audit_tenant_index(store)?;

    let channel_aliases = links_report
        .links
        .into_iter()
        .filter(is_channel_alias_link)
        .map(|link| {
            let (alias_scope, alias_id, subject_scope, subject_id) = channel_alias_parts(&link);
            InteractiveIdentityChannelAlias {
                alias_path: format!("{alias_scope}:{alias_id} -> {subject_scope}:{subject_id}"),
                commands: identity_alias_commands(&link, &alias_scope, &alias_id),
                left_scope: link.left_scope,
                left_id: link.left_id,
                right_scope: link.right_scope,
                right_id: link.right_id,
                link_type: link.link_type,
                status: link.status,
                source: link.source,
            }
        })
        .collect::<Vec<_>>();

    let memberships = memberships_report
        .memberships
        .into_iter()
        .map(|membership| InteractiveIdentityMembership {
            tenant_path: format!(
                "{}/{}/{}",
                membership.organization_id, membership.brand_id, membership.product_id
            ),
            commands: identity_membership_commands(&membership),
            permission_count: membership.permissions.len(),
            subject_scope: membership.subject_scope,
            subject_id: membership.subject_id,
            organization_id: membership.organization_id,
            brand_id: membership.brand_id,
            product_id: membership.product_id,
            role: membership.role,
            status: membership.status,
            expired: membership.expired,
            not_yet_valid: membership.not_yet_valid,
        })
        .collect::<Vec<_>>();
    let active_membership_count = memberships
        .iter()
        .filter(|membership| {
            membership.status == "active" && !membership.expired && !membership.not_yet_valid
        })
        .count();

    Ok(InteractiveIdentityPanel {
        schema_version: INTERACTIVE_IDENTITY_SCHEMA_VERSION.to_string(),
        status: "interactive_identity_ready".to_string(),
        project_root: context.project_root.clone(),
        context_status: context.status.clone(),
        identity_count: registry_report.identity_count,
        link_count,
        channel_alias_count: channel_aliases.len(),
        membership_count: memberships.len(),
        active_membership_count,
        tenant_audit_missing_count: tenant_audit.missing_count,
        current_context: InteractiveIdentityCurrentContext {
            organization_id: context.context.organization.id.clone(),
            organization_label: context.context.organization.label.clone(),
            brand_id: context.context.brand.id.clone(),
            brand_label: context.context.brand.label.clone(),
            product_id: context.context.product.id.clone(),
            product_label: context.context.product.label.clone(),
            user_id: context.context.user.id.clone(),
            user_label: context.context.user.label.clone(),
            channel_id: context.context.channel.id.clone(),
            channel_label: context.context.channel.label.clone(),
            memory_scope: context.context.memory_scope.clone(),
            personality_scope: context.context.personality_scope.clone(),
            tenant_policy_mode: context.context.tenant_policy_mode.clone(),
            source: context.source.clone(),
            warning_count: context.warnings.len(),
            warnings: context.warnings,
        },
        identities: registry_report.identities,
        channel_aliases,
        memberships,
        tenant_audit,
        commands: identity_commands(Path::new(&context.project_root)),
    })
}

struct GitCommandOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

fn git_command(args: &[&str]) -> GitCommandOutput {
    match Command::new("git").args(args).output() {
        Ok(output) => GitCommandOutput {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout)
                .trim_end()
                .to_string(),
            stderr: String::from_utf8_lossy(&output.stderr)
                .trim_end()
                .to_string(),
        },
        Err(error) => GitCommandOutput {
            success: false,
            stdout: String::new(),
            stderr: error.to_string(),
        },
    }
}

fn git_command_with_paths(args: &[&str], paths: &[String]) -> GitCommandOutput {
    let mut command = Command::new("git");
    command.args(args);
    command.arg("--");
    command.args(paths);
    match command.output() {
        Ok(output) => GitCommandOutput {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout)
                .trim_end()
                .to_string(),
            stderr: String::from_utf8_lossy(&output.stderr)
                .trim_end()
                .to_string(),
        },
        Err(error) => GitCommandOutput {
            success: false,
            stdout: String::new(),
            stderr: error.to_string(),
        },
    }
}

fn patch_workbench_ignored_paths(store: &ForgeStore, repository_path: &str) -> BTreeSet<String> {
    let root = PathBuf::from(repository_path);
    let mut ignored = BTreeSet::new();
    let store_path = store.path().to_path_buf();
    for path in [
        store_path.clone(),
        PathBuf::from(format!("{}-wal", store_path.display())),
        PathBuf::from(format!("{}-shm", store_path.display())),
        PathBuf::from(format!("{}-journal", store_path.display())),
    ] {
        if let Some(relative) = repo_relative_display_path(&path, &root) {
            ignored.insert(relative);
        }
    }
    ignored
}

fn repo_relative_display_path(path: &Path, root: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .filter(|relative| !relative.is_empty())
}

fn parse_patch_workbench_files(
    status: &str,
    ignored_paths: &BTreeSet<String>,
) -> Vec<InteractivePatchWorkbenchFile> {
    status
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| {
            let index_status = line.chars().next().unwrap_or(' ');
            let worktree_status = line.chars().nth(1).unwrap_or(' ');
            let path = line.get(3..).unwrap_or("").trim().to_string();
            if ignored_paths.contains(&path) {
                return None;
            }
            let untracked = index_status == '?' && worktree_status == '?';
            let staged = index_status != ' ' && index_status != '?' && index_status != '!';
            let unstaged =
                worktree_status != ' ' && worktree_status != '?' && worktree_status != '!';
            let status_label = patch_workbench_status_label(staged, unstaged, untracked);
            let commands = patch_workbench_file_commands(&path);
            let action_hint = patch_file_action_hint(staged, unstaged, untracked, &commands);

            Some(InteractivePatchWorkbenchFile {
                commands,
                path,
                index_status: index_status.to_string(),
                worktree_status: worktree_status.to_string(),
                status_label,
                action_hint,
                staged,
                unstaged,
                untracked,
            })
        })
        .collect()
}

fn patch_workbench_status_label(staged: bool, unstaged: bool, untracked: bool) -> String {
    if untracked {
        "untracked".to_string()
    } else if staged && unstaged {
        "staged_and_modified".to_string()
    } else if staged {
        "staged".to_string()
    } else if unstaged {
        "modified".to_string()
    } else {
        "changed".to_string()
    }
}

fn patch_file_action_hint(
    staged: bool,
    unstaged: bool,
    untracked: bool,
    commands: &InteractivePatchWorkbenchFileCommands,
) -> InteractivePatchFileActionHint {
    let (
        suggested_next_action,
        review_required,
        apply_blocked_until_review,
        primary_command,
        blocked_reason,
        rationale,
    ) = if untracked {
        (
            "create_patch_plan",
            false,
            true,
            commands.plan.clone(),
            "untracked_file_needs_patch_plan",
            "Untracked files are not part of the tracked diff review queue; create a patch plan with explicit workflow/task lineage before applying.",
        )
    } else if staged || unstaged {
        (
            "review_diff",
            true,
            true,
            commands.review.clone(),
            "diff_review_required",
            "Tracked changes must produce review evidence before apply approval can be offered.",
        )
    } else {
        (
            "inspect_diff",
            false,
            false,
            commands.diff.clone(),
            "ready_for_inspection",
            "No blocking file-specific action was detected; inspect the diff if more evidence is needed.",
        )
    };

    InteractivePatchFileActionHint {
        schema_version: INTERACTIVE_PATCH_FILE_ACTION_HINT_SCHEMA_VERSION.to_string(),
        suggested_next_action: suggested_next_action.to_string(),
        review_required,
        apply_blocked_until_review,
        primary_command,
        blocked_reason: blocked_reason.to_string(),
        rationale: rationale.to_string(),
    }
}

fn combined_diff_stat() -> String {
    let unstaged = git_command(&["diff", "--stat"]);
    let staged = git_command(&["diff", "--cached", "--stat"]);
    let mut stat_parts = Vec::new();
    if !unstaged.stdout.trim().is_empty() {
        stat_parts.push(unstaged.stdout);
    }
    if !staged.stdout.trim().is_empty() {
        stat_parts.push(format!("cached:\n{}", staged.stdout));
    }
    stat_parts.join("\n")
}

fn patch_workbench_diff_check_status() -> String {
    let unstaged = git_command(&["diff", "--check"]);
    let staged = git_command(&["diff", "--cached", "--check"]);
    if unstaged.success && staged.success {
        "passed".to_string()
    } else {
        "failed".to_string()
    }
}

fn build_patch_diff_preview(
    files: &[InteractivePatchWorkbenchFile],
    diff_present: bool,
) -> InteractivePatchDiffPreview {
    const MAX_PREVIEW_LINES: usize = 48;
    let selected_path = files
        .iter()
        .find(|file| !file.untracked && (file.unstaged || file.staged))
        .or_else(|| files.iter().find(|file| !file.untracked))
        .map(|file| file.path.clone());

    let Some(path) = selected_path else {
        return InteractivePatchDiffPreview {
            schema_version: "forge.interactive.patch_diff_preview.v1".to_string(),
            status: if diff_present {
                "diff_preview_unavailable".to_string()
            } else {
                "diff_preview_idle".to_string()
            },
            selected_path: None,
            line_count: 0,
            truncated: false,
            command: Vec::new(),
            lines: Vec::new(),
            notes: vec![
                "Inline diff preview is read-only and does not apply patches.".to_string(),
                "No tracked file diff is available for preview; untracked files should be opened through file-specific commands.".to_string(),
            ],
        };
    };

    let paths = vec![path.clone()];
    let mut command = patch_diff_preview_command(&path, false);
    let mut diff_output = git_command_with_paths(&["diff", "--unified=3"], &paths);
    if diff_output.success && diff_output.stdout.trim().is_empty() {
        command = patch_diff_preview_command(&path, true);
        diff_output = git_command_with_paths(&["diff", "--cached", "--unified=3"], &paths);
    }

    if !diff_output.success {
        return InteractivePatchDiffPreview {
            schema_version: "forge.interactive.patch_diff_preview.v1".to_string(),
            status: "diff_preview_failed".to_string(),
            selected_path: Some(path),
            line_count: 0,
            truncated: false,
            command,
            lines: Vec::new(),
            notes: vec![
                "Inline diff preview is read-only and does not apply patches.".to_string(),
                format!("git diff failed: {}", diff_output.stderr),
            ],
        };
    }

    let (lines, truncated) = parse_patch_diff_preview_lines(&diff_output.stdout, MAX_PREVIEW_LINES);
    let status = if lines.is_empty() {
        "diff_preview_empty"
    } else {
        "diff_preview_ready"
    };
    InteractivePatchDiffPreview {
        schema_version: "forge.interactive.patch_diff_preview.v1".to_string(),
        status: status.to_string(),
        selected_path: Some(path),
        line_count: lines.len(),
        truncated,
        command,
        lines,
        notes: vec![
            "Inline diff preview is read-only, bounded and intended for TUI/web/MCP rendering before review approval.".to_string(),
            "Use forge patch diff for full multi-file navigation and workflow artifact lineage.".to_string(),
        ],
    }
}

fn patch_diff_preview_command(path: &str, cached: bool) -> Vec<String> {
    let mut command = vec!["git".to_string(), "diff".to_string()];
    if cached {
        command.push("--cached".to_string());
    }
    command.extend([
        "--unified=3".to_string(),
        "--".to_string(),
        path.to_string(),
    ]);
    command
}

fn build_patch_diff_review_queue(
    files: &[InteractivePatchWorkbenchFile],
    diff_present: bool,
) -> InteractivePatchDiffReviewQueue {
    let mut queue_files = Vec::new();
    for file in files
        .iter()
        .filter(|file| !file.untracked && (file.unstaged || file.staged))
    {
        let diff = patch_workbench_file_diff(file);
        let (lines, _) = parse_patch_diff_preview_lines(&diff, usize::MAX);
        if lines.is_empty() {
            continue;
        }
        let hunk_count = lines
            .iter()
            .filter(|line| line.line_kind == "hunk_header")
            .count();
        let addition_count = lines
            .iter()
            .filter(|line| line.line_kind == "addition")
            .count();
        let deletion_count = lines
            .iter()
            .filter(|line| line.line_kind == "deletion")
            .count();
        queue_files.push(InteractivePatchDiffReviewQueueFile {
            path: file.path.clone(),
            review_status: "pending_review".to_string(),
            action_hint: file.action_hint.clone(),
            selected: queue_files.is_empty(),
            staged: file.staged,
            unstaged: file.unstaged,
            hunk_count,
            addition_count,
            deletion_count,
            line_count: lines.len(),
            commands: file.commands.clone(),
        });
    }

    let file_count = queue_files.len();
    let pending_review_count = queue_files
        .iter()
        .filter(|file| file.review_status == "pending_review")
        .count();
    let total_hunk_count = queue_files.iter().map(|file| file.hunk_count).sum();
    let total_addition_count = queue_files.iter().map(|file| file.addition_count).sum();
    let total_deletion_count = queue_files.iter().map(|file| file.deletion_count).sum();
    let selected_path = queue_files.first().map(|file| file.path.clone());
    let status = if file_count > 0 {
        "diff_review_queue_ready"
    } else if diff_present {
        "diff_review_queue_unavailable"
    } else {
        "diff_review_queue_idle"
    };

    InteractivePatchDiffReviewQueue {
        schema_version: "forge.interactive.patch_diff_review_queue.v1".to_string(),
        status: status.to_string(),
        selected_path,
        file_count,
        pending_review_count,
        total_hunk_count,
        total_addition_count,
        total_deletion_count,
        files: queue_files,
        notes: vec![
            "Review queue is read-only and only indexes tracked file diffs; untracked files remain visible in file lanes."
                .to_string(),
            "Use each file's diff command for full hunk navigation and review evidence before apply approval."
                .to_string(),
        ],
    }
}

fn build_patch_edit_intake(
    files: &[InteractivePatchWorkbenchFile],
    commands: &InteractivePatchWorkbenchCommands,
) -> InteractivePatchEditIntake {
    let path_missing = files.is_empty();
    let required_inputs = vec![
        patch_edit_input(
            "workflow_id",
            "Workflow",
            "workflow_id",
            true,
            "operator_or_workflow_focus",
            "wf_01",
            "--workflow",
        ),
        patch_edit_input(
            "task_id",
            "Task",
            "task_id",
            true,
            "operator_or_task_focus",
            "task-001",
            "--task",
        ),
        patch_edit_input(
            "intent",
            "Edit intent",
            "text",
            true,
            "operator_input",
            "Describe the bounded edit before creating a patch plan.",
            "--intent",
        ),
        patch_edit_input(
            "path",
            "Repository path",
            "repo_relative_path_multi_select",
            path_missing,
            "git_changed_files",
            "src/lib.rs",
            "--path",
        ),
        patch_edit_input(
            "plan_artifact",
            "Patch plan artifact",
            "artifact_id",
            true,
            "forge_patch_plan",
            "artifact_patch_plan",
            "--plan-artifact",
        ),
        patch_edit_input(
            "apply_artifact",
            "Patch apply artifact",
            "artifact_id",
            true,
            "forge_patch_apply",
            "artifact_patch_apply",
            "--apply-artifact",
        ),
        patch_edit_input(
            "revert_artifact",
            "Patch revert artifact",
            "artifact_id",
            true,
            "forge_patch_revert",
            "artifact_patch_revert",
            "--revert-artifact",
        ),
        patch_edit_input(
            "approved_by",
            "Approver",
            "operator_id",
            true,
            "human_approval",
            "arthur",
            "--approved-by",
        ),
        patch_edit_input(
            "confirm_restore",
            "Confirm restore",
            "boolean_confirmation",
            true,
            "human_approval",
            "true",
            "--confirm-restore",
        ),
    ];
    let forms = vec![
        patch_edit_form(
            "create_patch_plan",
            "Create patch plan",
            &["workflow_id", "task_id", "intent", "path"],
            false,
            commands.plan.clone(),
            &required_inputs,
            "Collect workflow lineage, task lineage, edit intent and repo paths before creating plan-only patch evidence.",
        ),
        patch_edit_form(
            "review_current_diff",
            "Review current diff",
            &["workflow_id", "task_id", "path"],
            false,
            commands.review.clone(),
            &required_inputs,
            "Persist diff/status/check evidence before any apply approval.",
        ),
        patch_edit_form(
            "inspect_patch_diff",
            "Inspect patch diff",
            &["workflow_id", "task_id", "path"],
            false,
            commands.diff.clone(),
            &required_inputs,
            "Open read-only multi-file diff navigation for the selected path set.",
        ),
        patch_edit_form(
            "apply_reviewed_patch",
            "Apply reviewed patch",
            &["workflow_id", "task_id", "path", "plan_artifact"],
            true,
            commands.apply.clone(),
            &required_inputs,
            "Apply only after review evidence, a patch plan artifact and explicit human approval are present.",
        ),
        patch_edit_form(
            "propose_patch_rollback",
            "Propose patch rollback",
            &["workflow_id", "task_id", "apply_artifact"],
            true,
            commands.revert.clone(),
            &required_inputs,
            "Record rollback intent from an apply artifact without restoring files implicitly.",
        ),
        patch_edit_form(
            "restore_approved_rollback",
            "Restore approved rollback",
            &["workflow_id", "task_id", "revert_artifact", "approved_by", "confirm_restore"],
            true,
            commands.restore.clone(),
            &required_inputs,
            "Restore repo-local files only after approved revert evidence and explicit confirmation.",
        ),
    ];
    let required_input_count = required_inputs
        .iter()
        .filter(|input| input.required)
        .count();
    let missing_required_input_count = required_inputs
        .iter()
        .filter(|input| input.required && input.missing)
        .count();
    let status = if path_missing {
        "patch_edit_intake_waiting_for_changes"
    } else {
        "patch_edit_intake_ready"
    };

    InteractivePatchEditIntake {
        schema_version: INTERACTIVE_PATCH_EDIT_INTAKE_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        default_action: "create_patch_plan".to_string(),
        required_input_count,
        missing_required_input_count,
        inferred_path_count: files.len(),
        required_inputs,
        forms,
        notes: vec![
            "This intake is read-only; it tells a TUI, web client or agent which fields must be collected before offering patch actions."
                .to_string(),
            "Commands stay permission-gated through the patch lifecycle and keep workflow/task lineage explicit."
                .to_string(),
        ],
    }
}

fn patch_edit_input(
    input_id: &str,
    label: &str,
    input_kind: &str,
    missing: bool,
    source: &str,
    example: &str,
    command_flag: &str,
) -> InteractivePatchEditInput {
    InteractivePatchEditInput {
        input_id: input_id.to_string(),
        label: label.to_string(),
        input_kind: input_kind.to_string(),
        required: true,
        missing,
        source: source.to_string(),
        example: example.to_string(),
        command_flag: command_flag.to_string(),
    }
}

fn patch_edit_form(
    action_id: &str,
    title: &str,
    required_input_ids: &[&str],
    requires_human_approval: bool,
    command_template: Vec<String>,
    inputs: &[InteractivePatchEditInput],
    rationale: &str,
) -> InteractivePatchEditForm {
    let missing_input_ids = required_input_ids
        .iter()
        .filter(|input_id| {
            inputs
                .iter()
                .any(|input| input.input_id == **input_id && input.missing)
        })
        .map(|input_id| (*input_id).to_string())
        .collect::<Vec<_>>();
    let ready = missing_input_ids.is_empty() && !requires_human_approval;

    InteractivePatchEditForm {
        action_id: action_id.to_string(),
        title: title.to_string(),
        ready,
        requires_human_approval,
        required_input_ids: required_input_ids
            .iter()
            .map(|input_id| (*input_id).to_string())
            .collect(),
        missing_input_ids,
        command_template,
        rationale: rationale.to_string(),
    }
}

fn build_patch_operation_plan(
    intake: &InteractivePatchEditIntake,
    approval_flow: &InteractivePatchApprovalFlow,
) -> InteractivePatchOperationPlan {
    let clean = approval_flow.current_gate == "no_changes";
    let blocked_by_diff_check = approval_flow.current_gate == "fix_diff_check";
    let steps = intake
        .forms
        .iter()
        .map(|form| patch_operation_step(form, clean, blocked_by_diff_check))
        .collect::<Vec<_>>();
    let ready_step_count = steps.iter().filter(|step| step.status == "ready").count();
    let blocked_step_count = steps.iter().filter(|step| step.status == "blocked").count();
    let requires_human_approval_count = steps
        .iter()
        .filter(|step| step.requires_human_approval)
        .count();
    let current_step = if clean {
        "none".to_string()
    } else {
        steps
            .iter()
            .find(|step| step.status != "complete" && step.status != "idle")
            .map(|step| step.step_id.clone())
            .unwrap_or_else(|| "none".to_string())
    };
    let status = if clean {
        "patch_operation_plan_idle"
    } else if blocked_by_diff_check || blocked_step_count > 0 {
        "patch_operation_plan_blocked"
    } else if ready_step_count > 0 {
        "patch_operation_plan_ready"
    } else {
        "patch_operation_plan_waiting_for_input"
    };

    InteractivePatchOperationPlan {
        schema_version: "forge.interactive.patch_operation_plan.v1".to_string(),
        status: status.to_string(),
        current_step,
        step_count: steps.len(),
        ready_step_count,
        blocked_step_count,
        requires_human_approval_count,
        steps,
        notes: vec![
            "Operation plan is derived from edit intake and approval flow; it does not mutate files."
                .to_string(),
            "Render these steps as the ordered patch lifecycle before enabling apply or restore actions."
                .to_string(),
        ],
    }
}

fn patch_operation_step(
    form: &InteractivePatchEditForm,
    clean: bool,
    blocked_by_diff_check: bool,
) -> InteractivePatchOperationStep {
    let status = if clean {
        "idle"
    } else if blocked_by_diff_check {
        "blocked"
    } else if form.ready {
        "ready"
    } else if !form.missing_input_ids.is_empty() {
        "waiting_for_input"
    } else if form.requires_human_approval {
        "needs_human_approval"
    } else {
        "blocked"
    };
    let depends_on = patch_operation_dependencies(&form.action_id);

    InteractivePatchOperationStep {
        step_id: form.action_id.clone(),
        title: form.title.clone(),
        status: status.to_string(),
        action_id: form.action_id.clone(),
        command: form.command_template.clone(),
        mutates_workflow: matches!(
            form.action_id.as_str(),
            "apply_reviewed_patch" | "propose_patch_rollback" | "restore_approved_rollback"
        ),
        requires_human_approval: form.requires_human_approval,
        depends_on,
        rationale: form.rationale.clone(),
    }
}

fn patch_operation_dependencies(action_id: &str) -> Vec<String> {
    match action_id {
        "create_patch_plan" => Vec::new(),
        "review_current_diff" => vec!["create_patch_plan".to_string()],
        "inspect_patch_diff" => vec!["review_current_diff".to_string()],
        "apply_reviewed_patch" => vec![
            "create_patch_plan".to_string(),
            "review_current_diff".to_string(),
            "inspect_patch_diff".to_string(),
            "human_approval_before_apply".to_string(),
        ],
        "propose_patch_rollback" => vec!["apply_reviewed_patch".to_string()],
        "restore_approved_rollback" => vec![
            "propose_patch_rollback".to_string(),
            "rollback_restore_approval".to_string(),
        ],
        _ => Vec::new(),
    }
}

fn patch_workbench_file_diff(file: &InteractivePatchWorkbenchFile) -> String {
    let paths = vec![file.path.clone()];
    let mut parts = Vec::new();
    if file.unstaged {
        let output = git_command_with_paths(&["diff", "--unified=3"], &paths);
        if output.success && !output.stdout.trim().is_empty() {
            parts.push(output.stdout);
        }
    }
    if file.staged {
        let output = git_command_with_paths(&["diff", "--cached", "--unified=3"], &paths);
        if output.success && !output.stdout.trim().is_empty() {
            parts.push(output.stdout);
        }
    }
    parts.join("\n")
}

fn parse_patch_diff_preview_lines(
    diff: &str,
    max_lines: usize,
) -> (Vec<InteractivePatchDiffPreviewLine>, bool) {
    let mut lines = Vec::new();
    let mut old_line = None;
    let mut new_line = None;
    let mut truncated = false;

    for raw_line in diff.lines() {
        if lines.len() >= max_lines {
            truncated = true;
            break;
        }

        if raw_line.starts_with("@@") {
            let (old_start, new_start) = parse_patch_diff_hunk_starts(raw_line);
            old_line = old_start;
            new_line = new_start;
            lines.push(InteractivePatchDiffPreviewLine {
                line_kind: "hunk_header".to_string(),
                old_line: None,
                new_line: None,
                text: raw_line.to_string(),
            });
        } else if raw_line.starts_with('+') && !raw_line.starts_with("+++") {
            let current_new_line = new_line;
            if let Some(value) = new_line.as_mut() {
                *value += 1;
            }
            lines.push(InteractivePatchDiffPreviewLine {
                line_kind: "addition".to_string(),
                old_line: None,
                new_line: current_new_line,
                text: raw_line.strip_prefix('+').unwrap_or(raw_line).to_string(),
            });
        } else if raw_line.starts_with('-') && !raw_line.starts_with("---") {
            let current_old_line = old_line;
            if let Some(value) = old_line.as_mut() {
                *value += 1;
            }
            lines.push(InteractivePatchDiffPreviewLine {
                line_kind: "deletion".to_string(),
                old_line: current_old_line,
                new_line: None,
                text: raw_line.strip_prefix('-').unwrap_or(raw_line).to_string(),
            });
        } else if let Some(context_text) = raw_line.strip_prefix(' ') {
            let current_old_line = old_line;
            let current_new_line = new_line;
            if let Some(value) = old_line.as_mut() {
                *value += 1;
            }
            if let Some(value) = new_line.as_mut() {
                *value += 1;
            }
            lines.push(InteractivePatchDiffPreviewLine {
                line_kind: "context".to_string(),
                old_line: current_old_line,
                new_line: current_new_line,
                text: context_text.to_string(),
            });
        } else {
            lines.push(InteractivePatchDiffPreviewLine {
                line_kind: "metadata".to_string(),
                old_line: None,
                new_line: None,
                text: raw_line.to_string(),
            });
        }
    }

    (lines, truncated)
}

fn parse_patch_diff_hunk_starts(header: &str) -> (Option<usize>, Option<usize>) {
    let mut old_line = None;
    let mut new_line = None;
    for part in header.split_whitespace() {
        if old_line.is_none() && part.starts_with('-') {
            old_line = parse_patch_diff_hunk_start(part, '-');
        } else if new_line.is_none() && part.starts_with('+') {
            new_line = parse_patch_diff_hunk_start(part, '+');
        }
    }
    (old_line, new_line)
}

fn parse_patch_diff_hunk_start(part: &str, marker: char) -> Option<usize> {
    part.strip_prefix(marker)?
        .split(',')
        .next()?
        .parse::<usize>()
        .ok()
}

fn build_patch_approval_flow(
    clean: bool,
    diff_present: bool,
    diff_check_status: &str,
    commands: &InteractivePatchWorkbenchCommands,
) -> InteractivePatchApprovalFlow {
    let status = if clean {
        "patch_approval_idle"
    } else if diff_check_status == "failed" {
        "patch_approval_blocked_by_diff_check"
    } else {
        "patch_approval_required"
    };
    let current_gate = if clean {
        "no_changes"
    } else if diff_check_status == "failed" {
        "fix_diff_check"
    } else if diff_present {
        "review_changed_files"
    } else {
        "classify_untracked_or_metadata_changes"
    };
    let review_status = if clean {
        "idle"
    } else if diff_check_status == "failed" {
        "blocked"
    } else {
        "required"
    };
    let approval_status = if clean {
        "idle"
    } else {
        "blocked_until_review"
    };
    let rollback_status = if clean {
        "idle"
    } else {
        "available_after_apply"
    };

    InteractivePatchApprovalFlow {
        schema_version: "forge.interactive.patch_approval_flow.v1".to_string(),
        status: status.to_string(),
        current_gate: current_gate.to_string(),
        requires_human_approval: !clean,
        apply_ready: false,
        gates: vec![
            InteractivePatchApprovalGate {
                gate_id: "diff_navigation".to_string(),
                title: "Navigate changed files and hunks".to_string(),
                status: if diff_present { "available" } else { "idle" }.to_string(),
                command: commands.diff.clone(),
                mutates_workflow: false,
                requires_human_approval: false,
                rationale: "Operators need a bounded multi-file diff view before review."
                    .to_string(),
            },
            InteractivePatchApprovalGate {
                gate_id: "diff_review_before_apply".to_string(),
                title: "Record patch review evidence".to_string(),
                status: review_status.to_string(),
                command: commands.review.clone(),
                mutates_workflow: false,
                requires_human_approval: false,
                rationale: "Patch review persists diff/status/check evidence before approval."
                    .to_string(),
            },
            InteractivePatchApprovalGate {
                gate_id: "human_approval_before_apply".to_string(),
                title: "Require human approval before apply".to_string(),
                status: approval_status.to_string(),
                command: commands.apply.clone(),
                mutates_workflow: true,
                requires_human_approval: true,
                rationale: "Applying a patch records workflow artifacts and must follow review plus approval."
                    .to_string(),
            },
            InteractivePatchApprovalGate {
                gate_id: "rollback_restore_approval".to_string(),
                title: "Keep rollback restore approval explicit".to_string(),
                status: rollback_status.to_string(),
                command: commands.restore.clone(),
                mutates_workflow: true,
                requires_human_approval: true,
                rationale: "Restoring files is destructive and requires explicit approved-by plus confirm flags."
                    .to_string(),
            },
        ],
        notes: vec![
            "Approval flow is read-only and never edits files by itself.".to_string(),
            "Use review evidence before apply, and use restore only from an approved rollback artifact."
                .to_string(),
        ],
    }
}

fn patch_workbench_commands() -> InteractivePatchWorkbenchCommands {
    InteractivePatchWorkbenchCommands {
        refresh: vec![
            "interactive".to_string(),
            "patch-workbench".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        status: vec![
            "interactive".to_string(),
            "patch-workbench".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        plan: vec![
            "patch".to_string(),
            "plan".to_string(),
            "--workflow".to_string(),
            "<workflow-id>".to_string(),
            "--task".to_string(),
            "<task-id>".to_string(),
            "--intent".to_string(),
            "<intent>".to_string(),
            "--path".to_string(),
            "<path>".to_string(),
            "--origin".to_string(),
            "forge_cli".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        review: vec![
            "patch".to_string(),
            "review".to_string(),
            "--workflow".to_string(),
            "<workflow-id>".to_string(),
            "--task".to_string(),
            "<task-id>".to_string(),
            "--path".to_string(),
            "<path>".to_string(),
            "--origin".to_string(),
            "forge_cli".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        diff: vec![
            "patch".to_string(),
            "diff".to_string(),
            "--workflow".to_string(),
            "<workflow-id>".to_string(),
            "--task".to_string(),
            "<task-id>".to_string(),
            "--path".to_string(),
            "<path>".to_string(),
            "--file-index".to_string(),
            "0".to_string(),
            "--hunk-index".to_string(),
            "0".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        apply: vec![
            "patch".to_string(),
            "apply".to_string(),
            "--workflow".to_string(),
            "<workflow-id>".to_string(),
            "--task".to_string(),
            "<task-id>".to_string(),
            "--path".to_string(),
            "<path>".to_string(),
            "--plan-artifact".to_string(),
            "<plan-artifact>".to_string(),
            "--origin".to_string(),
            "forge_cli".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        revert: vec![
            "patch".to_string(),
            "revert".to_string(),
            "--workflow".to_string(),
            "<workflow-id>".to_string(),
            "--task".to_string(),
            "<task-id>".to_string(),
            "--apply-artifact".to_string(),
            "<apply-artifact>".to_string(),
            "--origin".to_string(),
            "forge_cli".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        restore: vec![
            "patch".to_string(),
            "restore".to_string(),
            "--workflow".to_string(),
            "<workflow-id>".to_string(),
            "--task".to_string(),
            "<task-id>".to_string(),
            "--revert-artifact".to_string(),
            "<revert-artifact>".to_string(),
            "--approved-by".to_string(),
            "<operator>".to_string(),
            "--confirm-restore".to_string(),
            "--origin".to_string(),
            "forge_cli".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
    }
}

fn patch_workbench_file_commands(path: &str) -> InteractivePatchWorkbenchFileCommands {
    InteractivePatchWorkbenchFileCommands {
        plan: vec![
            "patch".to_string(),
            "plan".to_string(),
            "--workflow".to_string(),
            "<workflow-id>".to_string(),
            "--task".to_string(),
            "<task-id>".to_string(),
            "--intent".to_string(),
            "<intent>".to_string(),
            "--path".to_string(),
            path.to_string(),
            "--origin".to_string(),
            "forge_cli".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        review: vec![
            "patch".to_string(),
            "review".to_string(),
            "--workflow".to_string(),
            "<workflow-id>".to_string(),
            "--task".to_string(),
            "<task-id>".to_string(),
            "--path".to_string(),
            path.to_string(),
            "--origin".to_string(),
            "forge_cli".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        diff: vec![
            "patch".to_string(),
            "diff".to_string(),
            "--workflow".to_string(),
            "<workflow-id>".to_string(),
            "--task".to_string(),
            "<task-id>".to_string(),
            "--path".to_string(),
            path.to_string(),
            "--file-index".to_string(),
            "0".to_string(),
            "--hunk-index".to_string(),
            "0".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
    }
}

fn permissions_commands() -> InteractivePermissionsCommands {
    InteractivePermissionsCommands {
        refresh: vec![
            "interactive".to_string(),
            "permissions".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        list_memberships: vec![
            "identity".to_string(),
            "memberships".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        update_membership: vec![
            "identity".to_string(),
            "membership-update".to_string(),
            "--subject".to_string(),
            "<user-id>".to_string(),
            "--organization".to_string(),
            "<organization-id>".to_string(),
            "--brand".to_string(),
            "<brand-id>".to_string(),
            "--product".to_string(),
            "<product-id>".to_string(),
            "--grant".to_string(),
            "<permission>".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        list_addon_permissions: vec![
            "addons".to_string(),
            "permissions".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        authorize_addon_permission: vec![
            "addons".to_string(),
            "authorize-permission".to_string(),
            "--addon".to_string(),
            "<addon-id>".to_string(),
            "--permission".to_string(),
            "<permission-id>".to_string(),
            "--approved-by".to_string(),
            "<operator>".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        revoke_addon_permission: vec![
            "addons".to_string(),
            "revoke-permission".to_string(),
            "--addon".to_string(),
            "<addon-id>".to_string(),
            "--permission".to_string(),
            "<permission-id>".to_string(),
            "--approved-by".to_string(),
            "<operator>".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        list_interactions: vec![
            "interaction".to_string(),
            "list".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
    }
}

fn identity_commands(project_root: &Path) -> InteractiveIdentityCommands {
    let project_root = project_root.display().to_string();
    InteractiveIdentityCommands {
        refresh: vec![
            "interactive".to_string(),
            "identity".to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        context: vec![
            "identity".to_string(),
            "context".to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        sync: vec![
            "identity".to_string(),
            "sync".to_string(),
            "--project-root".to_string(),
            project_root,
            "--output".to_string(),
            "json".to_string(),
        ],
        registry: vec![
            "identity".to_string(),
            "registry".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        link: vec![
            "identity".to_string(),
            "link".to_string(),
            "--left-scope".to_string(),
            "<channel-scope>".to_string(),
            "--left-id".to_string(),
            "<channel-id>".to_string(),
            "--right-scope".to_string(),
            "user".to_string(),
            "--right-id".to_string(),
            "<user-id>".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        links: vec![
            "identity".to_string(),
            "links".to_string(),
            "--status".to_string(),
            "active".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        resolve: vec![
            "identity".to_string(),
            "resolve".to_string(),
            "--scope".to_string(),
            "<scope>".to_string(),
            "--id".to_string(),
            "<id>".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        memberships: vec![
            "identity".to_string(),
            "memberships".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        tenant_audit: vec![
            "identity".to_string(),
            "tenant-audit".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
    }
}

fn identity_alias_commands(
    link: &crate::identity::IdentityLinkView,
    alias_scope: &str,
    alias_id: &str,
) -> InteractiveIdentityAliasCommands {
    InteractiveIdentityAliasCommands {
        resolve: vec![
            "identity".to_string(),
            "resolve".to_string(),
            "--scope".to_string(),
            alias_scope.to_string(),
            "--id".to_string(),
            alias_id.to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        unlink: vec![
            "identity".to_string(),
            "unlink".to_string(),
            "--left-scope".to_string(),
            link.left_scope.clone(),
            "--left-id".to_string(),
            link.left_id.clone(),
            "--right-scope".to_string(),
            link.right_scope.clone(),
            "--right-id".to_string(),
            link.right_id.clone(),
            "--source".to_string(),
            "forge_cli".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
    }
}

fn channel_alias_parts(
    link: &crate::identity::IdentityLinkView,
) -> (String, String, String, String) {
    match (
        is_channel_identity_scope(&link.left_scope),
        is_channel_identity_scope(&link.right_scope),
    ) {
        (true, false) => (
            link.left_scope.clone(),
            link.left_id.clone(),
            link.right_scope.clone(),
            link.right_id.clone(),
        ),
        (false, true) => (
            link.right_scope.clone(),
            link.right_id.clone(),
            link.left_scope.clone(),
            link.left_id.clone(),
        ),
        _ => (
            link.left_scope.clone(),
            link.left_id.clone(),
            link.right_scope.clone(),
            link.right_id.clone(),
        ),
    }
}

fn identity_membership_commands(
    membership: &crate::identity::IdentityMembershipView,
) -> InteractiveIdentityMembershipCommands {
    InteractiveIdentityMembershipCommands {
        list: vec![
            "identity".to_string(),
            "memberships".to_string(),
            "--subject-scope".to_string(),
            membership.subject_scope.clone(),
            "--subject".to_string(),
            membership.subject_id.clone(),
            "--organization".to_string(),
            membership.organization_id.clone(),
            "--brand".to_string(),
            membership.brand_id.clone(),
            "--product".to_string(),
            membership.product_id.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        update: vec![
            "identity".to_string(),
            "membership-update".to_string(),
            "--subject-scope".to_string(),
            membership.subject_scope.clone(),
            "--subject".to_string(),
            membership.subject_id.clone(),
            "--organization".to_string(),
            membership.organization_id.clone(),
            "--brand".to_string(),
            membership.brand_id.clone(),
            "--product".to_string(),
            membership.product_id.clone(),
            "--grant".to_string(),
            "<permission>".to_string(),
            "--source".to_string(),
            "forge_cli".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
    }
}

fn is_channel_alias_link(link: &crate::identity::IdentityLinkView) -> bool {
    is_channel_identity_scope(&link.left_scope) || is_channel_identity_scope(&link.right_scope)
}

fn is_channel_identity_scope(scope: &str) -> bool {
    !matches!(
        scope,
        "organization" | "brand" | "product" | "user" | "workspace" | "project"
    )
}

fn permission_membership_commands(
    membership: &crate::identity::IdentityMembershipView,
) -> InteractivePermissionMembershipCommands {
    InteractivePermissionMembershipCommands {
        list: vec![
            "identity".to_string(),
            "memberships".to_string(),
            "--subject-scope".to_string(),
            membership.subject_scope.clone(),
            "--subject".to_string(),
            membership.subject_id.clone(),
            "--organization".to_string(),
            membership.organization_id.clone(),
            "--brand".to_string(),
            membership.brand_id.clone(),
            "--product".to_string(),
            membership.product_id.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        update: vec![
            "identity".to_string(),
            "membership-update".to_string(),
            "--subject-scope".to_string(),
            membership.subject_scope.clone(),
            "--subject".to_string(),
            membership.subject_id.clone(),
            "--organization".to_string(),
            membership.organization_id.clone(),
            "--brand".to_string(),
            membership.brand_id.clone(),
            "--product".to_string(),
            membership.product_id.clone(),
            "--grant".to_string(),
            "<permission>".to_string(),
            "--source".to_string(),
            "forge_cli".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
    }
}

fn addon_permission_commands(
    addon_id: &str,
    permission_id: &str,
) -> InteractiveAddonPermissionCommands {
    InteractiveAddonPermissionCommands {
        list: vec![
            "addons".to_string(),
            "permissions".to_string(),
            "--addon".to_string(),
            addon_id.to_string(),
            "--permission".to_string(),
            permission_id.to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        authorize: vec![
            "addons".to_string(),
            "authorize-permission".to_string(),
            "--addon".to_string(),
            addon_id.to_string(),
            "--permission".to_string(),
            permission_id.to_string(),
            "--approved-by".to_string(),
            "<operator>".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        revoke: vec![
            "addons".to_string(),
            "revoke-permission".to_string(),
            "--addon".to_string(),
            addon_id.to_string(),
            "--permission".to_string(),
            permission_id.to_string(),
            "--approved-by".to_string(),
            "<operator>".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
    }
}

fn approval_item_commands(workflow_id: &str, task_id: &str) -> InteractiveApprovalCommands {
    InteractiveApprovalCommands {
        list: vec![
            "interaction".to_string(),
            "list".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        answer: vec![
            "interaction".to_string(),
            "answer".to_string(),
            "--workflow".to_string(),
            workflow_id.to_string(),
            "--task".to_string(),
            task_id.to_string(),
            "--selected".to_string(),
            "<choice-id>".to_string(),
            "--origin".to_string(),
            "forge_cli".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        expire: vec![
            "interaction".to_string(),
            "expire".to_string(),
            "--workflow".to_string(),
            workflow_id.to_string(),
            "--task".to_string(),
            task_id.to_string(),
            "--origin".to_string(),
            "forge_cli".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
    }
}

pub fn build_interactive_task_board(store: &ForgeStore) -> Result<InteractiveTaskBoardPanel> {
    let workflows = list_workflows_with_filters(
        store,
        WorkflowRegistryFilters::new(WorkflowLifecycleFilter::All),
    )?;
    build_task_board_panel(store, &workflows.workflows)
}

pub fn build_interactive_workflow_mutation(
    store: &ForgeStore,
) -> Result<InteractiveWorkflowMutationPanel> {
    let home = build_interactive_home(store)?;
    Ok(home.dashboard.workflow_mutation_panel)
}

pub fn build_interactive_guided_cockpit(
    store: &ForgeStore,
) -> Result<InteractiveGuidedCockpitPanel> {
    let home = build_interactive_home(store)?;
    Ok(home.dashboard.guided_cockpit_panel)
}

pub fn build_interactive_workflow_sidebar(
    store: &ForgeStore,
) -> Result<InteractiveWorkflowSidebarPanel> {
    let workflows = list_workflows_with_filters(
        store,
        WorkflowRegistryFilters::new(WorkflowLifecycleFilter::All),
    )?;
    Ok(build_workflow_sidebar_panel(&workflows.workflows))
}

pub fn build_interactive_replacement_cli(
    store: &ForgeStore,
) -> Result<InteractiveReplacementCliPanel> {
    build_interactive_replacement_cli_with_options(
        store,
        InteractiveReplacementCliOptions::default(),
    )
}

pub fn build_interactive_replacement_cli_with_options(
    store: &ForgeStore,
    options: InteractiveReplacementCliOptions,
) -> Result<InteractiveReplacementCliPanel> {
    let project_root = options
        .project_root
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let project_root_display = project_root.display().to_string();
    let workflows = list_workflows_with_filters(
        store,
        WorkflowRegistryFilters::new(WorkflowLifecycleFilter::All),
    )?;
    let workflow_sidebar = build_workflow_sidebar_panel(&workflows.workflows);
    let task_board = build_task_board_panel(store, &workflows.workflows)?;
    let dag = build_workflow_dag_panel(store, &workflows.workflows)?;
    let patch_workbench = build_interactive_patch_workbench(store)?;
    let command_palette = build_interactive_command_palette(store, None)?;
    let action_registry = build_interactive_action_registry(store, None)?;
    let autocomplete = build_interactive_autocomplete(store, "/pa")?;
    let mut harness_options = InteractiveHarnessOptions::default_for_current_dir();
    harness_options.project_root = Some(project_root.clone());
    let harness = build_interactive_harness(store, harness_options)?;
    let sessions = build_interactive_sessions(store, InteractiveSessionsOptions::default())?;
    let release_gates = build_interactive_release_gates(store, "0.5", Some(&project_root))?;
    let external_brain_evidence_plan = interactive_release_gate_evidence_plan(
        store,
        "0.5",
        "replacement_grade_cli",
        Some(&project_root),
    )?;
    let structured_logs = build_interactive_structured_logs(store)?;
    let cost_ledger = build_cost_ledger(store, None, None, None, None).ok();
    let permissions = build_interactive_permissions(store)?;

    let replacement_gate = release_gates
        .gate_cards
        .iter()
        .find(|gate| gate.capability_id == "replacement_grade_cli");
    let promotion_ready = replacement_gate
        .map(|gate| gate.promotion_ready)
        .unwrap_or(false);
    let required_attached_evidence_kinds = replacement_gate
        .map(|gate| gate.required_attached_evidence_kinds.clone())
        .unwrap_or_default();
    let attached_evidence_kinds = replacement_gate
        .map(|gate| gate.attached_evidence_kinds.clone())
        .unwrap_or_default();
    let missing_attached_evidence_kinds = replacement_gate
        .map(|gate| gate.missing_attached_evidence_kinds.clone())
        .unwrap_or_default();
    let blockers = replacement_cli_evidence_blockers(
        replacement_gate.is_some(),
        promotion_ready,
        &missing_attached_evidence_kinds,
    );
    let next_actions =
        replacement_cli_next_actions(promotion_ready, &missing_attached_evidence_kinds);
    let replacement_commands = replacement_cli_commands(&project_root);
    let provider_readiness = replacement_cli_provider_readiness(
        &external_brain_evidence_plan.provider_candidates,
        &replacement_commands.collect_external_brain_evidence,
    );
    let provider_wrapper_plans = replacement_cli_provider_wrapper_plans(
        &external_brain_evidence_plan.provider_candidates,
        &replacement_commands,
        &project_root,
    );
    let provider_wrapper_plan_count = provider_wrapper_plans.len();
    let provider_wrapper_manifest_audit = replacement_cli_provider_wrapper_manifest_audit(
        &external_brain_evidence_plan,
        &replacement_commands,
        &project_root,
    );
    let provider_readiness_count = provider_readiness.len();
    let installed_provider_count = provider_readiness
        .iter()
        .filter(|provider| provider.installed)
        .count();
    let wrapper_required_provider_count = provider_readiness
        .iter()
        .filter(|provider| provider.wrapper_required)
        .count();

    let mut surfaces = vec![
        replacement_cli_surface(
            "operator_home",
            "No-argument operator home",
            "ready",
            true,
            &[
                "navigation_panel",
                "ui_composition_panel",
                "release_gates_panel",
            ],
            &[
                "no_argument_tui_home_available",
                "keyboard_navigation_modes_and_themes_declared",
                "dynamic_ui_composition_available",
            ],
            &[],
            &[
                "forge",
                &format!("forge interactive home --project-root {project_root_display} --output json"),
                "forge interactive slash-commands --output json",
            ],
        ),
        replacement_cli_surface(
            "workflow_operations",
            "Workflow operations",
            "ready",
            workflow_sidebar.workflow_count >= task_board.workflow_count && dag.workflow_count >= task_board.workflow_count,
            &[
                "workflow_sidebar_panel",
                "task_board_panel",
                "dag_panel",
            ],
            &[
                "workflow_sidebar_groups_navigation",
                "task_board_cards_handoffs_waits_checkpoints",
                "workflow_dag_dependency_drilldown",
            ],
            &[],
            &[
                "forge interactive workflow-sidebar --output json",
                "forge interactive task-board --output json",
                "forge interactive workflow-dag --output json",
            ],
        ),
        replacement_cli_surface(
            "file_editing_ux",
            "File editing and patch review UX",
            &patch_workbench.status,
            patch_workbench.edit_intake.required_input_count > 0
                && patch_workbench.approval_flow.gates.len() >= 4
                && patch_workbench.commands.review.contains(&"review".to_string())
                && patch_workbench.commands.restore.contains(&"restore".to_string()),
            &["patch_workbench_panel"],
            &[
                "patch_edit_intake_declares_required_inputs",
                "diff_preview_and_review_queue_available",
                "review_apply_revert_restore_gates_declared",
            ],
            &[],
            &[
                "forge interactive patch-workbench --output json",
                "forge patch plan --workflow <workflow-id> --task <task-id> --intent <intent> --path <path> --output json",
                "forge patch review --workflow <workflow-id> --task <task-id> --path <path> --output json",
            ],
        ),
        replacement_cli_surface(
            "action_discovery",
            "Action discovery and command completion",
            "ready",
            command_palette.entry_count > 0
                && action_registry.action_count > 0
                && autocomplete.suggestion_count > 0,
            &[
                "command_palette_panel",
                "action_registry_panel",
                "autocomplete_panel",
            ],
            &[
                "command_palette_action_registry_and_autocomplete_ready",
                "actions_expose_mutation_and_approval_flags",
                "autocomplete_returns_safe_invocation_plans",
            ],
            &[],
            &[
                "forge interactive command-palette --output json",
                "forge interactive action-registry --output json",
                "forge interactive autocomplete --input /pa --output json",
            ],
        ),
        replacement_cli_surface(
            "brain_harness_sessions",
            "Brain, harness and session control",
            &harness.status,
            sessions.session_count > 0
                && harness.headroom_plan.schema_version == "forge.harness.headroom_plan.v1",
            &[
                "harness_panel",
                "sessions_panel",
                "readiness_panel",
            ],
            &[
                "forge_first_harness_controls_available",
                "headroom_plan_and_stats_available",
                "session_lifecycle_operation_plans_available",
            ],
            &[],
            &[
                "forge interactive harness --output json",
                "forge interactive sessions --output json",
                "forge interactive readiness --output json",
            ],
        ),
        replacement_cli_surface(
            "observability_costs",
            "Observability, logs and costs",
            &structured_logs.status,
            structured_logs.schema_version == INTERACTIVE_STRUCTURED_LOGS_SCHEMA_VERSION
                && cost_ledger.is_some(),
            &[
                "structured_logs_panel",
                "cost_panel",
                "event_runtime_panel",
            ],
            &[
                "structured_logs_timeline_available",
                "cost_ledger_available",
                "event_runtime_panel_available_in_home",
            ],
            &[],
            &[
                "forge interactive structured-logs --output json",
                "forge cost ledger --output json",
                "forge interactive home --output json",
            ],
        ),
        replacement_cli_surface(
            "human_approvals",
            "Human approvals and permission gates",
            &permissions.status,
            permissions.schema_version == INTERACTIVE_PERMISSIONS_SCHEMA_VERSION,
            &[
                "permissions_panel",
                "release_gates_panel",
                "patch_workbench_panel",
            ],
            &[
                "pending_human_approvals_visible",
                "addon_permission_authorizations_visible",
                "patch_apply_and_restore_require_approval",
            ],
            &[],
            &[
                "forge interactive permissions --output json",
                "forge interaction list --output json",
                "forge interactive release-gates --output json",
            ],
        ),
    ];
    surfaces.push(replacement_cli_surface_owned(
        "milestone_evidence",
        "Replacement-grade CLI milestone evidence",
        replacement_gate
            .map(|gate| gate.status.as_str())
            .unwrap_or("missing"),
        promotion_ready,
        vec!["release_gates_panel".to_string()],
        replacement_cli_milestone_evidence_items(
            &required_attached_evidence_kinds,
            &attached_evidence_kinds,
            &missing_attached_evidence_kinds,
            &external_brain_evidence_plan,
        ),
        blockers.clone(),
        vec![
            "forge milestone cli-demo --origin codex --output json".to_string(),
            format!("forge {}", replacement_commands.evidence_plan.join(" ")),
            format!(
                "forge {}",
                replacement_commands
                    .collect_external_brain_evidence
                    .join(" ")
            ),
        ],
    ));

    let ready_surface_count = surfaces.iter().filter(|surface| surface.ready).count();
    let surface_count = surfaces.len();
    let blocked_surface_count = surface_count.saturating_sub(ready_surface_count);
    let readiness_percent = if surface_count == 0 {
        0
    } else {
        ((ready_surface_count * 100) / surface_count) as u64
    };
    let status = if promotion_ready {
        "replacement_cli_promotion_ready"
    } else if ready_surface_count + 1 >= surface_count {
        "replacement_cli_operator_ready_with_milestone_gaps"
    } else {
        "replacement_cli_needs_attention"
    };

    for surface in &mut surfaces {
        if !surface.ready && surface.blockers.is_empty() {
            surface.blockers.push(
                "surface evidence is not strong enough for replacement-grade readiness".to_string(),
            );
        }
    }

    Ok(InteractiveReplacementCliPanel {
        schema_version: INTERACTIVE_REPLACEMENT_CLI_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        project_root: project_root_display.clone(),
        milestone: "0.5".to_string(),
        capability_id: "replacement_grade_cli".to_string(),
        promotion_ready,
        required_attached_evidence_kinds,
        attached_evidence_kinds,
        missing_attached_evidence_kinds,
        surface_count,
        ready_surface_count,
        blocked_surface_count,
        readiness_percent,
        surfaces,
        external_brain_evidence_plan,
        provider_readiness_count,
        installed_provider_count,
        wrapper_required_provider_count,
        provider_readiness,
        provider_wrapper_plan_count,
        provider_wrapper_plans,
        provider_wrapper_manifest_audit,
        blockers,
        next_actions,
        commands: replacement_commands,
        notes: vec![
            "This panel is read-only; it aggregates replacement-grade CLI readiness without launching child CLIs or collecting evidence.".to_string(),
            "Provider wrapper plans are preparation guidance only; rendering them does not execute a model, mutate project files or count as release evidence.".to_string(),
            "Promotion remains false until required milestone evidence is attached and validated.".to_string(),
        ],
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "static readiness surface metadata stays compact and readable as fixture data"
)]
fn replacement_cli_surface(
    surface_id: &str,
    title: &str,
    status: &str,
    ready: bool,
    source_panels: &[&str],
    evidence: &[&str],
    blockers: &[&str],
    commands: &[&str],
) -> InteractiveReplacementCliSurface {
    replacement_cli_surface_owned(
        surface_id,
        title,
        status,
        ready,
        source_panels
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        evidence.iter().map(|value| (*value).to_string()).collect(),
        blockers.iter().map(|value| (*value).to_string()).collect(),
        commands.iter().map(|value| (*value).to_string()).collect(),
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "owned readiness surface metadata mirrors the static helper while allowing dynamic milestone evidence"
)]
fn replacement_cli_surface_owned(
    surface_id: &str,
    title: &str,
    status: &str,
    ready: bool,
    source_panels: Vec<String>,
    evidence: Vec<String>,
    blockers: Vec<String>,
    commands: Vec<String>,
) -> InteractiveReplacementCliSurface {
    InteractiveReplacementCliSurface {
        surface_id: surface_id.to_string(),
        title: title.to_string(),
        status: status.to_string(),
        ready,
        source_panels,
        evidence,
        blockers,
        commands,
    }
}

fn replacement_cli_provider_readiness(
    candidates: &[MilestoneEvidenceProviderCandidate],
    collect_external_brain_evidence_command: &[String],
) -> Vec<InteractiveReplacementCliProviderReadiness> {
    candidates
        .iter()
        .map(|candidate| {
            let collect_evidence_command = collect_external_brain_evidence_command
                .iter()
                .map(|part| {
                    if part == "<provider-id>" {
                        candidate.provider_id.clone()
                    } else {
                        part.clone()
                    }
                })
                .collect::<Vec<_>>();
            InteractiveReplacementCliProviderReadiness {
                provider_id: candidate.provider_id.clone(),
                brain_id: candidate.brain_id.clone(),
                binary: candidate.binary.clone(),
                installed: candidate.installed,
                detected_path: candidate.detected_path.clone(),
                readiness: candidate.readiness.clone(),
                version_status: candidate.version_status.clone(),
                wrapper_required: candidate.readiness == "cli_detected_wrapper_required",
                required_output_schema: "forge.connected_external_brain.provider_output.v1"
                    .to_string(),
                manifest_provider_template: candidate.manifest_provider_template.clone(),
                evidence_blocker: candidate.evidence_blocker.clone(),
                next_action: candidate.next_action.clone(),
                collect_evidence_command,
            }
        })
        .collect()
}

fn replacement_cli_provider_wrapper_plans(
    candidates: &[MilestoneEvidenceProviderCandidate],
    commands: &InteractiveReplacementCliCommands,
    project_root: &Path,
) -> Vec<InteractiveReplacementCliProviderWrapperPlan> {
    let wrapper_manifest_path = project_root
        .join(".forge")
        .join("connected-brain-runtimes.json")
        .display()
        .to_string();
    candidates
        .iter()
        .map(|candidate| {
            let required_output_schema = "forge.connected_external_brain.provider_output.v1";
            let evidence_plan_command =
                replacement_cli_provider_command(&commands.evidence_plan, &candidate.provider_id);
            let prepare_evidence_inputs_command = replacement_cli_provider_command(
                &commands.prepare_evidence_inputs,
                &candidate.provider_id,
            );
            let collect_evidence_command = replacement_cli_provider_command(
                &commands.collect_external_brain_evidence,
                &candidate.provider_id,
            );
            InteractiveReplacementCliProviderWrapperPlan {
                schema_version:
                    "forge.interactive.replacement_cli.provider_wrapper_plan.v1".to_string(),
                provider_id: candidate.provider_id.clone(),
                brain_id: candidate.brain_id.clone(),
                binary: candidate.binary.clone(),
                installed: candidate.installed,
                detected_path: candidate.detected_path.clone(),
                readiness: candidate.readiness.clone(),
                wrapper_required: candidate.readiness == "cli_detected_wrapper_required",
                wrapper_manifest_path: wrapper_manifest_path.clone(),
                required_output_schema: required_output_schema.to_string(),
                manifest_provider_template: candidate.manifest_provider_template.clone(),
                recommended_wrapper_command: replacement_cli_manifest_command(
                    &candidate.manifest_provider_template,
                ),
                evidence_plan_command,
                prepare_evidence_inputs_command,
                collect_evidence_command,
                counts_as_release_evidence: false,
                model_execution_allowed: false,
                mutates_project: false,
                safety_requirements: vec![
                    "This read-only plan does not execute a model or call the provider CLI."
                        .to_string(),
                    "Only prepare-evidence-inputs --apply may materialize secret-free templates, and it still does not count as release evidence.".to_string(),
                    "External brain evidence requires an operator-approved wrapper that emits forge.connected_external_brain.provider_output.v1.".to_string(),
                    "Credential values must stay in credential-vault or environment injection and must not be printed in the manifest.".to_string(),
                ],
                next_action: if candidate.installed {
                    format!(
                        "Review the provider template for `{}`, materialize secret-free inputs with prepare-evidence-inputs, then approve a wrapper before collecting evidence.",
                        candidate.provider_id
                    )
                } else {
                    format!(
                        "Install or configure `{}` before preparing a connected-brain wrapper.",
                        candidate.binary
                    )
                },
                promotion_impact: "Plan-only preparation keeps promotion blocked until collect-evidence records an approved real provider execution receipt.".to_string(),
            }
        })
        .collect()
}

fn replacement_cli_provider_wrapper_manifest_audit(
    evidence_plan: &InteractiveReleaseGateEvidencePlan,
    commands: &InteractiveReplacementCliCommands,
    project_root: &Path,
) -> InteractiveReplacementCliProviderWrapperManifestAudit {
    let manifest_path = project_root
        .join(".forge")
        .join("connected-brain-runtimes.json");
    let manifest_path_display = manifest_path.display().to_string();
    let mut audit = replacement_cli_empty_manifest_audit(
        "wrapper_manifest_missing",
        &manifest_path_display,
        evidence_plan.ready_to_collect_evidence,
        commands,
        "<provider-id>",
    );
    if !manifest_path.is_file() {
        audit
            .blockers
            .push("connected-brain-runtimes.json is missing".to_string());
        audit.next_action =
            "Materialize the secret-free connected-brain manifest template before provider execution.".to_string();
        return audit;
    }

    audit.manifest_present = true;
    let manifest_bytes = match fs::read(&manifest_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            audit.status = "wrapper_manifest_unreadable".to_string();
            audit
                .blockers
                .push(format!("manifest could not be read: {error}"));
            return audit;
        }
    };
    let manifest: serde_json::Value = match serde_json::from_slice(&manifest_bytes) {
        Ok(manifest) => manifest,
        Err(error) => {
            audit.status = "wrapper_manifest_invalid".to_string();
            audit
                .blockers
                .push(format!("manifest is not valid JSON: {error}"));
            return audit;
        }
    };
    audit.manifest_parseable = true;
    let providers = manifest
        .get("providers")
        .and_then(|providers| providers.as_array())
        .cloned()
        .unwrap_or_default();
    audit.provider_count = providers.len();
    let planned_provider_id = evidence_plan
        .config_checks
        .iter()
        .find(|check| check.id == "connected_brain_provider")
        .and_then(|check| check.selected_id.clone());
    let provider = planned_provider_id
        .as_deref()
        .and_then(|id| replacement_cli_manifest_provider_by_id(&providers, id))
        .or_else(|| {
            providers.iter().find(|provider| {
                replacement_cli_json_string_array(provider.get("capabilities"))
                    .iter()
                    .any(|capability| capability == "replacement_grade_cli")
            })
        });
    let Some(provider) = provider else {
        audit.status = "wrapper_manifest_provider_missing".to_string();
        audit.blockers.push(
            "no provider declares replacement_grade_cli in connected-brain-runtimes.json"
                .to_string(),
        );
        audit.next_action =
            "Add a provider with replacement_grade_cli capability before collecting provider evidence."
                .to_string();
        return audit;
    };

    let provider_id = replacement_cli_json_string(provider.get("id"))
        .unwrap_or_else(|| "<provider-id>".to_string());
    audit.selected_provider_id = Some(provider_id.clone());
    audit.selected_brain_id = replacement_cli_json_string(provider.get("brain_id"));
    audit.evidence_plan_command =
        replacement_cli_provider_command(&commands.evidence_plan, &provider_id);
    audit.prepare_evidence_inputs_command =
        replacement_cli_provider_command(&commands.prepare_evidence_inputs, &provider_id);
    audit.collect_evidence_command =
        replacement_cli_provider_command(&commands.collect_external_brain_evidence, &provider_id);

    let capabilities = replacement_cli_json_string_array(provider.get("capabilities"));
    audit.capability_declared = capabilities
        .iter()
        .any(|capability| capability == "replacement_grade_cli");
    let command = replacement_cli_json_string_array(provider.get("command"));
    audit.command_declared = !command.is_empty();
    audit.command_placeholders_absent = command
        .iter()
        .all(|part| !replacement_cli_manifest_placeholder(part));
    let (command_first_binary, command_path_status, command_executable) =
        replacement_cli_static_command_path_status(&command);
    audit.command_first_binary = command_first_binary;
    audit.command_path_status = command_path_status;
    audit.command_executable = command_executable;
    audit.approval_ready = replacement_cli_manifest_field_ready(provider.get("approved_by"))
        && replacement_cli_manifest_field_ready(provider.get("approval_ref"));
    audit.model_ready = replacement_cli_manifest_field_ready(provider.get("model_id"));
    audit.allow_model_execution = provider
        .get("allow_model_execution")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    audit.network_access_blocked = !provider
        .get("network_access")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    audit.device_access_blocked = !provider
        .get("device_access")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    audit.external_resources_untouched = !provider
        .get("external_resources_mutated")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);

    replacement_cli_push_manifest_audit_blockers(&mut audit);
    audit.ready_to_collect_evidence =
        audit.evidence_plan_ready && audit.blockers.is_empty() && audit.command_executable;
    audit.status = if audit.ready_to_collect_evidence {
        "wrapper_manifest_provider_ready"
    } else {
        "wrapper_manifest_provider_blocked"
    }
    .to_string();
    audit.next_action = if audit.ready_to_collect_evidence {
        "Run collect-evidence only after the operator is ready to execute the approved provider wrapper and inspect the receipt.".to_string()
    } else {
        "Fix the connected-brain provider manifest and wrapper executable before collecting provider evidence.".to_string()
    };
    audit
}

fn replacement_cli_empty_manifest_audit(
    status: &str,
    manifest_path: &str,
    evidence_plan_ready: bool,
    commands: &InteractiveReplacementCliCommands,
    provider_id: &str,
) -> InteractiveReplacementCliProviderWrapperManifestAudit {
    InteractiveReplacementCliProviderWrapperManifestAudit {
        schema_version:
            "forge.interactive.replacement_cli.provider_wrapper_manifest_audit.v1".to_string(),
        status: status.to_string(),
        manifest_path: manifest_path.to_string(),
        manifest_present: false,
        manifest_parseable: false,
        provider_count: 0,
        selected_provider_id: None,
        selected_brain_id: None,
        capability_declared: false,
        command_declared: false,
        command_placeholders_absent: false,
        command_first_binary: None,
        command_path_status: "not_checked".to_string(),
        command_executable: false,
        approval_ready: false,
        model_ready: false,
        allow_model_execution: false,
        network_access_blocked: false,
        device_access_blocked: false,
        external_resources_untouched: false,
        evidence_plan_ready,
        ready_to_collect_evidence: false,
        counts_as_release_evidence: false,
        model_execution_performed: false,
        blockers: Vec::new(),
        safety_requirements: vec![
            "This audit reads the manifest and filesystem metadata only; it does not execute the provider wrapper.".to_string(),
            "Release evidence is created only by collect-evidence after explicit operator approval.".to_string(),
            "Model execution remains false until the provider wrapper runs and emits a reviewed provider-output receipt.".to_string(),
        ],
        evidence_plan_command: replacement_cli_provider_command(&commands.evidence_plan, provider_id),
        prepare_evidence_inputs_command: replacement_cli_provider_command(
            &commands.prepare_evidence_inputs,
            provider_id,
        ),
        collect_evidence_command: replacement_cli_provider_command(
            &commands.collect_external_brain_evidence,
            provider_id,
        ),
        next_action: String::new(),
    }
}

fn replacement_cli_manifest_provider_by_id<'a>(
    providers: &'a [serde_json::Value],
    provider_id: &str,
) -> Option<&'a serde_json::Value> {
    providers.iter().find(|provider| {
        replacement_cli_json_string(provider.get("id")).as_deref() == Some(provider_id)
    })
}

fn replacement_cli_json_string(value: Option<&serde_json::Value>) -> Option<String> {
    value
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn replacement_cli_json_string_array(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| replacement_cli_json_string(Some(value)))
                .collect()
        })
        .unwrap_or_default()
}

fn replacement_cli_manifest_field_ready(value: Option<&serde_json::Value>) -> bool {
    replacement_cli_json_string(value)
        .as_deref()
        .is_some_and(|value| !replacement_cli_manifest_placeholder(value))
}

fn replacement_cli_manifest_placeholder(value: &str) -> bool {
    let value = value.trim();
    value.is_empty()
        || value.contains("<")
        || value.contains(">")
        || value.contains("placeholder")
        || value.contains("approved-")
        || value.contains("approval-or-change-record")
}

fn replacement_cli_static_command_path_status(
    command: &[String],
) -> (Option<String>, String, bool) {
    let Some(first) = command
        .first()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    else {
        return (None, "missing".to_string(), false);
    };
    if replacement_cli_manifest_placeholder(first) {
        return (Some(first.to_string()), "placeholder".to_string(), false);
    }
    let path = Path::new(first);
    if !path.is_absolute() {
        return (Some(first.to_string()), "not_absolute".to_string(), false);
    }
    let Ok(metadata) = fs::metadata(path) else {
        return (Some(first.to_string()), "missing".to_string(), false);
    };
    if !metadata.is_file() {
        return (Some(first.to_string()), "not_file".to_string(), false);
    }
    if replacement_cli_metadata_executable(&metadata) {
        (Some(first.to_string()), "executable".to_string(), true)
    } else {
        (Some(first.to_string()), "not_executable".to_string(), false)
    }
}

#[cfg(unix)]
fn replacement_cli_metadata_executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn replacement_cli_metadata_executable(metadata: &fs::Metadata) -> bool {
    !metadata.permissions().readonly()
}

fn replacement_cli_push_manifest_audit_blockers(
    audit: &mut InteractiveReplacementCliProviderWrapperManifestAudit,
) {
    if !audit.capability_declared {
        audit
            .blockers
            .push("provider must declare replacement_grade_cli capability".to_string());
    }
    if !audit.command_declared {
        audit
            .blockers
            .push("provider command is missing".to_string());
    }
    if !audit.command_placeholders_absent {
        audit
            .blockers
            .push("provider command still contains placeholders".to_string());
    }
    if !audit.command_executable {
        audit.blockers.push(format!(
            "provider wrapper command is not executable: {}",
            audit.command_path_status
        ));
    }
    if !audit.approval_ready {
        audit
            .blockers
            .push("approved_by and approval_ref must be concrete".to_string());
    }
    if !audit.model_ready {
        audit.blockers.push("model_id must be concrete".to_string());
    }
    if !audit.allow_model_execution {
        audit
            .blockers
            .push("allow_model_execution must be true for provider evidence".to_string());
    }
    if !audit.network_access_blocked {
        audit
            .blockers
            .push("network_access must be false for this gate".to_string());
    }
    if !audit.device_access_blocked {
        audit
            .blockers
            .push("device_access must be false for this gate".to_string());
    }
    if !audit.external_resources_untouched {
        audit
            .blockers
            .push("external_resources_mutated must be false".to_string());
    }
    if !audit.evidence_plan_ready {
        audit
            .blockers
            .push("milestone evidence plan is not ready".to_string());
    }
}

fn replacement_cli_provider_command(command: &[String], provider_id: &str) -> Vec<String> {
    command
        .iter()
        .map(|part| {
            if part == "<provider-id>" {
                provider_id.to_string()
            } else {
                part.clone()
            }
        })
        .collect()
}

fn replacement_cli_manifest_command(template: &serde_json::Value) -> Vec<String> {
    template
        .get("command")
        .and_then(|command| command.as_array())
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.as_str().map(ToString::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn replacement_cli_milestone_evidence_items(
    required: &[String],
    attached: &[String],
    missing: &[String],
    external_brain_evidence_plan: &InteractiveReleaseGateEvidencePlan,
) -> Vec<String> {
    let mut evidence = vec![
        "milestone_cli_demo_command_available".to_string(),
        "evidence_plan_lists_connected_brain_manifest".to_string(),
        "promotion_requires_attached_operator_evidence".to_string(),
        format!(
            "external_brain_evidence_plan_status:{}",
            external_brain_evidence_plan.status
        ),
        format!(
            "external_brain_provider_candidates:{}",
            external_brain_evidence_plan.provider_candidate_count
        ),
    ];
    evidence.extend(
        external_brain_evidence_plan
            .manifest_template_ids
            .iter()
            .map(|id| format!("manifest_template:{id}")),
    );
    evidence.extend(
        required
            .iter()
            .map(|kind| format!("required_evidence:{kind}")),
    );
    evidence.extend(
        attached
            .iter()
            .map(|kind| format!("attached_evidence:{kind}")),
    );
    evidence.extend(
        missing
            .iter()
            .map(|kind| format!("missing_evidence:{kind}")),
    );
    evidence
}

fn replacement_cli_evidence_blockers(
    gate_present: bool,
    promotion_ready: bool,
    missing: &[String],
) -> Vec<String> {
    if promotion_ready {
        return Vec::new();
    }
    if !gate_present {
        return vec![
            "replacement_grade_cli release gate is missing from the 0.5 manifest".to_string(),
        ];
    }
    let mut blockers = missing
        .iter()
        .map(|kind| replacement_cli_missing_evidence_blocker(kind))
        .collect::<Vec<_>>();
    if blockers.is_empty() {
        blockers.push(
            "replacement_grade_cli release gate is not promotable yet; inspect release gates for the current non-evidence blocker"
                .to_string(),
        );
    }
    blockers
}

fn replacement_cli_missing_evidence_blocker(kind: &str) -> String {
    match kind {
        "external_brain_provider_execution" => {
            "missing attached evidence kind: external_brain_provider_execution; approve a connected brain provider wrapper that emits forge.connected_external_brain.provider_output.v1 with real/model execution evidence"
                .to_string()
        }
        "broader_project_coding_research_workflow" => {
            "missing attached evidence kind: broader_project_coding_research_workflow; collect the deterministic Forge-owned multi-file coding/research workflow receipt"
                .to_string()
        }
        "terminal_file_editing_ux" => {
            "missing attached evidence kind: terminal_file_editing_ux; collect plan/review/diff/apply/revert/restore patch lifecycle evidence"
                .to_string()
        }
        other => format!("missing attached evidence kind: {other}"),
    }
}

fn replacement_cli_next_actions(promotion_ready: bool, missing: &[String]) -> Vec<String> {
    if promotion_ready {
        return vec![
            "Inspect forge milestone manifest --version 0.5 before claiming replacement-grade CLI promotion."
                .to_string(),
        ];
    }
    let mut actions = vec![
        "Run forge milestone cli-demo and inspect the replacement-grade flow evidence.".to_string(),
    ];
    if missing
        .iter()
        .any(|kind| kind == "external_brain_provider_execution")
    {
        actions.push(
            "Create or approve .forge/connected-brain-runtimes.json with a safe provider wrapper, then collect external_brain_provider_execution evidence."
                .to_string(),
        );
    }
    if missing
        .iter()
        .any(|kind| kind == "broader_project_coding_research_workflow")
    {
        actions.push(
            "Collect broader_project_coding_research_workflow evidence through Forge milestone evidence collection."
                .to_string(),
        );
    }
    if missing
        .iter()
        .any(|kind| kind == "terminal_file_editing_ux")
    {
        actions.push(
            "Collect terminal_file_editing_ux evidence from the patch lifecycle before treating terminal editing as promotion-ready."
                .to_string(),
        );
    }
    if actions.len() == 1 {
        actions.push(
            "Inspect forge interactive release-gates --version 0.5 --output json for the remaining non-evidence blocker."
                .to_string(),
        );
    }
    actions
}

fn replacement_cli_commands(project_root: &Path) -> InteractiveReplacementCliCommands {
    let project_root = project_root.display().to_string();
    InteractiveReplacementCliCommands {
        refresh: vec![
            "interactive".to_string(),
            "replacement-cli".to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        home: vec![
            "interactive".to_string(),
            "home".to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        command_palette: vec![
            "interactive".to_string(),
            "command-palette".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        action_registry: vec![
            "interactive".to_string(),
            "action-registry".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        autocomplete: vec![
            "interactive".to_string(),
            "autocomplete".to_string(),
            "--input".to_string(),
            "/pa".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        patch_workbench: vec![
            "interactive".to_string(),
            "patch-workbench".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        harness: vec![
            "interactive".to_string(),
            "harness".to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        sessions: vec![
            "interactive".to_string(),
            "sessions".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        release_gates: vec![
            "interactive".to_string(),
            "release-gates".to_string(),
            "--version".to_string(),
            "0.5".to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        cli_demo: vec![
            "milestone".to_string(),
            "cli-demo".to_string(),
            "--origin".to_string(),
            "codex".to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        evidence_plan: vec![
            "milestone".to_string(),
            "evidence-plan".to_string(),
            "--version".to_string(),
            "0.5".to_string(),
            "--capability".to_string(),
            "replacement_grade_cli".to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--connected-brain".to_string(),
            "<provider-id>".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        prepare_evidence_inputs: vec![
            "milestone".to_string(),
            "prepare-evidence-inputs".to_string(),
            "--version".to_string(),
            "0.5".to_string(),
            "--capability".to_string(),
            "replacement_grade_cli".to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--connected-brain".to_string(),
            "<provider-id>".to_string(),
            "--apply".to_string(),
            "--approved-by".to_string(),
            "<operator>".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        collect_external_brain_evidence: vec![
            "milestone".to_string(),
            "collect-evidence".to_string(),
            "--version".to_string(),
            "0.5".to_string(),
            "--capability".to_string(),
            "replacement_grade_cli".to_string(),
            "--kind".to_string(),
            "external_brain_provider_execution".to_string(),
            "--project-root".to_string(),
            project_root,
            "--connected-brain".to_string(),
            "<provider-id>".to_string(),
            "--approved-by".to_string(),
            "<operator>".to_string(),
            "--origin".to_string(),
            "codex".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
    }
}

pub fn build_interactive_multimodal_runtime(
    store: &ForgeStore,
    project_root: &Path,
    enable_experimental: bool,
) -> Result<InteractiveMultimodalRuntimePanel> {
    let addon_id = "forge.addon.multimodal";
    let addon_view_id = "multimodal.benchmark_center";
    let capability_id = "experimental_multimodal_runtime";
    let runtime_contract_id = "multimodal_runtime_benchmark.executor";
    let permission_id = "multimodal.runtime_benchmark";
    let project_root_display = project_root.display().to_string();

    let catalog = load_addon_catalog_from_store(store, &default_addon_dirs()).ok();
    let addon = catalog
        .as_ref()
        .and_then(|catalog| catalog.addons.iter().find(|addon| addon.id == addon_id));
    let has_addon = addon.is_some();
    let has_capability = addon
        .map(|addon| {
            addon
                .capabilities
                .iter()
                .any(|capability| capability.id == CAP_MULTIMODAL_RUNTIME)
        })
        .unwrap_or(false);
    let has_permission = addon
        .map(|addon| {
            addon
                .permissions
                .iter()
                .any(|permission| permission.id == permission_id)
        })
        .unwrap_or(false);
    let has_view = addon
        .map(|addon| addon.views.iter().any(|view| view.id == addon_view_id))
        .unwrap_or(false);
    let has_runtime_contract = addon
        .map(|addon| {
            addon
                .runtime_contracts
                .iter()
                .any(|contract| contract.id == runtime_contract_id)
        })
        .unwrap_or(false);
    let view_action_count = addon
        .and_then(|addon| addon.views.iter().find(|view| view.id == addon_view_id))
        .map(|view| view.actions.len())
        .unwrap_or(0);

    let feature_flag = resolve_multimodal_feature_flag(enable_experimental, Some(project_root));
    let status_report = build_multimodal_status_with_feature_flag(feature_flag.clone());
    let readiness = build_multimodal_readiness(MultimodalReadinessOptions {
        capability_id: "image_understanding",
        enable_experimental: feature_flag.enabled,
        explicit_allow: false,
        project_root: Some(project_root),
    })?;
    let install_plan = build_multimodal_install_plan("image_understanding", feature_flag.enabled)?;
    let benchmark_template =
        build_multimodal_benchmark_template("image_understanding", feature_flag.enabled)?;
    let demo_plan = build_multimodal_demo_plan("local_image_recognition", feature_flag.enabled)?;
    let guard = evaluate_multimodal_guard(
        "image_understanding",
        "runtime_benchmark",
        feature_flag.enabled,
        false,
    )?;
    let release_gates = build_interactive_release_gates(store, "0.5", Some(project_root))?;
    let production_runtime_evidence_plan =
        interactive_release_gate_evidence_plan(store, "0.5", capability_id, Some(project_root))?;
    let multimodal_gate = release_gates
        .gate_cards
        .iter()
        .find(|gate| gate.capability_id == capability_id);
    let promotion_ready = multimodal_gate
        .map(|gate| gate.promotion_ready)
        .unwrap_or(false);
    let required_attached_evidence_kinds = multimodal_gate
        .map(|gate| gate.required_attached_evidence_kinds.clone())
        .unwrap_or_default();
    let attached_evidence_kinds = multimodal_gate
        .map(|gate| gate.attached_evidence_kinds.clone())
        .unwrap_or_default();
    let missing_attached_evidence_kinds = multimodal_gate
        .map(|gate| gate.missing_attached_evidence_kinds.clone())
        .unwrap_or_default();
    let evidence_plan_status = multimodal_gate
        .map(|gate| gate.evidence_plan.status.clone())
        .unwrap_or_else(|| production_runtime_evidence_plan.status.clone());
    let ready_to_collect_evidence = multimodal_gate
        .map(|gate| gate.evidence_plan.ready_to_collect_evidence)
        .unwrap_or(production_runtime_evidence_plan.ready_to_collect_evidence);
    let missing_config_check_count = multimodal_gate
        .map(|gate| gate.evidence_plan.missing_config_check_count)
        .unwrap_or(production_runtime_evidence_plan.missing_config_check_count);
    let config_checks = multimodal_gate
        .map(|gate| gate.evidence_plan.config_checks.clone())
        .unwrap_or_else(|| production_runtime_evidence_plan.config_checks.clone());
    let manifest_template_ids = multimodal_gate
        .map(|gate| gate.evidence_plan.manifest_template_ids.clone())
        .unwrap_or_else(|| {
            production_runtime_evidence_plan
                .manifest_template_ids
                .clone()
        });
    let blockers = multimodal_runtime_evidence_blockers(
        multimodal_gate.is_some(),
        promotion_ready,
        &missing_attached_evidence_kinds,
        &config_checks,
    );
    let next_actions = multimodal_runtime_next_actions(
        promotion_ready,
        &missing_attached_evidence_kinds,
        &config_checks,
    );

    let addon_ready =
        has_addon && has_capability && has_permission && has_view && has_runtime_contract;
    let feature_guard_ready =
        !feature_flag.enabled || feature_flag.approved_by.is_some() || enable_experimental;
    let inventory_ready = status_report.capability_count >= 10
        && !status_report.installs_performed
        && status_report.available_count <= status_report.capability_count;
    let template_ready = install_plan.status == "plan_only"
        && matches!(
            benchmark_template.status.as_str(),
            "benchmark_template_ready" | "plan_only"
        )
        && readiness.schema_version == "forge.multimodal.readiness.v1";
    let guarded_runtime_ready = has_runtime_contract
        && guard.status == "denied"
        && guard.requires_human_approval
        && !guard.allowed;
    let demo_plan_ready = demo_plan.schema_version == "forge.multimodal.demo_plan.v1"
        && !demo_plan.stages.is_empty()
        && demo_plan.requires_human_approval_before_execution;
    let addon_view_ready = has_view && view_action_count >= 2;

    let mut surfaces = vec![
        multimodal_runtime_surface(
            "addon_ownership",
            "Addon ownership boundary",
            if addon_ready { "ready" } else { "missing" },
            addon_ready,
            &["addon_capability_panel", "ui_composition_panel"],
            &[
                "multimodal_behavior_declared_by_addon",
                "capability_multimodal_runtime_registered",
                "runtime_contract_multimodal_runtime_benchmark_declared",
            ],
            &multimodal_missing_addon_blockers(
                has_addon,
                has_capability,
                has_permission,
                has_view,
                has_runtime_contract,
            ),
            &[
                "forge interactive addon-capabilities --output json",
                "forge addons views --addon forge.addon.multimodal --output json",
            ],
        ),
        multimodal_runtime_surface(
            "feature_flag_guard",
            "Feature flag and guard",
            &feature_flag.source,
            feature_guard_ready,
            &["multimodal_status", "multimodal_guard"],
            &[
                "disabled_by_default_or_project_approved",
                "human_opt_in_required",
                "guard_denies_without_explicit_allow",
            ],
            &[],
            &[
                "forge multimodal status --project-root <project-root> --output json",
                "forge multimodal guard --capability image_understanding --action runtime_benchmark --project-root <project-root> --output json",
            ],
        ),
        multimodal_runtime_surface(
            "capability_inventory",
            "Capability inventory",
            &status_report.status,
            inventory_ready,
            &["multimodal_status"],
            &[
                "image_audio_video_3d_capabilities_indexed",
                "provider_and_local_runtime_candidates_visible",
                "model_storage_policy_visible",
            ],
            &[],
            &["forge multimodal status --project-root <project-root> --output json"],
        ),
        multimodal_runtime_surface(
            "benchmark_templates",
            "Readiness, install plan and benchmark templates",
            &benchmark_template.status,
            template_ready,
            &[
                "multimodal_readiness",
                "multimodal_install_plan",
                "multimodal_benchmark_template",
            ],
            &[
                "plan_only_install_manifest_available",
                "benchmark_template_available",
                "readiness_probe_declares_no_model_execution",
            ],
            &[],
            &[
                "forge multimodal install-plan --capability image_understanding --project-root <project-root> --output json",
                "forge multimodal readiness --capability image_understanding --project-root <project-root> --output json",
                "forge multimodal benchmark-template --capability image_understanding --project-root <project-root> --output json",
            ],
        ),
        multimodal_runtime_surface(
            "guarded_runtime_path",
            "Guarded runtime execution path",
            &guard.status,
            guarded_runtime_ready,
            &["multimodal_runtime_benchmark", "addon_runtime_contract"],
            &[
                "runtime_benchmark_requires_opt_in_and_allow_model",
                "addon_runtime_contract_can_dispatch_guarded_benchmark",
                "guard_records_denied_state_without_execution",
            ],
            &[],
            &[
                "forge multimodal runtime-benchmark --capability image_understanding --fixture static_image_labels --project-root <project-root> --approved-by <operator> --confirm-runtime-execution --allow-model --output json",
                "forge addons dispatch-contract --addon forge.addon.multimodal --runtime-contract multimodal_runtime_benchmark.executor --output json",
            ],
        ),
        multimodal_runtime_surface(
            "demo_plans",
            "Safe multimodal demo plans",
            &demo_plan.status,
            demo_plan_ready,
            &["multimodal_demo_plan", "multimodal_demo_receipt"],
            &[
                "local_image_audio_and_blender_demo_plans_available",
                "demo_receipts_require_local_fixture_confirmation",
                "device_access_remains_blocked_without_guard",
            ],
            &[],
            &[
                "forge multimodal demo-plan --demo local_image_recognition --project-root <project-root> --output json",
                "forge multimodal demo-receipt --demo local_image_recognition --fixture static_image_labels --project-root <project-root> --approved-by <operator> --confirm-local-fixture --allow-model --output json",
            ],
        ),
        multimodal_runtime_surface(
            "addon_view_actions",
            "Addon benchmark center actions",
            if addon_view_ready { "ready" } else { "missing" },
            addon_view_ready,
            &["multimodal_benchmark_center", "command_palette_panel"],
            &[
                "addon_view_actions_projected_to_command_palette",
                "permission_and_risk_metadata_preserved",
                "specialized_multimodal_ui_owned_by_addon",
            ],
            &[],
            &[
                "forge addons views --addon forge.addon.multimodal --surface ops_console --output json",
                "forge interactive command-palette --query multimodal --output json",
            ],
        ),
        multimodal_runtime_surface_owned(
            "production_evidence",
            "Production runtime evidence",
            multimodal_gate
                .map(|gate| gate.status.as_str())
                .unwrap_or("missing"),
            promotion_ready,
            vec![
                "release_gates_panel".to_string(),
                "milestone_evidence_plan".to_string(),
            ],
            multimodal_runtime_milestone_evidence_items(
                &required_attached_evidence_kinds,
                &attached_evidence_kinds,
                &missing_attached_evidence_kinds,
                &evidence_plan_status,
                &manifest_template_ids,
            ),
            blockers.clone(),
            vec![
                "forge milestone evidence-plan --version 0.5 --capability experimental_multimodal_runtime --project-root <project-root> --connected-runtime <runtime-id> --output json".to_string(),
                "forge milestone collect-evidence --version 0.5 --capability experimental_multimodal_runtime --project-root <project-root> --connected-runtime <runtime-id> --approved-by <operator> --output json".to_string(),
            ],
        ),
    ];

    for surface in &mut surfaces {
        if !surface.ready && surface.blockers.is_empty() {
            surface.blockers.push(
                "surface evidence is not strong enough for multimodal runtime readiness"
                    .to_string(),
            );
        }
    }

    let ready_surface_count = surfaces.iter().filter(|surface| surface.ready).count();
    let surface_count = surfaces.len();
    let blocked_surface_count = surface_count.saturating_sub(ready_surface_count);
    let readiness_percent = if surface_count == 0 {
        0
    } else {
        ((ready_surface_count * 100) / surface_count) as u64
    };
    let status = if promotion_ready {
        "multimodal_runtime_promotion_ready"
    } else if ready_surface_count + 1 >= surface_count {
        "multimodal_runtime_guarded_ready_with_production_gaps"
    } else {
        "multimodal_runtime_needs_attention"
    };

    Ok(InteractiveMultimodalRuntimePanel {
        schema_version: INTERACTIVE_MULTIMODAL_RUNTIME_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        project_root: project_root_display,
        capability_id: capability_id.to_string(),
        addon_id: addon_id.to_string(),
        addon_view_id: addon_view_id.to_string(),
        feature_flag_enabled: feature_flag.enabled,
        feature_flag_source: feature_flag.source.clone(),
        feature_flag_status: feature_flag.project_config_status.clone(),
        promotion_ready,
        required_attached_evidence_kinds,
        attached_evidence_kinds,
        missing_attached_evidence_kinds,
        evidence_plan_status,
        ready_to_collect_evidence,
        missing_config_check_count,
        config_checks,
        manifest_template_ids,
        production_runtime_evidence_plan,
        installs_performed: status_report.installs_performed
            || install_plan.installs_performed
            || readiness.installs_performed
            || benchmark_template.installs_performed
            || demo_plan.installs_performed,
        model_execution_performed: readiness.model_execution_performed,
        device_access_performed: readiness.device_access_performed
            || benchmark_template.device_access_performed
            || demo_plan.device_access_performed,
        network_access_performed: false,
        capability_count: status_report.capability_count,
        available_count: status_report.available_count,
        missing_count: status_report.missing_count,
        guard_status: guard.status,
        guard_allowed: guard.allowed,
        surface_count,
        ready_surface_count,
        blocked_surface_count,
        readiness_percent,
        surfaces,
        blockers,
        next_actions,
        commands: multimodal_runtime_commands(project_root),
        notes: vec![
            "This panel is read-only; it does not install models, execute models, access devices, access the network or mutate workflows.".to_string(),
            "Specialized multimodal behavior remains Addon-owned; Core only projects Addon contracts and guarded compatibility commands.".to_string(),
        ],
    })
}

fn multimodal_missing_addon_blockers(
    has_addon: bool,
    has_capability: bool,
    has_permission: bool,
    has_view: bool,
    has_runtime_contract: bool,
) -> Vec<&'static str> {
    let mut blockers = Vec::new();
    if !has_addon {
        blockers.push("missing forge.addon.multimodal");
    }
    if !has_capability {
        blockers.push("missing multimodal_runtime capability");
    }
    if !has_permission {
        blockers.push("missing multimodal.runtime_benchmark permission");
    }
    if !has_view {
        blockers.push("missing multimodal.benchmark_center view");
    }
    if !has_runtime_contract {
        blockers.push("missing multimodal_runtime_benchmark.executor contract");
    }
    blockers
}

#[expect(
    clippy::too_many_arguments,
    reason = "static multimodal surface metadata stays compact and readable as fixture data"
)]
fn multimodal_runtime_surface(
    surface_id: &str,
    title: &str,
    status: &str,
    ready: bool,
    source_panels: &[&str],
    evidence: &[&str],
    blockers: &[&str],
    commands: &[&str],
) -> InteractiveMultimodalRuntimeSurface {
    multimodal_runtime_surface_owned(
        surface_id,
        title,
        status,
        ready,
        source_panels
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        evidence.iter().map(|value| (*value).to_string()).collect(),
        blockers.iter().map(|value| (*value).to_string()).collect(),
        commands.iter().map(|value| (*value).to_string()).collect(),
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "owned readiness surface metadata mirrors the static helper while allowing dynamic milestone evidence"
)]
fn multimodal_runtime_surface_owned(
    surface_id: &str,
    title: &str,
    status: &str,
    ready: bool,
    source_panels: Vec<String>,
    evidence: Vec<String>,
    blockers: Vec<String>,
    commands: Vec<String>,
) -> InteractiveMultimodalRuntimeSurface {
    InteractiveMultimodalRuntimeSurface {
        surface_id: surface_id.to_string(),
        title: title.to_string(),
        status: status.to_string(),
        ready,
        source_panels,
        evidence,
        blockers,
        commands,
    }
}

fn multimodal_runtime_milestone_evidence_items(
    required: &[String],
    attached: &[String],
    missing: &[String],
    evidence_plan_status: &str,
    manifest_template_ids: &[String],
) -> Vec<String> {
    let mut evidence = vec![
        "production_runtime_benchmark_required".to_string(),
        "connected_runtime_manifest_template_available".to_string(),
        "promotion_requires_attached_operator_evidence".to_string(),
        format!("evidence_plan_status:{evidence_plan_status}"),
    ];
    evidence.extend(
        required
            .iter()
            .map(|kind| format!("required_evidence:{kind}")),
    );
    evidence.extend(
        attached
            .iter()
            .map(|kind| format!("attached_evidence:{kind}")),
    );
    evidence.extend(
        missing
            .iter()
            .map(|kind| format!("missing_evidence:{kind}")),
    );
    evidence.extend(
        manifest_template_ids
            .iter()
            .map(|template| format!("manifest_template:{template}")),
    );
    evidence
}

fn multimodal_runtime_evidence_blockers(
    gate_present: bool,
    promotion_ready: bool,
    missing: &[String],
    config_checks: &[MilestoneEvidencePlanConfigCheck],
) -> Vec<String> {
    if promotion_ready {
        return Vec::new();
    }
    if !gate_present {
        return vec![
            "experimental_multimodal_runtime release gate is missing from the 0.5 manifest"
                .to_string(),
        ];
    }
    let mut blockers = missing
        .iter()
        .map(|kind| multimodal_runtime_missing_evidence_blocker(kind))
        .collect::<Vec<_>>();
    blockers.extend(
        config_checks
            .iter()
            .filter(|check| matches!(check.status.as_str(), "missing" | "blocked" | "invalid"))
            .map(|check| {
                let path = check
                    .path
                    .as_deref()
                    .map(|path| format!(" at {path}"))
                    .unwrap_or_default();
                format!(
                    "config check {} is {}{}: {}",
                    check.id, check.status, path, check.summary
                )
            }),
    );
    if blockers.is_empty() {
        blockers.push(
            "experimental_multimodal_runtime release gate is not promotable yet; inspect release gates for the current non-evidence blocker"
                .to_string(),
        );
    }
    blockers
}

fn multimodal_runtime_missing_evidence_blocker(kind: &str) -> String {
    match kind {
        "production_runtime_benchmark" => {
            "missing attached evidence kind: production_runtime_benchmark; collect an operator-approved connected runtime benchmark with model guard approval and blocked network/device access"
                .to_string()
        }
        other => format!("missing attached evidence kind: {other}"),
    }
}

fn multimodal_runtime_next_actions(
    promotion_ready: bool,
    missing: &[String],
    config_checks: &[MilestoneEvidencePlanConfigCheck],
) -> Vec<String> {
    if promotion_ready {
        return vec![
            "Inspect forge milestone manifest --version 0.5 before claiming multimodal runtime promotion."
                .to_string(),
        ];
    }
    let mut actions = Vec::new();
    if config_checks.iter().any(|check| {
        check.id == "multimodal_feature_flag"
            && matches!(check.status.as_str(), "missing" | "blocked" | "invalid")
    }) {
        actions.push(
            "Enable the project-scoped .forge/multimodal.json feature flag only after operator approval."
                .to_string(),
        );
    }
    if config_checks.iter().any(|check| {
        check.id == "multimodal_runtime_manifest"
            && matches!(check.status.as_str(), "missing" | "blocked" | "invalid")
    }) {
        actions.push(
            "Prepare .forge/multimodal-runtimes.json with approved connected runtime metadata."
                .to_string(),
        );
    }
    if config_checks.iter().any(|check| {
        check.id == "multimodal_connected_runtime"
            && matches!(check.status.as_str(), "missing" | "blocked" | "invalid")
    }) {
        actions.push(
            "Replace connected runtime placeholders with an approved probe command, model manifest and production evidence metadata."
                .to_string(),
        );
    }
    if missing
        .iter()
        .any(|kind| kind == "production_runtime_benchmark")
    {
        actions.push(
            "Collect production_runtime_benchmark evidence only after opt-in, guard approval and operator approval."
                .to_string(),
        );
    }
    if actions.is_empty() {
        actions.push(
            "Inspect forge interactive release-gates --version 0.5 --output json for the remaining multimodal blocker."
                .to_string(),
        );
    }
    actions
}

fn multimodal_runtime_commands(project_root: &Path) -> InteractiveMultimodalRuntimeCommands {
    let project_root = project_root.display().to_string();
    InteractiveMultimodalRuntimeCommands {
        refresh: vec![
            "interactive".to_string(),
            "multimodal-runtime".to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        status: vec![
            "multimodal".to_string(),
            "status".to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        readiness: vec![
            "multimodal".to_string(),
            "readiness".to_string(),
            "--capability".to_string(),
            "image_understanding".to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        install_plan: vec![
            "multimodal".to_string(),
            "install-plan".to_string(),
            "--capability".to_string(),
            "image_understanding".to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        benchmark_template: vec![
            "multimodal".to_string(),
            "benchmark-template".to_string(),
            "--capability".to_string(),
            "image_understanding".to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        runtime_benchmark: vec![
            "multimodal".to_string(),
            "runtime-benchmark".to_string(),
            "--capability".to_string(),
            "image_understanding".to_string(),
            "--fixture".to_string(),
            "static_image_labels".to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--approved-by".to_string(),
            "<operator>".to_string(),
            "--confirm-runtime-execution".to_string(),
            "--allow-model".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        demo_plan: vec![
            "multimodal".to_string(),
            "demo-plan".to_string(),
            "--demo".to_string(),
            "local_image_recognition".to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        guard: vec![
            "multimodal".to_string(),
            "guard".to_string(),
            "--capability".to_string(),
            "image_understanding".to_string(),
            "--action".to_string(),
            "runtime_benchmark".to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        evidence_plan: vec![
            "milestone".to_string(),
            "evidence-plan".to_string(),
            "--version".to_string(),
            "0.5".to_string(),
            "--capability".to_string(),
            "experimental_multimodal_runtime".to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--connected-runtime".to_string(),
            "<runtime-id>".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        collect_evidence: vec![
            "milestone".to_string(),
            "collect-evidence".to_string(),
            "--version".to_string(),
            "0.5".to_string(),
            "--capability".to_string(),
            "experimental_multimodal_runtime".to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--connected-runtime".to_string(),
            "<runtime-id>".to_string(),
            "--approved-by".to_string(),
            "<operator>".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        addon_capabilities: vec![
            "interactive".to_string(),
            "addon-capabilities".to_string(),
            "--project-root".to_string(),
            project_root,
            "--output".to_string(),
            "json".to_string(),
        ],
    }
}

pub fn build_interactive_token_usage(store: &ForgeStore) -> Result<InteractiveTokenUsagePanel> {
    build_token_usage_panel(store)
}

pub fn build_interactive_artifacts(store: &ForgeStore) -> Result<InteractiveArtifactPanel> {
    let workflows = list_workflows_with_filters(
        store,
        WorkflowRegistryFilters::new(WorkflowLifecycleFilter::All),
    )?;
    build_artifact_panel(store, &workflows.workflows)
}

pub fn build_interactive_workflow_dag(store: &ForgeStore) -> Result<InteractiveWorkflowDagPanel> {
    let workflows = list_workflows_with_filters(
        store,
        WorkflowRegistryFilters::new(WorkflowLifecycleFilter::All),
    )?;
    build_workflow_dag_panel(store, &workflows.workflows)
}

pub fn build_interactive_context_memory(
    store: &ForgeStore,
    project_root: &Path,
) -> Result<InteractiveContextMemoryPanel> {
    let workflows = list_workflows_with_filters(
        store,
        WorkflowRegistryFilters::new(WorkflowLifecycleFilter::All),
    )?;
    Ok(build_context_memory_panel_from_summary(
        store,
        project_root,
        &workflows.summary.context_actions,
        &workflows.summary.context_quality,
    ))
}

pub fn build_interactive_schedules(store: &ForgeStore) -> InteractiveSchedulePanel {
    build_schedule_worker_status(store, "forge-scheduler", 1, 300)
        .map(interactive_schedule_panel_from_worker_status)
        .unwrap_or_else(|_| empty_interactive_schedule_panel())
}

pub fn build_interactive_structured_logs(
    store: &ForgeStore,
) -> Result<InteractiveStructuredLogsPanel> {
    let timeline = build_global_event_timeline(store, None, None, None, None, Some(20), None)?;
    Ok(build_structured_logs_panel(&timeline))
}

fn build_context_memory_panel_from_summary(
    store: &ForgeStore,
    project_root: &Path,
    context_actions: &RegistryContextActionSummary,
    context_quality: &RegistryContextQualitySummary,
) -> InteractiveContextMemoryPanel {
    let memory_policy = memory_policy_report_for_project(store, Some(project_root));
    let temporary_memory_rule = memory_policy
        .interface_policy
        .iter()
        .find(|policy| policy.default_scope == "processing")
        .map(|policy| policy.retention.clone())
        .unwrap_or_else(|| "processing memory is temporary until promoted".to_string());
    let memory_policy_status = memory_policy.status.clone();
    let memory_level_count = memory_policy.memory_levels.len();
    InteractiveContextMemoryPanel {
        schema_version: INTERACTIVE_CONTEXT_MEMORY_SCHEMA_VERSION.to_string(),
        status: "context_memory_ready".to_string(),
        project_root: project_root.display().to_string(),
        ready_for_handoff: context_actions.ready_for_handoff,
        blocked_tasks: context_actions.blocked_tasks,
        context_budget_pressure: context_quality.budget_pressure,
        memory_policy_status,
        memory_level_count,
        temporary_memory_rule,
        memory_policy,
        context_actions: context_actions.clone(),
        context_quality: context_quality.clone(),
        memory_commands: context_memory_memory_commands(project_root),
        context_commands: context_memory_context_commands(project_root),
        next_actions: context_memory_next_actions(project_root),
    }
}

fn build_operating_context_panel(
    store: &ForgeStore,
    project_root: &Path,
    identity: &InteractiveIdentityPanel,
    context_memory: &InteractiveContextMemoryPanel,
) -> Result<InteractiveOperatingContextPanel> {
    let context_report = inspect_project_operating_context(project_root)?;
    let context = &context_report.context;
    let memory_defaults = &context_memory.memory_policy.effective_defaults;
    let required_gates = vec![
        "organization_context_required".to_string(),
        "personality_decision_required".to_string(),
        "company_work_decision_required".to_string(),
    ];
    let prompt_packet_contract = InteractiveOperatingPromptPacketContract {
        schema_version: "forge.interactive.operating_prompt_packet_contract.v1".to_string(),
        status: "prompt_packet_gates_declared".to_string(),
        required_gates: required_gates.clone(),
        organization_context_required: required_gates
            .iter()
            .any(|gate| gate == "organization_context_required"),
        personality_decision_required: required_gates
            .iter()
            .any(|gate| gate == "personality_decision_required"),
        company_work_decision_required: required_gates
            .iter()
            .any(|gate| gate == "company_work_decision_required"),
        evidence_commands: vec![
            "forge context --workflow <workflow-id> --task <task-id> --project-root <project-root> --strict --output json".to_string(),
            "forge task handoff --workflow <workflow-id> --task <task-id> --executor <executor> --project-root <project-root> --output json".to_string(),
        ],
    };
    let company_work_contract = InteractiveOperatingCompanyWorkContract {
        schema_version: "forge.interactive.company_work_contract.v1".to_string(),
        status: "company_work_decision_required".to_string(),
        operating_depth: "compact_multidisciplinary_review".to_string(),
        departments: vec![
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
        sensitive_action_rule: "Public communication, shared memory writes, external broadcasts, financial commitments and customer-impacting actions require explicit governance.".to_string(),
    };
    let identity_summary = InteractiveOperatingIdentitySummary {
        identity_count: identity.identity_count,
        channel_alias_count: identity.channel_alias_count,
        membership_count: identity.membership_count,
        active_membership_count: identity.active_membership_count,
        tenant_audit_missing_count: identity.tenant_audit_missing_count,
    };
    let context_quality_status = if context_memory.context_quality.blocked > 0 {
        "context_quality_blocked"
    } else if context_memory.context_quality.total_warnings > 0 {
        "context_quality_warn"
    } else {
        "context_quality_ready"
    };
    let handoff_context_summary = InteractiveOperatingHandoffContextSummary {
        ready_for_handoff: context_memory.ready_for_handoff,
        blocked_tasks: context_memory.blocked_tasks,
        context_budget_pressure: context_memory.context_budget_pressure,
        context_quality_status: context_quality_status.to_string(),
        required_context_missing: context_memory.context_quality.required_context_missing,
    };
    let tenant_policy_status = operating_context_tenant_policy_status(
        &context.tenant_policy_mode,
        identity.active_membership_count,
    );
    let personality_status = operating_context_personality_status(&context.personality_scope);
    let memory_ready = context_memory.memory_policy_status == "memory_policy_ready";
    let prompt_ready = prompt_packet_contract.organization_context_required
        && prompt_packet_contract.personality_decision_required
        && prompt_packet_contract.company_work_decision_required;
    let status = if !memory_ready || !prompt_ready {
        "operating_context_needs_attention"
    } else if context_report.status == "loaded" || context_report.status == "project_context_loaded"
    {
        "operating_context_ready"
    } else {
        "operating_context_defaulted"
    };
    let tenant_path = format!(
        "{}/{}/{}",
        context.organization.id, context.brand.id, context.product.id
    );
    let prompt_packet_sample =
        operating_context_prompt_packet_sample(store, project_root, context, &tenant_path)?;
    let memory_isolation_evidence = operating_context_memory_isolation_evidence(
        context,
        memory_defaults,
        &context_memory.memory_policy.project_governance.status,
        &tenant_path,
    );

    Ok(InteractiveOperatingContextPanel {
        schema_version: INTERACTIVE_OPERATING_CONTEXT_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        project_root: context_report.project_root,
        context_status: context_report.status,
        tenant_path,
        organization_id: context.organization.id.clone(),
        organization_label: context.organization.label.clone(),
        brand_id: context.brand.id.clone(),
        brand_label: context.brand.label.clone(),
        product_id: context.product.id.clone(),
        product_label: context.product.label.clone(),
        user_id: context.user.id.clone(),
        channel_id: context.channel.id.clone(),
        tenant_policy_mode: context.tenant_policy_mode.clone(),
        tenant_policy_status,
        memory_scope: context.memory_scope.clone(),
        memory_policy_status: context_memory.memory_policy_status.clone(),
        memory_level: memory_defaults.memory_level.as_str().to_string(),
        memory_scopes: memory_defaults.default_scopes.clone(),
        memory_audience: memory_defaults.default_audience.as_str().to_string(),
        memory_governance_status: context_memory
            .memory_policy
            .project_governance
            .status
            .as_str()
            .to_string(),
        personality_scope: context.personality_scope.clone(),
        personality_status,
        brand_voice: context.brand_identity.voice.clone(),
        brand_tone: context.brand_identity.tone.clone(),
        brand_value_count: context.brand_identity.values.len(),
        design_token_source: context.design_system.token_source.clone(),
        component_source: context.design_system.component_source.clone(),
        prompt_packet_contract,
        company_work_contract,
        prompt_packet_sample,
        memory_isolation_evidence,
        identity_summary,
        handoff_context_summary,
        commands: operating_context_commands(project_root),
        next_actions: operating_context_next_actions(
            status,
            &context_report.warnings,
            identity.active_membership_count,
        ),
        notes: vec![
            "This panel is read-only and composes identity, memory policy, personality routing and prompt-packet gates from existing Forge state.".to_string(),
            "Organization context, personality decision and company-work decision are enforced in generated context and handoff packets, not delegated to a brain prompt alone.".to_string(),
        ],
    })
}

fn operating_context_tenant_policy_status(
    tenant_policy_mode: &str,
    active_membership_count: usize,
) -> String {
    if tenant_policy_mode == "enforce" && active_membership_count == 0 {
        "tenant_policy_enforce_needs_active_membership".to_string()
    } else if tenant_policy_mode == "enforce" {
        "tenant_policy_enforced".to_string()
    } else {
        "tenant_policy_audit".to_string()
    }
}

fn operating_context_personality_status(personality_scope: &str) -> String {
    let normalized = personality_scope.trim().to_ascii_lowercase();
    if normalized.contains("workflow") && normalized.contains("node") {
        "workflow_and_node_personality_ready".to_string()
    } else if normalized.contains("workflow") {
        "workflow_personality_ready".to_string()
    } else {
        "personality_scope_limited".to_string()
    }
}

fn operating_context_prompt_packet_sample(
    store: &ForgeStore,
    project_root: &Path,
    context: &OperatingContextSpec,
    tenant_path: &str,
) -> Result<InteractiveOperatingPromptPacketSample> {
    let workflows = store.load_workflows()?;
    let selected = workflows
        .iter()
        .rev()
        .filter(|workflow| {
            operating_context_same_tenant(&workflow.intent.operating_context, context)
        })
        .flat_map(|workflow| {
            workflow
                .tasks
                .iter()
                .map(move |task| (workflow, task, task.persona.is_some()))
        })
        .max_by_key(|(_, _, has_persona)| usize::from(*has_persona));

    let Some((workflow, task, has_persona)) = selected else {
        return Ok(InteractiveOperatingPromptPacketSample {
            schema_version: "forge.interactive.operating_prompt_packet_sample.v1".to_string(),
            status: "missing_workflow_task_sample".to_string(),
            source: "store_workflows".to_string(),
            workflow_id: None,
            task_id: None,
            task_title: None,
            task_executor: None,
            tenant_path: tenant_path.to_string(),
            persona_mode: None,
            persona_profile_id: None,
            selected_voice: None,
            selected_tone: None,
            validation_gates: Vec::new(),
            organization_context_sha256: None,
            personality_decision_sha256: None,
            company_work_decision_sha256: None,
            packet_sha256: None,
            handoff_status: None,
        });
    };

    let package = build_context_package_with_checkpoint_and_project(
        workflow,
        &task.id,
        DEFAULT_CONTEXT_BUDGET,
        None,
        Some(project_root),
    )?;
    let prompt_packet = package.prompt_packet;
    let explicit_persona_suffix = if has_persona {
        "explicit_node_persona"
    } else {
        "brand_default_persona"
    };

    Ok(InteractiveOperatingPromptPacketSample {
        schema_version: "forge.interactive.operating_prompt_packet_sample.v1".to_string(),
        status: format!("prompt_packet_sample_ready_{explicit_persona_suffix}"),
        source: "context_engine".to_string(),
        workflow_id: Some(workflow.id.clone()),
        task_id: Some(task.id.clone()),
        task_title: Some(task.title.clone()),
        task_executor: Some(executor_kind_label(&task.executor).to_string()),
        tenant_path: tenant_path.to_string(),
        persona_mode: prompt_packet.persona_mode,
        persona_profile_id: prompt_packet.persona_profile_id,
        selected_voice: Some(prompt_packet.personality_decision.selected_voice),
        selected_tone: Some(prompt_packet.personality_decision.selected_tone),
        validation_gates: prompt_packet.validation_gates,
        organization_context_sha256: Some(prompt_packet.organization_context_sha256),
        personality_decision_sha256: Some(prompt_packet.personality_decision_sha256),
        company_work_decision_sha256: Some(prompt_packet.company_work_decision_sha256),
        packet_sha256: Some(prompt_packet.packet_sha256),
        handoff_status: Some(prompt_packet.handoff_status),
    })
}

fn operating_context_same_tenant(
    left: &OperatingContextSpec,
    right: &OperatingContextSpec,
) -> bool {
    left.organization.id == right.organization.id
        && left.brand.id == right.brand.id
        && left.product.id == right.product.id
}

fn operating_context_memory_isolation_evidence(
    context: &OperatingContextSpec,
    memory_defaults: &MemoryEffectiveDefaults,
    project_governance_status: &str,
    tenant_path: &str,
) -> InteractiveOperatingMemoryIsolationEvidence {
    let mut isolation_keys = BTreeSet::from([
        format!("organization:{}", context.organization.id),
        format!("brand:{}", context.brand.id),
        format!("product:{}", context.product.id),
        format!("audience:{}", memory_defaults.default_audience),
    ]);
    for scope in &memory_defaults.default_scopes {
        isolation_keys.insert(format!("scope:{scope}"));
    }
    let tenant_scoped = memory_defaults
        .default_scopes
        .iter()
        .any(|scope| scope == "organization")
        && memory_defaults
            .default_scopes
            .iter()
            .any(|scope| scope == "project")
        && project_governance_status == "configured";
    let status = if tenant_scoped {
        "tenant_memory_isolation_ready"
    } else if project_governance_status == "configured" {
        "tenant_memory_isolation_partial"
    } else {
        "tenant_memory_isolation_missing_project_governance"
    };

    InteractiveOperatingMemoryIsolationEvidence {
        schema_version: "forge.interactive.operating_memory_isolation.v1".to_string(),
        status: status.to_string(),
        tenant_path: tenant_path.to_string(),
        organization_id: context.organization.id.clone(),
        brand_id: context.brand.id.clone(),
        product_id: context.product.id.clone(),
        memory_scope: context.memory_scope.clone(),
        allowed_scopes: memory_defaults.default_scopes.clone(),
        default_audience: memory_defaults.default_audience.clone(),
        project_governance_status: project_governance_status.to_string(),
        isolation_keys: isolation_keys.into_iter().collect(),
        governed_search_command: vec![
            "forge".to_string(),
            "memory".to_string(),
            "search".to_string(),
            "--organization".to_string(),
            context.organization.id.clone(),
            "--scope".to_string(),
            "organization".to_string(),
            "--audience".to_string(),
            memory_defaults.default_audience.clone(),
            "--query".to_string(),
            "<query>".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
    }
}

fn operating_context_commands(project_root: &Path) -> InteractiveOperatingContextCommands {
    let project_root = project_root.display().to_string();
    InteractiveOperatingContextCommands {
        refresh: vec![
            "forge".to_string(),
            "interactive".to_string(),
            "operating-context".to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        identity: vec![
            "forge".to_string(),
            "interactive".to_string(),
            "identity".to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        context_memory: vec![
            "forge".to_string(),
            "interactive".to_string(),
            "context-memory".to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        memory_policy: vec![
            "forge".to_string(),
            "memory".to_string(),
            "policy".to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        context_packet: vec![
            "forge".to_string(),
            "context".to_string(),
            "--workflow".to_string(),
            "<workflow-id>".to_string(),
            "--task".to_string(),
            "<task-id>".to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--strict".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        task_handoff: vec![
            "forge".to_string(),
            "task".to_string(),
            "handoff".to_string(),
            "--workflow".to_string(),
            "<workflow-id>".to_string(),
            "--task".to_string(),
            "<task-id>".to_string(),
            "--executor".to_string(),
            "<executor>".to_string(),
            "--project-root".to_string(),
            project_root,
            "--output".to_string(),
            "json".to_string(),
        ],
    }
}

fn operating_context_next_actions(
    status: &str,
    warnings: &[String],
    active_membership_count: usize,
) -> Vec<String> {
    let mut actions = Vec::new();
    if status == "operating_context_defaulted" || !warnings.is_empty() {
        actions.push(
            "Create .forge/operating-context.yaml when this project needs non-default organization, brand, product or policy.".to_string(),
        );
    }
    if active_membership_count == 0 {
        actions.push(
            "Run forge identity sync --project-root <project-root> --output json to materialize the current tenant and active operator membership.".to_string(),
        );
    }
    actions.push(
        "Use forge context --workflow <id> --task <id> --project-root <project-root> --strict --output json before external brain handoff.".to_string(),
    );
    actions
}

fn context_memory_memory_commands(project_root: &Path) -> BTreeMap<String, Vec<String>> {
    let project_root = project_root.display().to_string();
    BTreeMap::from([
        (
            "policy".to_string(),
            vec![
                "memory".to_string(),
                "policy".to_string(),
                "--project-root".to_string(),
                project_root.clone(),
                "--output".to_string(),
                "json".to_string(),
            ],
        ),
        (
            "search".to_string(),
            vec![
                "memory".to_string(),
                "search".to_string(),
                "--workflow".to_string(),
                "<workflow-id>".to_string(),
                "--query".to_string(),
                "<query>".to_string(),
                "--project-root".to_string(),
                project_root.clone(),
                "--output".to_string(),
                "json".to_string(),
            ],
        ),
        (
            "retention".to_string(),
            vec![
                "memory".to_string(),
                "retention".to_string(),
                "--workflow".to_string(),
                "<workflow-id>".to_string(),
                "--scope".to_string(),
                "processing".to_string(),
                "--project-root".to_string(),
                project_root,
                "--output".to_string(),
                "json".to_string(),
            ],
        ),
    ])
}

fn context_memory_context_commands(project_root: &Path) -> BTreeMap<String, Vec<String>> {
    BTreeMap::from([(
        "request".to_string(),
        vec![
            "context".to_string(),
            "--workflow".to_string(),
            "<workflow-id>".to_string(),
            "--task".to_string(),
            "<task-id>".to_string(),
            "--project-root".to_string(),
            project_root.display().to_string(),
            "--budget".to_string(),
            "1200".to_string(),
            "--strict".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
    )])
}

fn context_memory_next_actions(project_root: &Path) -> Vec<String> {
    let project_root = project_root.display();
    vec![
        format!("forge memory policy --project-root {project_root} --output json"),
        format!(
            "forge context --workflow <workflow-id> --task <task-id> --project-root {project_root} --budget 1200 --strict --output json"
        ),
        format!(
            "forge memory search --workflow <workflow-id> --query <query> --project-root {project_root} --output json"
        ),
        "forge interactive context-memory --output json".to_string(),
    ]
}

fn interactive_schedule_panel_from_worker_status(
    report: ScheduleWorkerStatusReport,
) -> InteractiveSchedulePanel {
    let summary = &report.summary;
    let assignment_plan = &report.worker_pool.assignment_plan;
    let assigned_workflows = assignment_plan
        .assigned
        .iter()
        .map(interactive_schedule_assignment)
        .collect::<Vec<_>>();
    let queued_workflows = assignment_plan
        .queued
        .iter()
        .map(interactive_schedule_assignment)
        .collect::<Vec<_>>();
    InteractiveSchedulePanel {
        schema_version: INTERACTIVE_SCHEDULES_SCHEMA_VERSION.to_string(),
        status: report.status.clone(),
        executor: report.executor.clone(),
        observed_at: report.observed_at.clone(),
        ttl_seconds: report.ttl_seconds,
        scanned_workflows: summary.scanned_workflows,
        due_workflows: summary.due_workflows,
        runnable_due_workflows: summary.runnable_due_workflows,
        blocked_due_workflows: summary.blocked_due_workflows,
        idle_workflows: summary.idle_workflows,
        paused_or_stopped_loop_workflows: summary.paused_or_stopped_loop_workflows,
        scheduled_nodes: summary.scheduled_nodes,
        cron_nodes: summary.cron_nodes,
        wait_until_nodes: summary.wait_until_nodes,
        delay_nodes: summary.delay_nodes,
        scale_to_zero_workflows: summary.scale_to_zero_workflows,
        worker_pool: InteractiveScheduleWorkerPool {
            max_workers: report.worker_pool.max_workers,
            available_workers: report.worker_pool.available_workers,
            assignable_due_workflows: report.worker_pool.assignable_due_workflows,
            worker_kind: report.worker_pool.worker_kind.clone(),
            deterministic: report.worker_pool.deterministic,
        },
        assignment_plan: InteractiveScheduleAssignmentPlan {
            schema_version: assignment_plan.schema_version.clone(),
            max_workers: assignment_plan.max_workers,
            assigned_count: assignment_plan.assigned.len(),
            queued_count: assignment_plan.queued.len(),
            deterministic_ordering: assignment_plan.deterministic_ordering,
            ordering_key: assignment_plan.ordering_key.clone(),
        },
        assigned_workflows,
        queued_workflows,
        sleep_until_next_wakeup: report.sleep.sleep_until_next_wakeup,
        next_wakeup_at: report.sleep.next_wakeup_at.clone(),
        sleep_seconds: report.sleep.sleep_seconds,
        sleep_mode: report.sleep.mode.clone(),
        sleep_reason: report.sleep.reason.clone(),
        backpressure_active: report.backpressure.active,
        queued_due_workflows: report.backpressure.queued_due_workflows,
        backpressure_reason: report.backpressure.reason.clone(),
        cancellation_supported: report.cancellation.supported,
        lease_ttl_seconds: report.cancellation.lease_ttl_seconds,
        cancellation_safe_points: report.cancellation.safe_points.clone(),
        workflows: report
            .workflows
            .iter()
            .map(|workflow| InteractiveScheduleWorkflow {
                workflow_id: workflow.workflow_id.clone(),
                goal: workflow.goal.clone(),
                status: workflow.status.clone(),
                due_nodes: workflow.due_nodes,
                next_wakeup_at: workflow.next_wakeup_at.clone(),
                scale_to_zero_eligible: workflow.scale_to_zero_eligible,
                blocked_loop_task_id: workflow.blocked_loop_task_id.clone(),
                blocked_loop_state: workflow.blocked_loop_state.clone(),
            })
            .collect(),
        commands: vec![
            "forge interactive schedules --output json".to_string(),
            "forge schedule worker-status --output json".to_string(),
            "forge schedule scan-due --output json".to_string(),
            "forge schedule list --output json".to_string(),
            "forge interactive structured-logs --output json".to_string(),
        ],
    }
}

fn interactive_schedule_assignment(
    assignment: &crate::schedule::ScheduleWorkerAssignment,
) -> InteractiveScheduleAssignment {
    InteractiveScheduleAssignment {
        workflow_id: assignment.workflow_id.clone(),
        goal: assignment.goal.clone(),
        schedule_task_id: assignment.schedule_task_id.clone(),
        due_nodes: assignment.due_nodes,
        next_run_at: assignment.next_run_at.clone(),
        lease_scope: assignment.lease_scope.clone(),
        wave: assignment.wave,
        queue_position: assignment.queue_position,
        executor: assignment.executor.clone(),
    }
}

fn empty_interactive_schedule_panel() -> InteractiveSchedulePanel {
    InteractiveSchedulePanel {
        schema_version: INTERACTIVE_SCHEDULES_SCHEMA_VERSION.to_string(),
        status: "no_scheduled_workflows".to_string(),
        executor: "forge-scheduler".to_string(),
        observed_at: String::new(),
        ttl_seconds: 300,
        scanned_workflows: 0,
        due_workflows: 0,
        runnable_due_workflows: 0,
        blocked_due_workflows: 0,
        idle_workflows: 0,
        paused_or_stopped_loop_workflows: 0,
        scheduled_nodes: 0,
        cron_nodes: 0,
        wait_until_nodes: 0,
        delay_nodes: 0,
        scale_to_zero_workflows: 0,
        worker_pool: InteractiveScheduleWorkerPool {
            max_workers: 1,
            available_workers: 1,
            assignable_due_workflows: 0,
            worker_kind: "local_scheduler_worker".to_string(),
            deterministic: true,
        },
        assignment_plan: InteractiveScheduleAssignmentPlan {
            schema_version: "forge.schedule.assignment_plan.v1".to_string(),
            max_workers: 1,
            assigned_count: 0,
            queued_count: 0,
            deterministic_ordering: true,
            ordering_key: "workflow_id".to_string(),
        },
        assigned_workflows: Vec::new(),
        queued_workflows: Vec::new(),
        sleep_until_next_wakeup: false,
        next_wakeup_at: None,
        sleep_seconds: 0,
        sleep_mode: "idle".to_string(),
        sleep_reason: "no scheduled workflows".to_string(),
        backpressure_active: false,
        queued_due_workflows: 0,
        backpressure_reason: "no due workflows".to_string(),
        cancellation_supported: true,
        lease_ttl_seconds: 300,
        cancellation_safe_points: vec![
            "before_scan".to_string(),
            "before_lease".to_string(),
            "before_run_due".to_string(),
        ],
        workflows: Vec::new(),
        commands: vec![
            "forge interactive schedules --output json".to_string(),
            "forge schedule worker-status --output json".to_string(),
            "forge schedule scan-due --output json".to_string(),
            "forge schedule list --output json".to_string(),
            "forge interactive structured-logs --output json".to_string(),
        ],
    }
}

pub fn build_interactive_addon_capabilities_default(
    store: &ForgeStore,
) -> InteractiveAddonCapabilityPanel {
    build_interactive_addon_capabilities_for_project(store, None)
}

fn addon_dirs_for_project(project_root: Option<&Path>) -> Vec<PathBuf> {
    project_root
        .map(|root| vec![root.join(".forge/addons")])
        .unwrap_or_else(default_addon_dirs)
}

fn project_root_display(project_root: Option<&Path>) -> String {
    project_root
        .map(|root| root.display().to_string())
        .unwrap_or_else(|| {
            env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .display()
                .to_string()
        })
}

fn project_root_command_arg(project_root: Option<&Path>) -> String {
    project_root
        .map(|root| {
            format!(
                " --project-root {}",
                shell_quote_command_value(&root.display().to_string())
            )
        })
        .unwrap_or_default()
}

fn shell_quote_command_value(value: &str) -> String {
    if value
        .chars()
        .any(|ch| ch.is_whitespace() || matches!(ch, '\'' | '"' | '(' | ')' | '&' | ';' | '|'))
    {
        format!("'{}'", value.replace('\'', "'\"'\"'"))
    } else {
        value.to_string()
    }
}

pub fn build_interactive_addon_capabilities_for_project(
    store: &ForgeStore,
    project_root: Option<&Path>,
) -> InteractiveAddonCapabilityPanel {
    let addon_dirs = addon_dirs_for_project(project_root);
    let catalog = load_addon_catalog_from_store(store, &addon_dirs).ok();
    build_interactive_addon_capabilities_with_project(store, catalog.as_ref(), project_root)
}

pub fn build_interactive_addon_capabilities(
    store: &ForgeStore,
    catalog: Option<&AddonCatalog>,
) -> InteractiveAddonCapabilityPanel {
    build_interactive_addon_capabilities_with_project(store, catalog, None)
}

pub fn build_interactive_addon_capabilities_with_project(
    store: &ForgeStore,
    catalog: Option<&AddonCatalog>,
    project_root: Option<&Path>,
) -> InteractiveAddonCapabilityPanel {
    let project_root_display = project_root_display(project_root);
    let project_root_arg = project_root_command_arg(project_root);
    let capability_index = list_addon_capability_index(store, None, None, None).ok();
    let observability = catalog
        .and_then(|catalog| addon_observability_report(store, catalog, None, None, 1000).ok());
    let event_adapters =
        catalog.map(|catalog| list_addon_event_adapters(catalog, None, None, None));
    let event_extension_registry = event_adapters
        .as_ref()
        .map(|report| report.event_extension_registry.clone())
        .unwrap_or_else(empty_addon_event_extension_registry);
    let event_adapter_count = event_adapters
        .as_ref()
        .map(|report| report.adapter_count)
        .unwrap_or(0);
    let event_extensions =
        render_addon_event_extension_entries(&event_extension_registry, event_adapter_count);
    let indexed_capabilities = capability_index
        .as_ref()
        .map(|index| {
            let mut capabilities = index.capabilities.iter().collect::<Vec<_>>();
            capabilities.sort_by(|left, right| {
                let left_core = left.addon_id == "forge.core.kernel";
                let right_core = right.addon_id == "forge.core.kernel";
                right_core
                    .cmp(&left_core)
                    .then_with(|| {
                        (left.capability_id == "workflow_runtime")
                            .cmp(&(right.capability_id == "workflow_runtime"))
                            .reverse()
                    })
                    .then_with(|| left.addon_id.cmp(&right.addon_id))
                    .then_with(|| left.capability_id.cmp(&right.capability_id))
            });
            capabilities
                .iter()
                .take(12)
                .map(|capability| {
                    format!(
                        "{}:{} [{}]",
                        capability.addon_id, capability.capability_id, capability.lifecycle
                    )
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let catalog_capabilities = observability
        .as_ref()
        .map(|report| {
            report
                .addons
                .iter()
                .filter(|addon| addon.addon_lifecycle == "enabled")
                .flat_map(|addon| {
                    addon
                        .capabilities
                        .iter()
                        .map(|capability| format!("{}:{} [enabled]", addon.addon_id, capability))
                })
                .take(8)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let capabilities = if indexed_capabilities.is_empty() {
        catalog_capabilities
    } else {
        indexed_capabilities
    };
    let status = if capability_index.is_some() || observability.is_some() {
        "addon_capabilities_ready"
    } else {
        "addon_capabilities_unavailable"
    };
    let indexed_capability_count = capability_index
        .as_ref()
        .map(|index| index.capability_count)
        .unwrap_or(0);
    let observed_capability_count = observability
        .as_ref()
        .map(|report| report.totals.capability_count)
        .unwrap_or(0);
    let capability_count = indexed_capability_count.max(observed_capability_count);
    let indexed_enabled_capability_count = capability_index
        .as_ref()
        .map(|index| index.enabled_count)
        .unwrap_or(0);
    let observed_enabled_capability_count = observability
        .as_ref()
        .map(|report| {
            report
                .addons
                .iter()
                .filter(|addon| addon.addon_lifecycle == "enabled")
                .map(|addon| addon.capability_count)
                .sum()
        })
        .unwrap_or(0);
    let enabled_capability_count =
        indexed_enabled_capability_count.max(observed_enabled_capability_count);
    let indexed_disabled_capability_count = capability_index
        .as_ref()
        .map(|index| index.disabled_count)
        .unwrap_or(0);
    let observed_disabled_capability_count = observability
        .as_ref()
        .map(|report| {
            report
                .addons
                .iter()
                .filter(|addon| addon.addon_lifecycle == "disabled")
                .map(|addon| addon.capability_count)
                .sum()
        })
        .unwrap_or(0);
    let disabled_capability_count =
        indexed_disabled_capability_count.max(observed_disabled_capability_count);

    InteractiveAddonCapabilityPanel {
        schema_version: INTERACTIVE_ADDON_CAPABILITY_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        project_root: project_root_display,
        addon_count: observability
            .as_ref()
            .map(|report| report.addon_count)
            .unwrap_or(0),
        enabled_addon_count: observability
            .as_ref()
            .map(|report| report.enabled_count)
            .unwrap_or(0),
        unauthorized_addon_count: observability
            .as_ref()
            .map(|report| report.unauthorized_count)
            .unwrap_or(0),
        capability_count,
        enabled_capability_count,
        disabled_capability_count,
        permission_count: observability
            .as_ref()
            .map(|report| report.totals.permission_count)
            .unwrap_or(0),
        runtime_contract_count: observability
            .as_ref()
            .map(|report| report.totals.runtime_contract_count)
            .unwrap_or(0),
        view_count: observability
            .as_ref()
            .map(|report| report.totals.view_count)
            .unwrap_or(0),
        dispatch_count: observability
            .as_ref()
            .map(|report| report.totals.dispatch_count)
            .unwrap_or(0),
        queued_dispatch_count: observability
            .as_ref()
            .map(|report| report.totals.queued_dispatch_count)
            .unwrap_or(0),
        event_type_count: event_extension_registry.event_type_count,
        event_channel_count: event_extension_registry.channel_count,
        event_trigger_count: event_extension_registry.trigger_count,
        event_listener_count: event_extension_registry.listener_count,
        event_adapter_count,
        event_extensions,
        event_extension_registry,
        capabilities,
        commands: vec![
            "forge addons capabilities --output json".to_string(),
            "forge addons observability --output json".to_string(),
            "forge events adapters --output json".to_string(),
            "forge addons views --surface tui --output json".to_string(),
            format!("forge interactive addon-capabilities{project_root_arg} --output json"),
            "forge interactive action-registry --query addon --output json".to_string(),
        ],
    }
}

pub fn build_interactive_core_boundary(store: &ForgeStore) -> InteractiveCoreBoundaryPanel {
    build_interactive_core_boundary_for_project(store, None)
}

pub fn build_interactive_core_boundary_for_project(
    store: &ForgeStore,
    project_root: Option<&Path>,
) -> InteractiveCoreBoundaryPanel {
    let addon_dirs = addon_dirs_for_project(project_root);
    let catalog = load_addon_catalog_from_store(store, &addon_dirs)
        .unwrap_or_else(|_| builtin_addon_catalog());
    let project_root_display = project_root_display(project_root);
    let project_root_arg = project_root_command_arg(project_root);
    let capability_index = list_addon_capability_index(store, None, None, None).ok();
    let core_addon = catalog
        .addons
        .iter()
        .find(|addon| addon.id == "forge.core.kernel");
    let core_capabilities = core_addon
        .map(|addon| {
            addon
                .capabilities
                .iter()
                .map(|capability| {
                    let domain_specific_id = domain_specific_capability_ids()
                        .contains(capability.id.as_str());
                    let non_core_domain = capability
                        .domains
                        .iter()
                        .any(|domain| domain.as_str() != "core");
                    let boundary_status = if domain_specific_id || non_core_domain {
                        "domain_specific_leak"
                    } else {
                        "core_universal"
                    };
                    let reason = if domain_specific_id {
                        "Capability id belongs to a domain Addon and should not be in Core."
                    } else if non_core_domain {
                        "Capability declares a non-core domain from inside the Core kernel."
                    } else {
                        "Capability is limited to workflow, event, context, identity or governance infrastructure."
                    };
                    InteractiveCoreCapabilityBoundary {
                        capability_id: capability.id.clone(),
                        title: capability.title.clone(),
                        domains: capability.domains.clone(),
                        boundary_status: boundary_status.to_string(),
                        reason: reason.to_string(),
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let domain_specific_core_leak_count = core_capabilities
        .iter()
        .filter(|capability| capability.boundary_status == "domain_specific_leak")
        .count();
    let addon_boundaries = catalog
        .addons
        .iter()
        .filter(|addon| addon.id != "forge.core.kernel")
        .map(interactive_addon_boundary_card)
        .collect::<Vec<_>>();
    let domain_addon_count = addon_boundaries.len();
    let addon_owned_capability_count = addon_boundaries
        .iter()
        .map(|addon| addon.capability_count)
        .sum::<usize>();
    let compatibility_boundaries = catalog
        .addons
        .iter()
        .filter(|addon| addon.id != "forge.core.kernel")
        .flat_map(interactive_compatibility_boundaries)
        .collect::<Vec<_>>();
    let compatibility_boundary_count = compatibility_boundaries.len();
    let event_extension_count = catalog
        .addons
        .iter()
        .filter(|addon| addon.id != "forge.core.kernel")
        .map(|addon| {
            addon.event_types.len()
                + addon.event_channels.len()
                + addon.event_triggers.len()
                + addon.event_listeners.len()
                + addon.event_adapters.len()
        })
        .sum::<usize>();
    let runtime_contract_count = catalog
        .addons
        .iter()
        .map(|addon| addon.runtime_contracts.len())
        .sum::<usize>();
    let addon_view_count = catalog
        .addons
        .iter()
        .filter(|addon| addon.id != "forge.core.kernel")
        .map(|addon| addon.views.len())
        .sum::<usize>();
    let permission_count = catalog
        .addons
        .iter()
        .filter(|addon| addon.id != "forge.core.kernel")
        .map(|addon| addon.permissions.len())
        .sum::<usize>();
    let indexed_capability_count = capability_index
        .as_ref()
        .map(|index| index.capability_count)
        .unwrap_or(0);
    let acceptance_gates = vec![
        core_boundary_gate(
            "core_is_minimal_and_universal",
            "Core kernel only declares universal infrastructure capabilities",
            core_addon.is_some() && domain_specific_core_leak_count == 0,
            format!(
                "{} core capabilities audited; {} domain-specific leaks",
                core_capabilities.len(),
                domain_specific_core_leak_count
            ),
            "forge interactive core-boundary --output json",
        ),
        core_boundary_gate(
            "domain_capabilities_are_addon_owned",
            "Domain capabilities live in Addons instead of Core",
            domain_addon_count > 0 && addon_owned_capability_count > 0,
            format!(
                "{} non-core Addons own {} capabilities",
                domain_addon_count, addon_owned_capability_count
            ),
            "forge interactive addon-capabilities --output json",
        ),
        core_boundary_gate(
            "addon_manifests_are_source_of_truth",
            "Addon manifests describe lifecycle, capabilities and contracts",
            catalog.addon_count > 1 && catalog.capability_count > 0,
            format!(
                "{} Addons and {} capabilities loaded from the catalog",
                catalog.addon_count, catalog.capability_count
            ),
            "forge addons catalog --output json",
        ),
        core_boundary_gate(
            "capability_registry_is_queryable",
            "Capability registry can be queried independently of the TUI",
            indexed_capability_count > 0,
            format!("{indexed_capability_count} capabilities indexed in the store"),
            "forge addons capabilities --output json",
        ),
        core_boundary_gate(
            "ui_composition_is_core_plus_addons",
            "UI composition can render Core widgets and Addon-owned views",
            addon_view_count > 0,
            format!("{addon_view_count} Addon TUI views are available for composition"),
            "forge interactive home --output json",
        ),
        core_boundary_gate(
            "runtime_contracts_route_specific_execution",
            "Domain execution is represented as Addon runtime contracts",
            runtime_contract_count > 0,
            format!(
                "{} runtime contracts; {} compatibility boundaries still visible",
                runtime_contract_count, compatibility_boundary_count
            ),
            "forge addons runtime-contracts --output json",
        ),
        core_boundary_gate(
            "event_extensions_are_addon_owned",
            "Domain events, channels and adapters are attached through Addons",
            event_extension_count > 0,
            format!("{event_extension_count} non-core event extension declarations"),
            "forge events adapters --output json",
        ),
        core_boundary_gate(
            "permissions_lifecycle_observability_are_visible",
            "Security, lifecycle and observability remain inspectable",
            permission_count > 0
                && catalog
                    .addons
                    .iter()
                    .any(|addon| addon.lifecycle == "enabled"),
            format!("{permission_count} non-core permissions with lifecycle metadata"),
            "forge addons observability --output json",
        ),
    ];
    let status = if core_addon.is_none() {
        "core_boundary_missing_core_kernel"
    } else if domain_specific_core_leak_count > 0 {
        "core_boundary_needs_refactor"
    } else {
        "core_boundary_clean"
    };

    InteractiveCoreBoundaryPanel {
        schema_version: INTERACTIVE_CORE_BOUNDARY_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        project_root: project_root_display,
        core_addon_id: core_addon
            .map(|addon| addon.id.clone())
            .unwrap_or_else(|| "missing".to_string()),
        core_capability_count: core_capabilities.len(),
        addon_count: catalog.addon_count,
        domain_addon_count,
        addon_owned_capability_count,
        domain_specific_core_leak_count,
        compatibility_boundary_count,
        core_allowed_responsibilities: core_allowed_responsibilities(),
        core_kernel_capabilities: core_capabilities,
        addon_boundaries,
        compatibility_boundaries,
        acceptance_gates,
        notes: vec![
            "This panel is read-only and does not migrate capabilities automatically.".to_string(),
            "Compatibility executors are allowed only when the capability is still owned by an Addon contract.".to_string(),
            "A clean Core boundary is evidence for goal3, not proof that the whole Forge objective is complete.".to_string(),
        ],
        commands: vec![
            format!("forge interactive core-boundary{project_root_arg} --output json"),
            format!("forge interactive addon-capabilities{project_root_arg} --output json"),
            "forge addons catalog --output json".to_string(),
            "forge addons capabilities --output json".to_string(),
            "forge addons runtime-contracts --output json".to_string(),
            "forge addons observability --output json".to_string(),
            "forge interactive release-gates --output json".to_string(),
        ],
    }
}

fn interactive_addon_boundary_card(addon: &AddonManifest) -> InteractiveAddonBoundaryCard {
    let mut domains = BTreeSet::new();
    for capability in &addon.capabilities {
        for domain in &capability.domains {
            domains.insert(domain.clone());
        }
    }
    let sample_capabilities = addon
        .capabilities
        .iter()
        .take(5)
        .map(|capability| capability.id.clone())
        .collect::<Vec<_>>();
    InteractiveAddonBoundaryCard {
        addon_id: addon.id.clone(),
        source: addon.source.clone(),
        lifecycle: addon.lifecycle.clone(),
        capability_count: addon.capabilities.len(),
        runtime_contract_count: addon.runtime_contracts.len(),
        view_count: addon.views.len(),
        domains: domains.into_iter().collect(),
        sample_capabilities,
        boundary_summary: "addon_owned_domain_surface".to_string(),
    }
}

fn interactive_compatibility_boundaries(
    addon: &AddonManifest,
) -> Vec<InteractiveCompatibilityBoundary> {
    addon
        .runtime_contracts
        .iter()
        .filter(|contract| {
            addon.source.contains("compat")
                || contract.runtime == "forge_core_builtin"
                || contract.entrypoint.starts_with("forge.")
                || contract.entrypoint.starts_with("planner:")
        })
        .map(|contract| InteractiveCompatibilityBoundary {
            addon_id: addon.id.clone(),
            contract_id: contract.id.clone(),
            capability_id: contract.capability_id.clone(),
            compatibility_executor: if contract.runtime.is_empty() {
                contract.entrypoint.clone()
            } else {
                format!("{}:{}", contract.runtime, contract.entrypoint)
            },
            target_boundary: "external_addon_worker_or_governed_runtime_contract".to_string(),
            migration_state: "compatibility_executor_visible_but_addon_owned".to_string(),
        })
        .collect()
}

fn core_boundary_gate(
    gate_id: &str,
    title: &str,
    passed: bool,
    evidence: String,
    evidence_command: &str,
) -> InteractiveCoreBoundaryGate {
    InteractiveCoreBoundaryGate {
        gate_id: gate_id.to_string(),
        title: title.to_string(),
        passed,
        evidence,
        evidence_command: evidence_command.to_string(),
    }
}

fn domain_specific_capability_ids() -> BTreeSet<&'static str> {
    [
        "workflow_automation_research",
        "hackathon_factory",
        "daily_goal_research",
        "visual_workspace",
        "async_runtime",
        "telegram_notification",
        CAP_SOURCE_CODE_PATCH_LIFECYCLE,
        CAP_MULTIMODAL_RUNTIME,
    ]
    .into_iter()
    .collect()
}

fn core_allowed_responsibilities() -> Vec<String> {
    [
        "workflow_runtime",
        "dynamic_workflow_graphs",
        "event_ingress_and_state_transitions",
        "context_routing",
        "memory_governance",
        "identity_and_tenant_routing",
        "personality_routing",
        "human_collaboration_controls",
        "observability_and_cost_policy",
        "addon_registry_and_capability_resolution",
    ]
    .into_iter()
    .map(ToString::to_string)
    .collect()
}

fn empty_addon_event_extension_registry() -> AddonEventExtensionRegistry {
    AddonEventExtensionRegistry {
        schema_version: ADDON_EVENT_EXTENSIONS_SCHEMA_VERSION.to_string(),
        status: "addon_event_extensions_unavailable".to_string(),
        event_type_count: 0,
        trigger_count: 0,
        listener_count: 0,
        channel_count: 0,
        event_types: Vec::new(),
        triggers: Vec::new(),
        listeners: Vec::new(),
        channels: Vec::new(),
    }
}

fn render_addon_event_extension_entries(
    registry: &AddonEventExtensionRegistry,
    adapter_count: usize,
) -> Vec<String> {
    let mut entries = Vec::new();
    entries.push(format!(
        "event types {}, channels {}, triggers {}, listeners {}, adapters {}",
        registry.event_type_count,
        registry.channel_count,
        registry.trigger_count,
        registry.listener_count,
        adapter_count
    ));
    entries.extend(registry.event_types.iter().take(4).map(|event_type| {
        format!(
            "type {} via {} [{}]",
            event_type.event_type.id, event_type.addon_id, event_type.event_type.transport
        )
    }));
    entries.extend(registry.channels.iter().take(4).map(|channel| {
        format!(
            "channel {} via {} [{}:{}]",
            channel.channel.id,
            channel.addon_id,
            channel.channel.transport,
            channel.channel.direction
        )
    }));
    entries.extend(registry.triggers.iter().take(4).map(|trigger| {
        format!(
            "trigger {} -> {} via {}",
            trigger.trigger.id, trigger.trigger.workflow_extension_id, trigger.addon_id
        )
    }));
    entries.extend(registry.listeners.iter().take(4).map(|listener| {
        format!(
            "listener {} -> {} via {}",
            listener.listener.id, listener.listener.handler, listener.addon_id
        )
    }));
    entries
}

fn default_interactive_harness_shim_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".forge/bin")
}

fn readiness_next_actions(
    no_usable_executors: bool,
    needs_executor_approval: bool,
    needs_runtime_approval: bool,
    harness_doctor: &HarnessDoctorReport,
) -> Vec<String> {
    let mut actions = vec![
        "forge sync all --home $HOME --shim-dir $HOME/.forge/bin --allow codex --allow opencode --output json".to_string(),
    ];
    if no_usable_executors || needs_executor_approval {
        actions.push("review executor approvals before handoff".to_string());
    }
    if needs_runtime_approval {
        actions.push("review runtime approvals before async substrate use".to_string());
    }
    if !harness_doctor.forge_first_ready || !harness_doctor.shim_ready {
        actions.push(
            "forge harness adoption-plan --executor codex --shim-dir $HOME/.forge/bin --project-root . --output json"
                .to_string(),
        );
        actions.push(
            "forge harness bootstrap --executor codex --shim-dir $HOME/.forge/bin --project-root . --apply --approved-by <operator> --output json"
                .to_string(),
        );
    }
    actions.push(
        "forge harness doctor --executor codex --shim-dir $HOME/.forge/bin --project-root . --output json"
            .to_string(),
    );
    actions
}

fn interactive_harness_commands(
    executor: &str,
    shim_dir: &Path,
    project_root: &Path,
    context_budget: usize,
    token_headroom: bool,
) -> InteractiveHarnessCommands {
    let shim_dir = shim_dir.display().to_string();
    let project_root = project_root.display().to_string();
    let mut refresh = vec![
        "interactive".to_string(),
        "harness".to_string(),
        "--executor".to_string(),
        executor.to_string(),
        "--shim-dir".to_string(),
        shim_dir.clone(),
        "--project-root".to_string(),
        project_root.clone(),
        "--output".to_string(),
        "json".to_string(),
    ];
    if token_headroom {
        refresh.push("--token-headroom".to_string());
    }
    InteractiveHarnessCommands {
        refresh,
        mode: vec![
            "harness".to_string(),
            "mode".to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        doctor: vec![
            "harness".to_string(),
            "doctor".to_string(),
            "--executor".to_string(),
            executor.to_string(),
            "--shim-dir".to_string(),
            shim_dir.clone(),
            "--project-root".to_string(),
            project_root.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        shim_status: vec![
            "harness".to_string(),
            "shim-status".to_string(),
            "--executor".to_string(),
            executor.to_string(),
            "--shim-dir".to_string(),
            shim_dir.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        wrap_plan: vec![
            "harness".to_string(),
            "wrap-plan".to_string(),
            "--executor".to_string(),
            executor.to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--context-budget".to_string(),
            context_budget.to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        headroom_plan: vec![
            "harness".to_string(),
            "headroom-plan".to_string(),
            "--executor".to_string(),
            executor.to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--context-budget".to_string(),
            context_budget.to_string(),
            if token_headroom {
                "--token-headroom".to_string()
            } else {
                "--no-token-headroom".to_string()
            },
            "--output".to_string(),
            "json".to_string(),
        ],
        headroom_stats: vec![
            "harness".to_string(),
            "headroom-stats".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        lineage_plan: vec![
            "harness".to_string(),
            "adoption-plan".to_string(),
            "--executor".to_string(),
            executor.to_string(),
            "--shim-dir".to_string(),
            shim_dir.clone(),
            "--project-root".to_string(),
            project_root.clone(),
            "--workflow".to_string(),
            "<workflow-id>".to_string(),
            "--task".to_string(),
            "<task-id>".to_string(),
            "--run".to_string(),
            "<run-id>".to_string(),
            "--context-budget".to_string(),
            context_budget.to_string(),
            if token_headroom {
                "--token-headroom".to_string()
            } else {
                "--no-token-headroom".to_string()
            },
            "--output".to_string(),
            "json".to_string(),
        ],
        lineage_exec_dry_run: vec![
            "harness".to_string(),
            "exec".to_string(),
            "--executor".to_string(),
            executor.to_string(),
            "--forge-first".to_string(),
            "--project-root".to_string(),
            project_root.clone(),
            "--workflow".to_string(),
            "<workflow-id>".to_string(),
            "--task".to_string(),
            "<task-id>".to_string(),
            "--run".to_string(),
            "<run-id>".to_string(),
            "--context-budget".to_string(),
            context_budget.to_string(),
            if token_headroom {
                "--token-headroom".to_string()
            } else {
                "--no-token-headroom".to_string()
            },
            "--output".to_string(),
            "json".to_string(),
            "--".to_string(),
            executor.to_string(),
        ],
        adoption_plan: vec![
            "harness".to_string(),
            "adoption-plan".to_string(),
            "--executor".to_string(),
            executor.to_string(),
            "--shim-dir".to_string(),
            shim_dir.clone(),
            "--project-root".to_string(),
            project_root.clone(),
            "--context-budget".to_string(),
            context_budget.to_string(),
            if token_headroom {
                "--token-headroom".to_string()
            } else {
                "--no-token-headroom".to_string()
            },
            "--output".to_string(),
            "json".to_string(),
        ],
        activation_profile: vec![
            "harness".to_string(),
            "activation-profile".to_string(),
            "--executor".to_string(),
            executor.to_string(),
            "--shim-dir".to_string(),
            shim_dir.clone(),
            "--project-root".to_string(),
            project_root.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        bootstrap_project_harness: vec![
            "harness".to_string(),
            "bootstrap".to_string(),
            "--executor".to_string(),
            executor.to_string(),
            "--shim-dir".to_string(),
            shim_dir.clone(),
            "--project-root".to_string(),
            project_root.clone(),
            "--apply".to_string(),
            "--approved-by".to_string(),
            "<operator>".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        install_shims: vec![
            "harness".to_string(),
            "install-shims".to_string(),
            "--executor".to_string(),
            executor.to_string(),
            "--shim-dir".to_string(),
            shim_dir.clone(),
            "--project-root".to_string(),
            project_root.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        exec: vec![
            "harness".to_string(),
            "exec".to_string(),
            "--executor".to_string(),
            executor.to_string(),
            "--project-root".to_string(),
            project_root,
            "--context-budget".to_string(),
            context_budget.to_string(),
            "--output".to_string(),
            "json".to_string(),
            "--".to_string(),
            executor.to_string(),
        ],
        sessions: vec![
            "sessions".to_string(),
            "--provider".to_string(),
            executor.to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        sync: vec![
            "sync".to_string(),
            "executors".to_string(),
            "--shim-dir".to_string(),
            shim_dir,
            "--output".to_string(),
            "json".to_string(),
        ],
    }
}

fn interactive_session_card(session: &BrainSessionState) -> InteractiveSessionCard {
    InteractiveSessionCard {
        session_id: session.session_id.clone(),
        provider_id: session.provider_id.clone(),
        provider_kind: session.provider_kind.clone(),
        readiness: session.readiness.clone(),
        launch_mode: session.launch_mode.clone(),
        forge_first_ready: session.forge_first_ready,
        lifecycle_state: session.lifecycle_state.clone(),
        recorded_plan_count: session.recorded_plan_count,
        lifecycle_event_count: session.lifecycle_event_count,
        last_origin: session.last_origin.clone(),
        last_workflow_id: session.last_workflow_id.clone(),
        last_task_id: session.last_task_id.clone(),
        last_run_id: session.last_run_id.clone(),
        operation_plan: session.operation_plan.clone(),
        commands: InteractiveSessionCardCommands {
            history: vec![
                "sessions".to_string(),
                "history".to_string(),
                "--session".to_string(),
                session.session_id.clone(),
                "--output".to_string(),
                "json".to_string(),
            ],
            lifecycle: vec![
                "sessions".to_string(),
                "lifecycle".to_string(),
                "--session".to_string(),
                session.session_id.clone(),
                "--state".to_string(),
                "<opened|attached|detached|closed|failed|abandoned>".to_string(),
                "--origin".to_string(),
                "forge_cli".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            launch_plan: vec![
                "shells".to_string(),
                "--executor".to_string(),
                session.provider_id.clone(),
                "--output".to_string(),
                "json".to_string(),
            ],
            record_plan: vec![
                "shells".to_string(),
                "--executor".to_string(),
                session.provider_id.clone(),
                "--record-session".to_string(),
                "--origin".to_string(),
                "forge_cli".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
        },
    }
}

fn interactive_sessions_commands(report: &BrainSessionsReport) -> InteractiveSessionsCommands {
    let mut refresh = vec![
        "interactive".to_string(),
        "sessions".to_string(),
        "--output".to_string(),
        "json".to_string(),
    ];
    if let Some(provider_id) = &report.filter.provider_id {
        refresh.push("--provider".to_string());
        refresh.push(provider_id.clone());
    }
    if let Some(lifecycle_state) = &report.filter.lifecycle_state {
        refresh.push("--state".to_string());
        refresh.push(lifecycle_state.clone());
    }
    if let Some(readiness) = &report.filter.readiness {
        refresh.push("--readiness".to_string());
        refresh.push(readiness.clone());
    }
    InteractiveSessionsCommands {
        refresh,
        list: vec![
            "sessions".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        brains: vec![
            "brains".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        shells: vec![
            "shells".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        lifecycle: vec![
            "sessions".to_string(),
            "lifecycle".to_string(),
            "--session".to_string(),
            "<session-id>".to_string(),
            "--state".to_string(),
            "<opened|attached|detached|closed|failed|abandoned>".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
    }
}

fn readiness_commands() -> InteractiveReadinessCommands {
    InteractiveReadinessCommands {
        sync: vec![
            "sync".to_string(),
            "all".to_string(),
            "--home".to_string(),
            "$HOME".to_string(),
            "--shim-dir".to_string(),
            "$HOME/.forge/bin".to_string(),
            "--allow".to_string(),
            "codex".to_string(),
            "--allow".to_string(),
            "opencode".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        brains: vec![
            "brains".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        sessions: vec![
            "sessions".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        shells: vec![
            "shells".to_string(),
            "--executor".to_string(),
            "codex".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        harness_mode: vec![
            "harness".to_string(),
            "mode".to_string(),
            "--project-root".to_string(),
            ".".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        harness_doctor: vec![
            "harness".to_string(),
            "doctor".to_string(),
            "--executor".to_string(),
            "codex".to_string(),
            "--shim-dir".to_string(),
            "$HOME/.forge/bin".to_string(),
            "--project-root".to_string(),
            ".".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        harness_adoption_plan: vec![
            "harness".to_string(),
            "adoption-plan".to_string(),
            "--executor".to_string(),
            "codex".to_string(),
            "--shim-dir".to_string(),
            "$HOME/.forge/bin".to_string(),
            "--project-root".to_string(),
            ".".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        bootstrap_project_harness: vec![
            "harness".to_string(),
            "bootstrap".to_string(),
            "--executor".to_string(),
            "codex".to_string(),
            "--shim-dir".to_string(),
            "$HOME/.forge/bin".to_string(),
            "--project-root".to_string(),
            ".".to_string(),
            "--apply".to_string(),
            "--approved-by".to_string(),
            "<operator>".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        headroom_plan: vec![
            "harness".to_string(),
            "headroom-plan".to_string(),
            "--executor".to_string(),
            "codex".to_string(),
            "--project-root".to_string(),
            ".".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        headroom_stats: vec![
            "harness".to_string(),
            "headroom-stats".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
    }
}

fn release_gate_commands(version: &str) -> InteractiveReleaseGateCommands {
    InteractiveReleaseGateCommands {
        refresh: vec![
            "interactive".to_string(),
            "release-gates".to_string(),
            "--version".to_string(),
            version.to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        status: vec![
            "milestone".to_string(),
            "status".to_string(),
            "--version".to_string(),
            version.to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        manifest: vec![
            "milestone".to_string(),
            "manifest".to_string(),
            "--version".to_string(),
            version.to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        cli_demo: vec![
            "milestone".to_string(),
            "cli-demo".to_string(),
            "--origin".to_string(),
            "codex".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        multimodal_readiness: vec![
            "multimodal".to_string(),
            "readiness".to_string(),
            "--capability".to_string(),
            "image_understanding".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
    }
}

fn release_gate_next_commands(capability_id: &str) -> Vec<String> {
    match capability_id {
        "replacement_grade_cli" => vec![
            "forge milestone prepare-evidence-inputs --version 0.5 --capability replacement_grade_cli --project-root <project-root> --connected-brain <provider-id> --apply --approved-by <operator> --output json".to_string(),
            "forge milestone evidence-plan --version 0.5 --capability replacement_grade_cli --project-root <project-root> --connected-brain <provider-id> --output json".to_string(),
            "forge milestone collect-evidence --version 0.5 --capability replacement_grade_cli --kind external_brain_provider_execution --project-root <project-root> --connected-brain <provider-id> --approved-by <operator> --origin codex --output json".to_string(),
            "forge milestone collect-evidence --version 0.5 --capability replacement_grade_cli --kind broader_project_coding_research_workflow --project-root <project-root> --approved-by <operator> --origin codex --output json".to_string(),
            "forge milestone collect-evidence --version 0.5 --capability replacement_grade_cli --kind terminal_file_editing_ux --project-root <project-root> --approved-by <operator> --origin codex --output json".to_string(),
            "forge milestone cli-demo --origin codex --output json".to_string(),
            "forge milestone attach-evidence --version 0.5 --capability replacement_grade_cli --kind external_brain_provider_execution --summary \"Operator-approved provider receipt.\" --artifact <path> --approved-by <operator> --output json".to_string(),
            "forge interactive replacement-cli --output json".to_string(),
            "forge interactive harness --output json".to_string(),
            "forge interactive patch-workbench --output json".to_string(),
        ],
        "experimental_multimodal_runtime" => vec![
            "forge milestone prepare-evidence-inputs --version 0.5 --capability experimental_multimodal_runtime --project-root <project-root> --connected-runtime <runtime-id> --apply --approved-by <operator> --output json".to_string(),
            "forge milestone evidence-plan --version 0.5 --capability experimental_multimodal_runtime --project-root <project-root> --connected-runtime <runtime-id> --output json".to_string(),
            "forge milestone collect-evidence --version 0.5 --capability experimental_multimodal_runtime --project-root <project-root> --connected-runtime <runtime-id> --approved-by <operator> --output json".to_string(),
            "forge multimodal status --output json".to_string(),
            "forge multimodal readiness --capability image_understanding --output json".to_string(),
            "forge multimodal benchmark-template --capability image_understanding --output json"
                .to_string(),
            "forge milestone attach-evidence --version 0.5 --capability experimental_multimodal_runtime --kind production_runtime_benchmark --summary \"Operator-approved runtime receipt.\" --artifact <path> --approved-by <operator> --output json".to_string(),
        ],
        _ => vec![
            "forge milestone status --version 0.5 --output json".to_string(),
            "forge milestone manifest --version 0.5 --output json".to_string(),
        ],
    }
}

fn release_gate_attached_evidence_state(
    required_kinds: &[String],
    missing_kinds: &[String],
    attached_count: usize,
    promotion_ready: bool,
) -> String {
    if required_kinds.is_empty() {
        "no_required_attached_evidence".to_string()
    } else if missing_kinds.is_empty() && promotion_ready {
        "required_attached_evidence_present".to_string()
    } else if missing_kinds.is_empty() {
        "required_attached_evidence_invalid".to_string()
    } else if attached_count > 0 {
        "partial_required_attached_evidence".to_string()
    } else {
        "required_attached_evidence_missing".to_string()
    }
}

pub fn route_interactive_input(
    store: &ForgeStore,
    input: &str,
    origin: &str,
) -> Result<InteractiveRouteReport> {
    let trimmed = input.trim();
    if is_repl_exit_command(trimmed) {
        return Ok(local_exit_route(trimmed));
    }
    if let Some(pm_goal) = parse_pm_goal(trimmed) {
        return route_pm_workflow(store, pm_goal, origin);
    }
    if trimmed.starts_with('/') {
        return Ok(route_slash_command(trimmed));
    }

    if can_answer_directly(trimmed) {
        return Ok(InteractiveRouteReport {
            status: "routed".to_string(),
            schema_version: INTERACTIVE_ROUTE_SCHEMA_VERSION.to_string(),
            input_kind: "chat".to_string(),
            routing_decision: "direct_answer".to_string(),
            routing_explanation:
                "Simple low-risk request answered from current state without durable execution."
                    .to_string(),
            workflow_created: false,
            run_id: None,
            workflow_id: None,
            answer: Some(
                "Forge can answer this from current runtime state; no workflow was created."
                    .to_string(),
            ),
            slash_command: None,
            product_decision_id: None,
            product_decision_revision: None,
            retention_decision: no_retention_decision(),
        });
    }

    let request = start_async_request(store, trimmed, origin)?;
    let retention_decision = decide_retention(trimmed, true);
    Ok(InteractiveRouteReport {
        status: "routed".to_string(),
        schema_version: INTERACTIVE_ROUTE_SCHEMA_VERSION.to_string(),
        input_kind: "chat".to_string(),
        routing_decision: "new_workflow".to_string(),
        routing_explanation: classify_workflow_reason(trimmed),
        workflow_created: true,
        run_id: Some(request.run_id),
        workflow_id: Some(request.workflow_id),
        answer: None,
        slash_command: None,
        product_decision_id: None,
        product_decision_revision: None,
        retention_decision,
    })
}

fn is_repl_exit_command(input: &str) -> bool {
    matches!(
        input.trim().to_ascii_lowercase().as_str(),
        "q" | "quit" | "exit" | "/quit" | "/exit"
    )
}

fn local_exit_route(input: &str) -> InteractiveRouteReport {
    let normalized = input.trim().to_ascii_lowercase();
    let command_name = if normalized.contains("quit") || normalized == "q" {
        "/quit"
    } else {
        "/exit"
    };

    InteractiveRouteReport {
        status: "routed".to_string(),
        schema_version: INTERACTIVE_ROUTE_SCHEMA_VERSION.to_string(),
        input_kind: "local_command".to_string(),
        routing_decision: "exit_repl".to_string(),
        routing_explanation:
            "Local operator exit command; Forge closes the REPL without creating workflow state."
                .to_string(),
        workflow_created: false,
        run_id: None,
        workflow_id: None,
        answer: Some("goodbye".to_string()),
        slash_command: Some(SlashCommandRoute {
            name: command_name.to_string(),
            recognized: true,
            input_arguments: Vec::new(),
            input_argument_text: String::new(),
            equivalent_command: Vec::new(),
            mutates_workflow: false,
            risk_level: "low".to_string(),
            execution_boundary: "local_repl_exit_not_executed_by_forge".to_string(),
        }),
        product_decision_id: None,
        product_decision_revision: None,
        retention_decision: no_retention_decision(),
    }
}

fn parse_pm_goal(input: &str) -> Option<&str> {
    input
        .trim()
        .strip_prefix("/pm")
        .map(str::trim)
        .filter(|goal| !goal.is_empty())
}

fn route_pm_workflow(
    store: &ForgeStore,
    pm_goal: &str,
    origin: &str,
) -> Result<InteractiveRouteReport> {
    let workflow_goal = format!("Product/PM guided workflow: {pm_goal}");
    let request = start_async_request(store, &workflow_goal, origin)?;
    let decision = record_product_decision(
        store,
        &request.workflow_id,
        ProductDecisionInput {
            title: format!("Product/PM entrypoint decision for {pm_goal}"),
            rationale: "Product/PM mode creates durable workflow state first so product and business outcome, alternatives, trade-offs, success metrics and backlog mutation are auditable before executor work.".to_string(),
            alternatives: vec![
                "answer as transient chat without durable workflow state".to_string(),
                "create a technical workflow without recording product rationale".to_string(),
            ],
            trade_offs: vec![
                "adds one governance revision before execution".to_string(),
                "improves adoption by making PM intent inspectable from the main CLI/TUI entrypoint".to_string(),
            ],
            success_metrics: vec![
                "workflow can be inspected from the interactive dashboard".to_string(),
                "initial product decision is visible in workflow registry and inspect output".to_string(),
                "backlog mutation is recorded before executor handoff".to_string(),
            ],
            backlog_mutation: "prioritize_pm_guided_workflow_creation".to_string(),
            author: origin.to_string(),
            affected_goals: vec![workflow_goal],
            affected_tasks: Vec::new(),
            affected_artifacts: Vec::new(),
            origin: origin.to_string(),
        },
    )?;

    Ok(InteractiveRouteReport {
        status: "routed".to_string(),
        schema_version: INTERACTIVE_ROUTE_SCHEMA_VERSION.to_string(),
        input_kind: "slash_command".to_string(),
        routing_decision: "pm_workflow_created".to_string(),
        routing_explanation: "Product/PM entrypoint created a durable workflow and initial product decision before executor handoff.".to_string(),
        workflow_created: true,
        run_id: Some(request.run_id),
        workflow_id: Some(request.workflow_id),
        answer: None,
        slash_command: Some(SlashCommandRoute {
            name: "/pm".to_string(),
            recognized: true,
            input_arguments: vec![pm_goal.to_string()],
            input_argument_text: pm_goal.to_string(),
            equivalent_command: vec![
                "forge".to_string(),
                "interactive".to_string(),
                "route".to_string(),
                "--input".to_string(),
                format!("/pm {pm_goal}"),
            ],
            mutates_workflow: true,
            risk_level: "medium".to_string(),
            execution_boundary: "workflow_created_not_external_command".to_string(),
        }),
        product_decision_id: Some(decision.decision_id),
        product_decision_revision: Some(decision.revision),
        retention_decision: RetentionDecision {
            schema_version: "forge.interactive.retention_decision.v1".to_string(),
            action: "retain".to_string(),
            reason: "Product/PM workflow contains durable product decision state and should remain inspectable.".to_string(),
            confidence: 0.91,
            requires_human_approval: false,
        },
    })
}

pub fn render_interactive_home(report: &InteractiveHomeReport) -> String {
    let d = &report.dashboard;
    let quick_actions = d.quick_actions.join(" ");
    let next_commands = d.useful_next_commands.join(" | ");
    let forge_controlled_surfaces = if d.forge_controlled_surfaces.is_empty() {
        "none".to_string()
    } else {
        d.forge_controlled_surfaces.join(", ")
    };
    let shell_entrypoints = if d.shell_entrypoints.is_empty() {
        "none".to_string()
    } else {
        d.shell_entrypoints.join(" | ")
    };
    let attention_actions = if d.attention_actions.is_empty() {
        "none".to_string()
    } else {
        d.attention_actions.join(" | ")
    };
    let harness_doctor_checks = if d.harness_doctor_panel.readiness_checks.is_empty() {
        "none".to_string()
    } else {
        d.harness_doctor_panel.readiness_checks.join(", ")
    };
    let token_usage_sources = render_token_usage_source_summary(&d.token_usage_panel);
    let token_usage_retrieve = render_token_usage_retrieve_summary(&d.token_usage_panel);
    let workflow_focus = if d.workflow_focus.is_empty() {
        "none".to_string()
    } else {
        d.workflow_focus
            .iter()
            .map(|workflow| {
                format!(
                    "{} [{}] {} / {} / {}",
                    workflow.workflow_id,
                    workflow.lifecycle_state,
                    workflow.operator_action,
                    workflow.goal,
                    workflow.tasks
                )
            })
            .collect::<Vec<_>>()
            .join(" | ")
    };
    let workflow_sidebar = render_workflow_sidebar_summary(&d.workflow_sidebar_panel);
    let guided_cockpit_steps = render_guided_cockpit_step_summary(&d.guided_cockpit_panel);
    let guided_cockpit_panes = render_guided_cockpit_pane_summary(&d.guided_cockpit_panel);
    let guided_cockpit_policy = render_guided_cockpit_policy_summary(&d.guided_cockpit_panel);
    let guided_cockpit_next = if d.guided_cockpit_panel.next_commands.is_empty() {
        "none".to_string()
    } else {
        d.guided_cockpit_panel.next_commands.join(" | ")
    };
    let replacement_cli_surfaces = render_replacement_cli_surface_summary(&d.replacement_cli_panel);
    let multimodal_runtime_surfaces =
        render_multimodal_runtime_surface_summary(&d.multimodal_runtime_panel);
    let navigation_keys = render_navigation_keybindings(&d.navigation_panel);
    let command_palette_entries = render_command_palette_entry_summary(&d.command_palette_panel);
    let autocomplete_suggestions = render_autocomplete_suggestion_summary(&d.autocomplete_panel);
    let ui_composition_regions = render_ui_composition_region_summary(&d.ui_composition_panel);
    let session_cards = render_session_card_summary(&d.sessions_panel);
    let patch_workbench_files = render_patch_workbench_file_summary(&d.patch_workbench_panel);
    let permission_memberships = render_permission_membership_summary(&d.permissions_panel);
    let permission_approvals = render_permission_approval_summary(&d.permissions_panel);
    let identity_aliases = render_identity_alias_summary(&d.identity_panel);
    let identity_memberships = render_identity_membership_summary(&d.identity_panel);
    let identity_context = format!(
        "{}/{}/{} user {} channel {}",
        d.identity_panel.current_context.organization_id,
        d.identity_panel.current_context.brand_id,
        d.identity_panel.current_context.product_id,
        d.identity_panel.current_context.user_id,
        d.identity_panel.current_context.channel_id
    );
    let operating_context_gates = d
        .operating_context_panel
        .prompt_packet_contract
        .required_gates
        .join("+");
    let task_board_lanes = render_task_board_lane_summary(&d.task_board_panel);
    let workflow_mutation_cards = render_workflow_mutation_card_summary(&d.workflow_mutation_panel);
    let workflow_mutation_proposals =
        render_workflow_mutation_proposal_summary(&d.workflow_mutation_panel);
    let artifact_workflows = render_artifact_workflow_summary(&d.artifact_panel);
    let artifact_entries = render_artifact_entry_summary(&d.artifact_panel);
    let dag_workflows = render_workflow_dag_summary(&d.dag_panel);
    let digital_twin_workflows = if d.digital_twin_panel.workflows.is_empty() {
        "none".to_string()
    } else {
        d.digital_twin_panel
            .workflows
            .iter()
            .take(5)
            .map(|workflow| {
                format!(
                    "{} {} done {}, remaining {}, approvals {}",
                    workflow.workflow_id,
                    workflow.live_state.what_is_happening,
                    workflow.counts.done_count,
                    workflow.counts.remaining_count,
                    workflow.counts.awaiting_approval_count
                )
            })
            .collect::<Vec<_>>()
            .join(" | ")
    };
    let latest_events = if d.event_panel.latest_events.is_empty() {
        "none".to_string()
    } else {
        d.event_panel.latest_events.join(" | ")
    };
    let event_runtime_summary = render_event_runtime_panel(&d.event_runtime_panel);
    let structured_logs = render_structured_log_summary(&d.structured_logs_panel);
    let improvement_candidates = render_improvement_candidate_summary(&d.improvement_loop_panel);
    let operational_cockpit_sections =
        render_operational_cockpit_sections(&d.operational_cockpit_panel);
    let architecture_tracks = render_architecture_track_summary(&d.architecture_compass_panel);
    let architecture_benchmarks =
        render_architecture_benchmark_summary(&d.architecture_compass_panel);
    let architecture_execution_plan =
        render_architecture_execution_plan_summary(&d.architecture_compass_panel);
    let core_boundary = render_core_boundary_summary(&d.core_boundary_panel);
    let addon_capabilities = render_addon_capability_summary(&d.addon_capability_panel);
    let addon_event_extensions = render_addon_event_extension_summary(&d.addon_capability_panel);
    let addon_renderer_families = if d.addon_renderer_panel.families.is_empty() {
        "none".to_string()
    } else {
        d.addon_renderer_panel.families.join(", ")
    };
    let run_ids_line = if d.active_run_ids.is_empty() {
        String::new()
    } else {
        format!("Active run IDs: {}\n", d.active_run_ids.join(", "))
    };
    format!(
        "{mark}\n{name}\n\n\
         Forge operational TUI\n\
         Guided cockpit: {guided_cockpit_status}; visual {guided_cockpit_visual}; steps {guided_completed}/{guided_total}; current {guided_current}; blocked {guided_blocked}; confirmations {guided_confirmations}; panes {guided_panes}; next {guided_next}\n\
         Guided steps: {guided_steps}\n\
         Safe actions: {guided_policy}\n\
         Active workflows: {active_workflows}; active runs {active_runs}; focus {workflow_focus}\n\
         Events/schedules: events {event_total}, visible {event_visible}, scheduled {scheduled_workflows}, due {schedule_due}, next {schedule_next}\n\
         Addons/capabilities: addons {addon_count}, enabled {addon_enabled}, capabilities {addon_capabilities_count}, permissions {addon_permissions}, contracts {addon_contracts}, event types {addon_event_types}, triggers {addon_event_triggers}, listeners {addon_event_listeners}, adapters {addon_event_adapters}; {addon_capabilities}; events {addon_event_extensions}\n\
         Costs: estimated ${cost_estimated:.4}, observed ${cost_observed:.4}, nodes {cost_nodes}, AI {cost_ai_nodes}, deterministic {cost_deterministic_nodes}, avoided-model {cost_avoided_nodes}\n\
         Improvement loop: {improvement_status}; candidates {improvement_candidates_count}, critical {improvement_critical}, high {improvement_high}, parallel {improvement_parallel}, avoidable AI {improvement_avoidable_ai}, final outcome {improvement_final_outcome}, stale/attention {improvement_stale}, validation failures {improvement_validation_failures}, context {improvement_context}; top {improvement_candidates}\n\
         Handoffs/approvals: ready handoffs {task_board_ready_handoffs}, human waits {task_board_human_waits}, pending approvals {pending_approvals}, context blocked {context_blocked}\n\
         Workflow mutation/replanning: {workflow_mutation_status}; workflows {workflow_mutation_workflows}, active {workflow_mutation_active}, mutable {workflow_mutation_mutable}, proposals {workflow_mutation_pending}/{workflow_mutation_applied}, checkpoints {workflow_mutation_checkpoints}; cards {workflow_mutation_cards}\n\
         Architecture compass: {architecture_status}; tracks {architecture_track_count}, docs {architecture_doc_count}; {architecture_tracks}\n\
         Core boundary: {core_boundary}\n\
         Architecture execution plan: {architecture_execution_plan}\n\
         Smoke test: forge smoke operational-tui --output json\n\n\
         Active runs: {active_runs}\n\
         {run_ids_line}\
         Operational cockpit: {cockpit_attention}; {cockpit_priority}; active work {cockpit_active_work}, ready handoffs {cockpit_ready_handoffs}, human waits {cockpit_human_waits}, due workflows {cockpit_due_workflows}, brain {cockpit_selected_brain}; sections {cockpit_sections}\n\
         Architecture compass: {architecture_status}; benchmarks {architecture_benchmarks}\n\
         Core boundary: {core_boundary}\n\
         Architecture execution plan: {architecture_execution_plan}\n\
         Runs needing attention: {runs_needing_attention}\n\
         Scheduled workflows: {scheduled_workflows}\n\
         Looping workflows: {looping_workflows}\n\
         Paused/idle workflows: {paused_idle_workflows}\n\
         Recent artifacts: {recent_artifacts}\n\
         Product decisions: {product_decisions}\n\
         Pending approvals: {pending_approvals}\n\
         Validation failures: {validation_failures}\n\
         Executor availability: {executor_availability}\n\
         Brain router: {brain_router}\n\
         Forge-controlled surfaces: {forge_controlled_surfaces}\n\
         Shell entrypoints: {shell_entrypoints}\n\
         Harness center: {harness_center_status}; executor {harness_center_executor}; forge-first {harness_center_forge_first}; token headroom {harness_center_token_headroom}; shim {harness_center_shim}; action {harness_center_command}\n\
         Token usage panel: {token_usage_status}; blobs {token_usage_blobs}, original tokens {token_usage_original_tokens}, compressed tokens {token_usage_compressed_tokens}, saved tokens {token_usage_saved_tokens}, savings {token_usage_savings:.2}%, over budget {token_usage_over_budget}; primary {token_usage_primary_source}/{token_usage_primary_kind}; sources {token_usage_sources}; retrieve {token_usage_retrieve}\n\
         Session center: {sessions_status}; sessions {sessions_count}, ready {sessions_ready}, planned events {sessions_planned_events}, lifecycle events {sessions_lifecycle_events}; {session_cards}\n\
         Harness mode: {harness_effective_mode} from {harness_source}; project config {harness_project_status}; audit {harness_audit_command}\n\
         Harness doctor: {harness_doctor_status} for {harness_doctor_executor}; shim {harness_doctor_shim_dir}; checks {harness_doctor_checks}; audit {harness_doctor_command}\n\
         Runtime/node status: {runtime_node_status}\n\
         Scheduler worker status: {scheduler_worker_status}\n\
         Workflow sidebar: {workflow_sidebar}\n\
         Replacement CLI: {replacement_cli_status}; readiness {replacement_cli_readiness}% ({replacement_cli_ready}/{replacement_cli_surfaces_count}); promotion_ready {replacement_cli_promotion_ready}; surfaces {replacement_cli_surfaces}\n\
         Multimodal runtime: {multimodal_runtime_status}; readiness {multimodal_runtime_readiness}% ({multimodal_runtime_ready}/{multimodal_runtime_surfaces_count}); addon {multimodal_runtime_addon}; view {multimodal_runtime_view}; feature {multimodal_runtime_feature_enabled} from {multimodal_runtime_feature_source}; promotion_ready {multimodal_runtime_promotion_ready}; surfaces {multimodal_runtime_surfaces}\n\
         Workflow focus: {workflow_focus}\n\
         Navigation panel: {navigation_status}; default {navigation_default_mode}, theme {navigation_theme}, modes {navigation_modes}, keys {navigation_keys}\n\
         Command palette: {command_palette_status}; query {command_palette_query}, groups {command_palette_groups}, entries {command_palette_entry_count}; {command_palette_entries}\n\
         Autocomplete: {autocomplete_status}; input {autocomplete_input}, suggestions {autocomplete_suggestion_count}; {autocomplete_suggestions}\n\
         UI composition: {ui_composition_status}; layout {ui_composition_layout}, regions {ui_composition_regions_count}, widgets {ui_composition_widgets} ({ui_composition_core_widgets} core, {ui_composition_addon_widgets} addon); {ui_composition_regions}\n\
         Patch workbench: {patch_workbench_status}; clean {patch_workbench_clean}, files {patch_workbench_files_count}, staged {patch_workbench_staged}, unstaged {patch_workbench_unstaged}, untracked {patch_workbench_untracked}, diff {patch_workbench_diff_present}, check {patch_workbench_diff_check}; {patch_workbench_files}\n\
         Permission center: {permissions_status}; memberships {permissions_memberships}, active {permissions_active}, addon permissions {permissions_addons}, approved {permissions_approved_addons}, pending approvals {permissions_pending}, timed out {permissions_timed_out}; memberships {permission_memberships}; approvals {permission_approvals}\n\
         Identity center: {identity_status}; context {identity_context}, identities {identity_count}, aliases {identity_alias_count}, memberships {identity_membership_count}, tenant audit missing {identity_tenant_missing}; aliases {identity_aliases}; memberships {identity_memberships}\n\
         Operating context: {operating_context_status}; tenant {operating_tenant_path}, policy {operating_tenant_policy_status}, memory {operating_memory_level}/{operating_memory_scopes}, audience {operating_memory_audience}, personality {operating_personality_status}, gates {operating_prompt_gates}\n\
         Operational digital twin: {digital_twin_status}; workflows {digital_twin_workflows_count}, happening {digital_twin_happening}, done {digital_twin_done}, remaining {digital_twin_remaining}, validated {digital_twin_validated}, rejected {digital_twin_rejected}, approvals {digital_twin_approvals}; {digital_twin_workflows}\n\
         DAG panel: {dag_status}; workflows {dag_workflows_count}, nodes {dag_nodes}, edges {dag_edges}, running {dag_running}, blocked {dag_blocked}, waits {dag_waits}, human waits {dag_human_waits}; {dag_workflows}\n\
         Task board: {task_board_status}; workflows {task_board_workflows}, tasks {task_board_tasks}, ready handoffs {task_board_ready_handoffs}, human waits {task_board_human_waits}, checkpoints {task_board_checkpoints}, artifacts {task_board_artifacts}; lanes {task_board_lanes}\n\
         Workflow mutation/replanning: {workflow_mutation_status}; mode {workflow_mutation_mode}, workflows {workflow_mutation_workflows}, tasks {workflow_mutation_tasks}, ready handoffs {workflow_mutation_ready_handoffs}, human waits {workflow_mutation_human_waits}, proposals {workflow_mutation_pending}/{workflow_mutation_applied}, events {workflow_mutation_events}, cost ${workflow_mutation_cost:.4}; cards {workflow_mutation_cards}; proposals {workflow_mutation_proposals}; next {workflow_mutation_next}\n\
         Artifact panel: {artifact_status}; workflows {artifact_workflows_count}, artifacts {artifact_count}, bytes {artifact_bytes}; workflows {artifact_workflows}; entries {artifact_entries}\n\
         Schedule panel: {schedule_status}; due {schedule_due}, runnable {schedule_runnable}, cron {schedule_cron}, wait_until {schedule_wait_until}, next {schedule_next}\n\
         Event timeline: {event_status}; visible {event_visible}/{event_total}; latest {latest_events}\n\
         Event runtime: {event_runtime_summary}\n\
         Structured logs: {structured_logs_status}; logs {structured_logs_count}/{structured_logs_total}, next cursor {structured_logs_next_cursor}, has more {structured_logs_has_more}; {structured_logs}\n\
         Cost panel: {cost_status}; workflows {cost_workflows}, nodes {cost_nodes}, estimated ${cost_estimated:.4}, observed ${cost_observed:.4}\n\
         Improvement loop: {improvement_status}; workflows {improvement_total_workflows}, matched {improvement_matched_workflows}, candidates {improvement_candidates_count}, critical {improvement_critical}, high {improvement_high}, parallel {improvement_parallel}, avoidable AI {improvement_avoidable_ai}, final outcome {improvement_final_outcome}, stale/attention {improvement_stale}; top {improvement_candidates}\n\
         Context/memory panel: ready {context_ready}, blocked {context_blocked}, budget pressure {context_budget_pressure}, memory {memory_policy_status}\n\
         Core boundary: {core_boundary}\n\
         Addons/capabilities: {addon_capability_status}; addons {addon_count}, enabled {addon_enabled}, capabilities {addon_capabilities_count}, enabled capabilities {addon_enabled_capabilities}, disabled capabilities {addon_disabled_capabilities}, permissions {addon_permissions}, runtime contracts {addon_contracts}, views {addon_views}, dispatches {addon_dispatches}, queued {addon_queued_dispatches}, event types {addon_event_types}, channels {addon_event_channels}, triggers {addon_event_triggers}, listeners {addon_event_listeners}, adapters {addon_event_adapters}; {addon_capabilities}; events {addon_event_extensions}\n\
         Addon UI renderers: {addon_renderer_status}; safe {addon_safe_renderers}/{addon_renderers}, families {addon_renderer_family_count} ({addon_renderer_families})\n\
         Repository context: {repository_context}\n\
         Estimated costs: {estimated_costs}\n\
         Attention actions: {attention_actions}\n\
         Quick actions: {quick_actions}\n\
         Useful next commands: {next_commands}\n",
        mark = report.banner.mark,
        name = report.banner.name,
        guided_cockpit_status = d.guided_cockpit_panel.status,
        guided_cockpit_visual = d.guided_cockpit_panel.visual_mode,
        guided_completed = d.guided_cockpit_panel.completed_step_count,
        guided_total = d.guided_cockpit_panel.total_step_count,
        guided_current = d.guided_cockpit_panel.current_step_id,
        guided_blocked = d.guided_cockpit_panel.blocked_step_count,
        guided_confirmations = d.guided_cockpit_panel.confirmation_step_count,
        guided_panes = guided_cockpit_panes,
        guided_next = guided_cockpit_next,
        guided_steps = guided_cockpit_steps,
        guided_policy = guided_cockpit_policy,
        active_runs = d.active_runs,
        run_ids_line = run_ids_line,
        cockpit_attention = d.operational_cockpit_panel.attention_level,
        cockpit_priority = d.operational_cockpit_panel.priority_summary,
        cockpit_active_work = d.operational_cockpit_panel.active_work_count,
        cockpit_ready_handoffs = d.operational_cockpit_panel.ready_handoff_count,
        cockpit_human_waits = d.operational_cockpit_panel.pending_human_wait_count,
        cockpit_due_workflows = d.operational_cockpit_panel.due_workflow_count,
        cockpit_selected_brain = d.operational_cockpit_panel.selected_brain,
        cockpit_sections = operational_cockpit_sections,
        architecture_status = d.architecture_compass_panel.status,
        architecture_track_count = d.architecture_compass_panel.tracks.len(),
        architecture_doc_count = d.architecture_compass_panel.source_documents.len(),
        architecture_tracks = architecture_tracks,
        architecture_benchmarks = architecture_benchmarks,
        architecture_execution_plan = architecture_execution_plan,
        core_boundary = core_boundary,
        runs_needing_attention = d.runs_needing_attention,
        scheduled_workflows = d.scheduled_workflows,
        looping_workflows = d.looping_workflows,
        paused_idle_workflows = d.paused_idle_workflows,
        recent_artifacts = d.recent_artifacts,
        product_decisions = d.product_decisions,
        pending_approvals = d.pending_approvals,
        validation_failures = d.validation_failures,
        executor_availability = d.executor_availability,
        brain_router = d.brain_router,
        forge_controlled_surfaces = forge_controlled_surfaces,
        shell_entrypoints = shell_entrypoints,
        harness_center_status = d.harness_panel.status,
        harness_center_executor = d.harness_panel.executor,
        harness_center_forge_first = d.harness_panel.forge_first_ready,
        harness_center_token_headroom = d.harness_panel.token_headroom_ready,
        harness_center_shim = d.harness_panel.shim_status.status,
        harness_center_command = "forge interactive harness --output json",
        token_usage_status = d.token_usage_panel.status,
        token_usage_blobs = d.token_usage_panel.total_headroom_blobs,
        token_usage_original_tokens = d.token_usage_panel.total_original_tokens,
        token_usage_compressed_tokens = d.token_usage_panel.total_compressed_tokens,
        token_usage_saved_tokens = d.token_usage_panel.estimated_saved_tokens,
        token_usage_savings = d.token_usage_panel.average_savings_percent,
        token_usage_over_budget = d.token_usage_panel.over_budget_after_headroom_count,
        token_usage_primary_source = d.token_usage_panel.primary_source,
        token_usage_primary_kind = d.token_usage_panel.primary_content_kind,
        token_usage_sources = token_usage_sources,
        token_usage_retrieve = token_usage_retrieve,
        sessions_status = d.sessions_panel.status,
        sessions_count = d.sessions_panel.session_count,
        sessions_ready = d.sessions_panel.ready_session_count,
        sessions_planned_events = d.sessions_panel.planned_event_count,
        sessions_lifecycle_events = d.sessions_panel.lifecycle_event_count,
        session_cards = session_cards,
        harness_effective_mode = d.harness_mode_panel.effective_mode,
        harness_source = d.harness_mode_panel.forge_first_source,
        harness_project_status = d.harness_mode_panel.project_config_status,
        harness_audit_command = "forge harness mode --output json",
        harness_doctor_status = d.harness_doctor_panel.status,
        harness_doctor_executor = d.harness_doctor_panel.executor,
        harness_doctor_shim_dir = d.harness_doctor_panel.shim_dir,
        harness_doctor_checks = harness_doctor_checks,
        harness_doctor_command = "forge harness doctor --executor codex --shim-dir $HOME/.forge/bin --project-root . --output json",
        runtime_node_status = d.runtime_node_status,
        scheduler_worker_status = d.scheduler_worker_status,
        workflow_sidebar = workflow_sidebar,
        replacement_cli_status = d.replacement_cli_panel.status,
        replacement_cli_readiness = d.replacement_cli_panel.readiness_percent,
        replacement_cli_ready = d.replacement_cli_panel.ready_surface_count,
        replacement_cli_surfaces_count = d.replacement_cli_panel.surface_count,
        replacement_cli_promotion_ready = d.replacement_cli_panel.promotion_ready,
        replacement_cli_surfaces = replacement_cli_surfaces,
        multimodal_runtime_status = d.multimodal_runtime_panel.status,
        multimodal_runtime_readiness = d.multimodal_runtime_panel.readiness_percent,
        multimodal_runtime_ready = d.multimodal_runtime_panel.ready_surface_count,
        multimodal_runtime_surfaces_count = d.multimodal_runtime_panel.surface_count,
        multimodal_runtime_addon = d.multimodal_runtime_panel.addon_id,
        multimodal_runtime_view = d.multimodal_runtime_panel.addon_view_id,
        multimodal_runtime_feature_enabled = d.multimodal_runtime_panel.feature_flag_enabled,
        multimodal_runtime_feature_source = d.multimodal_runtime_panel.feature_flag_source,
        multimodal_runtime_promotion_ready = d.multimodal_runtime_panel.promotion_ready,
        multimodal_runtime_surfaces = multimodal_runtime_surfaces,
        workflow_focus = workflow_focus,
        navigation_status = d.navigation_panel.status,
        navigation_default_mode = d.navigation_panel.default_display_mode,
        navigation_theme = d.navigation_panel.active_theme,
        navigation_modes = d.navigation_panel.display_modes.join(", "),
        navigation_keys = navigation_keys,
        command_palette_status = d.command_palette_panel.status,
        command_palette_query = if d.command_palette_panel.query.is_empty() {
            "none"
        } else {
            d.command_palette_panel.query.as_str()
        },
        command_palette_groups = d.command_palette_panel.group_count,
        command_palette_entry_count = d.command_palette_panel.entry_count,
        command_palette_entries = command_palette_entries,
        autocomplete_status = d.autocomplete_panel.status,
        autocomplete_input = if d.autocomplete_panel.input.is_empty() {
            "none"
        } else {
            d.autocomplete_panel.input.as_str()
        },
        autocomplete_suggestion_count = d.autocomplete_panel.suggestion_count,
        autocomplete_suggestions = autocomplete_suggestions,
        ui_composition_status = d.ui_composition_panel.status,
        ui_composition_layout = d.ui_composition_panel.layout_kind,
        ui_composition_regions_count = d.ui_composition_panel.region_count,
        ui_composition_widgets = d.ui_composition_panel.widget_count,
        ui_composition_core_widgets = d.ui_composition_panel.core_widget_count,
        ui_composition_addon_widgets = d.ui_composition_panel.addon_widget_count,
        ui_composition_regions = ui_composition_regions,
        patch_workbench_status = d.patch_workbench_panel.status,
        patch_workbench_clean = d.patch_workbench_panel.clean,
        patch_workbench_files_count = d.patch_workbench_panel.changed_path_count,
        patch_workbench_staged = d.patch_workbench_panel.staged_path_count,
        patch_workbench_unstaged = d.patch_workbench_panel.unstaged_path_count,
        patch_workbench_untracked = d.patch_workbench_panel.untracked_path_count,
        patch_workbench_diff_present = d.patch_workbench_panel.diff_present,
        patch_workbench_diff_check = d.patch_workbench_panel.diff_check_status,
        patch_workbench_files = patch_workbench_files,
        permissions_status = d.permissions_panel.status,
        permissions_memberships = d.permissions_panel.membership_count,
        permissions_active = d.permissions_panel.active_membership_count,
        permissions_addons = d.permissions_panel.addon_authorization_count,
        permissions_approved_addons = d.permissions_panel.approved_addon_permission_count,
        permissions_pending = d.permissions_panel.pending_human_approval_count,
        permissions_timed_out = d.permissions_panel.timed_out_human_approval_count,
        permission_memberships = permission_memberships,
        permission_approvals = permission_approvals,
        identity_status = d.identity_panel.status,
        identity_context = identity_context,
        identity_count = d.identity_panel.identity_count,
        identity_alias_count = d.identity_panel.channel_alias_count,
        identity_membership_count = d.identity_panel.membership_count,
        identity_tenant_missing = d.identity_panel.tenant_audit_missing_count,
        identity_aliases = identity_aliases,
        identity_memberships = identity_memberships,
        operating_context_status = d.operating_context_panel.status,
        operating_tenant_path = d.operating_context_panel.tenant_path,
        operating_tenant_policy_status = d.operating_context_panel.tenant_policy_status,
        operating_memory_level = d.operating_context_panel.memory_level,
        operating_memory_scopes = d.operating_context_panel.memory_scopes.join("+"),
        operating_memory_audience = d.operating_context_panel.memory_audience,
        operating_personality_status = d.operating_context_panel.personality_status,
        operating_prompt_gates = operating_context_gates,
        digital_twin_status = d.digital_twin_panel.schema_version,
        digital_twin_workflows_count = d.digital_twin_panel.workflow_count,
        digital_twin_happening = d.digital_twin_panel.global_counts.happening_now_count,
        digital_twin_done = d.digital_twin_panel.global_counts.done_count,
        digital_twin_remaining = d.digital_twin_panel.global_counts.remaining_count,
        digital_twin_validated = d.digital_twin_panel.global_counts.validated_count,
        digital_twin_rejected = d.digital_twin_panel.global_counts.rejected_count,
        digital_twin_approvals = d.digital_twin_panel.global_counts.awaiting_approval_count,
        digital_twin_workflows = digital_twin_workflows,
        dag_status = d.dag_panel.status,
        dag_workflows_count = d.dag_panel.workflow_count,
        dag_nodes = d.dag_panel.node_count,
        dag_edges = d.dag_panel.edge_count,
        dag_running = d.dag_panel.running_node_count,
        dag_blocked = d.dag_panel.blocked_node_count,
        dag_waits = d.dag_panel.wait_node_count,
        dag_human_waits = d.dag_panel.human_wait_count,
        dag_workflows = dag_workflows,
        task_board_status = d.task_board_panel.status,
        task_board_workflows = d.task_board_panel.workflow_count,
        task_board_tasks = d.task_board_panel.task_count,
        task_board_ready_handoffs = d.task_board_panel.ready_handoffs,
        task_board_human_waits = d.task_board_panel.pending_human_interactions,
        task_board_checkpoints = d.task_board_panel.checkpoint_resume_candidates,
        task_board_artifacts = d.task_board_panel.artifact_count,
        task_board_lanes = task_board_lanes,
        workflow_mutation_status = d.workflow_mutation_panel.status,
        workflow_mutation_mode = d.workflow_mutation_panel.operation_mode,
        workflow_mutation_workflows = d.workflow_mutation_panel.workflow_count,
        workflow_mutation_active = d.workflow_mutation_panel.active_workflow_count,
        workflow_mutation_mutable = d.workflow_mutation_panel.mutable_workflow_count,
        workflow_mutation_tasks = d.workflow_mutation_panel.task_count,
        workflow_mutation_ready_handoffs = d.workflow_mutation_panel.ready_handoff_count,
        workflow_mutation_human_waits = d.workflow_mutation_panel.human_wait_count,
        workflow_mutation_checkpoints = d
            .workflow_mutation_panel
            .checkpoint_resume_candidate_count,
        workflow_mutation_pending = d
            .workflow_mutation_panel
            .pending_modifier_proposal_count,
        workflow_mutation_applied = d
            .workflow_mutation_panel
            .applied_modifier_proposal_count,
        workflow_mutation_events = d.workflow_mutation_panel.event_count,
        workflow_mutation_cost = d.workflow_mutation_panel.estimated_cost_total_usd,
        workflow_mutation_cards = workflow_mutation_cards,
        workflow_mutation_proposals = workflow_mutation_proposals,
        workflow_mutation_next = if d.workflow_mutation_panel.next_actions.is_empty() {
            "none".to_string()
        } else {
            d.workflow_mutation_panel.next_actions.join(" | ")
        },
        artifact_status = d.artifact_panel.status,
        artifact_workflows_count = d.artifact_panel.workflow_count,
        artifact_count = d.artifact_panel.artifact_count,
        artifact_bytes = d.artifact_panel.total_bytes,
        artifact_workflows = artifact_workflows,
        artifact_entries = artifact_entries,
        schedule_status = d.schedule_panel.status,
        schedule_due = d.schedule_panel.due_workflows,
        schedule_runnable = d.schedule_panel.runnable_due_workflows,
        schedule_cron = d.schedule_panel.cron_nodes,
        schedule_wait_until = d.schedule_panel.wait_until_nodes,
        schedule_next = d
            .schedule_panel
            .next_wakeup_at
            .as_deref()
            .unwrap_or("none"),
        event_status = d.event_panel.status,
        event_visible = d.event_panel.visible_event_count,
        event_total = d.event_panel.total_event_count,
        latest_events = latest_events,
        event_runtime_summary = event_runtime_summary,
        structured_logs_status = d.structured_logs_panel.status,
        structured_logs_count = d.structured_logs_panel.log_count,
        structured_logs_total = d.structured_logs_panel.total_event_count,
        structured_logs_next_cursor = d
            .structured_logs_panel
            .next_cursor
            .map(|cursor| cursor.to_string())
            .unwrap_or_else(|| "none".to_string()),
        structured_logs_has_more = d.structured_logs_panel.has_more,
        structured_logs = structured_logs,
        cost_status = d.cost_panel.status,
        cost_workflows = d.cost_panel.workflow_count,
        cost_nodes = d.cost_panel.node_count,
        cost_ai_nodes = d.cost_panel.ai_node_count,
        cost_deterministic_nodes = d.cost_panel.deterministic_node_count,
        cost_avoided_nodes = d.cost_panel.model_call_avoided_node_count,
        cost_estimated = d.cost_panel.estimated_task_cost_total_usd,
        cost_observed = d.cost_panel.observed_event_cost_total_usd,
        improvement_status = d.improvement_loop_panel.status,
        improvement_total_workflows = d.improvement_loop_panel.total_workflows,
        improvement_matched_workflows = d.improvement_loop_panel.matched_workflows,
        improvement_candidates_count = d.improvement_loop_panel.candidate_count,
        improvement_critical = d.improvement_loop_panel.critical_candidate_count,
        improvement_high = d.improvement_loop_panel.high_candidate_count,
        improvement_parallel = d.improvement_loop_panel.parallel_ready_candidate_count,
        improvement_avoidable_ai = d.improvement_loop_panel.avoidable_ai_candidate_count,
        improvement_final_outcome = d.improvement_loop_panel.final_outcome_candidate_count,
        improvement_stale = d
            .improvement_loop_panel
            .stale_or_attention_candidate_count,
        improvement_validation_failures = d.improvement_loop_panel.validation_failure_count,
        improvement_context = d.improvement_loop_panel.context_quality_status,
        improvement_candidates = improvement_candidates,
        context_ready = d.context_memory_panel.ready_for_handoff,
        context_blocked = d.context_memory_panel.blocked_tasks,
        context_budget_pressure = d.context_memory_panel.context_budget_pressure,
        memory_policy_status = d.context_memory_panel.memory_policy_status,
        active_workflows = d.task_board_panel.workflow_count,
        addon_capability_status = d.addon_capability_panel.status,
        addon_count = d.addon_capability_panel.addon_count,
        addon_enabled = d.addon_capability_panel.enabled_addon_count,
        addon_capabilities_count = d.addon_capability_panel.capability_count,
        addon_enabled_capabilities = d.addon_capability_panel.enabled_capability_count,
        addon_disabled_capabilities = d.addon_capability_panel.disabled_capability_count,
        addon_permissions = d.addon_capability_panel.permission_count,
        addon_contracts = d.addon_capability_panel.runtime_contract_count,
        addon_views = d.addon_capability_panel.view_count,
        addon_dispatches = d.addon_capability_panel.dispatch_count,
        addon_queued_dispatches = d.addon_capability_panel.queued_dispatch_count,
        addon_event_types = d.addon_capability_panel.event_type_count,
        addon_event_channels = d.addon_capability_panel.event_channel_count,
        addon_event_triggers = d.addon_capability_panel.event_trigger_count,
        addon_event_listeners = d.addon_capability_panel.event_listener_count,
        addon_event_adapters = d.addon_capability_panel.event_adapter_count,
        addon_capabilities = addon_capabilities,
        addon_event_extensions = addon_event_extensions,
        addon_renderer_status = d.addon_renderer_panel.status,
        addon_safe_renderers = d.addon_renderer_panel.safe_renderer_count,
        addon_renderers = d.addon_renderer_panel.renderer_count,
        addon_renderer_family_count = d.addon_renderer_panel.family_count,
        addon_renderer_families = addon_renderer_families,
        repository_context = d.repository_context,
        estimated_costs = d.estimated_costs,
        attention_actions = attention_actions,
        quick_actions = quick_actions,
        next_commands = next_commands,
    )
}

pub fn render_interactive_ui_composition(panel: &InteractiveUiCompositionPanel) -> String {
    let renderer_families = if panel.addon_renderer_families.is_empty() {
        "none".to_string()
    } else {
        panel.addon_renderer_families.join(", ")
    };
    let commands = [
        ("refresh", &panel.commands.refresh),
        ("inspect_addons", &panel.commands.inspect_addons),
        ("open_task_board", &panel.commands.open_task_board),
    ]
    .into_iter()
    .map(|(name, command)| format!("{name} {}", command.join(" ")))
    .collect::<Vec<_>>()
    .join(" | ");

    format!(
        "UI composition: {status}; layout {layout}; regions {regions}; widgets {widgets} ({core} core, {addons} addon)\nRegions: {region_summary}\nAddon renderer families: {renderer_families}\nCommands: {commands}\n",
        status = panel.status,
        layout = panel.layout_kind,
        regions = panel.region_count,
        widgets = panel.widget_count,
        core = panel.core_widget_count,
        addons = panel.addon_widget_count,
        region_summary = render_ui_composition_region_summary(panel),
        renderer_families = renderer_families,
        commands = commands,
    )
}

pub fn render_interactive_guided_cockpit(panel: &InteractiveGuidedCockpitPanel) -> String {
    format!(
        "Guided cockpit: {status}; visual {visual}; steps {completed}/{total}; current {current}; blocked {blocked}; confirmations {confirmations}\nPanes: {panes}\nSteps: {steps}\nSafe actions: {policy}\nNext: {next}\nNotes: {notes}\n",
        status = panel.status,
        visual = panel.visual_mode,
        completed = panel.completed_step_count,
        total = panel.total_step_count,
        current = panel.current_step_id,
        blocked = panel.blocked_step_count,
        confirmations = panel.confirmation_step_count,
        panes = render_guided_cockpit_pane_summary(panel),
        steps = render_guided_cockpit_step_summary(panel),
        policy = render_guided_cockpit_policy_summary(panel),
        next = if panel.next_commands.is_empty() {
            "none".to_string()
        } else {
            panel.next_commands.join(" | ")
        },
        notes = if panel.notes.is_empty() {
            "none".to_string()
        } else {
            panel.notes.join(" | ")
        },
    )
}

fn render_guided_cockpit_step_summary(panel: &InteractiveGuidedCockpitPanel) -> String {
    if panel.steps.is_empty() {
        return "none".to_string();
    }
    panel
        .steps
        .iter()
        .map(|step| {
            format!(
                "{}. {} [{}] panel {} risk {} apply {} evidence {}",
                step.order,
                step.step_id,
                step.status,
                step.primary_panel,
                step.risk_level,
                step.can_apply_now,
                step.evidence
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn render_guided_cockpit_pane_summary(panel: &InteractiveGuidedCockpitPanel) -> String {
    if panel.layout_panes.is_empty() {
        return "none".to_string();
    }
    panel
        .layout_panes
        .iter()
        .map(|pane| {
            format!(
                "{}:{}:{} via {}",
                pane.focus_key, pane.pane_id, pane.role, pane.source_panel
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn render_guided_cockpit_policy_summary(panel: &InteractiveGuidedCockpitPanel) -> String {
    if panel.safe_action_policy.is_empty() {
        "none".to_string()
    } else {
        panel.safe_action_policy.join(" | ")
    }
}

pub fn render_interactive_operational_cockpit(
    panel: &InteractiveOperationalCockpitPanel,
) -> String {
    format!(
        "Operational cockpit: {attention}; {priority}; active work {active_work}, ready handoffs {ready_handoffs}, human waits {human_waits}, due workflows {due_workflows}, brain {selected_brain}; sections {sections}; modifier lane {modifier_lane}; event runtime {event_runtime}; next {next_actions}",
        attention = panel.attention_level,
        priority = panel.priority_summary,
        active_work = panel.active_work_count,
        ready_handoffs = panel.ready_handoff_count,
        human_waits = panel.pending_human_wait_count,
        due_workflows = panel.due_workflow_count,
        selected_brain = panel.selected_brain,
        sections = render_operational_cockpit_sections(panel),
        modifier_lane = render_operational_modifier_lane(&panel.modifier_lane),
        event_runtime = render_event_runtime_panel(&panel.event_runtime),
        next_actions = if panel.next_actions.is_empty() {
            "none".to_string()
        } else {
            panel.next_actions.join(" | ")
        },
    )
}

pub fn render_interactive_improvement_loop(panel: &InteractiveImprovementLoopPanel) -> String {
    format!(
        "Improvement loop: {status}; workflows {total_workflows}, matched {matched_workflows}, candidates {candidate_count}, critical {critical}, high {high}, parallel {parallel}, avoidable AI {avoidable_ai}, final outcome {final_outcome}, stale/attention {stale}; logs {logs}/{events}; cost estimated ${estimated:.4}, observed ${observed:.4}; validation failures {validation_failures}; context {context}\nTop candidates: {candidates}\nCommands: refresh {refresh}; candidates {candidate_command}; cost {cost_command}; logs {logs_command}; task board {task_board}; validate {validate}; apply {apply}; benchmark {benchmark}; promote {promote}\nNext: {next_actions}\nNotes: {notes}\n",
        status = panel.status,
        total_workflows = panel.total_workflows,
        matched_workflows = panel.matched_workflows,
        candidate_count = panel.candidate_count,
        critical = panel.critical_candidate_count,
        high = panel.high_candidate_count,
        parallel = panel.parallel_ready_candidate_count,
        avoidable_ai = panel.avoidable_ai_candidate_count,
        final_outcome = panel.final_outcome_candidate_count,
        stale = panel.stale_or_attention_candidate_count,
        logs = panel.structured_log_count,
        events = panel.event_count,
        estimated = panel.estimated_cost_total_usd,
        observed = panel.observed_cost_total_usd,
        validation_failures = panel.validation_failure_count,
        context = panel.context_quality_status,
        candidates = render_improvement_candidate_summary(panel),
        refresh = panel.commands.refresh.join(" "),
        candidate_command = panel.commands.candidates.join(" "),
        cost_command = panel.commands.cost_ledger.join(" "),
        logs_command = panel.commands.structured_logs.join(" "),
        task_board = panel.commands.task_board.join(" "),
        validate = panel.commands.validate.join(" "),
        apply = panel.commands.apply_event_policy.join(" "),
        benchmark = panel.commands.benchmark_event_policy.join(" "),
        promote = panel.commands.promote_event_policy.join(" "),
        next_actions = if panel.next_actions.is_empty() {
            "none".to_string()
        } else {
            panel.next_actions.join(" | ")
        },
        notes = if panel.notes.is_empty() {
            "none".to_string()
        } else {
            panel.notes.join(" | ")
        },
    )
}

fn render_improvement_candidate_summary(panel: &InteractiveImprovementLoopPanel) -> String {
    if panel.top_candidates.is_empty() {
        return "none".to_string();
    }
    panel
        .top_candidates
        .iter()
        .map(|candidate| {
            format!(
                "{}[{} score {}]:{} parallel {} avoidable ${:.4} outcome {} reasons {} cmds {}",
                candidate.workflow_id,
                candidate.priority,
                candidate.score,
                candidate.recommended_action,
                candidate.ready_parallel_task_count,
                candidate.avoidable_estimated_cost_usd,
                candidate.outcome_status,
                if candidate.reason_codes.is_empty() {
                    "none".to_string()
                } else {
                    candidate.reason_codes.join("+")
                },
                if candidate.suggested_commands.is_empty() {
                    "none".to_string()
                } else {
                    candidate.suggested_commands.join(" || ")
                },
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn render_event_runtime_panel(panel: &InteractiveEventRuntimePanel) -> String {
    let events = panel
        .event_cards
        .iter()
        .take(5)
        .map(|event| format!("{}:{}:{}", event.event_id, event.origin, event.action))
        .collect::<Vec<_>>()
        .join(", ");
    let services = panel
        .service_cards
        .iter()
        .take(5)
        .map(|service| {
            format!(
                "{}:{}:{}",
                service.service_id, service.service_kind, service.status
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let lifecycle_actions = panel
        .workflow_lifecycle
        .actions
        .iter()
        .map(|action| {
            if action.action == action.normalized_route {
                format!("{}:{}", action.action, action.status)
            } else {
                format!(
                    "{}->{}:{}",
                    action.action, action.normalized_route, action.status
                )
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{}; pending events {}; workers {}/{}; wakeable workflows {}; recommendation {}; lifecycle {}/{} {} {}; events {}; services {}",
        panel.status,
        panel.pending_event_count,
        panel.running_service_count,
        panel.service_count,
        panel.wakeable_workflow_count,
        panel.recommended_action,
        panel.workflow_lifecycle.validated_action_count,
        panel.workflow_lifecycle.action_count,
        panel.workflow_lifecycle.status,
        if lifecycle_actions.is_empty() {
            "none".to_string()
        } else {
            lifecycle_actions
        },
        if events.is_empty() {
            "none".to_string()
        } else {
            events
        },
        if services.is_empty() {
            "none".to_string()
        } else {
            services
        }
    )
}

pub fn render_interactive_event_runtime(panel: &InteractiveEventRuntimePanel) -> String {
    render_event_runtime_panel(panel)
}

fn render_operational_modifier_lane(panel: &InteractiveOperationalModifierLanePanel) -> String {
    let proposals = panel
        .proposal_cards
        .iter()
        .take(5)
        .map(|proposal| {
            format!(
                "{}:{}:{}",
                proposal.proposal_id, proposal.target_kind, proposal.status
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{}; pending proposals {}; applied {}; proposals {}",
        panel.status,
        panel.pending_count,
        panel.applied_count,
        if proposals.is_empty() {
            "none".to_string()
        } else {
            proposals
        }
    )
}

pub fn render_interactive_task_board(panel: &InteractiveTaskBoardPanel) -> String {
    format!(
        "Task board: {status}; workflows {workflow_count}, tasks {task_count}, ready handoffs {ready_handoffs}, human waits {human_waits}, checkpoints {checkpoints}, artifacts {artifacts}\nLanes: {lanes}\nCards: {cards}\n",
        status = panel.status,
        workflow_count = panel.workflow_count,
        task_count = panel.task_count,
        ready_handoffs = panel.ready_handoffs,
        human_waits = panel.pending_human_interactions,
        checkpoints = panel.checkpoint_resume_candidates,
        artifacts = panel.artifact_count,
        lanes = render_task_board_lane_summary(panel),
        cards = render_task_board_card_summary(panel),
    )
}

pub fn render_interactive_workflow_mutation(panel: &InteractiveWorkflowMutationPanel) -> String {
    format!(
        "Workflow mutation: {status}; mode {mode}; workflows {workflow_count}, active {active}, mutable {mutable}, tasks {tasks}, ready handoffs {ready_handoffs}, human waits {human_waits}, checkpoints {checkpoints}, artifacts {artifacts}, proposals {pending}/{applied}, events {events}, cost ${cost:.4}\nWorkflows: {workflows}\nProposals: {proposals}\nCommands: refresh {refresh}; task-board {task_board}; dag {dag}; cockpit {cockpit}; ops {ops}; update-goal {update_goal}; update-node-brain {update_node_brain}; attach {attach}; logs {logs}\nNext: {next_actions}\nNotes: {notes}\n",
        status = panel.status,
        mode = panel.operation_mode,
        workflow_count = panel.workflow_count,
        active = panel.active_workflow_count,
        mutable = panel.mutable_workflow_count,
        tasks = panel.task_count,
        ready_handoffs = panel.ready_handoff_count,
        human_waits = panel.human_wait_count,
        checkpoints = panel.checkpoint_resume_candidate_count,
        artifacts = panel.artifact_count,
        pending = panel.pending_modifier_proposal_count,
        applied = panel.applied_modifier_proposal_count,
        events = panel.event_count,
        cost = panel.estimated_cost_total_usd,
        workflows = render_workflow_mutation_card_summary(panel),
        proposals = render_workflow_mutation_proposal_summary(panel),
        refresh = panel.commands.refresh.join(" "),
        task_board = panel.commands.task_board.join(" "),
        dag = panel.commands.workflow_dag.join(" "),
        cockpit = panel.commands.operational_cockpit.join(" "),
        ops = panel.commands.ops_console.join(" "),
        update_goal = panel.commands.update_goal.join(" "),
        update_node_brain = panel.commands.update_node_brain.join(" "),
        attach = panel.commands.attach_artifact.join(" "),
        logs = panel.commands.structured_logs.join(" "),
        next_actions = if panel.next_actions.is_empty() {
            "none".to_string()
        } else {
            panel.next_actions.join(" | ")
        },
        notes = if panel.notes.is_empty() {
            "none".to_string()
        } else {
            panel.notes.join(" | ")
        },
    )
}

pub fn render_interactive_workflow_sidebar(panel: &InteractiveWorkflowSidebarPanel) -> String {
    format!(
        "Workflow sidebar: {status}; workflows {workflow_count}, groups {group_count}, selected {selected_workflow_id} in {selected_group_id}, active {active_count}, attention {attention_count}, event_driven {event_driven_count}, scheduled {scheduled_count}, completed {completed_count}\nGroups: {groups}\nKeyboard: {keyboard}\nCommands: {commands}\n",
        status = panel.status,
        workflow_count = panel.workflow_count,
        group_count = panel.group_count,
        selected_workflow_id = panel.selected_workflow_id,
        selected_group_id = panel.selected_group_id,
        active_count = panel.active_count,
        attention_count = panel.attention_count,
        event_driven_count = panel.event_driven_count,
        scheduled_count = panel.scheduled_count,
        completed_count = panel.completed_count,
        groups = render_workflow_sidebar_summary(panel),
        keyboard = panel.keyboard_hints.join(", "),
        commands = [
            panel.commands.refresh.join(" "),
            panel.commands.list.join(" "),
            panel.commands.task_board.join(" "),
            panel.commands.workflow_dag.join(" "),
        ]
        .join(" | "),
    )
}

pub fn render_interactive_replacement_cli(panel: &InteractiveReplacementCliPanel) -> String {
    format!(
        "Replacement CLI: {status}; milestone {milestone}; capability {capability_id}; readiness {readiness_percent}% ({ready_surface_count}/{surface_count}); promotion_ready {promotion_ready}\nExternal brain evidence: {external_plan_status}; ready {external_ready}; providers {external_providers}; templates {external_templates}\nProvider readiness: {provider_readiness}\nProvider wrapper plans: {provider_wrapper_plans}\nWrapper manifest audit: {wrapper_manifest_status}; ready {wrapper_manifest_ready}; provider {wrapper_manifest_provider}; command {wrapper_command_status}; evidence {wrapper_counts_as_evidence}\nSurfaces: {surfaces}\nBlockers: {blockers}\nCommands: {commands}\n",
        status = panel.status,
        milestone = panel.milestone,
        capability_id = panel.capability_id,
        readiness_percent = panel.readiness_percent,
        ready_surface_count = panel.ready_surface_count,
        surface_count = panel.surface_count,
        promotion_ready = panel.promotion_ready,
        external_plan_status = panel.external_brain_evidence_plan.status,
        external_ready = panel.external_brain_evidence_plan.ready_to_collect_evidence,
        external_providers = panel.external_brain_evidence_plan.provider_candidate_count,
        external_templates = if panel
            .external_brain_evidence_plan
            .manifest_template_ids
            .is_empty()
        {
            "none".to_string()
        } else {
            panel
                .external_brain_evidence_plan
                .manifest_template_ids
                .join(",")
        },
        provider_readiness = render_replacement_cli_provider_readiness_summary(panel),
        provider_wrapper_plans = render_replacement_cli_provider_wrapper_plan_summary(panel),
        wrapper_manifest_status = panel.provider_wrapper_manifest_audit.status,
        wrapper_manifest_ready = panel.provider_wrapper_manifest_audit.ready_to_collect_evidence,
        wrapper_manifest_provider = panel
            .provider_wrapper_manifest_audit
            .selected_provider_id
            .as_deref()
            .unwrap_or("none"),
        wrapper_command_status = panel.provider_wrapper_manifest_audit.command_path_status,
        wrapper_counts_as_evidence = panel
            .provider_wrapper_manifest_audit
            .counts_as_release_evidence,
        surfaces = render_replacement_cli_surface_summary(panel),
        blockers = if panel.blockers.is_empty() {
            "none".to_string()
        } else {
            panel.blockers.join("; ")
        },
        commands = [
            format!("forge {}", panel.commands.refresh.join(" ")),
            format!("forge {}", panel.commands.patch_workbench.join(" ")),
            format!("forge {}", panel.commands.harness.join(" ")),
            format!("forge {}", panel.commands.cli_demo.join(" ")),
            format!("forge {}", panel.commands.evidence_plan.join(" ")),
            format!(
                "forge {}",
                panel.commands.collect_external_brain_evidence.join(" ")
            ),
        ]
        .join(" | "),
    )
}

fn render_replacement_cli_provider_readiness_summary(
    panel: &InteractiveReplacementCliPanel,
) -> String {
    if panel.provider_readiness.is_empty() {
        return "none".to_string();
    }
    panel
        .provider_readiness
        .iter()
        .take(5)
        .map(|provider| {
            format!(
                "{}:{}:{}",
                provider.provider_id, provider.readiness, provider.version_status
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_replacement_cli_provider_wrapper_plan_summary(
    panel: &InteractiveReplacementCliPanel,
) -> String {
    if panel.provider_wrapper_plans.is_empty() {
        return "none".to_string();
    }
    panel
        .provider_wrapper_plans
        .iter()
        .take(5)
        .map(|plan| {
            let state = if plan.installed {
                "wrapper_plan_ready"
            } else {
                "provider_missing"
            };
            format!(
                "{}:{}:evidence={}",
                plan.provider_id, state, plan.counts_as_release_evidence
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn render_interactive_multimodal_runtime(panel: &InteractiveMultimodalRuntimePanel) -> String {
    format!(
        "Multimodal runtime: {status}; capability {capability_id}; addon {addon_id}; view {addon_view_id}; readiness {readiness_percent}% ({ready_surface_count}/{surface_count}); promotion_ready {promotion_ready}; feature {feature_enabled} from {feature_source}/{feature_status}\nProduction runtime evidence: {evidence_status}; ready {evidence_ready}; templates {evidence_templates}; config checks {evidence_config_checks}\nSurfaces: {surfaces}\nBlockers: {blockers}\nCommands: {commands}\nNotes: {notes}\n",
        status = panel.status,
        capability_id = panel.capability_id,
        addon_id = panel.addon_id,
        addon_view_id = panel.addon_view_id,
        readiness_percent = panel.readiness_percent,
        ready_surface_count = panel.ready_surface_count,
        surface_count = panel.surface_count,
        promotion_ready = panel.promotion_ready,
        feature_enabled = panel.feature_flag_enabled,
        feature_source = panel.feature_flag_source,
        feature_status = panel.feature_flag_status,
        evidence_status = panel.production_runtime_evidence_plan.status,
        evidence_ready = panel
            .production_runtime_evidence_plan
            .ready_to_collect_evidence,
        evidence_templates = if panel
            .production_runtime_evidence_plan
            .manifest_template_ids
            .is_empty()
        {
            "none".to_string()
        } else {
            panel
                .production_runtime_evidence_plan
                .manifest_template_ids
                .join(",")
        },
        evidence_config_checks = panel
            .production_runtime_evidence_plan
            .config_check_count,
        surfaces = render_multimodal_runtime_surface_summary(panel),
        blockers = if panel.blockers.is_empty() {
            "none".to_string()
        } else {
            panel.blockers.join("; ")
        },
        commands = [
            format!("forge {}", panel.commands.refresh.join(" ")),
            format!("forge {}", panel.commands.status.join(" ")),
            format!("forge {}", panel.commands.runtime_benchmark.join(" ")),
            format!("forge {}", panel.commands.evidence_plan.join(" ")),
        ]
        .join(" | "),
        notes = if panel.notes.is_empty() {
            "none".to_string()
        } else {
            panel.notes.join("; ")
        },
    )
}

fn render_replacement_cli_surface_summary(panel: &InteractiveReplacementCliPanel) -> String {
    if panel.surfaces.is_empty() {
        return "none".to_string();
    }
    panel
        .surfaces
        .iter()
        .map(|surface| {
            let readiness = if surface.ready { "ready" } else { "blocked" };
            format!("{}[{}]:{}", surface.surface_id, readiness, surface.status)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_multimodal_runtime_surface_summary(panel: &InteractiveMultimodalRuntimePanel) -> String {
    if panel.surfaces.is_empty() {
        return "none".to_string();
    }
    panel
        .surfaces
        .iter()
        .map(|surface| {
            let readiness = if surface.ready { "ready" } else { "blocked" };
            format!("{}[{}]:{}", surface.surface_id, readiness, surface.status)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_workflow_sidebar_summary(panel: &InteractiveWorkflowSidebarPanel) -> String {
    if panel.groups.is_empty() {
        return "none".to_string();
    }
    panel
        .groups
        .iter()
        .map(|group| {
            let items = group
                .items
                .iter()
                .take(6)
                .map(|item| {
                    format!(
                        "{}{} [{}] {} action {} handoffs {} due {}",
                        if item.selected { "*" } else { "" },
                        item.workflow_id,
                        item.lifecycle_state,
                        item.title,
                        item.runtime.operator_action,
                        item.ready_handoff_count,
                        item.due_schedule_count
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}({}): {}", group.group_id, group.item_count, items)
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

pub fn render_interactive_artifacts(panel: &InteractiveArtifactPanel) -> String {
    format!(
        "Artifact panel: {status}; workflows {workflow_count}, artifacts {artifact_count}, bytes {total_bytes}\nWorkflows: {workflows}\nArtifacts: {artifacts}\nCommands: refresh {refresh}; task board {task_board}; workflows {workflow_list}\n",
        status = panel.status,
        workflow_count = panel.workflow_count,
        artifact_count = panel.artifact_count,
        total_bytes = panel.total_bytes,
        workflows = render_artifact_workflow_summary(panel),
        artifacts = render_artifact_entry_summary(panel),
        refresh = panel.commands.refresh.join(" "),
        task_board = panel.commands.task_board.join(" "),
        workflow_list = panel.commands.workflow_list.join(" "),
    )
}

pub fn render_interactive_token_usage(panel: &InteractiveTokenUsagePanel) -> String {
    format!(
        "Token usage: {status}; operational {operational_status}; action {recommended_action}; blobs {total_headroom_blobs}; original tokens {total_original_tokens}; compressed tokens {total_compressed_tokens}; saved tokens {estimated_saved_tokens}; savings {average_savings_percent:.2}%; over budget {over_budget}\nPrimary: {primary_source}/{primary_content_kind}\nSources: {sources}\nContent kinds: {content_kinds}\nRetrieve: {retrieve}\nCommands: refresh {refresh}; stats {stats}; analyze {analyze}; harness {harness}; costs {costs}\n",
        status = panel.status,
        operational_status = panel.operational_status,
        recommended_action = panel.recommended_action,
        total_headroom_blobs = panel.total_headroom_blobs,
        total_original_tokens = panel.total_original_tokens,
        total_compressed_tokens = panel.total_compressed_tokens,
        estimated_saved_tokens = panel.estimated_saved_tokens,
        average_savings_percent = panel.average_savings_percent,
        over_budget = panel.over_budget_after_headroom_count,
        primary_source = panel.primary_source,
        primary_content_kind = panel.primary_content_kind,
        sources = render_token_usage_source_summary(panel),
        content_kinds = render_token_usage_kind_summary(panel),
        retrieve = render_token_usage_retrieve_summary(panel),
        refresh = panel.commands.refresh.join(" "),
        stats = panel.commands.headroom_stats.join(" "),
        analyze = panel.commands.analyze_payload.join(" "),
        harness = panel.commands.harness.join(" "),
        costs = panel.commands.cost_ledger.join(" "),
    )
}

pub fn render_interactive_patch_workbench(panel: &InteractivePatchWorkbenchPanel) -> String {
    format!(
        "Patch workbench: {status}; clean {clean}, files {changed_path_count}, staged {staged_path_count}, unstaged {unstaged_path_count}, untracked {untracked_path_count}, diff {diff_present}, check {diff_check_status}\nRepository: {repository_path}\nAddon contract: {source_addon}; capability {capability_id}; permission {permission_id}; view {view_id}; runtime {runtime_contract_id} via {runtime}/{entrypoint}\nFiles: {files}\nDiff preview: {diff_preview}\nReview queue: {review_queue}\nEdit intake: {edit_intake}\nOperation plan: {operation_plan}\nApproval flow: {approval_status}; gate {approval_gate}; approval {requires_human_approval}; apply ready {apply_ready}\nCommands: {commands}\n",
        status = panel.status,
        clean = panel.clean,
        changed_path_count = panel.changed_path_count,
        staged_path_count = panel.staged_path_count,
        unstaged_path_count = panel.unstaged_path_count,
        untracked_path_count = panel.untracked_path_count,
        diff_present = panel.diff_present,
        diff_check_status = panel.diff_check_status,
        repository_path = panel.repository_path,
        source_addon = panel.addon_contract.source_addon,
        capability_id = panel.addon_contract.capability_id,
        permission_id = panel.addon_contract.permission_id,
        view_id = panel.addon_contract.view_id,
        runtime_contract_id = panel.addon_contract.runtime_contract_id,
        runtime = panel.addon_contract.runtime,
        entrypoint = panel.addon_contract.entrypoint,
        files = render_patch_workbench_file_summary(panel),
        diff_preview = render_patch_diff_preview(&panel.diff_preview),
        review_queue = render_patch_diff_review_queue(&panel.diff_review_queue),
        edit_intake = render_patch_edit_intake(&panel.edit_intake),
        operation_plan = render_patch_operation_plan(&panel.operation_plan),
        approval_status = panel.approval_flow.status,
        approval_gate = panel.approval_flow.current_gate,
        requires_human_approval = panel.approval_flow.requires_human_approval,
        apply_ready = panel.approval_flow.apply_ready,
        commands = render_patch_workbench_commands(&panel.commands),
    )
}

fn render_patch_workbench_commands(commands: &InteractivePatchWorkbenchCommands) -> String {
    format!(
        "plan {}; review {}; diff {}; apply {}; revert {}; restore {}",
        commands.plan.join(" "),
        commands.review.join(" "),
        commands.diff.join(" "),
        commands.apply.join(" "),
        commands.revert.join(" "),
        commands.restore.join(" "),
    )
}

fn render_patch_operation_plan(plan: &InteractivePatchOperationPlan) -> String {
    let steps = if plan.steps.is_empty() {
        "none".to_string()
    } else {
        plan.steps
            .iter()
            .map(|step| format!("{}:{}", step.step_id, step.status))
            .collect::<Vec<_>>()
            .join(" -> ")
    };
    format!(
        "{status}; current {current}; steps {step_count}; approval steps {approval_count}; {steps}",
        status = plan.status,
        current = plan.current_step,
        step_count = plan.step_count,
        approval_count = plan.requires_human_approval_count,
        steps = steps,
    )
}

fn render_patch_edit_intake(intake: &InteractivePatchEditIntake) -> String {
    let missing_inputs = intake
        .required_inputs
        .iter()
        .filter(|input| input.required && input.missing)
        .map(|input| input.input_id.as_str())
        .collect::<Vec<_>>();
    let missing = if missing_inputs.is_empty() {
        "none".to_string()
    } else {
        missing_inputs.join(", ")
    };
    let forms = if intake.forms.is_empty() {
        "none".to_string()
    } else {
        intake
            .forms
            .iter()
            .take(6)
            .map(|form| {
                format!(
                    "{} ready {} approval {}",
                    form.action_id, form.ready, form.requires_human_approval
                )
            })
            .collect::<Vec<_>>()
            .join(" | ")
    };

    format!(
        "{}; default {}; missing {}; inferred paths {}; forms {}",
        intake.status, intake.default_action, missing, intake.inferred_path_count, forms
    )
}

fn render_patch_diff_review_queue(queue: &InteractivePatchDiffReviewQueue) -> String {
    let files = queue
        .files
        .iter()
        .take(6)
        .map(|file| {
            format!(
                "{} [{} hunks, +{}, -{}, {}]",
                file.path,
                file.hunk_count,
                file.addition_count,
                file.deletion_count,
                file.review_status
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    if files.is_empty() {
        format!(
            "{}; files {}; pending {}; hunks {}",
            queue.status, queue.file_count, queue.pending_review_count, queue.total_hunk_count
        )
    } else {
        format!(
            "{}; files {}; pending {}; hunks {}; {}",
            queue.status,
            queue.file_count,
            queue.pending_review_count,
            queue.total_hunk_count,
            files
        )
    }
}

fn render_patch_diff_preview(preview: &InteractivePatchDiffPreview) -> String {
    let path = preview.selected_path.as_deref().unwrap_or("-");
    let rendered_lines = preview
        .lines
        .iter()
        .take(8)
        .map(|line| {
            let prefix = match line.line_kind.as_str() {
                "addition" => "+",
                "deletion" => "-",
                "context" => " ",
                "hunk_header" => "@",
                _ => "#",
            };
            format!("{prefix}{}", line.text)
        })
        .collect::<Vec<_>>()
        .join(" | ");
    if rendered_lines.is_empty() {
        format!(
            "{} for {}; lines {}; truncated {}",
            preview.status, path, preview.line_count, preview.truncated
        )
    } else {
        format!(
            "{} for {}; lines {}; truncated {}; {}",
            preview.status, path, preview.line_count, preview.truncated, rendered_lines
        )
    }
}

pub fn render_interactive_permissions(panel: &InteractivePermissionsPanel) -> String {
    format!(
        "Permission center: {status}; memberships {membership_count}, active {active_membership_count}, addon permissions {addon_authorization_count}, approved {approved_addon_permission_count}, pending approvals {pending_human_approval_count}, timed out {timed_out_human_approval_count}\nMemberships: {memberships}\nAddon permissions: {addon_permissions}\nApprovals: {approvals}\n",
        status = panel.status,
        membership_count = panel.membership_count,
        active_membership_count = panel.active_membership_count,
        addon_authorization_count = panel.addon_authorization_count,
        approved_addon_permission_count = panel.approved_addon_permission_count,
        pending_human_approval_count = panel.pending_human_approval_count,
        timed_out_human_approval_count = panel.timed_out_human_approval_count,
        memberships = render_permission_membership_summary(panel),
        addon_permissions = render_addon_permission_summary(panel),
        approvals = render_permission_approval_summary(panel),
    )
}

pub fn render_interactive_identity(panel: &InteractiveIdentityPanel) -> String {
    format!(
        "Identity center: {status}; context {organization}/{brand}/{product}, user {user}, channel {channel}, policy {policy}\nContext: source {context_source}, memory {memory_scope}, personality {personality_scope}, warnings {warning_count}; tenant audit {tenant_audit_status} missing {tenant_audit_missing_count}\nRegistry: identities {identity_count}, active links {link_count}, channel aliases {channel_alias_count}, memberships {membership_count}, active memberships {active_membership_count}\nIdentities: {identities}\nChannel aliases: {aliases}\nMemberships: {memberships}\nCommands: {commands}\n",
        status = panel.status,
        organization = panel.current_context.organization_id,
        brand = panel.current_context.brand_id,
        product = panel.current_context.product_id,
        user = panel.current_context.user_id,
        channel = panel.current_context.channel_id,
        policy = panel.current_context.tenant_policy_mode,
        context_source = panel.current_context.source,
        memory_scope = panel.current_context.memory_scope,
        personality_scope = panel.current_context.personality_scope,
        warning_count = panel.current_context.warning_count,
        tenant_audit_status = panel.tenant_audit.status,
        identity_count = panel.identity_count,
        link_count = panel.link_count,
        channel_alias_count = panel.channel_alias_count,
        membership_count = panel.membership_count,
        active_membership_count = panel.active_membership_count,
        tenant_audit_missing_count = panel.tenant_audit_missing_count,
        identities = render_identity_record_summary(panel),
        aliases = render_identity_alias_summary(panel),
        memberships = render_identity_membership_summary(panel),
        commands = render_identity_command_summary(panel),
    )
}

pub fn render_interactive_operating_context(panel: &InteractiveOperatingContextPanel) -> String {
    format!(
        "Operating context: {status}; tenant {tenant_path}, policy {tenant_policy_status}, project {project_root}\nOrganization: {organization}/{brand}/{product}; user {user}; channel {channel}; context {context_status}\nMemory: policy {memory_policy_status}, level {memory_level}, scopes {memory_scopes}, audience {memory_audience}, governance {memory_governance_status}\nMemory isolation: {memory_isolation_status}; keys {memory_isolation_keys}\nPersonality: {personality_status}; scope {personality_scope}; voice {brand_voice}; tone {brand_tone}; brand values {brand_value_count}; design tokens {design_token_source}; components {component_source}\nPrompt packet: {prompt_packet_status}; gates {prompt_packet_gates}\nPrompt packet sample: {prompt_sample_status}; task {prompt_sample_task}; persona {prompt_sample_persona}; packet {prompt_sample_hash}\nCompany work: {company_work_status}; departments {company_work_departments}; decisions {company_work_decisions}\nIdentity summary: identities {identity_count}, aliases {alias_count}, memberships {membership_count}, active {active_membership_count}, tenant missing {tenant_missing}\nHandoff context: ready {ready_handoffs}, blocked {blocked_tasks}, budget pressure {budget_pressure}, quality {quality_status}, missing required {required_missing}\nCommands: refresh {refresh}; identity {identity}; memory {context_memory}; context {context_packet}; handoff {task_handoff}\nNext: {next_actions}\n",
        status = panel.status,
        tenant_path = panel.tenant_path,
        tenant_policy_status = panel.tenant_policy_status,
        project_root = panel.project_root,
        organization = panel.organization_id,
        brand = panel.brand_id,
        product = panel.product_id,
        user = panel.user_id,
        channel = panel.channel_id,
        context_status = panel.context_status,
        memory_policy_status = panel.memory_policy_status,
        memory_level = panel.memory_level,
        memory_scopes = panel.memory_scopes.join(","),
        memory_audience = panel.memory_audience,
        memory_governance_status = panel.memory_governance_status,
        memory_isolation_status = panel.memory_isolation_evidence.status,
        memory_isolation_keys = panel.memory_isolation_evidence.isolation_keys.join(","),
        personality_status = panel.personality_status,
        personality_scope = panel.personality_scope,
        brand_voice = panel.brand_voice,
        brand_tone = panel.brand_tone,
        brand_value_count = panel.brand_value_count,
        design_token_source = panel.design_token_source,
        component_source = panel.component_source,
        prompt_packet_status = panel.prompt_packet_contract.status,
        prompt_packet_gates = panel.prompt_packet_contract.required_gates.join(","),
        prompt_sample_status = panel.prompt_packet_sample.status,
        prompt_sample_task = panel
            .prompt_packet_sample
            .task_id
            .as_deref()
            .unwrap_or("none"),
        prompt_sample_persona = panel
            .prompt_packet_sample
            .persona_mode
            .as_deref()
            .unwrap_or("brand_default"),
        prompt_sample_hash = panel
            .prompt_packet_sample
            .packet_sha256
            .as_deref()
            .unwrap_or("missing"),
        company_work_status = panel.company_work_contract.status,
        company_work_departments = panel.company_work_contract.departments.join(","),
        company_work_decisions = panel.company_work_contract.required_decisions.join(","),
        identity_count = panel.identity_summary.identity_count,
        alias_count = panel.identity_summary.channel_alias_count,
        membership_count = panel.identity_summary.membership_count,
        active_membership_count = panel.identity_summary.active_membership_count,
        tenant_missing = panel.identity_summary.tenant_audit_missing_count,
        ready_handoffs = panel.handoff_context_summary.ready_for_handoff,
        blocked_tasks = panel.handoff_context_summary.blocked_tasks,
        budget_pressure = panel.handoff_context_summary.context_budget_pressure,
        quality_status = panel.handoff_context_summary.context_quality_status,
        required_missing = panel.handoff_context_summary.required_context_missing,
        refresh = panel.commands.refresh.join(" "),
        identity = panel.commands.identity.join(" "),
        context_memory = panel.commands.context_memory.join(" "),
        context_packet = panel.commands.context_packet.join(" "),
        task_handoff = panel.commands.task_handoff.join(" "),
        next_actions = if panel.next_actions.is_empty() {
            "none".to_string()
        } else {
            panel.next_actions.join(" | ")
        },
    )
}

fn render_identity_record_summary(panel: &InteractiveIdentityPanel) -> String {
    if panel.identities.is_empty() {
        return "none".to_string();
    }
    panel
        .identities
        .iter()
        .take(8)
        .map(|identity| format!("{}:{} ({})", identity.scope, identity.id, identity.label))
        .collect::<Vec<_>>()
        .join("; ")
}

fn render_identity_alias_summary(panel: &InteractiveIdentityPanel) -> String {
    if panel.channel_aliases.is_empty() {
        return "none".to_string();
    }
    panel
        .channel_aliases
        .iter()
        .take(8)
        .map(|alias| {
            format!(
                "{} [{} {}, source {}, commands {}; commands {}]",
                alias.alias_path,
                alias.link_type,
                alias.status,
                alias.source,
                alias.commands.resolve.join(" "),
                alias.commands.unlink.join(" ")
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn render_identity_membership_summary(panel: &InteractiveIdentityPanel) -> String {
    if panel.memberships.is_empty() {
        return "none".to_string();
    }
    panel
        .memberships
        .iter()
        .take(8)
        .map(|membership| {
            format!(
                "{}:{} -> {} ({}, {} permissions, status {}, commands {})",
                membership.subject_scope,
                membership.subject_id,
                membership.tenant_path,
                membership.role,
                membership.permission_count,
                membership.status,
                membership.commands.update.join(" ")
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn render_identity_command_summary(panel: &InteractiveIdentityPanel) -> String {
    [
        panel.commands.sync.join(" "),
        panel.commands.context.join(" "),
        panel.commands.link.join(" "),
        panel.commands.resolve.join(" "),
        panel.commands.memberships.join(" "),
        panel.commands.tenant_audit.join(" "),
    ]
    .join(" | ")
}

pub fn render_interactive_readiness(panel: &InteractiveReadinessPanel) -> String {
    let usable_executors = if panel.usable_executors.is_empty() {
        "none".to_string()
    } else {
        panel.usable_executors.join(", ")
    };
    let next_actions = if panel.next_actions.is_empty() {
        "none".to_string()
    } else {
        panel.next_actions.join(" | ")
    };
    format!(
        "Interactive readiness: {status}; executors {usable_executor_count}/{executor_count}, brains {brain_count}, shells {forge_first_shell_count}/{shell_count} Forge-first, selected brain {selected_brain}\nHarness mode: {harness_mode}; harness doctor: {harness_doctor}; adoption-plan {harness_adoption_plan}; bootstrap {bootstrap_action}; headroom {headroom_status}; headroom action {headroom_action}; usable executors: {usable_executors}\nNext actions: {next_actions}\n",
        status = panel.status,
        usable_executor_count = panel.usable_executor_count,
        executor_count = panel.executor_count,
        brain_count = panel.brain_count,
        forge_first_shell_count = panel.forge_first_shell_count,
        shell_count = panel.shell_count,
        selected_brain = panel.selected_brain,
        harness_mode = panel.harness_mode.status,
        harness_doctor = panel.harness_doctor.status,
        harness_adoption_plan = panel.harness_adoption_plan.status,
        bootstrap_action = panel.harness_adoption_plan.next_action,
        headroom_status = panel.headroom_operational_status,
        headroom_action = panel.headroom_recommended_action,
        usable_executors = usable_executors,
        next_actions = next_actions,
    )
}

pub fn render_interactive_release_gates(panel: &InteractiveReleaseGatesPanel) -> String {
    let blocked_by = if panel.blocked_by.is_empty() {
        "none".to_string()
    } else {
        panel.blocked_by.join(", ")
    };
    let gate_summary = if panel.gate_cards.is_empty() {
        "none".to_string()
    } else {
        panel
            .gate_cards
            .iter()
            .filter(|gate| !gate.promotion_ready)
            .map(|gate| format!("{} [{}]", gate.capability_id, gate.status))
            .collect::<Vec<_>>()
            .join(" | ")
    };
    let next_actions = if panel.next_actions.is_empty() {
        "none".to_string()
    } else {
        panel.next_actions.join(" | ")
    };
    format!(
        "Release gates: {status}; milestone {milestone}; promotion {decision}; blocked {blocked_gate_count}/{gate_count}; attached evidence {attached_evidence_count}; blocked by {blocked_by}\nGates: {gate_summary}\nGate details: {gate_details}\nNext actions: {next_actions}\n",
        status = panel.status,
        milestone = panel.milestone,
        decision = panel.promotion_decision.decision,
        blocked_gate_count = panel.blocked_gate_count,
        gate_count = panel.gate_count,
        attached_evidence_count = panel.attached_evidence_count,
        blocked_by = blocked_by,
        gate_summary = gate_summary,
        gate_details = render_release_gate_details(panel),
        next_actions = next_actions,
    )
}

fn render_release_gate_details(panel: &InteractiveReleaseGatesPanel) -> String {
    let details = panel
        .gate_cards
        .iter()
        .filter(|gate| !gate.promotion_ready || gate.attached_evidence_count > 0)
        .take(8)
        .map(render_release_gate_card_detail)
        .collect::<Vec<_>>();
    if details.is_empty() {
        "none".to_string()
    } else {
        details.join(" | ")
    }
}

fn render_release_gate_card_detail(gate: &InteractiveReleaseGateCard) -> String {
    format!(
        "{} [{}] ready {}; evidence_state {}; required {}; missing {}; attached {}; plan {}; commands {}; collect {}; next {}",
        gate.capability_id,
        gate.status,
        gate.promotion_ready,
        gate.attached_evidence_state,
        render_release_gate_list(&gate.required_attached_evidence_kinds),
        render_release_gate_list(&gate.missing_attached_evidence_kinds),
        render_release_gate_attached_summary(gate),
        render_release_gate_evidence_plan_summary(&gate.evidence_plan),
        render_release_gate_command_summary(&gate.next_commands),
        render_release_gate_command_summary(&gate.evidence_plan.evidence_collection_commands),
        gate.evidence_plan.next_action,
    )
}

fn render_release_gate_evidence_plan_summary(plan: &InteractiveReleaseGateEvidencePlan) -> String {
    format!(
        "{} ready {}; missing_config {}/{}; templates {}; paths {}; provider_candidates {}; gates {}",
        plan.status,
        plan.ready_to_collect_evidence,
        plan.missing_config_check_count,
        plan.config_check_count,
        render_release_gate_list(&plan.manifest_template_ids),
        render_release_gate_list(&plan.manifest_template_paths),
        render_release_gate_provider_candidate_summary(&plan.provider_candidates),
        render_release_gate_template_summary(&plan.promotion_gate_templates),
    )
}

fn render_release_gate_provider_candidate_summary(
    candidates: &[MilestoneEvidenceProviderCandidate],
) -> String {
    if candidates.is_empty() {
        return "none".to_string();
    }
    candidates
        .iter()
        .take(5)
        .map(|candidate| {
            format!(
                "{}:{}:{}",
                candidate.provider_id, candidate.readiness, candidate.version_status
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_release_gate_template_summary(templates: &[MilestonePromotionGateTemplate]) -> String {
    if templates.is_empty() {
        return "none".to_string();
    }
    templates
        .iter()
        .take(6)
        .map(|template| {
            format!(
                "{} gates {}",
                template.evidence_kind,
                render_release_gate_list(&template.gate_ids)
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn render_release_gate_attached_summary(gate: &InteractiveReleaseGateCard) -> String {
    if gate.attached_evidence.is_empty() {
        return gate.attached_evidence_count.to_string();
    }
    let evidence = gate
        .attached_evidence
        .iter()
        .take(4)
        .map(|evidence| {
            format!(
                "{}:{}:{}",
                evidence.kind, evidence.promotion_impact, evidence.artifact_sha256
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{} ({})", gate.attached_evidence_count, evidence)
}

fn render_release_gate_command_summary(commands: &[String]) -> String {
    if commands.is_empty() {
        "none".to_string()
    } else {
        commands
            .iter()
            .take(6)
            .cloned()
            .collect::<Vec<_>>()
            .join(" -> ")
    }
}

fn render_release_gate_list(items: &[String]) -> String {
    if items.is_empty() {
        "none".to_string()
    } else {
        items.join(", ")
    }
}

pub fn render_interactive_harness(panel: &InteractiveHarnessPanel) -> String {
    let next_actions = if panel.next_actions.is_empty() {
        "none".to_string()
    } else {
        panel.next_actions.join(" | ")
    };
    format!(
        "Harness center: {status}; executor {executor}; mode {mode}; doctor {doctor}; shim {shim}; headroom {headroom}; headroom-plan {headroom_plan}; adoption-plan {adoption_plan}; forge-first readiness {forge_first_readiness}; compatibility {compatibility_status}; headroom-stats {headroom_stats} ({headroom_blob_count} blobs); headroom action {headroom_action}; adoption action {adoption_action}; session lifecycle {session_lifecycle_status} for {session_id}\nProject: {project_root}; shim dir: {shim_dir}\nPrimary actions: doctor | shim-status | wrap-plan | headroom-plan | adoption-plan | lineage-plan | lineage-exec-dry-run | bootstrap | headroom-stats | install-shims | exec\nWrapper plan: {wrapper_plan}\nForge-first adoption: {forge_first_adoption}\nHeadroom runtime: {headroom_runtime}\nOrchestration: {orchestration}\nCompatibility: {compatibility}\nLifecycle gates: {lifecycle_gates}\nHeadroom stats: {headroom_details}\nNext actions: {next_actions}\n",
        status = panel.status,
        executor = panel.executor,
        mode = panel.mode.effective_mode,
        doctor = panel.doctor.status,
        shim = panel.shim_status.status,
        headroom = panel.headroom_preview.status,
        headroom_plan = panel.headroom_plan.status,
        adoption_plan = panel.adoption_plan.status,
        forge_first_readiness = panel.forge_first_adoption_readiness.status,
        compatibility_status = panel.executor_compatibility.status,
        headroom_stats = panel.headroom_stats.status,
        headroom_blob_count = panel.headroom_stats.total_blobs,
        headroom_action = panel.headroom_recommended_action,
        adoption_action = panel.adoption_plan.next_action,
        session_lifecycle_status = panel.session_lifecycle_plan.status,
        session_id = panel.session_lifecycle_plan.session_id,
        project_root = panel.project_root,
        shim_dir = panel.shim_dir,
        wrapper_plan = render_harness_wrapper_plan(&panel.wrapper_plan),
        forge_first_adoption = render_harness_forge_first_adoption(
            &panel.forge_first_adoption_readiness
        ),
        headroom_runtime = render_harness_headroom_runtime(&panel.wrapper_plan),
        orchestration = render_harness_orchestration(&panel.wrapper_plan),
        compatibility = render_harness_executor_compatibility(&panel.executor_compatibility),
        lifecycle_gates = render_harness_lifecycle_gates(&panel.session_lifecycle_plan),
        headroom_details = render_harness_headroom_stats(&panel.headroom_stats),
        next_actions = next_actions,
    )
}

fn render_harness_forge_first_adoption(
    readiness: &InteractiveHarnessForgeFirstAdoptionReadiness,
) -> String {
    format!(
        "{} ready {}; active {}; shim {}; activation {} reason {} possible {}; headroom {}; lineage {}; blockers {}; next {}; routes {}",
        readiness.schema_version,
        readiness.ready_to_use_as_default,
        readiness.forge_first_default_active,
        readiness.shim_ready,
        readiness.activation_status,
        readiness.activation_reason,
        readiness.activation_possible,
        readiness.token_headroom_ready,
        readiness.lineage_policy_ready,
        if readiness.blocked_reasons.is_empty() {
            "none".to_string()
        } else {
            readiness.blocked_reasons.join(", ")
        },
        if readiness.next_commands.is_empty() {
            "none".to_string()
        } else {
            readiness
                .next_commands
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(" | ")
        },
        if readiness.controlled_routes.is_empty() {
            "none".to_string()
        } else {
            readiness
                .controlled_routes
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        }
    )
}

fn render_harness_wrapper_plan(plan: &CliWrapperPlanReport) -> String {
    let env = plan
        .env
        .iter()
        .filter(|item| {
            matches!(
                item.name.as_str(),
                "FORGE_HARNESS"
                    | "FORGE_PROMPT_PACKET_REQUIRED"
                    | "FORGE_CONTEXT_ROUTING"
                    | "FORGE_MEMORY_ROUTING"
                    | "FORGE_SKILL_ROUTING"
                    | "FORGE_MCP_ROUTING"
                    | "FORGE_HEADROOM_RUNTIME_PLAN"
                    | "FORGE_HEADROOM_INTERCEPT"
                    | "FORGE_TOKEN_HEADROOM_REQUIRED"
                    | "FORGE_SESSION_LIFECYCLE"
                    | "FORGE_EVENT_RECEIPTS"
            )
        })
        .map(|item| format!("{}={}", item.name, item.value))
        .collect::<Vec<_>>()
        .join(", ");
    let provider_wrapper = &plan.connected_brain_provider_wrapper;
    format!(
        "forge_first {}; strategy {}; launch {}; provider wrapper {} {}; env {}",
        plan.forge_first,
        plan.wrapper_strategy,
        plan.launch_command.join(" "),
        provider_wrapper.status,
        provider_wrapper.wrapper_path,
        if env.is_empty() {
            "none".to_string()
        } else {
            env
        }
    )
}

fn render_harness_headroom_runtime(plan: &CliWrapperPlanReport) -> String {
    let runtime = &plan.headroom_runtime_plan;
    let points = runtime
        .interception_points
        .iter()
        .take(4)
        .map(|point| format!("{}:{}", point.point_id, point.action))
        .collect::<Vec<_>>()
        .join(", ");
    let routes = runtime
        .content_routes
        .iter()
        .take(5)
        .map(|route| format!("{}={}", route.content_kind, route.strategy))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{} enabled {}, mode {}, store {}, points {}, routes {}",
        runtime.schema_version,
        runtime.enabled,
        runtime.mode,
        runtime.reversible_store.uri_scheme,
        if points.is_empty() {
            "none".to_string()
        } else {
            points
        },
        if routes.is_empty() {
            "none".to_string()
        } else {
            routes
        }
    )
}

fn render_harness_orchestration(plan: &CliWrapperPlanReport) -> String {
    let contract = &plan.orchestration_contract;
    let stages = contract
        .routing_stages
        .iter()
        .take(8)
        .map(|stage| format!("{}:{}->{}", stage.id, stage.owner, stage.target))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "control {}; status {}; gates {}; stages {}",
        contract.control_plane,
        contract.status,
        if contract.gates.is_empty() {
            "none".to_string()
        } else {
            contract.gates.join(", ")
        },
        if stages.is_empty() {
            "none".to_string()
        } else {
            stages
        }
    )
}

fn render_harness_executor_compatibility(report: &HarnessExecutorCompatibilityReport) -> String {
    let selected = &report.selected_compatibility;
    let surfaces = selected
        .supported_surfaces
        .iter()
        .take(5)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let readiness = selected
        .readiness
        .iter()
        .take(8)
        .map(|item| format!("{}:{}", item.surface, item.status))
        .collect::<Vec<_>>()
        .join(", ");
    let families = report
        .canonical_executor_families
        .iter()
        .map(|family| format!("{}={}", family.executor, family.adapter_family))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{} status {}; selected {} via {}; posture {}; default_ready {}; score {}%; native {}; surfaces {}; readiness {}; blocked {}; recommended {}; families {}",
        report.schema_version,
        report.status,
        report.selected_executor,
        report.selected_adapter_family,
        selected.adoption_posture,
        selected.ready_as_forge_first_default,
        selected.readiness_score_percent,
        selected.native_entrypoint,
        if surfaces.is_empty() {
            "none".to_string()
        } else {
            surfaces
        },
        if readiness.is_empty() {
            "none".to_string()
        } else {
            readiness
        },
        if selected.blocked_surfaces.is_empty() {
            "none".to_string()
        } else {
            selected.blocked_surfaces.join(", ")
        },
        if selected.recommended_surfaces.is_empty() {
            "none".to_string()
        } else {
            selected.recommended_surfaces.join(", ")
        },
        if families.is_empty() {
            "none".to_string()
        } else {
            families
        }
    )
}

fn render_harness_lifecycle_gates(plan: &HarnessSessionLifecyclePlan) -> String {
    let missing = if plan.missing_lineage.is_empty() {
        "none".to_string()
    } else {
        plan.missing_lineage.join(", ")
    };
    let gates = plan
        .gates
        .iter()
        .take(6)
        .map(|gate| {
            format!(
                "{}:{} {}",
                gate.gate_id,
                gate.status,
                gate.command.join(" ")
            )
        })
        .collect::<Vec<_>>()
        .join(" | ");
    format!(
        "{} lineage {}; missing {}; gates {}",
        plan.session_id,
        plan.lineage_complete,
        missing,
        if gates.is_empty() {
            "none".to_string()
        } else {
            gates
        }
    )
}

fn render_harness_headroom_stats(stats: &HeadroomStatsReport) -> String {
    let source = stats
        .primary_savings_source
        .as_deref()
        .or_else(|| stats.by_source.first().map(|bucket| bucket.source.as_str()))
        .unwrap_or("none");
    let kind = stats
        .primary_savings_content_kind
        .as_deref()
        .or_else(|| {
            stats
                .by_content_kind
                .first()
                .map(|bucket| bucket.content_kind.as_str())
        })
        .unwrap_or("none");
    let retrieve = stats
        .next_commands
        .iter()
        .find(|command| command.contains("retrieve-headroom"))
        .map(String::as_str)
        .unwrap_or("none");
    format!(
        "blobs {}; saved_tokens {}; average_savings {:.2}%; source {}; kind {}; retrieve {}",
        stats.total_blobs,
        stats.total_estimated_saved_tokens,
        stats.average_savings_percent,
        source,
        kind,
        retrieve
    )
}

pub fn render_interactive_command_palette(panel: &InteractiveCommandPalettePanel) -> String {
    let query = if panel.query.is_empty() {
        "none"
    } else {
        panel.query.as_str()
    };
    format!(
        "Command palette: {status}; query {query}, groups {group_count}, entries {entry_count}\nEntries: {entries}\n",
        status = panel.status,
        query = query,
        group_count = panel.group_count,
        entry_count = panel.entry_count,
        entries = render_command_palette_entry_summary(panel),
    )
}

pub fn render_interactive_action_registry(panel: &InteractiveActionRegistryPanel) -> String {
    let query = if panel.query.is_empty() {
        "none"
    } else {
        panel.query.as_str()
    };
    format!(
        "Action registry: {status}; query {query}, groups {group_count}, actions {action_count}, enabled {enabled_action_count}, blocked {blocked_action_count}, diagnostic {diagnostic_action_count}\nActions: {actions}\n",
        status = panel.status,
        query = query,
        group_count = panel.group_count,
        action_count = panel.action_count,
        enabled_action_count = panel.enabled_action_count,
        blocked_action_count = panel.blocked_action_count,
        diagnostic_action_count = panel.diagnostic_action_count,
        actions = render_action_registry_action_summary(panel),
    )
}

pub fn render_interactive_action_invocation(report: &InteractiveActionInvocationReport) -> String {
    let command = if report.selected_command_text.is_empty() {
        "none".to_string()
    } else {
        report.selected_command_text.clone()
    };
    format!(
        "Action invocation: {status}; action {action_id}; matches {match_count}; can_execute {can_execute}; diagnostic_only {diagnostic_only}; not executed\nSelected command: {command}\nSource: source {source_panel}; risk {risk_level}; mutates_workflow {mutates_workflow}; requires_approval {requires_approval}\nRecommended action: {recommended_action}; blocked_reason {blocked_reason}\n",
        status = report.status,
        action_id = report.requested_action_id,
        match_count = report.match_count,
        can_execute = report.can_execute,
        diagnostic_only = report.diagnostic_only,
        command = command,
        source_panel = report.source_panel,
        risk_level = report.risk_level,
        mutates_workflow = report.mutates_workflow,
        requires_approval = report.requires_approval,
        recommended_action = report.recommended_action,
        blocked_reason = report.blocked_reason,
    )
}

pub fn render_interactive_autocomplete(panel: &InteractiveAutocompletePanel) -> String {
    let input = if panel.input.is_empty() {
        "none"
    } else {
        panel.input.as_str()
    };
    format!(
        "Autocomplete: {status}; input {input}, suggestions {suggestion_count}\nSuggestions: {suggestions}\n",
        status = panel.status,
        input = input,
        suggestion_count = panel.suggestion_count,
        suggestions = render_autocomplete_suggestion_summary(panel),
    )
}

pub fn render_interactive_sessions(panel: &InteractiveSessionsPanel) -> String {
    let session_cards = render_session_card_summary(panel);
    let operation_details = render_session_operation_summary(panel);
    let next_actions = if panel.next_actions.is_empty() {
        "none".to_string()
    } else {
        panel.next_actions.join(" | ")
    };
    format!(
        "Session center: {status}; controller {controller}; sessions {session_count}, ready {ready_session_count}, planned events {planned_event_count}, lifecycle events {lifecycle_event_count}\nSessions: {session_cards}\nOperations: {operation_details}\nNext actions: {next_actions}\n",
        status = panel.status,
        controller = panel.controller,
        session_count = panel.session_count,
        ready_session_count = panel.ready_session_count,
        planned_event_count = panel.planned_event_count,
        lifecycle_event_count = panel.lifecycle_event_count,
        session_cards = session_cards,
        operation_details = operation_details,
        next_actions = next_actions,
    )
}

fn render_session_card_summary(panel: &InteractiveSessionsPanel) -> String {
    if panel.session_cards.is_empty() {
        "none".to_string()
    } else {
        panel
            .session_cards
            .iter()
            .take(8)
            .map(|session| {
                format!(
                    "{} {} {} {} {}",
                    session.session_id,
                    session.provider_id,
                    session.readiness,
                    session.lifecycle_state,
                    session.operation_plan.recommended_action
                )
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

fn render_session_operation_summary(panel: &InteractiveSessionsPanel) -> String {
    if panel.session_cards.is_empty() {
        "none".to_string()
    } else {
        panel
            .session_cards
            .iter()
            .take(8)
            .map(|session| {
                let plan = &session.operation_plan;
                format!(
                    "{} action {}; lineage {}; requires context {}; requires handoff {}; requires heartbeat {}; commands history {}; lifecycle {}; launch_plan {}; record_plan {}; context {}; handoff {}; heartbeat {}",
                    session.session_id,
                    plan.recommended_action,
                    plan.lineage_complete,
                    plan.requires_context,
                    plan.requires_handoff,
                    plan.requires_heartbeat,
                    plan.commands.history.join(" "),
                    render_session_lifecycle_command_summary(plan),
                    plan.commands.launch_plan.join(" "),
                    plan.commands.record_plan.join(" "),
                    render_optional_session_command(&plan.commands.context),
                    render_optional_session_command(&plan.commands.handoff),
                    render_optional_session_command(&plan.commands.heartbeat),
                )
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

fn render_session_lifecycle_command_summary(plan: &BrainSessionOperationPlan) -> String {
    [
        plan.commands.open.as_ref(),
        plan.commands.attach.as_ref(),
        plan.commands.detach.as_ref(),
        plan.commands.close.as_ref(),
    ]
    .into_iter()
    .flatten()
    .map(|command| command.join(" "))
    .collect::<Vec<_>>()
    .join(" -> ")
}

fn render_optional_session_command(command: &Option<Vec<String>>) -> String {
    command
        .as_ref()
        .map(|command| command.join(" "))
        .unwrap_or_else(|| "none".to_string())
}

fn render_patch_workbench_file_summary(panel: &InteractivePatchWorkbenchPanel) -> String {
    if panel.files.is_empty() {
        "none".to_string()
    } else {
        panel
            .files
            .iter()
            .take(12)
            .map(|file| {
                format!(
                    "{} ({} -> {})",
                    file.path, file.status_label, file.action_hint.suggested_next_action
                )
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

fn render_permission_membership_summary(panel: &InteractivePermissionsPanel) -> String {
    if panel.memberships.is_empty() {
        "none".to_string()
    } else {
        panel
            .memberships
            .iter()
            .take(8)
            .map(|membership| {
                format!(
                    "{}:{}@{} ({}, {} permissions, grants {}, denies {}, commands {})",
                    membership.subject_scope,
                    membership.subject_id,
                    membership.tenant_path,
                    membership.role,
                    membership.permission_count,
                    render_permission_list(&membership.permission_grants),
                    render_permission_list(&membership.permission_denies),
                    membership.commands.update.join(" ")
                )
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

fn render_addon_permission_summary(panel: &InteractivePermissionsPanel) -> String {
    if panel.addon_permissions.is_empty() {
        "none".to_string()
    } else {
        panel
            .addon_permissions
            .iter()
            .take(8)
            .map(|authorization| {
                format!(
                    "{}:{} ({}, risk {}, approved_by {}, source {}, commands {})",
                    authorization.addon_id,
                    authorization.permission_id,
                    authorization.status,
                    authorization.risk,
                    authorization.approved_by,
                    authorization.source,
                    authorization.commands.revoke.join(" ")
                )
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

fn render_permission_approval_summary(panel: &InteractivePermissionsPanel) -> String {
    if panel.approval_items.is_empty() {
        "none".to_string()
    } else {
        panel
            .approval_items
            .iter()
            .take(8)
            .map(|item| {
                format!(
                    "{}:{} {} ({}, required {}, commands {})",
                    item.workflow_id,
                    item.task_id,
                    item.kind,
                    item.state,
                    item.required,
                    item.commands.answer.join(" ")
                )
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

fn render_permission_list(items: &[String]) -> String {
    if items.is_empty() {
        "none".to_string()
    } else {
        items.join(",")
    }
}

pub fn render_interactive_workflow_dag(panel: &InteractiveWorkflowDagPanel) -> String {
    format!(
        "Workflow DAG: {status}; workflows {workflow_count}, nodes {node_count}, edges {edge_count}, running {running}, blocked {blocked}, waits {waits}, human waits {human_waits}\nWorkflows: {workflows}\nNodes: {nodes}\nEdges: {edges}\nCommands: {commands}\n",
        status = panel.status,
        workflow_count = panel.workflow_count,
        node_count = panel.node_count,
        edge_count = panel.edge_count,
        running = panel.running_node_count,
        blocked = panel.blocked_node_count,
        waits = panel.wait_node_count,
        human_waits = panel.human_wait_count,
        workflows = render_workflow_dag_summary(panel),
        nodes = render_workflow_dag_node_summary(panel),
        edges = render_workflow_dag_edge_summary(panel),
        commands = render_workflow_dag_command_summary(panel),
    )
}

pub fn render_interactive_schedules(panel: &InteractiveSchedulePanel) -> String {
    format!(
        "Schedules: {status}; worker {executor}; scanned {scanned}, due {due}, runnable {runnable}, blocked {blocked}, idle {idle}, cron {cron}, wait_until {wait_until}, delay {delay}, scale_to_zero {scale_to_zero}, next {next}, sleep {sleep_seconds}s\nWorker pool: max {max_workers}, available {available_workers}, assignable {assignable}, queued {queued}, backpressure {backpressure}, deterministic {deterministic}\nAssignments: assigned {assigned}; queued {queued_workflows}\nWorkflows: {workflows}\nCommands: {commands}\n",
        status = panel.status,
        executor = panel.executor,
        scanned = panel.scanned_workflows,
        due = panel.due_workflows,
        runnable = panel.runnable_due_workflows,
        blocked = panel.blocked_due_workflows,
        idle = panel.idle_workflows,
        cron = panel.cron_nodes,
        wait_until = panel.wait_until_nodes,
        delay = panel.delay_nodes,
        scale_to_zero = panel.scale_to_zero_workflows,
        next = panel.next_wakeup_at.as_deref().unwrap_or("none"),
        sleep_seconds = panel.sleep_seconds,
        max_workers = panel.worker_pool.max_workers,
        available_workers = panel.worker_pool.available_workers,
        assignable = panel.worker_pool.assignable_due_workflows,
        queued = panel.queued_due_workflows,
        backpressure = panel.backpressure_active,
        deterministic = panel.worker_pool.deterministic,
        assigned = render_schedule_assignment_summary(&panel.assigned_workflows),
        queued_workflows = render_schedule_assignment_summary(&panel.queued_workflows),
        workflows = render_schedule_workflow_summary(panel),
        commands = if panel.commands.is_empty() {
            "none".to_string()
        } else {
            panel.commands.join(" | ")
        },
    )
}

pub fn render_interactive_context_memory(panel: &InteractiveContextMemoryPanel) -> String {
    format!(
        "Context/memory center: {status}; project {project_root}; ready handoffs {ready_for_handoff}, blocked {blocked_tasks}, budget pressure {context_budget_pressure}\nMemory policy: {memory_policy_status}; level {memory_level}, scopes {memory_scopes}, audience {memory_audience}, governance {governance_status}, levels {memory_level_count}; temporary {temporary_memory_rule}\nContext actions: workflows {action_workflows}, tasks {action_tasks}, ready {action_ready}, blocked {action_blocked}, increase budget {increase_budget}, resume checkpoints {resume_checkpoints}, partial retry {partial_retry}\nContext quality: passed {quality_passed}, warnings {quality_warnings}, blocked {quality_blocked}, required missing {required_missing}, compressed {compressed}, score avg {average_score_bps}bps\nCommands: {commands}\nNext actions: {next_actions}\n",
        status = panel.status,
        project_root = panel.project_root,
        ready_for_handoff = panel.ready_for_handoff,
        blocked_tasks = panel.blocked_tasks,
        context_budget_pressure = panel.context_budget_pressure,
        memory_policy_status = panel.memory_policy_status,
        memory_level = panel.memory_policy.effective_defaults.memory_level.as_str(),
        memory_scopes = panel
            .memory_policy
            .effective_defaults
            .default_scopes
            .join(","),
        memory_audience = panel
            .memory_policy
            .effective_defaults
            .default_audience
            .as_str(),
        governance_status = panel.memory_policy.project_governance.status.as_str(),
        memory_level_count = panel.memory_level_count,
        temporary_memory_rule = panel.temporary_memory_rule,
        action_workflows = panel.context_actions.workflows,
        action_tasks = panel.context_actions.total_tasks,
        action_ready = panel.context_actions.ready_for_handoff,
        action_blocked = panel.context_actions.blocked_tasks,
        increase_budget = panel.context_actions.increase_context_budget,
        resume_checkpoints = panel.context_actions.resume_from_checkpoint,
        partial_retry = panel.context_actions.partial_retry_recommended,
        quality_passed = panel.context_quality.passed,
        quality_warnings = panel.context_quality.total_warnings,
        quality_blocked = panel.context_quality.blocked,
        required_missing = panel.context_quality.required_context_missing,
        compressed = panel.context_quality.compressed_context,
        average_score_bps = panel.context_quality.average_score_bps,
        commands = render_context_memory_command_summary(panel),
        next_actions = if panel.next_actions.is_empty() {
            "none".to_string()
        } else {
            panel.next_actions.join(" | ")
        },
    )
}

pub fn render_interactive_structured_logs(panel: &InteractiveStructuredLogsPanel) -> String {
    let next_cursor = panel
        .next_cursor
        .map(|cursor| cursor.to_string())
        .unwrap_or_else(|| "none".to_string());
    format!(
        "Structured logs: {status}; logs {log_count}/{total_event_count}, next cursor {next_cursor}, has more {has_more}\nLogs: {logs}\n",
        status = panel.status,
        log_count = panel.log_count,
        total_event_count = panel.total_event_count,
        next_cursor = next_cursor,
        has_more = panel.has_more,
        logs = render_structured_log_summary(panel),
    )
}

fn render_context_memory_command_summary(panel: &InteractiveContextMemoryPanel) -> String {
    let mut commands = Vec::new();
    for (name, command) in &panel.memory_commands {
        commands.push(format!("memory {name}: forge {}", command.join(" ")));
    }
    for (name, command) in &panel.context_commands {
        commands.push(format!("context {name}: forge {}", command.join(" ")));
    }
    if commands.is_empty() {
        "none".to_string()
    } else {
        commands.join(" | ")
    }
}

pub fn render_interactive_addon_capabilities(panel: &InteractiveAddonCapabilityPanel) -> String {
    format!(
        "Addons/capabilities: {status}; project {project_root}; addons {addon_count}, enabled {enabled_addon_count}, unauthorized {unauthorized_addon_count}, capabilities {capability_count}, enabled capabilities {enabled_capability_count}, disabled capabilities {disabled_capability_count}, permissions {permission_count}, runtime contracts {runtime_contract_count}, views {view_count}, dispatches {dispatch_count}, queued {queued_dispatch_count}, event types {event_type_count}, channels {event_channel_count}, triggers {event_trigger_count}, listeners {event_listener_count}, adapters {event_adapter_count}\nCapabilities: {capabilities}\nEvent extensions: {event_extensions}\nCommands: {commands}\n",
        status = panel.status,
        project_root = panel.project_root,
        addon_count = panel.addon_count,
        enabled_addon_count = panel.enabled_addon_count,
        unauthorized_addon_count = panel.unauthorized_addon_count,
        capability_count = panel.capability_count,
        enabled_capability_count = panel.enabled_capability_count,
        disabled_capability_count = panel.disabled_capability_count,
        permission_count = panel.permission_count,
        runtime_contract_count = panel.runtime_contract_count,
        view_count = panel.view_count,
        dispatch_count = panel.dispatch_count,
        queued_dispatch_count = panel.queued_dispatch_count,
        event_type_count = panel.event_type_count,
        event_channel_count = panel.event_channel_count,
        event_trigger_count = panel.event_trigger_count,
        event_listener_count = panel.event_listener_count,
        event_adapter_count = panel.event_adapter_count,
        capabilities = render_addon_capability_summary(panel),
        event_extensions = render_addon_event_extension_summary(panel),
        commands = if panel.commands.is_empty() {
            "none".to_string()
        } else {
            panel.commands.join(" | ")
        },
    )
}

fn render_addon_event_extension_summary(panel: &InteractiveAddonCapabilityPanel) -> String {
    if panel.event_extensions.is_empty() {
        "none".to_string()
    } else {
        panel
            .event_extensions
            .iter()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

fn build_interactive_event_panel(timeline: &GlobalEventTimelineReport) -> InteractiveEventPanel {
    InteractiveEventPanel {
        status: timeline.status.clone(),
        total_event_count: timeline.total_event_count,
        visible_event_count: timeline.event_count,
        latest_events: timeline
            .events
            .iter()
            .rev()
            .take(5)
            .map(|event| format!("{} {} {}", event.occurred_at, event.workflow_id, event.kind))
            .collect(),
    }
}

fn build_event_runtime_panel(
    store: &ForgeStore,
    project_root: &Path,
    workflows: &[WorkflowRegistryRow],
) -> InteractiveEventRuntimePanel {
    let project_root_text = project_root.display().to_string();
    let commands = event_runtime_commands(&project_root_text);
    let workflow_lifecycle = build_event_workflow_lifecycle_panel(&project_root_text);
    let persistent_workflow_count = workflows
        .iter()
        .filter(|workflow| workflow.runtime.persistent)
        .count();
    let wakeable_workflow_count = workflows
        .iter()
        .filter(|workflow| {
            matches!(
                workflow.runtime.operator_action.as_str(),
                "keep_event_listener_ready" | "wake_on_event"
            )
        })
        .count();

    let operating_context = match load_project_operating_context(project_root) {
        Ok(context) => context,
        Err(error) => {
            return InteractiveEventRuntimePanel {
                schema_version: INTERACTIVE_EVENT_RUNTIME_SCHEMA_VERSION.to_string(),
                status: "event_runtime_unavailable".to_string(),
                project_root: project_root_text,
                pending_event_count: 0,
                sampled_event_count: 0,
                service_count: 0,
                running_service_count: 0,
                persistent_workflow_count,
                wakeable_workflow_count,
                action_required: false,
                recommended_action: "inspect_event_runtime".to_string(),
                recommendation_reason: format!("failed to load operating context: {error}"),
                workflow_lifecycle,
                event_cards: Vec::new(),
                service_cards: Vec::new(),
                commands,
                notes: vec![
                    "Event runtime panel is read-only and does not route events.".to_string(),
                ],
            };
        }
    };

    let inbox =
        list_inbound_event_inbox_for_context(store, Some("pending"), 20, &operating_context);
    let (pending_event_count, event_cards, mut notes) = match inbox {
        Ok(report) => {
            let cards = report
                .events
                .iter()
                .take(10)
                .map(|event| InteractiveEventRuntimeEventCard {
                    event_id: event.id.clone(),
                    origin: event.origin.clone(),
                    action: event.action.clone(),
                    status: event.status.clone(),
                    workflow_id: event
                        .data
                        .get("workflow_id")
                        .and_then(|value| value.as_str())
                        .map(ToString::to_string),
                    created_at: event.created_at.clone(),
                })
                .collect::<Vec<_>>();
            (report.event_count, cards, Vec::new())
        }
        Err(error) => (
            0,
            Vec::new(),
            vec![format!("pending inbox unavailable: {error}")],
        ),
    };

    let services = list_event_services(store, project_root, Some("worker"), None, 20);
    let (service_count, running_service_count, service_cards) = match services {
        Ok(report) => {
            let running = report
                .services
                .iter()
                .filter(|service| service.status == "running")
                .count();
            let cards = report
                .services
                .iter()
                .take(10)
                .map(|service| InteractiveEventRuntimeServiceCard {
                    service_id: service.id.clone(),
                    service_kind: service.service_kind.clone(),
                    status: service.status.clone(),
                    lease_owner: service.lease_owner.clone(),
                    lease_expires_at: service.lease_expires_at.clone(),
                })
                .collect::<Vec<_>>();
            (report.service_count, running, cards)
        }
        Err(error) => {
            notes.push(format!("event services unavailable: {error}"));
            (0, 0, Vec::new())
        }
    };

    let action_required =
        pending_event_count > 0 || (wakeable_workflow_count > 0 && running_service_count == 0);
    let recommended_action = if action_required && running_service_count == 0 {
        "start_event_worker_supervisor"
    } else if pending_event_count > 0 {
        "observe_active_event_worker"
    } else if wakeable_workflow_count > 0 {
        "keep_event_worker_ready"
    } else {
        "no_event_runtime_action"
    };
    let recommendation_reason = if pending_event_count > 0 && wakeable_workflow_count > 0 {
        "pending inbound events and wakeable persistent workflows require event worker supervision"
    } else if pending_event_count > 0 {
        "pending inbound events require event worker supervision"
    } else if wakeable_workflow_count > 0 && running_service_count == 0 {
        "persistent workflows are waiting for events without an active worker"
    } else {
        "no pending inbound events or wakeable workflows require action"
    };
    let status = if action_required {
        "event_runtime_action_required"
    } else if running_service_count > 0 {
        "event_runtime_worker_running"
    } else {
        "event_runtime_idle"
    };
    notes.push(
        "Read-only panel; run the recommended command explicitly to route events.".to_string(),
    );

    InteractiveEventRuntimePanel {
        schema_version: INTERACTIVE_EVENT_RUNTIME_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        project_root: project_root_text,
        pending_event_count,
        sampled_event_count: event_cards.len(),
        service_count,
        running_service_count,
        persistent_workflow_count,
        wakeable_workflow_count,
        action_required,
        recommended_action: recommended_action.to_string(),
        recommendation_reason: recommendation_reason.to_string(),
        workflow_lifecycle,
        event_cards,
        service_cards,
        commands,
        notes,
    }
}

fn build_event_workflow_lifecycle_panel(
    project_root: &str,
) -> InteractiveEventWorkflowLifecyclePanel {
    let actions = vec![
        event_workflow_lifecycle_action(
            project_root,
            EventWorkflowLifecycleActionSpec {
                action: "start_workflow",
                normalized_route: "start_workflow",
                status: "validated",
                purpose:
                    "Create a workflow from an inbound event without requiring an existing workflow.",
                required_payload_fields: &["goal"],
                example_input: r#"{"goal":"Demonstrate an event-started workflow"}"#,
                acceptance_gates: &[
                "workflow_id is created and persisted",
                "inbound event status becomes routed",
                "tenant policy is checked before workflow creation",
            ],
            },
        ),
        event_workflow_lifecycle_action(
            project_root,
            EventWorkflowLifecycleActionSpec {
                action: "continue_workflow",
                normalized_route: "continue_workflow",
                status: "validated",
                purpose: "Continue an existing workflow by attaching an artifact, recording a checkpoint, answering a human wait, completing a ready task or driving a run.",
                required_payload_fields: &["workflow_id or run_id", "continue_action"],
                example_input: r#"{"workflow_id":"<workflow-id>","continue_action":"drive_run"}"#,
                acceptance_gates: &[
                "workflow_id or run_id resolves to one workflow",
                "continue_action is one of attach_artifact, checkpoint, answer_interaction, complete_task or drive_run",
                "route result is recorded under event lineage",
            ],
            },
        ),
        event_workflow_lifecycle_action(
            project_root,
            EventWorkflowLifecycleActionSpec {
                action: "pause_workflow",
                normalized_route: "pause_workflow",
                status: "validated",
                purpose: "Pause a workflow through the same event inbox used by external channels.",
                required_payload_fields: &["workflow_id"],
                example_input: r#"{"workflow_id":"<workflow-id>"}"#,
                acceptance_gates: &[
                "workflow status is changed through Forge-owned workflow mutation",
                "pause revision is persisted",
                "event routing records the originating adapter policy",
            ],
            },
        ),
        event_workflow_lifecycle_action(
            project_root,
            EventWorkflowLifecycleActionSpec {
                action: "resume_workflow",
                normalized_route: "resume_workflow",
                status: "validated",
                purpose: "Resume a paused workflow and derive the correct runnable status from current task state.",
                required_payload_fields: &["workflow_id"],
                example_input: r#"{"workflow_id":"<workflow-id>"}"#,
                acceptance_gates: &[
                "workflow status is restored through Forge-owned workflow mutation",
                "resume revision is persisted",
                "event routing records the originating adapter policy",
            ],
            },
        ),
        event_workflow_lifecycle_action(
            project_root,
            EventWorkflowLifecycleActionSpec {
                action: "modify_workflow",
                normalized_route: "modify_workflow",
                status: "validated",
                purpose: "Modify a live workflow goal from an event without stopping the workflow.",
                required_payload_fields: &["workflow_id", "goal"],
                example_input: r#"{"workflow_id":"<workflow-id>","goal":"Updated operating objective"}"#,
                acceptance_gates: &[
                "goal revision is persisted",
                "intent is reparsed from the new goal",
                "previous and new deliverables are reported",
            ],
            },
        ),
        event_workflow_lifecycle_action(
            project_root,
            EventWorkflowLifecycleActionSpec {
                action: "end_workflow",
                normalized_route: "complete_workflow",
                status: "validated_with_completion_gate",
                purpose: "End a workflow by routing the event to the validation-gated completion path.",
                required_payload_fields: &["workflow_id"],
                example_input: r#"{"workflow_id":"<workflow-id>"}"#,
                acceptance_gates: &[
                "alias end_workflow normalizes to complete_workflow",
                "validate_workflow must be promotable before completion",
                "completion revision and route result are persisted",
            ],
            },
        ),
    ];
    let validated_action_count = actions
        .iter()
        .filter(|action| action.status.starts_with("validated"))
        .count();
    let needs_attention_count = actions.len().saturating_sub(validated_action_count);

    InteractiveEventWorkflowLifecyclePanel {
        schema_version: INTERACTIVE_EVENT_WORKFLOW_LIFECYCLE_SCHEMA_VERSION.to_string(),
        status: if needs_attention_count == 0 {
            "event_workflow_lifecycle_ready".to_string()
        } else {
            "event_workflow_lifecycle_needs_attention".to_string()
        },
        action_count: actions.len(),
        validated_action_count,
        needs_attention_count,
        core_owned_actions: actions
            .iter()
            .map(|action| action.action.clone())
            .collect::<Vec<_>>(),
        addon_owned_channels: vec![
            "telegram".to_string(),
            "whatsapp".to_string(),
            "discord".to_string(),
            "email".to_string(),
            "sms".to_string(),
            "voice".to_string(),
            "api".to_string(),
            "webhook".to_string(),
            "cron".to_string(),
            "kafka".to_string(),
            "rabbitmq".to_string(),
            "mqtt".to_string(),
            "database".to_string(),
            "file_watch".to_string(),
            "sensor".to_string(),
            "telemetry".to_string(),
        ],
        actions,
        notes: vec![
            "Core owns generic workflow lifecycle mutations; Addons own channel ingress, auth, schema mapping and permissions.".to_string(),
            "The panel is read-only; operators still execute explicit event commands or managed workers.".to_string(),
        ],
    }
}

struct EventWorkflowLifecycleActionSpec<'a> {
    action: &'a str,
    normalized_route: &'a str,
    status: &'a str,
    purpose: &'a str,
    required_payload_fields: &'a [&'a str],
    example_input: &'a str,
    acceptance_gates: &'a [&'a str],
}

fn event_workflow_lifecycle_action(
    project_root: &str,
    spec: EventWorkflowLifecycleActionSpec<'_>,
) -> InteractiveEventWorkflowLifecycleAction {
    InteractiveEventWorkflowLifecycleAction {
        action: spec.action.to_string(),
        normalized_route: spec.normalized_route.to_string(),
        status: spec.status.to_string(),
        purpose: spec.purpose.to_string(),
        required_payload_fields: spec
            .required_payload_fields
            .iter()
            .map(|field| (*field).to_string())
            .collect(),
        core_boundary: "Forge Core routes lifecycle actions and persists workflow/event lineage; it does not embed channel-specific handlers.".to_string(),
        addon_boundary: "Ingress channels, auth verification, schema mapping, permissions and external delivery stay in Addons/adapters.".to_string(),
        primary_command: vec![
            "forge".to_string(),
            "events".to_string(),
            "ingest".to_string(),
            "--origin".to_string(),
            "webhook".to_string(),
            "--action".to_string(),
            spec.action.to_string(),
            "--project-root".to_string(),
            project_root.to_string(),
            "--input".to_string(),
            spec.example_input.to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        evidence_commands: vec![
            "forge events route --event <event-id> --project-root <project-root> --output json".to_string(),
            "forge events inbox --status routed --project-root <project-root> --output json".to_string(),
            "forge interactive event-runtime --project-root <project-root> --output json".to_string(),
            "forge interactive structured-logs --output json".to_string(),
        ],
        acceptance_gates: spec
            .acceptance_gates
            .iter()
            .map(|gate| (*gate).to_string())
            .collect(),
        risk_controls: vec![
            "adapter policy must allow the normalized action when a matching Addon adapter is declared".to_string(),
            "tenant policy is enforced before exposing or mutating tenant-bound workflows".to_string(),
            "mutations are persisted through Forge-owned workflow APIs, never directly by channel handlers".to_string(),
        ],
    }
}

fn event_runtime_commands(project_root: &str) -> InteractiveEventRuntimeCommands {
    InteractiveEventRuntimeCommands {
        inbox: vec![
            "forge".to_string(),
            "events".to_string(),
            "inbox".to_string(),
            "--status".to_string(),
            "pending".to_string(),
            "--project-root".to_string(),
            project_root.to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        runtime_reconcile: vec![
            "forge".to_string(),
            "events".to_string(),
            "runtime-reconcile".to_string(),
            "--project-root".to_string(),
            project_root.to_string(),
            "--recover-stale-services".to_string(),
            "--scan-schedules".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        service_supervise: vec![
            "forge".to_string(),
            "events".to_string(),
            "service-supervise".to_string(),
            "--kind".to_string(),
            "worker".to_string(),
            "--project-root".to_string(),
            project_root.to_string(),
            "--status".to_string(),
            "pending".to_string(),
            "--limit".to_string(),
            "20".to_string(),
            "--max-runs".to_string(),
            "12".to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        webhook_ingress: vec![
            "forge".to_string(),
            "events".to_string(),
            "webhook-ingress".to_string(),
            "--host".to_string(),
            "127.0.0.1".to_string(),
            "--port".to_string(),
            "8787".to_string(),
            "--path".to_string(),
            "/webhook".to_string(),
            "--project-root".to_string(),
            project_root.to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
        services: vec![
            "forge".to_string(),
            "events".to_string(),
            "services".to_string(),
            "--project-root".to_string(),
            project_root.to_string(),
            "--output".to_string(),
            "json".to_string(),
        ],
    }
}

fn build_structured_logs_panel(
    timeline: &GlobalEventTimelineReport,
) -> InteractiveStructuredLogsPanel {
    let logs = timeline
        .events
        .iter()
        .rev()
        .take(12)
        .map(structured_log_entry)
        .collect::<Vec<_>>();

    InteractiveStructuredLogsPanel {
        schema_version: INTERACTIVE_STRUCTURED_LOGS_SCHEMA_VERSION.to_string(),
        status: "structured_logs_ready".to_string(),
        total_event_count: timeline.total_event_count,
        log_count: logs.len(),
        next_cursor: timeline.page.next_cursor,
        has_more: timeline.page.has_more,
        logs,
    }
}

fn structured_log_entry(event: &WorkflowEventEnvelope) -> InteractiveStructuredLogEntry {
    InteractiveStructuredLogEntry {
        event_id: event.event_id.clone(),
        store_sequence: event.store_sequence,
        workflow_id: event.workflow_id.clone(),
        kind: event.kind.clone(),
        category: event.category.clone(),
        severity: event.severity.clone(),
        origin: event.origin.clone(),
        source: event.source.clone(),
        occurred_at: event.occurred_at.clone(),
        correlation: structured_log_correlation(event),
        observability: serde_json::to_value(&event.observability).unwrap_or_default(),
        payload_preview: truncate_display(&event.data.to_string(), 240),
    }
}

fn structured_log_correlation(event: &WorkflowEventEnvelope) -> serde_json::Value {
    let mut correlation = serde_json::to_value(&event.correlation).unwrap_or_default();
    if let serde_json::Value::Object(map) = &mut correlation {
        ensure_correlation_field(
            map,
            "task_id",
            structured_log_nested_string(
                &event.data,
                &["task_id", "task"],
                &[
                    ("checkpoint", &["task_id", "task"]),
                    ("interaction", &["task_id", "task"]),
                    ("task", &["id", "task_id"]),
                ],
            ),
        );
        ensure_correlation_field(
            map,
            "artifact_id",
            structured_log_nested_string(
                &event.data,
                &["artifact_id", "artifact"],
                &[("artifact", &["id", "artifact_id"])],
            ),
        );
        ensure_correlation_field(
            map,
            "interaction_id",
            structured_log_nested_string(
                &event.data,
                &["interaction_id"],
                &[("interaction", &["id", "interaction_id"])],
            ),
        );
    }
    correlation
}

fn ensure_correlation_field(
    map: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: Option<String>,
) {
    let has_value = map
        .get(key)
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| !value.trim().is_empty());
    if !has_value {
        if let Some(value) = value {
            map.insert(key.to_string(), serde_json::Value::String(value));
        }
    }
}

fn structured_log_nested_string(
    data: &serde_json::Value,
    top_level_keys: &[&str],
    nested_keys: &[(&str, &[&str])],
) -> Option<String> {
    structured_log_string(data, top_level_keys).or_else(|| {
        nested_keys.iter().find_map(|(container, keys)| {
            data.get(*container)
                .and_then(|value| structured_log_string(value, keys))
        })
    })
}

fn structured_log_string(data: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| data.get(*key).and_then(serde_json::Value::as_str))
        .map(str::to_string)
}

fn build_navigation_panel() -> InteractiveNavigationPanel {
    InteractiveNavigationPanel {
        schema_version: INTERACTIVE_NAVIGATION_SCHEMA_VERSION.to_string(),
        status: "navigation_ready".to_string(),
        default_display_mode: "detailed".to_string(),
        display_modes: vec![
            "compact".to_string(),
            "detailed".to_string(),
            "focus".to_string(),
        ],
        active_theme: "forge_dark".to_string(),
        themes: vec![
            "forge_dark".to_string(),
            "forge_light".to_string(),
            "high_contrast".to_string(),
        ],
        keybindings: vec![
            navigation_key("j", "focus_next", "panel", "Move focus to the next panel"),
            navigation_key(
                "k",
                "focus_previous",
                "panel",
                "Move focus to the previous panel",
            ),
            navigation_key("enter", "open_focused", "panel", "Open the focused item"),
            navigation_key(
                "/",
                "command_palette",
                "global",
                "Open slash command routing",
            ),
            navigation_key("t", "cycle_theme", "global", "Cycle available themes"),
            navigation_key(
                "m",
                "cycle_display_mode",
                "global",
                "Cycle compact, detailed and focus display modes",
            ),
            navigation_key(
                "r",
                "refresh",
                "global",
                "Refresh the advanced cockpit frame",
            ),
        ],
    }
}

fn navigation_key(
    key: &str,
    action: &str,
    target: &str,
    description: &str,
) -> InteractiveKeyBinding {
    InteractiveKeyBinding {
        key: key.to_string(),
        action: action.to_string(),
        target: target.to_string(),
        description: description.to_string(),
    }
}

fn build_interactive_addon_renderer_panel(
    report: &OpsAddonViewRendererReport,
) -> InteractiveAddonRendererPanel {
    InteractiveAddonRendererPanel {
        status: report.status.clone(),
        renderer_count: report.renderer_count,
        safe_renderer_count: report.safe_renderer_count,
        family_count: report.family_count,
        families: report.families.clone(),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_operational_cockpit_panel(
    active_runs: usize,
    runs_needing_attention: usize,
    scheduled_workflows: usize,
    looping_workflows: usize,
    paused_idle_workflows: usize,
    pending_approvals: usize,
    validation_failures: usize,
    task_board: &InteractiveTaskBoardPanel,
    schedule: &InteractiveSchedulePanel,
    sessions: &InteractiveSessionsPanel,
    harness: &InteractiveHarnessPanel,
    structured_logs: &InteractiveStructuredLogsPanel,
    cost: &InteractiveCostPanel,
    context_memory: &InteractiveContextMemoryPanel,
    modifier_lane: &OpsModifierLane,
    event_runtime: &InteractiveEventRuntimePanel,
) -> InteractiveOperationalCockpitPanel {
    let active_work_count = active_runs
        + task_board.ready_handoffs
        + schedule.runnable_due_workflows
        + event_runtime.pending_event_count;
    let attention_level = if validation_failures > 0 || runs_needing_attention > 0 {
        "critical"
    } else if pending_approvals > 0
        || task_board.pending_human_interactions > 0
        || task_board.ready_handoffs > 0
        || schedule.due_workflows > 0
        || !harness.forge_first_ready
        || modifier_lane.pending_count > 0
        || event_runtime.action_required
    {
        "attention"
    } else {
        "normal"
    };
    let selected_brain = sessions
        .session_report
        .selected_provider_id
        .clone()
        .unwrap_or_else(|| "none".to_string());
    let priority_summary = format!(
        "{active_work_count} active signals, {runs_needing_attention} runs needing attention, {} ready handoffs, {} human waits, {pending_approvals} pending approvals, {} modifier proposals, {} pending events",
        task_board.ready_handoffs,
        task_board.pending_human_interactions,
        modifier_lane.pending_count,
        event_runtime.pending_event_count
    );
    let modifier_panel = build_operational_modifier_lane_panel(modifier_lane);
    let sections = vec![
        operational_cockpit_section(
            "attention",
            "Attention",
            if attention_level == "critical" {
                "critical"
            } else {
                "watch"
            },
            runs_needing_attention + validation_failures,
            format!(
                "{runs_needing_attention} runs need attention; {validation_failures} validation failures"
            ),
            "forge request list --status needs_attention",
            vec![
                "forge request list --status stale".to_string(),
                "forge interactive structured-logs --output json".to_string(),
            ],
        ),
        operational_cockpit_section(
            "workflow",
            "Workflow",
            "operational",
            active_runs + scheduled_workflows + looping_workflows + paused_idle_workflows,
            format!(
                "{active_runs} active, {scheduled_workflows} scheduled, {looping_workflows} looping, {paused_idle_workflows} paused or idle"
            ),
            "forge list",
            vec![
                "forge schedule list --output json".to_string(),
                "forge interactive workflow-dag --output json".to_string(),
            ],
        ),
        operational_cockpit_section(
            "handoff",
            "Handoff",
            if task_board.ready_handoffs > 0 {
                "ready"
            } else {
                "waiting"
            },
            task_board.ready_handoffs + context_memory.blocked_tasks,
            format!(
                "{} ready handoffs; {} context-blocked tasks; {} checkpoints",
                task_board.ready_handoffs,
                context_memory.blocked_tasks,
                task_board.checkpoint_resume_candidates
            ),
            "forge interactive task-board --output json",
            vec![
                "forge interactive readiness --output json".to_string(),
                "forge context --workflow <workflow-id> --task <task-id> --strict --output json"
                    .to_string(),
            ],
        ),
        operational_cockpit_section(
            "human",
            "Human",
            if pending_approvals > 0 || task_board.pending_human_interactions > 0 {
                "awaiting_human"
            } else {
                "clear"
            },
            pending_approvals + task_board.pending_human_interactions,
            format!(
                "{pending_approvals} pending approvals; {} human waits",
                task_board.pending_human_interactions
            ),
            "forge interactive permissions --output json",
            vec![
                "forge interactive identity --output json".to_string(),
                "forge interactions list --output json".to_string(),
            ],
        ),
        operational_cockpit_section(
            "modifier",
            "Modifier",
            if modifier_lane.pending_count > 0 {
                "pending_strategy"
            } else if modifier_lane.applied_count > 0 {
                "applied_history"
            } else {
                "idle"
            },
            modifier_lane.pending_count + modifier_lane.applied_count,
            format!(
                "{} pending proposals; {} applied proposals; mode {}",
                modifier_lane.pending_count,
                modifier_lane.applied_count,
                modifier_panel.operation_mode
            ),
            "forge interactive operational-cockpit --output json",
            vec![
                "forge ops serve --project-root . --host 127.0.0.1 --port 8765"
                    .to_string(),
                "POST /api/modifier/propose-goal".to_string(),
                "POST /api/modifier/apply".to_string(),
            ],
        ),
        operational_cockpit_section(
            "event_runtime",
            "Event runtime",
            if event_runtime.action_required {
                "action_required"
            } else if event_runtime.running_service_count > 0 {
                "worker_running"
            } else {
                "idle"
            },
            event_runtime.pending_event_count + event_runtime.wakeable_workflow_count,
            format!(
                "{} pending events; {} wakeable workflows; {} running workers; recommendation {}",
                event_runtime.pending_event_count,
                event_runtime.wakeable_workflow_count,
                event_runtime.running_service_count,
                event_runtime.recommended_action
            ),
            "forge events runtime-reconcile --project-root . --recover-stale-services --scan-schedules --output json",
            vec![
                "forge events inbox --status pending --project-root . --output json".to_string(),
                "forge events service-supervise --kind worker --project-root . --status pending --limit 20 --max-runs 12 --output json".to_string(),
                "forge events services --project-root . --output json".to_string(),
            ],
        ),
        operational_cockpit_section(
            "brain",
            "Brain",
            if sessions.ready_session_count > 0 {
                "ready"
            } else {
                "needs_sync"
            },
            sessions.ready_session_count + usize::from(harness.forge_first_ready),
            format!(
                "selected brain {selected_brain}; {} ready sessions; harness {}; headroom {}",
                sessions.ready_session_count, harness.doctor.status, harness.headroom_operational_status
            ),
            "forge interactive sessions --output json",
            vec![
                "forge interactive readiness --output json".to_string(),
                "forge interactive harness --output json".to_string(),
            ],
        ),
        operational_cockpit_section(
            "observability",
            "Observability",
            if structured_logs.has_more {
                "has_more"
            } else {
                "current"
            },
            structured_logs.log_count + schedule.due_workflows,
            format!(
                "{} visible logs from {} events; {} due workflows; estimated cost ${:.4}",
                structured_logs.log_count,
                structured_logs.total_event_count,
                schedule.due_workflows,
                cost.estimated_task_cost_total_usd
            ),
            "forge interactive structured-logs --output json",
            vec![
                "forge schedule worker-status --output json".to_string(),
                "forge cost ledger --output json".to_string(),
            ],
        ),
    ];

    InteractiveOperationalCockpitPanel {
        schema_version: INTERACTIVE_OPERATIONAL_COCKPIT_SCHEMA_VERSION.to_string(),
        status: "operational_cockpit_ready".to_string(),
        attention_level: attention_level.to_string(),
        priority_summary,
        active_work_count,
        needs_attention_count: runs_needing_attention,
        ready_handoff_count: task_board.ready_handoffs,
        pending_human_wait_count: task_board.pending_human_interactions,
        pending_approval_count: pending_approvals,
        pending_modifier_proposal_count: modifier_lane.pending_count,
        pending_event_count: event_runtime.pending_event_count,
        validation_failure_count: validation_failures,
        due_workflow_count: schedule.due_workflows,
        selected_brain,
        ready_session_count: sessions.ready_session_count,
        forge_first_ready: harness.forge_first_ready,
        headroom_operational_status: harness.headroom_operational_status.clone(),
        event_count: structured_logs.total_event_count,
        estimated_cost_total_usd: cost.estimated_task_cost_total_usd,
        sections,
        modifier_lane: modifier_panel,
        event_runtime: event_runtime.clone(),
        next_actions: vec![
            "forge interactive operational-cockpit --output json".to_string(),
            "forge interactive task-board --output json".to_string(),
            "forge interactive readiness --output json".to_string(),
            "forge ops serve --project-root . --host 127.0.0.1 --port 8765".to_string(),
            "forge events runtime-reconcile --project-root . --recover-stale-services --scan-schedules --output json".to_string(),
            "forge interactive action-registry --query operational --output json".to_string(),
            "forge interactive structured-logs --output json".to_string(),
            "forge interactive sessions --output json".to_string(),
        ],
    }
}

fn build_operational_modifier_lane_panel(
    lane: &OpsModifierLane,
) -> InteractiveOperationalModifierLanePanel {
    let status = if lane.pending_count > 0 {
        "modifier_lane_pending_review"
    } else if lane.applied_count > 0 {
        "modifier_lane_applied_history"
    } else {
        "modifier_lane_idle"
    };
    let proposal_cards = lane
        .proposals
        .iter()
        .take(12)
        .map(operational_modifier_proposal_card)
        .collect::<Vec<_>>();
    let mut next_actions = vec![
        "forge ops serve --project-root . --host 127.0.0.1 --port 8765".to_string(),
        "POST /api/modifier/propose-goal".to_string(),
        "POST /api/modifier/propose-task".to_string(),
    ];
    if lane.pending_count > 0 {
        next_actions.insert(
            0,
            "review pending modifier proposals before applying runtime mutations".to_string(),
        );
        next_actions.push("POST /api/modifier/apply".to_string());
    }

    InteractiveOperationalModifierLanePanel {
        schema_version: INTERACTIVE_OPERATIONAL_MODIFIER_LANE_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        operation_mode: "human_ai_assisted_runtime_mutation".to_string(),
        purpose: lane.purpose.clone(),
        pending_count: lane.pending_count,
        applied_count: lane.applied_count,
        proposal_cards,
        commands: InteractiveOperationalModifierLaneCommands {
            serve_console: vec![
                "forge".to_string(),
                "ops".to_string(),
                "serve".to_string(),
                "--project-root".to_string(),
                ".".to_string(),
                "--host".to_string(),
                "127.0.0.1".to_string(),
                "--port".to_string(),
                "8765".to_string(),
            ],
            refresh_cockpit: vec![
                "forge".to_string(),
                "interactive".to_string(),
                "operational-cockpit".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            snapshot_route: "GET /api/snapshot".to_string(),
            propose_goal_route: "POST /api/modifier/propose-goal".to_string(),
            propose_task_route: "POST /api/modifier/propose-task".to_string(),
            apply_proposal_route: "POST /api/modifier/apply".to_string(),
        },
        next_actions,
        notes: vec![
            "The modifier lane is read-only in the TUI; applying a proposal remains an explicit ops-console/API mutation.".to_string(),
            "Use it for human+AI strategic updates to workflow goals or nodes while execution continues.".to_string(),
        ],
    }
}

fn operational_modifier_proposal_card(
    proposal: &OpsModifierProposal,
) -> InteractiveOperationalModifierProposalCard {
    InteractiveOperationalModifierProposalCard {
        proposal_id: proposal.proposal_id.clone(),
        workflow_id: proposal.workflow_id.clone(),
        target_kind: proposal.target_kind.clone(),
        task_id: proposal.task_id.clone(),
        title: proposal.title.clone(),
        summary: proposal.summary.clone(),
        rationale: proposal.rationale.clone(),
        author: proposal.author.clone(),
        status: proposal.status.clone(),
        created_at: proposal.created_at.clone(),
        applied_at: proposal.applied_at.clone(),
        applied_revision: proposal.applied_revision,
        apply_route: "/api/modifier/apply".to_string(),
        inspect_command: vec![
            "forge".to_string(),
            "inspect".to_string(),
            "--workflow".to_string(),
            proposal.workflow_id.clone(),
            "--output".to_string(),
            "json".to_string(),
        ],
        apply_payload_hint: format!("proposal_id={}", proposal.proposal_id),
    }
}

fn operational_cockpit_section(
    section_id: &str,
    title: &str,
    status: &str,
    signal_count: usize,
    summary: String,
    primary_command: &str,
    secondary_commands: Vec<String>,
) -> InteractiveOperationalCockpitSection {
    InteractiveOperationalCockpitSection {
        section_id: section_id.to_string(),
        title: title.to_string(),
        status: status.to_string(),
        signal_count,
        summary,
        primary_command: primary_command.to_string(),
        secondary_commands,
    }
}

struct GuidedCockpitInputs<'a> {
    active_runs: usize,
    pending_approvals: usize,
    validation_failures: usize,
    task_board_panel: &'a InteractiveTaskBoardPanel,
    dag_panel: &'a InteractiveWorkflowDagPanel,
    workflow_mutation_panel: &'a InteractiveWorkflowMutationPanel,
    artifact_panel: &'a InteractiveArtifactPanel,
    event_panel: &'a InteractiveEventPanel,
    cost_panel: &'a InteractiveCostPanel,
    improvement_loop_panel: &'a InteractiveImprovementLoopPanel,
}

fn build_guided_cockpit_panel(inputs: GuidedCockpitInputs<'_>) -> InteractiveGuidedCockpitPanel {
    let workflow_count = inputs.task_board_panel.workflow_count;
    let task_count = inputs.task_board_panel.task_count;
    let has_workflow = workflow_count > 0;
    let has_dag = inputs.dag_panel.node_count > 0;
    let has_active_run = inputs.active_runs > 0;
    let has_ready_handoff = inputs.task_board_panel.ready_handoffs > 0;
    let has_human_wait = inputs.task_board_panel.pending_human_interactions > 0;
    let has_approval = inputs.pending_approvals > 0 || has_human_wait;
    let has_artifacts = inputs.artifact_panel.artifact_count > 0;
    let has_events = inputs.event_panel.total_event_count > 0;
    let has_costs = inputs.cost_panel.node_count > 0 || inputs.cost_panel.workflow_count > 0;
    let close_ready = has_workflow
        && inputs.validation_failures == 0
        && inputs.improvement_loop_panel.final_outcome_candidate_count == 0;

    let mut steps = vec![
        guided_step(
            1,
            "create_workflow",
            "Criar workflow",
            if has_workflow {
                "completed"
            } else {
                "ready"
            },
            format!("{workflow_count} workflows tracked by the task board"),
            "workflow_sidebar_panel",
            "forge interactive route --input \"<objective>\" --origin forge_cli --output json",
            "forge plan --goal \"<objective>\" --output json",
            "medium",
            !has_workflow,
            !has_workflow,
            !has_workflow,
            Some("forge request cancel --run <run-id> --output json"),
        ),
        guided_step(
            2,
            "view_dag",
            "Ver DAG",
            if has_dag {
                "completed"
            } else if has_workflow {
                "ready"
            } else {
                "blocked_until_workflow"
            },
            format!(
                "{} DAG nodes, {} edges, {} human waits",
                inputs.dag_panel.node_count, inputs.dag_panel.edge_count, inputs.dag_panel.human_wait_count
            ),
            "dag_panel",
            "forge interactive workflow-dag --output json",
            "forge inspect <workflow-id> --verbose --output json",
            "low",
            false,
            false,
            has_workflow,
            None,
        ),
        guided_step(
            3,
            "start_run",
            "Iniciar run",
            if has_active_run {
                "completed"
            } else if has_workflow {
                "ready"
            } else {
                "blocked_until_workflow"
            },
            format!("{} active runs", inputs.active_runs),
            "operational_cockpit_panel",
            "forge request start --goal \"<objective>\" --origin forge_cli --output json",
            "forge interactive guided-cockpit --output json",
            "medium",
            has_workflow && !has_active_run,
            has_workflow && !has_active_run,
            has_workflow && !has_active_run,
            Some("forge request cancel --run <run-id> --output json"),
        ),
        guided_step(
            4,
            "tasks_handoffs",
            "Ver tasks/handoffs",
            if has_ready_handoff {
                "completed"
            } else if task_count > 0 {
                "ready"
            } else {
                "blocked_until_task_output"
            },
            format!(
                "{} tasks, {} ready handoffs, {} pending human waits",
                task_count, inputs.task_board_panel.ready_handoffs, inputs.task_board_panel.pending_human_interactions
            ),
            "task_board_panel",
            "forge interactive task-board --output json",
            "forge interactive operating-context --output json",
            "low",
            false,
            false,
            task_count > 0,
            None,
        ),
        guided_step(
            5,
            "approve_decide",
            "Aprovar/decidir",
            if has_approval {
                "needs_confirmation"
            } else if has_workflow {
                "completed"
            } else {
                "blocked_until_workflow"
            },
            format!(
                "{} pending approvals, {} human waits",
                inputs.pending_approvals, inputs.task_board_panel.pending_human_interactions
            ),
            "permissions_panel",
            "forge interactive permissions --output json",
            "forge interaction list --output json",
            "medium",
            has_approval,
            has_approval,
            has_approval,
            Some("forge interaction expire --workflow <workflow-id> --task <task-id> --origin forge_cli --output json"),
        ),
        guided_step(
            6,
            "artifacts",
            "Ver artifacts",
            if has_artifacts {
                "completed"
            } else if task_count > 0 {
                "ready"
            } else {
                "blocked_until_task_output"
            },
            format!(
                "{} artifacts across {} workflows",
                inputs.artifact_panel.artifact_count, inputs.artifact_panel.workflow_count
            ),
            "artifact_panel",
            "forge interactive artifacts --output json",
            "forge interactive task-board --output json",
            "low",
            false,
            false,
            task_count > 0,
            None,
        ),
        guided_step(
            7,
            "cost_events",
            "Ver custo/eventos",
            if has_events && has_costs {
                "completed"
            } else if has_events || has_costs {
                "partial"
            } else {
                "ready"
            },
            format!(
                "{} events, {} cost nodes, estimated ${:.4}",
                inputs.event_panel.total_event_count,
                inputs.cost_panel.node_count,
                inputs.cost_panel.estimated_task_cost_total_usd
            ),
            "structured_logs_panel",
            "forge interactive structured-logs --output json",
            "forge cost ledger --output json",
            "low",
            false,
            false,
            true,
            None,
        ),
        guided_step(
            8,
            "close_outcome",
            "Fechar outcome",
            if close_ready {
                "completed"
            } else if has_workflow {
                "needs_action"
            } else {
                "blocked_until_workflow"
            },
            format!(
                "{} validation failures, {} final-outcome candidates, {} mutation proposals",
                inputs.validation_failures,
                inputs.improvement_loop_panel.final_outcome_candidate_count,
                inputs.workflow_mutation_panel.pending_modifier_proposal_count
            ),
            "improvement_loop_panel",
            "forge workflow validate --workflow <workflow-id> --output json",
            "forge interactive improvement-loop --output json",
            "medium",
            has_workflow && !close_ready,
            has_workflow && !close_ready,
            has_workflow && !close_ready,
            Some("forge workflow ensure-final-audit --workflow <workflow-id> --output json"),
        ),
    ];

    let completed_step_count = steps
        .iter()
        .filter(|step| step.status == "completed")
        .count();
    let blocked_step_count = steps
        .iter()
        .filter(|step| step.status.starts_with("blocked"))
        .count();
    let confirmation_step_count = steps
        .iter()
        .filter(|step| step.status == "needs_confirmation")
        .count();
    let current_step_id = steps
        .iter()
        .find(|step| step.status != "completed")
        .map(|step| step.step_id.clone())
        .unwrap_or_else(|| "done".to_string());
    let current_commands = steps
        .iter()
        .find(|step| step.step_id == current_step_id)
        .map(|step| {
            let mut commands = vec![step.preview_command.clone(), step.primary_command.clone()];
            if let Some(rollback) = &step.rollback_command {
                commands.push(rollback.clone());
            }
            commands
        })
        .unwrap_or_else(|| vec!["forge smoke operational-tui --output json".to_string()]);
    let next_command = current_commands
        .first()
        .cloned()
        .unwrap_or_else(|| "forge smoke operational-tui --output json".to_string());
    let status = if confirmation_step_count > 0 {
        "guided_cockpit_waiting_confirmation"
    } else if blocked_step_count > 0 {
        "guided_cockpit_in_progress"
    } else if completed_step_count == steps.len() {
        "guided_cockpit_complete"
    } else {
        "guided_cockpit_ready"
    };
    let total_step_count = steps.len();

    InteractiveGuidedCockpitPanel {
        schema_version: INTERACTIVE_GUIDED_COCKPIT_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        title: "Forge 0.5 guided cockpit".to_string(),
        visual_mode: "three_column_focus_timeline".to_string(),
        completed_step_count,
        total_step_count,
        blocked_step_count,
        confirmation_step_count,
        current_step_id,
        layout_panes: vec![
            guided_pane(
                "left_workflows",
                "Workflows",
                "navigation",
                "workflow_sidebar_panel",
                "1",
            ),
            guided_pane(
                "center_execution",
                "Execution",
                "primary",
                "task_board_panel",
                "2",
            ),
            guided_pane(
                "right_timeline",
                "Timeline",
                "observability",
                "structured_logs_panel",
                "3",
            ),
            guided_pane(
                "bottom_actions",
                "Safe actions",
                "command_bar",
                "command_palette_panel",
                "/",
            ),
        ],
        steps: {
            steps.shrink_to_fit();
            steps
        },
        safe_action_policy: vec![
            "Read-only panels open without confirmation.".to_string(),
            "Mutating actions require preview plus explicit confirmation.".to_string(),
            "Rollback or recovery command is visible for mutation, approval and close-out steps.".to_string(),
            "Failed actions must be inspectable through structured logs and the event timeline.".to_string(),
        ],
        next_command,
        next_commands: current_commands,
        notes: vec![
            "`forge` opens this cockpit first, like opencode/gemini style entrypoints.".to_string(),
            "The eight steps turn the README five-minute flow into an operator checklist.".to_string(),
            "The panel is read-only; actions are surfaced as explicit commands with preview and recovery.".to_string(),
        ],
    }
}

fn guided_pane(
    pane_id: &str,
    title: &str,
    role: &str,
    source_panel: &str,
    focus_key: &str,
) -> InteractiveGuidedCockpitPane {
    InteractiveGuidedCockpitPane {
        pane_id: pane_id.to_string(),
        title: title.to_string(),
        role: role.to_string(),
        source_panel: source_panel.to_string(),
        focus_key: focus_key.to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
fn guided_step(
    order: usize,
    step_id: &str,
    title: &str,
    status: &str,
    evidence: String,
    primary_panel: &str,
    primary_command: &str,
    preview_command: &str,
    risk_level: &str,
    requires_confirmation: bool,
    mutates_workflow: bool,
    can_apply_now: bool,
    rollback_command: Option<&str>,
) -> InteractiveGuidedCockpitStep {
    InteractiveGuidedCockpitStep {
        step_id: step_id.to_string(),
        order,
        title: title.to_string(),
        status: status.to_string(),
        evidence,
        primary_panel: primary_panel.to_string(),
        primary_command: primary_command.to_string(),
        preview_command: preview_command.to_string(),
        risk_level: risk_level.to_string(),
        requires_confirmation,
        mutates_workflow,
        can_apply_now,
        rollback_command: rollback_command.map(ToString::to_string),
    }
}

fn build_ui_composition_panel(
    addon_renderer_report: &OpsAddonViewRendererReport,
    project_root: &Path,
) -> InteractiveUiCompositionPanel {
    let project_root_text = project_root.display().to_string();
    let mut addon_widgets = addon_renderer_report
        .renderers
        .iter()
        .take(24)
        .map(addon_ui_widget)
        .collect::<Vec<_>>();

    let mut addon_region_widgets = vec![
        core_ui_widget(
            "addon_capability_panel",
            "Addons/capabilities",
            "addon_capability_panel",
            "capability_index_renderer",
            "standard",
            "full",
            vec!["forge interactive addon-capabilities --output json".to_string()],
        ),
        core_ui_widget(
            "addon_renderer_panel",
            "Addon UI renderers",
            "addon_renderer_panel",
            "data_list_renderer",
            "standard",
            "full",
            vec!["forge addons views --output json".to_string()],
        ),
    ];
    addon_region_widgets.append(&mut addon_widgets);

    let regions = vec![
        ui_region(
            "top_bar",
            "Navigation and shell readiness",
            "navigation",
            10,
            vec![
                core_ui_widget(
                    "navigation_panel",
                    "Navigation panel",
                    "navigation_panel",
                    "navigation_renderer",
                    "compact",
                    "full",
                    vec![format!(
                        "forge interactive home --project-root {project_root_text} --output json"
                    )],
                ),
                core_ui_widget(
                    "command_palette_panel",
                    "Command palette",
                    "command_palette_panel",
                    "command_palette_renderer",
                    "compact",
                    "full",
                    vec!["forge interactive command-palette --output json".to_string()],
                ),
                core_ui_widget(
                    "action_registry_panel",
                    "Action registry",
                    "action_registry_panel",
                    "action_registry_renderer",
                    "compact",
                    "full",
                    vec!["forge interactive action-registry --output json".to_string()],
                ),
                core_ui_widget(
                    "autocomplete_panel",
                    "Autocomplete",
                    "autocomplete_panel",
                    "autocomplete_renderer",
                    "compact",
                    "full",
                    vec![
                        "forge interactive autocomplete --input <input> --output json".to_string(),
                    ],
                ),
                core_ui_widget(
                    "harness_panel",
                    "Harness center",
                    "harness_panel",
                    "status_renderer",
                    "standard",
                    "full",
                    vec![format!(
                        "forge interactive harness --project-root {project_root_text} --output json"
                    )],
                ),
                core_ui_widget(
                    "replacement_cli_panel",
                    "Replacement CLI readiness",
                    "replacement_cli_panel",
                    "readiness_matrix_renderer",
                    "detailed",
                    "full",
                    vec![format!(
                        "forge interactive replacement-cli --project-root {project_root_text} --output json"
                    )],
                ),
                core_ui_widget(
                    "architecture_compass_panel",
                    "Architecture compass",
                    "architecture_compass_panel",
                    "architecture_gap_matrix_renderer",
                    "detailed",
                    "full",
                    vec![format!(
                        "forge interactive architecture --project-root {project_root_text} --output json"
                    )],
                ),
                core_ui_widget(
                    "multimodal_runtime_panel",
                    "Multimodal runtime readiness",
                    "multimodal_runtime_panel",
                    "addon_runtime_readiness_renderer",
                    "detailed",
                    "full",
                    vec![format!(
                        "forge interactive multimodal-runtime --project-root {project_root_text} --output json"
                    )],
                ),
                core_ui_widget(
                    "sessions_panel",
                    "Session center",
                    "sessions_panel",
                    "session_lifecycle_renderer",
                    "standard",
                    "full",
                    vec!["forge interactive sessions --output json".to_string()],
                ),
                core_ui_widget(
                    "harness_mode_panel",
                    "Harness mode",
                    "harness_mode_panel",
                    "status_renderer",
                    "compact",
                    "half",
                    vec!["forge harness mode --output json".to_string()],
                ),
                core_ui_widget(
                    "harness_doctor_panel",
                    "Harness doctor",
                    "harness_doctor_panel",
                    "status_renderer",
                    "compact",
                    "half",
                    vec![
                        "forge harness doctor --executor codex --shim-dir $HOME/.forge/bin --project-root . --output json"
                            .to_string(),
                    ],
                ),
            ],
        ),
        ui_region(
            "operations",
            "Workflow operations",
            "primary_work_area",
            20,
            vec![
                core_ui_widget(
                    "guided_cockpit_panel",
                    "Guided cockpit",
                    "guided_cockpit_panel",
                    "guided_cockpit_renderer",
                    "detailed",
                    "full",
                    vec!["forge interactive guided-cockpit --output json".to_string()],
                ),
                core_ui_widget(
                    "workflow_sidebar_panel",
                    "Workflow sidebar",
                    "workflow_sidebar_panel",
                    "navigation_list_renderer",
                    "compact",
                    "third",
                    vec!["forge interactive workflow-sidebar --output json".to_string()],
                ),
                core_ui_widget(
                    "operational_cockpit_panel",
                    "Operational cockpit",
                    "operational_cockpit_panel",
                    "cockpit_renderer",
                    "detailed",
                    "full",
                    vec!["forge interactive operational-cockpit --output json".to_string()],
                ),
                core_ui_widget(
                    "digital_twin_panel",
                    "Operational digital twin",
                    "digital_twin_panel",
                    "dashboard_renderer",
                    "detailed",
                    "full",
                    vec!["forge ops snapshot --output json".to_string()],
                ),
                core_ui_widget(
                    "dag_panel",
                    "Workflow DAG",
                    "dag_panel",
                    "graph_renderer",
                    "detailed",
                    "full",
                    vec!["forge interactive workflow-dag --output json".to_string()],
                ),
                core_ui_widget(
                    "task_board_panel",
                    "Task board",
                    "task_board_panel",
                    "task_board_renderer",
                    "detailed",
                    "full",
                    vec!["forge interactive task-board --output json".to_string()],
                ),
                core_ui_widget(
                    "workflow_mutation_panel",
                    "Workflow mutation planner",
                    "workflow_mutation_panel",
                    "workflow_mutation_renderer",
                    "detailed",
                    "full",
                    vec!["forge interactive workflow-mutation --output json".to_string()],
                ),
                core_ui_widget(
                    "patch_workbench_panel",
                    "Patch workbench",
                    "patch_workbench_panel",
                    "diff_review_renderer",
                    "detailed",
                    "full",
                    vec!["forge interactive patch-workbench --output json".to_string()],
                ),
            ],
        ),
        ui_region(
            "observability",
            "Observability and governance",
            "side_panel",
            30,
            vec![
                core_ui_widget(
                    "schedule_panel",
                    "Schedule panel",
                    "schedule_panel",
                    "timeline_renderer",
                    "standard",
                    "half",
                    vec!["forge interactive schedules --output json".to_string()],
                ),
                core_ui_widget(
                    "event_panel",
                    "Event timeline",
                    "event_panel",
                    "timeline_renderer",
                    "standard",
                    "half",
                    vec!["forge events timeline --output json".to_string()],
                ),
                core_ui_widget(
                    "structured_logs_panel",
                    "Structured logs",
                    "structured_logs_panel",
                    "log_renderer",
                    "standard",
                    "half",
                    vec!["forge interactive structured-logs --output json".to_string()],
                ),
                core_ui_widget(
                    "token_usage_panel",
                    "Token usage",
                    "token_usage_panel",
                    "token_usage_renderer",
                    "standard",
                    "half",
                    vec!["forge interactive token-usage --output json".to_string()],
                ),
                core_ui_widget(
                    "artifact_panel",
                    "Artifact panel",
                    "artifact_panel",
                    "artifact_evidence_renderer",
                    "standard",
                    "half",
                    vec!["forge interactive artifacts --output json".to_string()],
                ),
                core_ui_widget(
                    "release_gates_panel",
                    "Release gates",
                    "release_gates_panel",
                    "release_gate_renderer",
                    "standard",
                    "half",
                    vec!["forge interactive release-gates --output json".to_string()],
                ),
                core_ui_widget(
                    "core_boundary_panel",
                    "Core boundary",
                    "core_boundary_panel",
                    "boundary_audit_renderer",
                    "standard",
                    "half",
                    vec!["forge interactive core-boundary --output json".to_string()],
                ),
                core_ui_widget(
                    "cost_panel",
                    "Cost panel",
                    "cost_panel",
                    "metric_renderer",
                    "standard",
                    "half",
                    vec!["forge cost ledger --output json".to_string()],
                ),
                core_ui_widget(
                    "improvement_loop_panel",
                    "Improvement loop",
                    "improvement_loop_panel",
                    "improvement_loop_renderer",
                    "detailed",
                    "full",
                    vec!["forge interactive improvement-loop --output json".to_string()],
                ),
                core_ui_widget(
                    "context_memory_panel",
                    "Context/memory panel",
                    "context_memory_panel",
                    "policy_renderer",
                    "standard",
                    "half",
                    vec!["forge interactive context-memory --output json".to_string()],
                ),
                core_ui_widget(
                    "operating_context_panel",
                    "Operating context",
                    "operating_context_panel",
                    "tenant_context_renderer",
                    "standard",
                    "full",
                    vec!["forge interactive operating-context --output json".to_string()],
                ),
                core_ui_widget(
                    "permissions_panel",
                    "Permission center",
                    "permissions_panel",
                    "permission_center_renderer",
                    "standard",
                    "half",
                    vec!["forge interactive permissions --output json".to_string()],
                ),
                core_ui_widget(
                    "identity_panel",
                    "Identity center",
                    "identity_panel",
                    "identity_center_renderer",
                    "standard",
                    "half",
                    vec!["forge interactive identity --output json".to_string()],
                ),
            ],
        ),
        ui_region(
            "addons",
            "Addon workspace",
            "addon_region",
            40,
            addon_region_widgets,
        ),
    ];

    let widget_count = regions.iter().map(|region| region.widget_count).sum();
    let core_widget_count = regions
        .iter()
        .flat_map(|region| region.widgets.iter())
        .filter(|widget| widget.source == "core")
        .count();
    let addon_widget_count = regions
        .iter()
        .flat_map(|region| region.widgets.iter())
        .filter(|widget| widget.source == "addon")
        .count();

    InteractiveUiCompositionPanel {
        schema_version: INTERACTIVE_UI_COMPOSITION_SCHEMA_VERSION.to_string(),
        status: "ui_composition_ready".to_string(),
        layout_kind: "operator_workspace".to_string(),
        region_count: regions.len(),
        widget_count,
        core_widget_count,
        addon_widget_count,
        addon_renderer_families: addon_renderer_report.families.clone(),
        regions,
        commands: InteractiveUiCompositionCommands {
            refresh: vec![
                "interactive".to_string(),
                "ui-composition".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            inspect_addons: vec![
                "addons".to_string(),
                "views".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            open_task_board: vec![
                "interactive".to_string(),
                "task-board".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
        },
    }
}

fn ui_region(
    region_id: &str,
    title: &str,
    role: &str,
    order: i64,
    widgets: Vec<InteractiveUiWidget>,
) -> InteractiveUiRegion {
    InteractiveUiRegion {
        region_id: region_id.to_string(),
        title: title.to_string(),
        role: role.to_string(),
        order,
        widget_count: widgets.len(),
        widgets,
    }
}

fn core_ui_widget(
    widget_id: &str,
    title: &str,
    panel: &str,
    renderer_family: &str,
    layout_density: &str,
    layout_width: &str,
    commands: Vec<String>,
) -> InteractiveUiWidget {
    InteractiveUiWidget {
        widget_id: widget_id.to_string(),
        title: title.to_string(),
        source: "core".to_string(),
        panel: panel.to_string(),
        renderer_family: renderer_family.to_string(),
        safe_renderer: true,
        layout_density: layout_density.to_string(),
        layout_width: layout_width.to_string(),
        commands,
    }
}

fn addon_ui_widget(renderer: &crate::ops::OpsAddonViewRenderer) -> InteractiveUiWidget {
    InteractiveUiWidget {
        widget_id: format!("addon:{}:{}", renderer.addon_id, renderer.view_id),
        title: defaulted_ui(&renderer.title, &renderer.view_id),
        source: "addon".to_string(),
        panel: renderer.view_id.clone(),
        renderer_family: renderer.renderer_family.clone(),
        safe_renderer: renderer.safe_renderer,
        layout_density: defaulted_ui(&renderer.layout_density, "standard"),
        layout_width: defaulted_ui(&renderer.layout_width, "auto"),
        commands: vec![renderer.tui_affordance.clone()],
    }
}

fn defaulted_ui(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value.to_string()
    }
}

fn render_navigation_keybindings(panel: &InteractiveNavigationPanel) -> String {
    if panel.keybindings.is_empty() {
        return "none".to_string();
    }

    panel
        .keybindings
        .iter()
        .map(|binding| format!("{}={}", binding.key, binding.action))
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_command_palette_entry_summary(panel: &InteractiveCommandPalettePanel) -> String {
    if panel.entries.is_empty() {
        return "none".to_string();
    }

    panel
        .entries
        .iter()
        .take(12)
        .map(|entry| {
            let workflow = entry.workflow_id.as_deref().unwrap_or("global");
            format!(
                "{} [{}] {} workflow={} risk={} mutates={} approval={}",
                entry.action_id,
                entry.source_panel,
                entry.commands.join(" "),
                workflow,
                entry.risk_level,
                entry.mutates_workflow,
                entry.requires_approval
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn render_action_registry_action_summary(panel: &InteractiveActionRegistryPanel) -> String {
    if panel.actions.is_empty() {
        return "none".to_string();
    }

    panel
        .actions
        .iter()
        .take(12)
        .map(|action| {
            format!(
                "{} [{}] enabled={} risk={} mutates={} approval={} plan={} next={}",
                action.action_id,
                action.source_panel,
                action.enabled,
                action.risk_level,
                action.mutates_workflow,
                action.requires_approval,
                action.operation_plan.status,
                action
                    .operation_plan
                    .next_commands
                    .first()
                    .map(|command| command.join(" "))
                    .unwrap_or_else(|| "none".to_string())
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn render_autocomplete_suggestion_summary(panel: &InteractiveAutocompletePanel) -> String {
    if panel.suggestions.is_empty() {
        return "none".to_string();
    }

    panel
        .suggestions
        .iter()
        .take(12)
        .map(|suggestion| {
            format!(
                "{} [{}] {} score={} risk={} mutates={} approval={}",
                suggestion.label,
                suggestion.kind,
                suggestion.source_panel,
                suggestion.score,
                suggestion.risk_level,
                suggestion.mutates_workflow,
                suggestion.requires_approval
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn render_structured_log_summary(panel: &InteractiveStructuredLogsPanel) -> String {
    if panel.logs.is_empty() {
        return "none".to_string();
    }

    panel
        .logs
        .iter()
        .take(5)
        .map(|entry| {
            format!(
                "#{} {} {} {} {} category {} source {} correlation {} observability {} payload {}",
                entry.store_sequence,
                entry.severity,
                entry.workflow_id,
                entry.kind,
                entry.origin,
                entry.category,
                entry.source,
                structured_log_correlation_summary(&entry.correlation),
                structured_log_json_summary(&entry.observability),
                entry.payload_preview
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn render_schedule_assignment_summary(assignments: &[InteractiveScheduleAssignment]) -> String {
    if assignments.is_empty() {
        "none".to_string()
    } else {
        assignments
            .iter()
            .take(5)
            .map(|assignment| {
                format!(
                    "{} task {} due {} wave {} pos {}",
                    assignment.workflow_id,
                    assignment.schedule_task_id,
                    assignment.due_nodes,
                    assignment.wave,
                    assignment.queue_position
                )
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

fn render_schedule_workflow_summary(panel: &InteractiveSchedulePanel) -> String {
    if panel.workflows.is_empty() {
        "none".to_string()
    } else {
        panel
            .workflows
            .iter()
            .take(8)
            .map(|workflow| {
                format!(
                    "{} {} due {}, next {}, scale_to_zero {}",
                    workflow.workflow_id,
                    workflow.status,
                    workflow.due_nodes,
                    workflow.next_wakeup_at.as_deref().unwrap_or("none"),
                    workflow.scale_to_zero_eligible
                )
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

fn render_addon_capability_summary(panel: &InteractiveAddonCapabilityPanel) -> String {
    if panel.capabilities.is_empty() {
        "none".to_string()
    } else {
        panel.capabilities.join(" | ")
    }
}

fn render_core_boundary_summary(panel: &InteractiveCoreBoundaryPanel) -> String {
    let failed_gates = panel
        .acceptance_gates
        .iter()
        .filter(|gate| !gate.passed)
        .count();
    let addon_samples = if panel.addon_boundaries.is_empty() {
        "none".to_string()
    } else {
        panel
            .addon_boundaries
            .iter()
            .take(5)
            .map(|addon| {
                format!(
                    "{} caps {} domains {}",
                    addon.addon_id,
                    addon.capability_count,
                    if addon.domains.is_empty() {
                        "none".to_string()
                    } else {
                        addon.domains.join("+")
                    }
                )
            })
            .collect::<Vec<_>>()
            .join(" | ")
    };
    format!(
        "{}; core caps {}, domain addons {}, addon-owned caps {}, leaks {}, compatibility {}, gates {}/{} passed; addons {}",
        panel.status,
        panel.core_capability_count,
        panel.domain_addon_count,
        panel.addon_owned_capability_count,
        panel.domain_specific_core_leak_count,
        panel.compatibility_boundary_count,
        panel.acceptance_gates.len().saturating_sub(failed_gates),
        panel.acceptance_gates.len(),
        addon_samples
    )
}

pub fn render_interactive_core_boundary(panel: &InteractiveCoreBoundaryPanel) -> String {
    let core_capabilities = if panel.core_kernel_capabilities.is_empty() {
        "none".to_string()
    } else {
        panel
            .core_kernel_capabilities
            .iter()
            .map(|capability| {
                format!(
                    "{}:{} domains {}",
                    capability.capability_id,
                    capability.boundary_status,
                    if capability.domains.is_empty() {
                        "none".to_string()
                    } else {
                        capability.domains.join("+")
                    }
                )
            })
            .collect::<Vec<_>>()
            .join(" | ")
    };
    let compatibility = if panel.compatibility_boundaries.is_empty() {
        "none".to_string()
    } else {
        panel
            .compatibility_boundaries
            .iter()
            .take(8)
            .map(|boundary| {
                format!(
                    "{}:{} via {}",
                    boundary.addon_id, boundary.capability_id, boundary.compatibility_executor
                )
            })
            .collect::<Vec<_>>()
            .join(" | ")
    };
    let gates = if panel.acceptance_gates.is_empty() {
        "none".to_string()
    } else {
        panel
            .acceptance_gates
            .iter()
            .map(|gate| {
                format!(
                    "{}:{} ({})",
                    gate.gate_id,
                    if gate.passed {
                        "passed"
                    } else {
                        "needs_evidence"
                    },
                    gate.evidence
                )
            })
            .collect::<Vec<_>>()
            .join(" | ")
    };
    format!(
        "Core boundary: {summary}; project {project_root}\nCore responsibilities: {responsibilities}\nCore kernel: {core_capabilities}\nCompatibility boundaries: {compatibility}\nAcceptance gates: {gates}\nCommands: {commands}\n",
        summary = render_core_boundary_summary(panel),
        project_root = panel.project_root,
        responsibilities = panel.core_allowed_responsibilities.join(", "),
        core_capabilities = core_capabilities,
        compatibility = compatibility,
        gates = gates,
        commands = panel.commands.join(" | "),
    )
}

fn structured_log_correlation_summary(correlation: &serde_json::Value) -> String {
    let Some(map) = correlation.as_object() else {
        return structured_log_json_summary(correlation);
    };
    let fields = [
        "task_id",
        "run_id",
        "artifact_id",
        "interaction_id",
        "node_ref",
        "addon_id",
        "workflow_id",
    ];
    let parts = fields
        .iter()
        .filter_map(|field| {
            map.get(*field)
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(|value| {
                    let label = field.strip_suffix("_id").unwrap_or(field);
                    format!("{label}={value}")
                })
        })
        .collect::<Vec<_>>();

    if parts.is_empty() {
        structured_log_json_summary(correlation)
    } else {
        parts.join(",")
    }
}

fn structured_log_json_summary(value: &serde_json::Value) -> String {
    serde_json::to_string(value)
        .map(|json| truncate_display(&json, 180))
        .unwrap_or_else(|_| "unavailable".to_string())
}

fn render_operational_cockpit_sections(panel: &InteractiveOperationalCockpitPanel) -> String {
    if panel.sections.is_empty() {
        return "none".to_string();
    }

    panel
        .sections
        .iter()
        .map(|section| {
            format!(
                "{}:{} signals {} -> {}",
                section.section_id, section.status, section.signal_count, section.primary_command
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

pub fn render_interactive_architecture_compass(
    panel: &InteractiveArchitectureCompassPanel,
) -> String {
    format!(
        "Architecture compass: {status}; docs {doc_count}; tracks {track_count}; dependencies {dependency_count}; conflicts {conflict_count}\nOperating context: tenant {tenant}; organization {organization}; brand {brand}; product {product}; memory {memory_level}/{memory_scopes}; personality {personality}; gates {gates}\nTracks: {tracks}\nExecution plan: {execution_plan}\nBenchmarks: {benchmarks}\nReuse: {reuse}\nNext commands: {commands}\n",
        status = panel.status,
        doc_count = panel.source_documents.len(),
        track_count = panel.tracks.len(),
        dependency_count = panel.dependencies.len(),
        conflict_count = panel.conflicts.len(),
        tenant = panel.operating_context.tenant_path,
        organization = panel.operating_context.organization_id,
        brand = panel.operating_context.brand_id,
        product = panel.operating_context.product_id,
        memory_level = panel.operating_context.memory_level,
        memory_scopes = panel.operating_context.memory_scopes.join("+"),
        personality = panel.operating_context.personality_status,
        gates = panel.operating_context.prompt_packet_gates.join("+"),
        tracks = render_architecture_track_summary(panel),
        execution_plan = render_architecture_execution_plan_summary(panel),
        benchmarks = render_architecture_benchmark_summary(panel),
        reuse = if panel.reuse_opportunities.is_empty() {
            "none".to_string()
        } else {
            panel.reuse_opportunities.join(" | ")
        },
        commands = if panel.next_commands.is_empty() {
            "none".to_string()
        } else {
            panel.next_commands.join(" | ")
        },
    )
}

fn render_architecture_execution_plan_summary(
    panel: &InteractiveArchitectureCompassPanel,
) -> String {
    let plan = &panel.execution_plan;
    if plan.increments.is_empty() {
        return format!("{} with no increments", plan.status);
    }

    let increments = plan
        .increments
        .iter()
        .map(|increment| {
            format!(
                "{}.{}:{} gates {} evidence {}",
                increment.priority,
                increment.increment_id,
                increment.status,
                increment.acceptance_gates.len(),
                increment.evidence_commands.join("+")
            )
        })
        .collect::<Vec<_>>()
        .join(" | ");
    format!(
        "{} {}; rule {}; next {}",
        plan.status, increments, plan.selection_rule, plan.next_command
    )
}

fn render_architecture_track_summary(panel: &InteractiveArchitectureCompassPanel) -> String {
    if panel.tracks.is_empty() {
        return "none".to_string();
    }

    panel
        .tracks
        .iter()
        .map(|track| {
            format!(
                "{}:{} gaps {} next {}",
                track.track_id,
                track.status,
                track.gaps.len(),
                track.next_increment
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn render_architecture_benchmark_summary(panel: &InteractiveArchitectureCompassPanel) -> String {
    if panel.benchmark_sources.is_empty() {
        return "none".to_string();
    }

    panel
        .benchmark_sources
        .iter()
        .take(6)
        .map(|source| format!("{} -> {}", source.source, source.forge_boundary))
        .collect::<Vec<_>>()
        .join(" | ")
}

fn render_ui_composition_region_summary(panel: &InteractiveUiCompositionPanel) -> String {
    if panel.regions.is_empty() {
        return "none".to_string();
    }

    panel
        .regions
        .iter()
        .map(|region| {
            let addon_widgets = region
                .widgets
                .iter()
                .filter(|widget| widget.source == "addon")
                .count();
            format!(
                "{} [{}] widgets {}, addon {}",
                region.region_id, region.role, region.widget_count, addon_widgets
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn render_task_board_lane_summary(panel: &InteractiveTaskBoardPanel) -> String {
    if panel.lanes.is_empty() {
        return "none".to_string();
    }
    panel
        .lanes
        .iter()
        .map(|lane| {
            let next_actions = if lane.next_actions.is_empty() {
                "none".to_string()
            } else {
                lane.next_actions.join(" -> ")
            };
            format!(
                "{} [{}] tasks {}/{}, cards {}, ready handoffs {}, human waits {}, checkpoints {}, artifacts {}, actions {}",
                lane.workflow_id,
                lane.lifecycle_state,
                lane.completed_tasks,
                lane.total_tasks,
                lane.task_cards.len(),
                lane.ready_handoffs,
                lane.pending_human_interactions,
                lane.checkpoint_resume_candidates,
                lane.artifact_count,
                next_actions
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn render_task_board_card_summary(panel: &InteractiveTaskBoardPanel) -> String {
    let cards = panel
        .lanes
        .iter()
        .flat_map(|lane| {
            lane.task_cards
                .iter()
                .map(move |card| (lane.workflow_id.as_str(), card))
        })
        .take(16)
        .map(|(workflow_id, card)| {
            let checkpoint = card.checkpoint_id.as_deref().unwrap_or("none");
            format!(
                "{}/{} [{}] next {} human {}/{} handoff {} checkpoint {} commands {}",
                workflow_id,
                card.task_id,
                card.status,
                card.next_action,
                card.human_required,
                card.human_interaction_state,
                card.ready_for_handoff,
                checkpoint,
                prioritized_task_board_card_commands(card)
            )
        })
        .collect::<Vec<_>>();

    if cards.is_empty() {
        "none".to_string()
    } else {
        cards.join(" | ")
    }
}

fn render_workflow_mutation_card_summary(panel: &InteractiveWorkflowMutationPanel) -> String {
    if panel.workflow_cards.is_empty() {
        return "none".to_string();
    }

    panel
        .workflow_cards
        .iter()
        .take(12)
        .map(|card| {
            format!(
                "{} [{}] action {} targets {} dag {}/{} handoffs {} waits {} checkpoints {} commands goal {} brain {}",
                card.workflow_id,
                card.lifecycle_state,
                card.recommended_action,
                if card.mutable_targets.is_empty() {
                    "none".to_string()
                } else {
                    card.mutable_targets.join("+")
                },
                card.dag_node_count,
                card.dag_edge_count,
                card.ready_handoffs,
                card.human_waits,
                card.checkpoint_resume_candidates,
                card.commands.update_goal.join(" "),
                card.commands.update_node_brain.join(" ")
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn render_workflow_mutation_proposal_summary(panel: &InteractiveWorkflowMutationPanel) -> String {
    if panel.proposal_cards.is_empty() {
        return "none".to_string();
    }

    panel
        .proposal_cards
        .iter()
        .take(12)
        .map(|proposal| {
            format!(
                "{}:{}:{}:{}",
                proposal.workflow_id, proposal.proposal_id, proposal.target_kind, proposal.status
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn prioritized_task_board_card_commands(card: &InteractiveTaskBoardTaskCard) -> String {
    if card.commands.is_empty() {
        return "none".to_string();
    }

    let preferred = if card.next_action == "answer_human_interaction" {
        Some("forge interaction list")
    } else if card.next_action == "resume_from_checkpoint" {
        Some("forge context ")
    } else if card.ready_for_handoff || card.next_action.contains("handoff") {
        Some("forge task handoff ")
    } else {
        None
    };
    let mut commands = Vec::new();
    if let Some(preferred) = preferred {
        commands.extend(
            card.commands
                .iter()
                .filter(|command| command.starts_with(preferred))
                .cloned(),
        );
    }
    for command in &card.commands {
        if !commands.contains(command) {
            commands.push(command.clone());
        }
    }
    commands.join(" -> ")
}

fn render_artifact_workflow_summary(panel: &InteractiveArtifactPanel) -> String {
    if panel.workflows.is_empty() {
        return "none".to_string();
    }

    panel
        .workflows
        .iter()
        .take(8)
        .map(|workflow| {
            format!(
                "{} [{}] artifacts {}, bytes {}, command {}",
                workflow.workflow_id,
                workflow.lifecycle_state,
                workflow.artifact_count,
                workflow.total_bytes,
                workflow.commands.list.join(" ")
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn render_artifact_entry_summary(panel: &InteractiveArtifactPanel) -> String {
    let entries = panel
        .workflows
        .iter()
        .flat_map(|workflow| {
            workflow
                .artifacts
                .iter()
                .map(move |artifact| (workflow.workflow_id.as_str(), artifact))
        })
        .take(20)
        .map(|(workflow_id, artifact)| {
            format!(
                "{}/{} {} bytes {} path {}",
                workflow_id, artifact.kind, artifact.artifact_id, artifact.bytes, artifact.path
            )
        })
        .collect::<Vec<_>>();

    if entries.is_empty() {
        "none".to_string()
    } else {
        entries.join(" | ")
    }
}

fn render_token_usage_source_summary(panel: &InteractiveTokenUsagePanel) -> String {
    if panel.source_buckets.is_empty() {
        return "none".to_string();
    }

    panel
        .source_buckets
        .iter()
        .take(8)
        .map(|bucket| {
            format!(
                "{} blobs {}, saved {} tokens ({:.2}%)",
                bucket.source,
                bucket.blob_count,
                bucket.estimated_saved_tokens,
                bucket.savings_percent
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn render_token_usage_kind_summary(panel: &InteractiveTokenUsagePanel) -> String {
    if panel.content_kind_buckets.is_empty() {
        return "none".to_string();
    }

    panel
        .content_kind_buckets
        .iter()
        .take(8)
        .map(|bucket| {
            format!(
                "{} blobs {}, saved {} tokens ({:.2}%)",
                bucket.content_kind,
                bucket.blob_count,
                bucket.estimated_saved_tokens,
                bucket.savings_percent
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn render_token_usage_retrieve_summary(panel: &InteractiveTokenUsagePanel) -> String {
    if panel.retrieve_commands.is_empty() {
        "none".to_string()
    } else {
        panel
            .retrieve_commands
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

fn render_workflow_dag_summary(panel: &InteractiveWorkflowDagPanel) -> String {
    if panel.workflows.is_empty() {
        return "none".to_string();
    }

    panel
        .workflows
        .iter()
        .take(5)
        .map(|workflow| {
            format!(
                "{} [{}] nodes {}, edges {}, roots {}, blocked {}, human waits {}",
                workflow.workflow_id,
                workflow.lifecycle_state,
                workflow.node_count,
                workflow.edge_count,
                workflow.ready_root_count,
                workflow.blocked_node_count,
                workflow.human_wait_count
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn render_workflow_dag_node_summary(panel: &InteractiveWorkflowDagPanel) -> String {
    let nodes = panel
        .workflows
        .iter()
        .flat_map(|workflow| {
            workflow
                .nodes
                .iter()
                .map(move |node| (workflow.workflow_id.as_str(), node))
        })
        .take(20)
        .map(|(workflow_id, node)| {
            format!(
                "{}/{} [{}] exec {} deps {}/{} ready {} human {}/{} title {}",
                workflow_id,
                node.task_id,
                node.status,
                node.executor,
                node.dependency_count,
                node.dependent_count,
                node.ready_for_execution,
                node.human_required,
                node.human_interaction_state,
                node.title
            )
        })
        .collect::<Vec<_>>();

    if nodes.is_empty() {
        "none".to_string()
    } else {
        nodes.join(" | ")
    }
}

fn render_workflow_dag_edge_summary(panel: &InteractiveWorkflowDagPanel) -> String {
    let edges = panel
        .workflows
        .iter()
        .flat_map(|workflow| {
            workflow
                .edges
                .iter()
                .map(move |edge| (workflow.workflow_id.as_str(), edge))
        })
        .take(20)
        .map(|(workflow_id, edge)| {
            format!(
                "{}/{} -> {} kind {} dependency_status {}",
                workflow_id,
                edge.from_task_id,
                edge.to_task_id,
                edge.edge_kind,
                edge.dependency_status
            )
        })
        .collect::<Vec<_>>();

    if edges.is_empty() {
        "none".to_string()
    } else {
        edges.join(" | ")
    }
}

fn render_workflow_dag_command_summary(panel: &InteractiveWorkflowDagPanel) -> String {
    let commands = panel
        .workflows
        .iter()
        .take(12)
        .map(|workflow| {
            format!(
                "{} inspect {}; task_board {}; validate {}",
                workflow.workflow_id,
                workflow.commands.inspect.join(" "),
                workflow.commands.task_board.join(" "),
                workflow.commands.validate.join(" ")
            )
        })
        .collect::<Vec<_>>();

    if commands.is_empty() {
        "none".to_string()
    } else {
        commands.join(" | ")
    }
}

fn build_workflow_sidebar_panel(rows: &[WorkflowRegistryRow]) -> InteractiveWorkflowSidebarPanel {
    let selected_workflow_id = select_sidebar_workflow(rows);
    let group_specs = [
        ("active", "Active workflows"),
        ("attention", "Needs attention"),
        ("event_driven", "Event-driven"),
        ("scheduled", "Scheduled"),
        ("completed", "Completed or scaled down"),
        ("other", "Other workflows"),
    ];
    let groups = group_specs
        .iter()
        .filter_map(|(group_id, title)| {
            let mut items = rows
                .iter()
                .filter(|row| workflow_sidebar_group_matches(row, group_id))
                .map(|row| workflow_sidebar_item(row, &selected_workflow_id))
                .collect::<Vec<_>>();
            items.sort_by(|left, right| {
                right
                    .selected
                    .cmp(&left.selected)
                    .then_with(|| right.active_run_count.cmp(&left.active_run_count))
                    .then_with(|| right.ready_handoff_count.cmp(&left.ready_handoff_count))
                    .then_with(|| right.due_schedule_count.cmp(&left.due_schedule_count))
                    .then_with(|| left.workflow_id.cmp(&right.workflow_id))
            });
            (!items.is_empty()).then(|| InteractiveWorkflowSidebarGroup {
                group_id: (*group_id).to_string(),
                title: (*title).to_string(),
                item_count: items.len(),
                items,
            })
        })
        .collect::<Vec<_>>();
    let selected_group_id = groups
        .iter()
        .find(|group| {
            group
                .items
                .iter()
                .any(|item| item.workflow_id == selected_workflow_id)
        })
        .map(|group| group.group_id.clone())
        .unwrap_or_else(|| "none".to_string());
    let selected_index = groups
        .iter()
        .flat_map(|group| group.items.iter())
        .position(|item| item.workflow_id == selected_workflow_id)
        .unwrap_or(0);
    let active_count = rows
        .iter()
        .filter(|row| workflow_sidebar_group_matches(row, "active"))
        .count();
    let attention_count = rows
        .iter()
        .filter(|row| workflow_sidebar_group_matches(row, "attention"))
        .count();
    let event_driven_count = rows
        .iter()
        .filter(|row| workflow_sidebar_group_matches(row, "event_driven"))
        .count();
    let scheduled_count = rows
        .iter()
        .filter(|row| workflow_sidebar_group_matches(row, "scheduled"))
        .count();
    let completed_count = rows
        .iter()
        .filter(|row| workflow_sidebar_group_matches(row, "completed"))
        .count();

    InteractiveWorkflowSidebarPanel {
        schema_version: INTERACTIVE_WORKFLOW_SIDEBAR_SCHEMA_VERSION.to_string(),
        status: "workflow_sidebar_ready".to_string(),
        workflow_count: rows.len(),
        group_count: groups.len(),
        selected_workflow_id,
        selected_group_id,
        selected_index,
        active_count,
        attention_count,
        event_driven_count,
        scheduled_count,
        completed_count,
        groups,
        keyboard_hints: vec![
            "j/k move selection".to_string(),
            "enter inspect selected workflow".to_string(),
            "d open DAG".to_string(),
            "t open task board".to_string(),
            "e open event timeline".to_string(),
        ],
        commands: InteractiveWorkflowSidebarCommands {
            refresh: vec![
                "interactive".to_string(),
                "workflow-sidebar".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            list: vec![
                "list".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            task_board: vec![
                "interactive".to_string(),
                "task-board".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            workflow_dag: vec![
                "interactive".to_string(),
                "workflow-dag".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
        },
    }
}

fn select_sidebar_workflow(rows: &[WorkflowRegistryRow]) -> String {
    rows.iter()
        .find(|row| workflow_sidebar_row_is_active(row))
        .or_else(|| {
            rows.iter()
                .find(|row| workflow_sidebar_group_matches(row, "attention"))
        })
        .or_else(|| {
            rows.iter()
                .find(|row| workflow_sidebar_group_matches(row, "event_driven"))
        })
        .or_else(|| rows.first())
        .map(|row| row.workflow_id.clone())
        .unwrap_or_else(|| "none".to_string())
}

fn workflow_sidebar_group_matches(row: &WorkflowRegistryRow, group_id: &str) -> bool {
    match group_id {
        "active" => workflow_sidebar_row_is_active(row),
        "attention" => {
            matches!(
                row.runtime.operator_action.as_str(),
                "repair_workflow" | "run_due_schedule"
            ) || row.task_summary.blocked > 0
                || row.task_summary.failed > 0
                || row.human_interaction_summary.pending_required > 0
        }
        "event_driven" => {
            row.runtime.persistent
                && (row.runtime.scale_to_zero_policy == "idle_waiting_for_events"
                    || matches!(
                        row.runtime.operator_action.as_str(),
                        "keep_event_listener_ready" | "wake_on_event"
                    ))
        }
        "scheduled" => row.schedule_summary.scheduled_nodes > 0,
        "completed" => matches!(
            row.runtime.operational_state.as_str(),
            "completed" | "scaled_to_zero"
        ),
        "other" => {
            !workflow_sidebar_group_matches(row, "active")
                && !workflow_sidebar_group_matches(row, "attention")
                && !workflow_sidebar_group_matches(row, "event_driven")
                && !workflow_sidebar_group_matches(row, "scheduled")
                && !workflow_sidebar_group_matches(row, "completed")
        }
        _ => false,
    }
}

fn workflow_sidebar_row_is_active(row: &WorkflowRegistryRow) -> bool {
    row.active_run_count > 0
        || row.running
        || row
            .run_statuses
            .iter()
            .any(|status| matches!(status.as_str(), "accepted" | "resumed" | "running"))
}

fn workflow_sidebar_item(
    row: &WorkflowRegistryRow,
    selected_workflow_id: &str,
) -> InteractiveWorkflowSidebarItem {
    InteractiveWorkflowSidebarItem {
        workflow_id: row.workflow_id.clone(),
        selected: row.workflow_id == selected_workflow_id,
        title: truncate_display(&row.current_goal, 72),
        lifecycle_state: row.lifecycle_state.clone(),
        current_goal: row.current_goal.clone(),
        active_run_count: row.active_run_count,
        ready_handoff_count: row.context_actions.ready_for_handoff,
        pending_human_interaction_count: row.human_interaction_summary.pending_required,
        due_schedule_count: row.schedule_summary.due_nodes,
        artifact_count: row.artifact_count,
        runtime: row.runtime.clone(),
        schedule_summary: row.schedule_summary.clone(),
        commands: InteractiveWorkflowSidebarItemCommands {
            inspect: vec!["inspect".to_string(), row.workflow_id.clone()],
            task_board: vec![
                "interactive".to_string(),
                "task-board".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            workflow_dag: vec![
                "interactive".to_string(),
                "workflow-dag".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            events: vec![
                "events".to_string(),
                "timeline".to_string(),
                "--workflow".to_string(),
                row.workflow_id.clone(),
                "--output".to_string(),
                "json".to_string(),
            ],
            validate: vec![
                "validate".to_string(),
                "--workflow".to_string(),
                row.workflow_id.clone(),
                "--output".to_string(),
                "json".to_string(),
            ],
        },
    }
}

fn build_artifact_panel(
    store: &ForgeStore,
    rows: &[WorkflowRegistryRow],
) -> Result<InteractiveArtifactPanel> {
    let mut artifact_count = 0;
    let mut total_bytes = 0;
    let mut artifact_workflow_count = 0;
    let mut workflows = Vec::new();

    for row in rows {
        let workflow = store.load_workflow(&row.workflow_id)?;
        let listed_artifacts = list_workflow_artifacts(&store.base_dir(), &row.workflow_id)?;
        let artifact_bytes_by_path = listed_artifacts
            .into_iter()
            .map(|artifact| (artifact.path, (artifact.sha256, artifact.bytes)))
            .collect::<BTreeMap<_, _>>();
        let mut workflow_artifacts = Vec::new();
        let mut workflow_bytes = 0;

        for artifact in &workflow.artifacts {
            let (sha256, bytes) = artifact_bytes_by_path
                .get(&artifact.path)
                .cloned()
                .unwrap_or_else(|| (artifact.sha256.clone(), 0));
            workflow_bytes += bytes;
            workflow_artifacts.push(InteractiveArtifactEntry {
                artifact_id: artifact.id.clone(),
                kind: artifact.kind.clone(),
                path: artifact.path.clone(),
                sha256,
                bytes,
                created_at: artifact.created_at.to_rfc3339(),
                lineage_summary: artifact
                    .lineage
                    .as_ref()
                    .map(|lineage| {
                        format!(
                            "run {} schedule {} loop {} triggered_by {}",
                            lineage.run_id,
                            lineage.schedule_task_id,
                            lineage.loop_task_id,
                            lineage.triggered_by
                        )
                    })
                    .unwrap_or_else(|| "direct workflow attachment".to_string()),
                commands: InteractiveArtifactEntryCommands {
                    open: vec![
                        "artifacts".to_string(),
                        "--workflow".to_string(),
                        row.workflow_id.clone(),
                        "--output".to_string(),
                        "json".to_string(),
                    ],
                    inspect_workflow: vec!["inspect".to_string(), row.workflow_id.clone()],
                },
            });
        }

        artifact_count += workflow_artifacts.len();
        total_bytes += workflow_bytes;
        if !workflow_artifacts.is_empty() {
            artifact_workflow_count += 1;
        }
        if !workflow_artifacts.is_empty() && workflows.len() < 12 {
            workflows.push(InteractiveArtifactWorkflow {
                workflow_id: row.workflow_id.clone(),
                lifecycle_state: row.lifecycle_state.clone(),
                goal: truncate_display(&row.current_goal, 96),
                artifact_count: workflow_artifacts.len(),
                total_bytes: workflow_bytes,
                artifacts: workflow_artifacts,
                commands: InteractiveArtifactWorkflowCommands {
                    list: vec![
                        "artifacts".to_string(),
                        "--workflow".to_string(),
                        row.workflow_id.clone(),
                        "--output".to_string(),
                        "json".to_string(),
                    ],
                    inspect: vec!["inspect".to_string(), row.workflow_id.clone()],
                    task_board: vec!["interactive".to_string(), "task-board".to_string()],
                },
            });
        }
    }

    Ok(InteractiveArtifactPanel {
        schema_version: INTERACTIVE_ARTIFACTS_SCHEMA_VERSION.to_string(),
        status: "artifacts_ready".to_string(),
        workflow_count: artifact_workflow_count,
        artifact_count,
        total_bytes,
        workflows,
        commands: InteractiveArtifactPanelCommands {
            refresh: vec![
                "interactive".to_string(),
                "artifacts".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            task_board: vec![
                "interactive".to_string(),
                "task-board".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            workflow_list: vec![
                "list".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
        },
    })
}

fn build_token_usage_panel(store: &ForgeStore) -> Result<InteractiveTokenUsagePanel> {
    let headroom_stats = build_headroom_stats_report(
        store,
        HeadroomStatsOptions {
            source: None,
            content_kind: None,
            limit: 10,
        },
    )?;
    let primary_source = headroom_stats
        .primary_savings_source
        .clone()
        .or_else(|| {
            headroom_stats
                .by_source
                .first()
                .map(|bucket| bucket.source.clone())
        })
        .unwrap_or_else(|| "none".to_string());
    let primary_content_kind = headroom_stats
        .primary_savings_content_kind
        .clone()
        .or_else(|| {
            headroom_stats
                .by_content_kind
                .first()
                .map(|bucket| bucket.content_kind.clone())
        })
        .unwrap_or_else(|| "none".to_string());
    let retrieve_commands = headroom_stats
        .next_commands
        .iter()
        .filter(|command| command.contains("retrieve-headroom"))
        .cloned()
        .collect::<Vec<_>>();
    let retrieve_top = retrieve_commands
        .first()
        .and_then(|_| headroom_stats.top_saved_blobs.first())
        .map(|blob| {
            vec![
                "harness".to_string(),
                "retrieve-headroom".to_string(),
                "--ref".to_string(),
                blob.retrieval_ref.clone(),
                "--include-content".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ]
        })
        .unwrap_or_else(|| {
            vec![
                "harness".to_string(),
                "retrieve-headroom".to_string(),
                "--ref".to_string(),
                "<retrieval-ref>".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ]
        });

    Ok(InteractiveTokenUsagePanel {
        schema_version: INTERACTIVE_TOKEN_USAGE_SCHEMA_VERSION.to_string(),
        status: "token_usage_ready".to_string(),
        operational_status: headroom_stats.operational_status.clone(),
        recommended_action: headroom_stats.recommended_action.clone(),
        total_headroom_blobs: headroom_stats.total_blobs,
        total_original_tokens: headroom_stats.total_estimated_original_tokens,
        total_compressed_tokens: headroom_stats.total_estimated_compressed_tokens,
        estimated_saved_tokens: headroom_stats.total_estimated_saved_tokens,
        average_savings_percent: headroom_stats.average_savings_percent,
        over_budget_after_headroom_count: headroom_stats.over_budget_after_headroom_count,
        primary_source,
        primary_content_kind,
        retrieve_commands,
        source_buckets: headroom_stats.by_source.clone(),
        content_kind_buckets: headroom_stats.by_content_kind.clone(),
        headroom_stats,
        commands: InteractiveTokenUsageCommands {
            refresh: vec![
                "interactive".to_string(),
                "token-usage".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            headroom_stats: vec![
                "harness".to_string(),
                "headroom-stats".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            analyze_payload: vec![
                "harness".to_string(),
                "token-headroom".to_string(),
                "--content".to_string(),
                "<payload>".to_string(),
                "--kind".to_string(),
                "log".to_string(),
                "--budget-tokens".to_string(),
                "<n>".to_string(),
                "--persist".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            retrieve_top,
            harness: vec![
                "interactive".to_string(),
                "harness".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            cost_ledger: vec![
                "cost".to_string(),
                "ledger".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
        },
    })
}

fn build_workflow_dag_panel(
    store: &ForgeStore,
    rows: &[WorkflowRegistryRow],
) -> Result<InteractiveWorkflowDagPanel> {
    let mut node_count = 0;
    let mut edge_count = 0;
    let mut running_node_count = 0;
    let mut blocked_node_count = 0;
    let mut wait_node_count = 0;
    let mut human_wait_count = 0;
    let mut workflows = Vec::new();

    for row in rows {
        let workflow = store.load_workflow(&row.workflow_id)?;
        let dag = build_workflow_dag(row, &workflow.tasks);
        node_count += dag.node_count;
        edge_count += dag.edge_count;
        running_node_count += dag
            .nodes
            .iter()
            .filter(|node| node.status == "running")
            .count();
        blocked_node_count += dag.blocked_node_count;
        wait_node_count += dag
            .nodes
            .iter()
            .filter(|node| node.executor == "wait")
            .count();
        human_wait_count += dag.human_wait_count;

        if workflows.len() < 12 {
            workflows.push(dag);
        }
    }

    Ok(InteractiveWorkflowDagPanel {
        schema_version: INTERACTIVE_WORKFLOW_DAG_SCHEMA_VERSION.to_string(),
        status: "workflow_dag_ready".to_string(),
        workflow_count: rows.len(),
        node_count,
        edge_count,
        running_node_count,
        blocked_node_count,
        wait_node_count,
        human_wait_count,
        workflows,
    })
}

fn build_workflow_dag(row: &WorkflowRegistryRow, tasks: &[AtomicTask]) -> InteractiveWorkflowDag {
    let mut dependent_counts = BTreeMap::<String, usize>::new();
    let task_statuses = tasks
        .iter()
        .map(|task| (task.id.clone(), task_status_label(&task.status).to_string()))
        .collect::<BTreeMap<_, _>>();
    let mut edges = Vec::new();

    for task in tasks {
        for dependency in &task.dependencies {
            *dependent_counts.entry(dependency.clone()).or_default() += 1;
            edges.push(InteractiveWorkflowDagEdge {
                from_task_id: dependency.clone(),
                to_task_id: task.id.clone(),
                edge_kind: "dependency".to_string(),
                dependency_status: task_statuses
                    .get(dependency)
                    .cloned()
                    .unwrap_or_else(|| "missing".to_string()),
            });
        }
    }

    let completed_tasks = tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Completed)
        .map(|task| task.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let nodes = tasks
        .iter()
        .map(|task| {
            let human_interaction_state = task
                .human_interaction
                .as_ref()
                .map(|interaction| interaction.state.clone())
                .unwrap_or_else(|| "none".to_string());
            let human_required = task.human_required
                || task
                    .human_interaction
                    .as_ref()
                    .is_some_and(|interaction| interaction.required);
            let ready_for_execution = task.status == TaskStatus::Pending
                && task
                    .dependencies
                    .iter()
                    .all(|dependency| completed_tasks.contains(dependency.as_str()));

            InteractiveWorkflowDagNode {
                task_id: task.id.clone(),
                title: task.title.clone(),
                status: task_status_label(&task.status).to_string(),
                executor: executor_kind_label(&task.executor).to_string(),
                dependency_count: task.dependencies.len(),
                dependent_count: dependent_counts.get(&task.id).copied().unwrap_or(0),
                ready_for_execution,
                human_required,
                human_interaction_state,
            }
        })
        .collect::<Vec<_>>();

    let ready_root_count = nodes
        .iter()
        .filter(|node| node.ready_for_execution && node.dependency_count == 0)
        .count();
    let blocked_node_count = nodes.iter().filter(|node| node.status == "blocked").count();
    let human_wait_count = nodes
        .iter()
        .filter(|node| node.human_required && node.human_interaction_state == "pending")
        .count();
    let edge_count = edges.len();

    InteractiveWorkflowDag {
        workflow_id: row.workflow_id.clone(),
        lifecycle_state: row.lifecycle_state.clone(),
        goal: truncate_display(&row.current_goal, 96),
        node_count: nodes.len(),
        edge_count,
        ready_root_count,
        blocked_node_count,
        human_wait_count,
        nodes,
        edges,
        commands: InteractiveWorkflowDagCommands {
            inspect: vec!["inspect".to_string(), row.workflow_id.clone()],
            task_board: vec!["interactive".to_string(), "task-board".to_string()],
            validate: vec![
                "validate".to_string(),
                "--workflow".to_string(),
                row.workflow_id.clone(),
            ],
        },
    }
}

fn build_task_board_panel(
    store: &ForgeStore,
    rows: &[WorkflowRegistryRow],
) -> Result<InteractiveTaskBoardPanel> {
    let mut ready_handoffs = 0;
    let mut checkpoint_resume_candidates = 0;
    let mut task_count = 0;
    let mut blocked_tasks = 0;
    let mut failed_tasks = 0;
    let mut running_tasks = 0;
    let mut pending_human_interactions = 0;
    let mut artifact_count = 0;
    let mut lanes = Vec::new();
    for row in rows {
        let checkpoints = load_task_board_checkpoints(store, &row.workflow_id)?;
        let lane_ready_handoffs = row
            .context_action_refs
            .iter()
            .filter(|action| action.ready_for_handoff)
            .count();
        let lane_checkpoint_resume_candidates = checkpoints.len();
        let lane_pending_human_interactions = row.human_interaction_summary.pending_required;
        task_count += row.task_summary.total;
        blocked_tasks += row.task_summary.blocked;
        failed_tasks += row.task_summary.failed;
        running_tasks += row.task_summary.running;
        ready_handoffs += lane_ready_handoffs;
        checkpoint_resume_candidates += lane_checkpoint_resume_candidates;
        pending_human_interactions += lane_pending_human_interactions;
        artifact_count += row.artifact_count;

        if lanes.len() < 12 {
            lanes.push(InteractiveTaskBoardLane {
                workflow_id: row.workflow_id.clone(),
                lifecycle_state: row.lifecycle_state.clone(),
                goal: truncate_display(&row.current_goal, 96),
                total_tasks: row.task_summary.total,
                pending_tasks: row.task_summary.pending,
                running_tasks: row.task_summary.running,
                completed_tasks: row.task_summary.completed,
                blocked_tasks: row.task_summary.blocked,
                failed_tasks: row.task_summary.failed,
                ready_handoffs: lane_ready_handoffs,
                checkpoint_resume_candidates: lane_checkpoint_resume_candidates,
                pending_human_interactions: lane_pending_human_interactions,
                artifact_count: row.artifact_count,
                next_actions: task_board_next_actions(row, &checkpoints),
                task_cards: build_task_board_task_cards(store, row, &checkpoints)?,
            });
        }
    }

    Ok(InteractiveTaskBoardPanel {
        schema_version: INTERACTIVE_TASK_BOARD_SCHEMA_VERSION.to_string(),
        status: "task_board_ready".to_string(),
        workflow_count: rows.len(),
        task_count,
        ready_handoffs,
        blocked_tasks,
        failed_tasks,
        running_tasks,
        checkpoint_resume_candidates,
        pending_human_interactions,
        artifact_count,
        lanes,
    })
}

fn build_workflow_mutation_panel(
    rows: &[WorkflowRegistryRow],
    task_board: &InteractiveTaskBoardPanel,
    dag: &InteractiveWorkflowDagPanel,
    modifier_lane: &InteractiveOperationalModifierLanePanel,
    event_panel: &InteractiveEventPanel,
    cost_panel: &InteractiveCostPanel,
) -> InteractiveWorkflowMutationPanel {
    let workflow_cards = rows
        .iter()
        .take(12)
        .map(|row| workflow_mutation_card(row, task_board, dag, modifier_lane))
        .collect::<Vec<_>>();
    let active_workflow_count = workflow_cards.iter().filter(|card| card.active).count();
    let mutable_workflow_count = workflow_cards
        .iter()
        .filter(|card| !card.mutable_targets.is_empty())
        .count();
    let task_count = task_board.task_count;
    let ready_handoff_count = task_board.ready_handoffs;
    let human_wait_count = task_board.pending_human_interactions;
    let checkpoint_resume_candidate_count = task_board.checkpoint_resume_candidates;
    let artifact_count = task_board.artifact_count;
    let status = if modifier_lane.pending_count > 0
        || ready_handoff_count > 0
        || human_wait_count > 0
        || checkpoint_resume_candidate_count > 0
    {
        "workflow_mutation_actionable"
    } else if !workflow_cards.is_empty() {
        "workflow_mutation_ready"
    } else {
        "workflow_mutation_idle"
    };
    let proposal_cards = modifier_lane
        .proposal_cards
        .iter()
        .take(12)
        .cloned()
        .collect();
    let mut next_actions = vec![
        "forge interactive workflow-mutation --output json".to_string(),
        "forge interactive task-board --output json".to_string(),
        "forge interactive workflow-dag --output json".to_string(),
        "forge interactive operational-cockpit --output json".to_string(),
        "forge ops serve --project-root . --host 127.0.0.1 --port 8765".to_string(),
        "forge interactive action-registry --query workflow --output json".to_string(),
    ];
    if modifier_lane.pending_count > 0 {
        next_actions.insert(
            0,
            "review pending modifier proposals, then apply through the ops console/API if approved"
                .to_string(),
        );
    } else if workflow_cards.is_empty() {
        next_actions.push(
            "start a workflow with forge request start --goal \"...\" --origin forge_cli"
                .to_string(),
        );
    } else {
        next_actions.push(
            "choose a workflow card, inspect its task, then mutate through workflow update-goal, update-node-brain or attach-artifact".to_string(),
        );
    }

    InteractiveWorkflowMutationPanel {
        schema_version: INTERACTIVE_WORKFLOW_MUTATION_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        operation_mode: "read_only_replanning_surface_governed_by_workflow_mutations".to_string(),
        workflow_count: rows.len(),
        active_workflow_count,
        mutable_workflow_count,
        task_count,
        ready_handoff_count,
        human_wait_count,
        checkpoint_resume_candidate_count,
        artifact_count,
        pending_modifier_proposal_count: modifier_lane.pending_count,
        applied_modifier_proposal_count: modifier_lane.applied_count,
        event_count: event_panel.total_event_count,
        estimated_cost_total_usd: cost_panel.estimated_task_cost_total_usd,
        workflow_cards,
        proposal_cards,
        commands: InteractiveWorkflowMutationCommands {
            refresh: vec![
                "interactive".to_string(),
                "workflow-mutation".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            task_board: vec![
                "interactive".to_string(),
                "task-board".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            workflow_dag: vec![
                "interactive".to_string(),
                "workflow-dag".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            operational_cockpit: vec![
                "interactive".to_string(),
                "operational-cockpit".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            action_registry: vec![
                "interactive".to_string(),
                "action-registry".to_string(),
                "--query".to_string(),
                "workflow".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            ops_console: vec![
                "ops".to_string(),
                "serve".to_string(),
                "--project-root".to_string(),
                ".".to_string(),
                "--host".to_string(),
                "127.0.0.1".to_string(),
                "--port".to_string(),
                "8765".to_string(),
            ],
            propose_goal_route: "POST /api/modifier/propose-goal".to_string(),
            propose_task_route: "POST /api/modifier/propose-task".to_string(),
            apply_proposal_route: "POST /api/modifier/apply".to_string(),
            update_goal: vec![
                "workflow".to_string(),
                "update-goal".to_string(),
                "--workflow".to_string(),
                "<workflow-id>".to_string(),
                "--goal".to_string(),
                "<new-goal>".to_string(),
                "--origin".to_string(),
                "forge_cli".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            update_node_brain: vec![
                "workflow".to_string(),
                "update-node-brain".to_string(),
                "--workflow".to_string(),
                "<workflow-id>".to_string(),
                "--task".to_string(),
                "<task-id>".to_string(),
                "--default-brain".to_string(),
                "<brain>".to_string(),
                "--origin".to_string(),
                "forge_cli".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            attach_artifact: vec![
                "workflow".to_string(),
                "attach-artifact".to_string(),
                "--workflow".to_string(),
                "<workflow-id>".to_string(),
                "--path".to_string(),
                "<artifact-path>".to_string(),
                "--kind".to_string(),
                "<kind>".to_string(),
                "--origin".to_string(),
                "forge_cli".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            validate: vec![
                "validate".to_string(),
                "--workflow".to_string(),
                "<workflow-id>".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            context: vec![
                "context".to_string(),
                "--workflow".to_string(),
                "<workflow-id>".to_string(),
                "--task".to_string(),
                "<task-id>".to_string(),
                "--strict".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            handoff: vec![
                "task".to_string(),
                "handoff".to_string(),
                "--workflow".to_string(),
                "<workflow-id>".to_string(),
                "--task".to_string(),
                "<task-id>".to_string(),
                "--executor".to_string(),
                "<executor>".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            structured_logs: vec![
                "interactive".to_string(),
                "structured-logs".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
        },
        next_actions,
        notes: vec![
            "The panel is read-only; every mutation must still go through Forge workflow/ops APIs so revisions, origin and validation remain auditable.".to_string(),
            "Use it to keep DAG, task board, modifier lane, handoffs, costs and logs in one replanning routine while a workflow keeps running.".to_string(),
        ],
    }
}

fn workflow_mutation_card(
    row: &WorkflowRegistryRow,
    task_board: &InteractiveTaskBoardPanel,
    dag: &InteractiveWorkflowDagPanel,
    modifier_lane: &InteractiveOperationalModifierLanePanel,
) -> InteractiveWorkflowMutationCard {
    let lane = task_board
        .lanes
        .iter()
        .find(|lane| lane.workflow_id == row.workflow_id);
    let dag_workflow = dag
        .workflows
        .iter()
        .find(|workflow| workflow.workflow_id == row.workflow_id);
    let ready_handoffs = lane
        .map(|lane| lane.ready_handoffs)
        .unwrap_or(row.context_actions.ready_for_handoff);
    let human_waits = lane
        .map(|lane| lane.pending_human_interactions)
        .unwrap_or(row.human_interaction_summary.pending_required);
    let checkpoint_resume_candidates = lane
        .map(|lane| lane.checkpoint_resume_candidates)
        .unwrap_or(0);
    let task_count = lane
        .map(|lane| lane.total_tasks)
        .unwrap_or(row.task_summary.total);
    let dag_node_count = dag_workflow
        .map(|workflow| workflow.node_count)
        .unwrap_or(task_count);
    let dag_edge_count = dag_workflow
        .map(|workflow| workflow.edge_count)
        .unwrap_or(0);
    let has_pending_proposal = modifier_lane
        .proposal_cards
        .iter()
        .any(|proposal| proposal.workflow_id == row.workflow_id && proposal.status == "pending");
    let mut mutable_targets = vec![
        "workflow_goal".to_string(),
        "task_node_brain".to_string(),
        "artifact_attachment".to_string(),
    ];
    if task_count > 0 {
        mutable_targets.push("task_node".to_string());
    }
    if ready_handoffs > 0 {
        mutable_targets.push("executor_handoff".to_string());
    }
    if human_waits > 0 {
        mutable_targets.push("human_wait_resolution".to_string());
    }
    if checkpoint_resume_candidates > 0 {
        mutable_targets.push("checkpoint_resume".to_string());
    }
    let recommended_action = if has_pending_proposal {
        "review_modifier_proposals"
    } else if human_waits > 0 {
        "resolve_human_waits_before_mutation"
    } else if ready_handoffs > 0 {
        "prepare_handoff_or_update_node_brain"
    } else if checkpoint_resume_candidates > 0 {
        "resume_or_replan_from_checkpoint"
    } else if workflow_sidebar_row_is_active(row) {
        "monitor_or_update_goal"
    } else {
        "inspect_before_replanning"
    };
    let context_task = row
        .context_action_refs
        .first()
        .map(|action| action.task_id.clone())
        .unwrap_or_else(|| "<task-id>".to_string());
    let handoff_executor = row
        .context_action_refs
        .first()
        .map(|action| action.executor.clone())
        .unwrap_or_else(|| "<executor>".to_string());

    InteractiveWorkflowMutationCard {
        workflow_id: row.workflow_id.clone(),
        lifecycle_state: row.lifecycle_state.clone(),
        goal: truncate_display(&row.current_goal, 120),
        active: workflow_sidebar_row_is_active(row),
        task_count,
        ready_handoffs,
        human_waits,
        checkpoint_resume_candidates,
        artifact_count: row.artifact_count,
        dag_node_count,
        dag_edge_count,
        mutable_targets,
        recommended_action: recommended_action.to_string(),
        commands: InteractiveWorkflowMutationWorkflowCommands {
            inspect: vec!["inspect".to_string(), row.workflow_id.clone()],
            task_board: vec![
                "interactive".to_string(),
                "task-board".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            workflow_dag: vec![
                "interactive".to_string(),
                "workflow-dag".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            validate: vec![
                "validate".to_string(),
                "--workflow".to_string(),
                row.workflow_id.clone(),
                "--output".to_string(),
                "json".to_string(),
            ],
            update_goal: vec![
                "workflow".to_string(),
                "update-goal".to_string(),
                "--workflow".to_string(),
                row.workflow_id.clone(),
                "--goal".to_string(),
                "<new-goal>".to_string(),
                "--origin".to_string(),
                "forge_cli".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            update_node_brain: vec![
                "workflow".to_string(),
                "update-node-brain".to_string(),
                "--workflow".to_string(),
                row.workflow_id.clone(),
                "--task".to_string(),
                context_task.clone(),
                "--default-brain".to_string(),
                "<brain>".to_string(),
                "--origin".to_string(),
                "forge_cli".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            attach_artifact: vec![
                "workflow".to_string(),
                "attach-artifact".to_string(),
                "--workflow".to_string(),
                row.workflow_id.clone(),
                "--path".to_string(),
                "<artifact-path>".to_string(),
                "--kind".to_string(),
                "<kind>".to_string(),
                "--origin".to_string(),
                "forge_cli".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            context: vec![
                "context".to_string(),
                "--workflow".to_string(),
                row.workflow_id.clone(),
                "--task".to_string(),
                context_task.clone(),
                "--strict".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            handoff: vec![
                "task".to_string(),
                "handoff".to_string(),
                "--workflow".to_string(),
                row.workflow_id.clone(),
                "--task".to_string(),
                context_task,
                "--executor".to_string(),
                handoff_executor,
                "--output".to_string(),
                "json".to_string(),
            ],
        },
    }
}

fn task_board_next_actions(
    row: &WorkflowRegistryRow,
    checkpoints: &[TaskCheckpoint],
) -> Vec<String> {
    let mut actions = vec![format!("forge inspect {}", row.workflow_id)];

    if let Some(handoff) = row
        .context_action_refs
        .iter()
        .find(|action| action.ready_for_handoff)
    {
        actions.push(format!(
            "forge task handoff --workflow {} --task {} --executor {}",
            row.workflow_id, handoff.task_id, handoff.executor
        ));
    }

    if let Some(task_id) = checkpoints
        .last()
        .map(|checkpoint| checkpoint.task_id.as_str())
    {
        actions.push(format!(
            "forge context --workflow {} --task {}",
            row.workflow_id, task_id
        ));
    }

    if row.human_interaction_summary.pending_required > 0 {
        actions.push("forge interaction list".to_string());
    }

    if row.artifact_count > 0 {
        actions.push(format!("forge artifacts --workflow {}", row.workflow_id));
    }

    actions
}

fn load_task_board_checkpoints(
    store: &ForgeStore,
    workflow_id: &str,
) -> Result<Vec<TaskCheckpoint>> {
    store
        .load_task_checkpoints(workflow_id, None)?
        .into_iter()
        .map(serde_json::from_value)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn build_task_board_task_cards(
    store: &ForgeStore,
    row: &WorkflowRegistryRow,
    checkpoints: &[TaskCheckpoint],
) -> Result<Vec<InteractiveTaskBoardTaskCard>> {
    let workflow = store.load_workflow(&row.workflow_id)?;
    let events = store.load_workflow_events(&row.workflow_id)?;
    let dependent_counts = task_dependent_counts(&workflow.tasks);
    Ok(workflow
        .tasks
        .iter()
        .map(|task| build_task_board_task_card(row, task, checkpoints, &events, &dependent_counts))
        .collect())
}

fn build_task_board_task_card(
    row: &WorkflowRegistryRow,
    task: &AtomicTask,
    checkpoints: &[TaskCheckpoint],
    events: &[StoreEvent],
    dependent_counts: &BTreeMap<String, usize>,
) -> InteractiveTaskBoardTaskCard {
    let action_ref = row
        .context_action_refs
        .iter()
        .find(|action| action.task_id == task.id);
    let checkpoint = latest_task_checkpoint(checkpoints, &task.id);
    let human_interaction_state = task
        .human_interaction
        .as_ref()
        .map(|interaction| interaction.state.clone())
        .unwrap_or_else(|| "none".to_string());
    let human_required = task.human_required
        || task
            .human_interaction
            .as_ref()
            .is_some_and(|interaction| interaction.required);
    let checkpoint_id = checkpoint
        .map(|checkpoint| checkpoint.checkpoint_id.clone())
        .or_else(|| action_ref.and_then(|action| action.checkpoint_id.clone()));
    let checkpoint_state = checkpoint.map(|checkpoint| checkpoint.state.clone());
    let ready_for_handoff = action_ref.is_some_and(|action| action.ready_for_handoff);
    let context_action = action_ref
        .map(|action| action.action.clone())
        .unwrap_or_else(|| "inspect_task".to_string());
    let next_action = task_board_task_next_action(
        human_required,
        &human_interaction_state,
        checkpoint_id.as_deref(),
        action_ref,
    );
    let history_events = task_history_events(events, &task.id);
    let latest_history_event = history_events
        .last()
        .map(|event| InteractiveTaskHistoryEvent {
            event_id: event.id,
            kind: event.kind.clone(),
            created_at: event.created_at.clone(),
        });

    InteractiveTaskBoardTaskCard {
        task_id: task.id.clone(),
        title: task.title.clone(),
        status: task_status_label(&task.status).to_string(),
        executor: executor_kind_label(&task.executor).to_string(),
        dependency_count: task.dependencies.len(),
        dependent_count: dependent_counts.get(&task.id).copied().unwrap_or(0),
        context_requirement_count: task.context_requirements.len(),
        validation_rule_count: task.validation_rules.len(),
        estimated_cost_usd: task.cost.estimated_cost_usd,
        cost_model: task.cost.cost_model.clone(),
        workflow_artifact_count: row.artifact_count,
        history_event_count: history_events.len(),
        latest_history_event,
        human_required,
        human_interaction_state,
        ready_for_handoff,
        context_action,
        checkpoint_id,
        checkpoint_state,
        next_action,
        commands: task_board_task_commands(row, task, action_ref, checkpoint),
    }
}

fn task_dependent_counts(tasks: &[AtomicTask]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for task in tasks {
        for dependency in &task.dependencies {
            *counts.entry(dependency.clone()).or_default() += 1;
        }
    }
    counts
}

fn task_history_events<'a>(events: &'a [StoreEvent], task_id: &str) -> Vec<&'a StoreEvent> {
    events
        .iter()
        .filter(|event| event_refs_task(event, task_id))
        .collect()
}

fn event_refs_task(event: &StoreEvent, task_id: &str) -> bool {
    json_string_matches(&event.data, &["task_id", "task"], task_id)
        || event.data.get("checkpoint").is_some_and(|checkpoint| {
            json_string_matches(checkpoint, &["task_id", "task"], task_id)
        })
        || event.data.get("interaction").is_some_and(|interaction| {
            json_string_matches(interaction, &["task_id", "task"], task_id)
        })
        || event
            .data
            .get("task")
            .is_some_and(|task| json_string_matches(task, &["id", "task_id"], task_id))
}

fn json_string_matches(value: &serde_json::Value, keys: &[&str], expected: &str) -> bool {
    keys.iter()
        .any(|key| value.get(*key).and_then(|value| value.as_str()) == Some(expected))
}

fn latest_task_checkpoint<'a>(
    checkpoints: &'a [TaskCheckpoint],
    task_id: &str,
) -> Option<&'a TaskCheckpoint> {
    checkpoints
        .iter()
        .rev()
        .find(|checkpoint| checkpoint.task_id == task_id)
}

fn task_board_task_next_action(
    human_required: bool,
    human_interaction_state: &str,
    checkpoint_id: Option<&str>,
    action_ref: Option<&RegistryContextActionRef>,
) -> String {
    if human_required && human_interaction_state == "pending" {
        return "answer_human_interaction".to_string();
    }

    if checkpoint_id.is_some() {
        return "resume_from_checkpoint".to_string();
    }

    action_ref
        .map(|action| action.action.clone())
        .unwrap_or_else(|| "inspect_task".to_string())
}

fn task_board_task_commands(
    row: &WorkflowRegistryRow,
    task: &AtomicTask,
    action_ref: Option<&RegistryContextActionRef>,
    checkpoint: Option<&TaskCheckpoint>,
) -> Vec<String> {
    let mut commands = vec![format!(
        "forge inspect {} --task {}",
        row.workflow_id, task.id
    )];

    if task
        .human_interaction
        .as_ref()
        .is_some_and(|interaction| interaction.required && interaction.state == "pending")
    {
        commands.push("forge interaction list".to_string());
    }

    if let Some(action) = action_ref {
        if action.ready_for_handoff {
            commands.push(format!(
                "forge task handoff --workflow {} --task {} --executor {}",
                row.workflow_id, task.id, action.executor
            ));
        }
    }

    if checkpoint.is_some()
        || action_ref
            .and_then(|action| action.checkpoint_id.as_ref())
            .is_some()
    {
        commands.push(format!(
            "forge context --workflow {} --task {}",
            row.workflow_id, task.id
        ));
    }

    commands
}

fn task_status_label(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Pending => "pending",
        TaskStatus::Running => "running",
        TaskStatus::Completed => "completed",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Failed => "failed",
    }
}

fn executor_kind_label(executor: &ExecutorKind) -> &'static str {
    match executor {
        ExecutorKind::Ai => "ai",
        ExecutorKind::Command => "command",
        ExecutorKind::Wait => "wait",
        ExecutorKind::Notification => "notification",
        ExecutorKind::Mixed => "mixed",
    }
}

fn truncate_display(value: &str, max_chars: usize) -> String {
    let total_chars = value.chars().count();
    if total_chars <= max_chars {
        return value.to_string();
    }
    if max_chars <= 3 {
        return value.chars().take(max_chars).collect();
    }
    let mut truncated: String = value.chars().take(max_chars - 3).collect();
    truncated.push_str("...");
    truncated
}

fn build_attention_actions(attention_runs: &[&crate::request::RequestListRow]) -> Vec<String> {
    if attention_runs.is_empty() {
        return Vec::new();
    }

    let mut actions = vec![
        "forge request list --status needs_attention".to_string(),
        "forge request list --status stale".to_string(),
    ];
    for run in attention_runs.iter().take(3) {
        actions.push(format!("forge request status --run {}", run.run_id));
        if run.activity.heartbeat_status == "stale" {
            actions.push(format!("forge request recover-stale --run {}", run.run_id));
        } else if run.status == "needs_attention" {
            actions.push(format!("forge request resume --run {}", run.run_id));
            actions.push(format!("forge request cancel --run {}", run.run_id));
        }
    }
    actions
}

fn route_slash_command(trimmed: &str) -> InteractiveRouteReport {
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    let one_token = tokens
        .first()
        .map(|t| t.to_ascii_lowercase())
        .unwrap_or_else(|| "/".to_string());
    let commands = slash_commands();
    let matched = commands
        .iter()
        .filter_map(|command| {
            let command_tokens = command.name.split_whitespace().collect::<Vec<_>>();
            if command_tokens.len() > tokens.len() {
                return None;
            }
            let matches = command_tokens
                .iter()
                .zip(tokens.iter())
                .all(|(expected, actual)| expected.eq_ignore_ascii_case(actual));
            matches.then_some((command, command_tokens.len()))
        })
        .max_by_key(|(_, consumed)| *consumed);
    let recognized = matched.is_some();
    let consumed_tokens = matched.map(|(_, consumed)| consumed).unwrap_or(1);
    let input_arguments = tokens
        .iter()
        .skip(consumed_tokens)
        .map(|token| (*token).to_string())
        .collect::<Vec<_>>();
    let input_argument_text = input_arguments.join(" ");
    let route = matched
        .map(|(command, _)| command)
        .map(|command| SlashCommandRoute {
            name: command.name.clone(),
            recognized: true,
            input_arguments: input_arguments.clone(),
            input_argument_text: input_argument_text.clone(),
            equivalent_command: command.equivalent_command.clone(),
            mutates_workflow: command.mutates_workflow,
            risk_level: command.risk_level.clone(),
            execution_boundary: "slash_command_not_executed".to_string(),
        })
        .unwrap_or_else(|| SlashCommandRoute {
            name: one_token,
            recognized: false,
            input_arguments,
            input_argument_text,
            equivalent_command: vec![
                "forge".to_string(),
                "interactive".to_string(),
                "slash-commands".to_string(),
            ],
            mutates_workflow: false,
            risk_level: "unknown".to_string(),
            execution_boundary: "catalog_lookup_not_executed".to_string(),
        });

    InteractiveRouteReport {
        status: "routed".to_string(),
        schema_version: INTERACTIVE_ROUTE_SCHEMA_VERSION.to_string(),
        input_kind: "slash_command".to_string(),
        routing_decision: "slash_command".to_string(),
        routing_explanation: if recognized {
            "Explicit slash command selected; Forge keeps this in command mode.".to_string()
        } else {
            "Unknown slash command; Forge exposes the command catalog instead of guessing."
                .to_string()
        },
        workflow_created: false,
        run_id: None,
        workflow_id: None,
        answer: None,
        slash_command: Some(route),
        product_decision_id: None,
        product_decision_revision: None,
        retention_decision: no_retention_decision(),
    }
}

fn can_answer_directly(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    let asks_state = lower.contains("status")
        || lower.contains("what is")
        || lower.contains("current")
        || lower.contains("help");
    asks_state && !requires_workflow(&lower)
}

fn executor_or_runtime_required(lower: &str) -> bool {
    lower.contains("codex")
        || lower.contains("opencode")
        || lower.contains("gemini")
        || lower.contains("claude")
        || lower.contains("brain")
        || lower.contains("cerebro")
        || lower.contains("cérebro")
        || lower.contains("memory")
        || lower.contains("memoria")
        || lower.contains("memória")
        || lower.contains("skill")
        || lower.contains("mcp")
        || lower.contains("docker")
        || lower.contains("k8s")
        || lower.contains("kubernetes")
        || lower.contains("knative")
}

fn cost_sensitive(lower: &str) -> bool {
    let has_cost_keyword =
        lower.contains("cost") || lower.contains("expensive") || lower.contains("budget");
    let has_expensive_action = lower.contains("deploy")
        || lower.contains("external")
        || lower.contains("telegram")
        || lower.contains("send")
        || lower.contains("notification")
        || lower.contains("artifact");
    has_cost_keyword && has_expensive_action
}

fn requires_workflow(lower: &str) -> bool {
    let base_terms = [
        "research",
        "pesquise",
        "implement",
        "code",
        "artifact",
        "pdf",
        "telegram",
        "schedule",
        "cron",
        "every day",
        "daily",
        "validate",
        "run",
        "workflow",
        "external",
        "deploy",
        "delete",
    ];
    base_terms.iter().any(|needle| lower.contains(needle))
        || executor_or_runtime_required(lower)
        || cost_sensitive(lower)
}

fn classify_workflow_reason(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    if lower.contains("every day")
        || lower.contains("daily")
        || lower.contains("schedule")
        || lower.contains("cron")
    {
        return "Request needs scheduled work, durable state and asynchronous continuation; Forge created a workflow/run.".to_string();
    }
    if lower.contains("artifact") || lower.contains("pdf") || lower.contains("telegram") {
        return "Request needs artifacts or external delivery records; Forge created a workflow/run for lineage and validation.".to_string();
    }
    if lower.contains("research") || lower.contains("validate") || lower.contains("implement") {
        return "Request needs multi-step execution and validation; Forge created a workflow/run."
            .to_string();
    }
    if executor_or_runtime_required(&lower) {
        return "Request references an executor or async runtime; Forge created a workflow/run for durable orchestration.".to_string();
    }
    if cost_sensitive(&lower) {
        return "Request has cost or budget implications; Forge created a workflow/run for tracking and simulation.".to_string();
    }
    "Request is not a simple low-risk answer; Forge created a workflow/run.".to_string()
}

fn decide_retention(input: &str, workflow_created: bool) -> RetentionDecision {
    if !workflow_created {
        return no_retention_decision();
    }

    let lower = input.to_ascii_lowercase();
    let has_artifact =
        lower.contains("artifact") || lower.contains("pdf") || lower.contains("report");
    let has_side_effect = lower.contains("telegram")
        || lower.contains("external")
        || lower.contains("send")
        || lower.contains("deploy");
    let asks_delete = lower.contains("delete") || lower.contains("remove");
    let recurring = lower.contains("every day")
        || lower.contains("daily")
        || lower.contains("schedule")
        || lower.contains("cron");

    if asks_delete && (has_artifact || has_side_effect) {
        return RetentionDecision {
            schema_version: "forge.interactive.retention_decision.v1".to_string(),
            action: "keep_until_approved".to_string(),
            reason:
                "Deletion requested, but the workflow mentions artifact lineage or external side effect evidence; human approval is required before deletion."
                    .to_string(),
            confidence: 0.94,
            requires_human_approval: true,
        };
    }

    if recurring || has_artifact || has_side_effect {
        return RetentionDecision {
            schema_version: "forge.interactive.retention_decision.v1".to_string(),
            action: "retain".to_string(),
            reason:
                "Workflow has likely reuse, recurring schedule, artifact value or delivery evidence."
                    .to_string(),
            confidence: 0.86,
            requires_human_approval: false,
        };
    }

    RetentionDecision {
        schema_version: "forge.interactive.retention_decision.v1".to_string(),
        action: "archive".to_string(),
        reason: "Workflow is execution-backed but not obviously recurring; archive after answer unless promoted.".to_string(),
        confidence: 0.68,
        requires_human_approval: false,
    }
}

fn no_retention_decision() -> RetentionDecision {
    RetentionDecision {
        schema_version: "forge.interactive.retention_decision.v1".to_string(),
        action: "none".to_string(),
        reason: "No durable workflow state was created.".to_string(),
        confidence: 1.0,
        requires_human_approval: false,
    }
}

fn slash_commands() -> Vec<SlashCommandSpec> {
    vec![
        slash(
            "/help",
            "Help",
            "Show interactive commands.",
            &["forge", "interactive", "slash-commands"],
            false,
            "low",
        ),
        slash(
            "/guided-cockpit",
            "Guided Cockpit",
            "Show the Forge 0.5 guided cockpit with the end-to-end operator checklist.",
            &["forge", "interactive", "guided-cockpit"],
            false,
            "low",
        ),
        slash(
            "/guide",
            "Guide",
            "Alias for the guided cockpit.",
            &["forge", "interactive", "guided-cockpit"],
            false,
            "low",
        ),
        slash(
            "/ui-composition",
            "UI Composition",
            "Show dynamic Core and Addon widget composition for TUI, web and agent dashboards.",
            &["forge", "interactive", "ui-composition"],
            false,
            "low",
        ),
        slash(
            "/status",
            "Status",
            "Show workflow or runtime status.",
            &["forge", "status", "--workflow", "<workflow-id>"],
            false,
            "low",
        ),
        slash(
            "/list",
            "List",
            "List workflows.",
            &["forge", "list"],
            false,
            "low",
        ),
        slash(
            "/inspect",
            "Inspect",
            "Inspect a workflow graph.",
            &["forge", "inspect", "<workflow-id>"],
            false,
            "low",
        ),
        slash(
            "/runs",
            "Runs",
            "List async requests.",
            &["forge", "request", "list"],
            false,
            "low",
        ),
        slash(
            "/workflows",
            "Workflows",
            "Show the workflow sidebar for operator navigation.",
            &["forge", "interactive", "workflow-sidebar"],
            false,
            "low",
        ),
        slash(
            "/workflow-sidebar",
            "Workflow Sidebar",
            "Show grouped workflow navigation with selected workflow and drill-down commands.",
            &["forge", "interactive", "workflow-sidebar"],
            false,
            "low",
        ),
        slash(
            "/replacement-cli",
            "Replacement CLI",
            "Show replacement-grade CLI readiness across TUI, actions, patch UX, harness, sessions and milestone evidence.",
            &["forge", "interactive", "replacement-cli"],
            false,
            "low",
        ),
        slash(
            "/multimodal-runtime",
            "Multimodal Runtime",
            "Show Addon-owned multimodal runtime readiness, guards, templates and production evidence blockers.",
            &[
                "forge",
                "interactive",
                "multimodal-runtime",
                "--project-root",
                ".",
            ],
            false,
            "low",
        ),
        slash(
            "/artifacts",
            "Artifacts",
            "Show workflow artifacts and evidence from the interactive panel.",
            &["forge", "interactive", "artifacts"],
            false,
            "low",
        ),
        slash(
            "/tokens",
            "Tokens",
            "Show token usage, context compression and headroom savings.",
            &["forge", "interactive", "token-usage"],
            false,
            "low",
        ),
        slash(
            "/task-board",
            "Task Board",
            "Show operational workflow lanes with handoffs, checkpoints, human waits and artifacts.",
            &["forge", "interactive", "task-board"],
            false,
            "low",
        ),
        slash(
            "/workflow-mutation",
            "Workflow Mutation",
            "Show the replanning surface that combines DAG, task-board, modifier lane, handoffs, costs and safe mutation commands.",
            &["forge", "interactive", "workflow-mutation"],
            false,
            "low",
        ),
        slash(
            "/cockpit",
            "Operational Cockpit",
            "Show the dedicated operational cockpit for attention, handoffs, waits, brain readiness and observability.",
            &["forge", "interactive", "operational-cockpit"],
            false,
            "low",
        ),
        slash(
            "/architecture",
            "Architecture Compass",
            "Show source-of-truth architecture tracks, implementation evidence, gaps, dependencies and benchmark boundaries.",
            &["forge", "interactive", "architecture"],
            false,
            "low",
        ),
        slash(
            "/core-boundary",
            "Core Boundary",
            "Audit whether the Core kernel remains universal and domain capabilities stay Addon-owned.",
            &["forge", "interactive", "core-boundary"],
            false,
            "low",
        ),
        slash(
            "/boundary",
            "Boundary",
            "Alias for the Core boundary audit.",
            &["forge", "interactive", "core-boundary"],
            false,
            "low",
        ),
        slash(
            "/readiness",
            "Readiness",
            "Show executor, brain, shell and harness readiness before operational handoff.",
            &["forge", "interactive", "readiness"],
            false,
            "low",
        ),
        slash(
            "/schedules",
            "Schedules",
            "Show scheduled workflows, due work, worker capacity, sleep plan and deterministic assignment queue.",
            &["forge", "interactive", "schedules"],
            false,
            "low",
        ),
        slash(
            "/addons",
            "Addons/Capabilities",
            "Show Addons, capabilities, permission gates, runtime contracts, views and dispatch state.",
            &["forge", "interactive", "addon-capabilities"],
            false,
            "low",
        ),
        slash(
            "/costs",
            "Costs",
            "Inspect or simulate workflow costs.",
            &["forge", "run", "--workflow", "<workflow-id>", "--simulate"],
            false,
            "medium",
        ),
        slash(
            "/improvement-loop",
            "Improvement Loop",
            "Show self-improvement candidates with log, cost, validation and outcome evidence.",
            &["forge", "interactive", "improvement-loop"],
            false,
            "low",
        ),
        slash(
            "/config",
            "Config",
            "Inspect Forge-owned config surfaces.",
            &["forge", "executors"],
            false,
            "low",
        ),
        slash(
            "/sync",
            "Sync",
            "Sync executor and runtime availability.",
            &["forge", "sync", "all"],
            true,
            "medium",
        ),
        slash(
            "/executors",
            "Executors",
            "List executor policy.",
            &["forge", "executors"],
            false,
            "low",
        ),
        slash(
            "/brains",
            "Brains",
            "List Forge-controlled execution brains and routing boundaries.",
            &["forge", "brains"],
            false,
            "low",
        ),
        slash(
            "/sessions",
            "Sessions",
            "Inspect Forge-controlled provider and shell session management state.",
            &["forge", "sessions", "--output", "json"],
            false,
            "low",
        ),
        slash(
            "/sessions history",
            "Session History",
            "Inspect one Forge-controlled shell session's chronological launch and lifecycle audit.",
            &[
                "forge",
                "sessions",
                "history",
                "--session",
                "<session-id>",
                "--output",
                "json",
            ],
            false,
            "low",
        ),
        slash(
            "/sessions lifecycle",
            "Session Lifecycle",
            "Record an auditable lifecycle state for a Forge-controlled shell session.",
            &[
                "forge",
                "sessions",
                "lifecycle",
                "--session",
                "<session-id>",
                "--state",
                "opened",
            ],
            true,
            "medium",
        ),
        slash(
            "/actions",
            "Actions",
            "List governed interactive actions from the stable registry. Use: /actions [query]",
            &[
                "forge",
                "interactive",
                "action-registry",
                "--query",
                "<query>",
                "--output",
                "json",
            ],
            false,
            "low",
        ),
        slash(
            "/action",
            "Action",
            "Resolve one selected interactive action into a safe invocation plan without executing it. Use: /action <action-id>",
            &[
                "forge",
                "interactive",
                "action-invocation",
                "--action",
                "<action-id>",
                "--output",
                "json",
            ],
            false,
            "low",
        ),
        slash(
            "/shells",
            "Shells",
            "List Forge-controlled TUI and external brain shell entrypoints.",
            &["forge", "brains"],
            false,
            "low",
        ),
        slash(
            "/harness",
            "Harness",
            "Audit the effective Forge-first CLI harness mode before opening brain shells.",
            &["forge", "harness", "mode", "--output", "json"],
            false,
            "low",
        ),
        slash(
            "/harness doctor",
            "Harness Doctor",
            "Audit full Forge-first CLI readiness for one brain before opening or handing off shells.",
            &[
                "forge",
                "harness",
                "doctor",
                "--executor",
                "<executor>",
                "--shim-dir",
                "<dir>",
                "--project-root",
                "<project-root>",
                "--output",
                "json",
            ],
            false,
            "low",
        ),
        slash(
            "/harness headroom-plan",
            "Harness Headroom Plan",
            "Inspect token-headroom wrapper policy before opening Forge-controlled brain shells.",
            &[
                "forge",
                "harness",
                "headroom-plan",
                "--executor",
                "<executor>",
                "--project-root",
                "<project-root>",
                "--output",
                "json",
            ],
            false,
            "low",
        ),
        slash(
            "/harness headroom-stats",
            "Harness Headroom Stats",
            "Inspect persisted token-headroom savings and retrieval evidence for CLI output.",
            &["forge", "harness", "headroom-stats", "--output", "json"],
            false,
            "low",
        ),
        slash(
            "/harness adoption-plan",
            "Harness Adoption Plan",
            "Inspect the governed Forge-first adoption plan before writing project policy or shims.",
            &[
                "forge",
                "harness",
                "adoption-plan",
                "--executor",
                "<executor>",
                "--shim-dir",
                "<dir>",
                "--project-root",
                "<project-root>",
                "--output",
                "json",
            ],
            false,
            "low",
        ),
        slash(
            "/harness bootstrap",
            "Harness Bootstrap",
            "Apply the reviewed Forge-first harness policy and Forge-owned CLI shims after operator approval.",
            &[
                "forge",
                "harness",
                "bootstrap",
                "--executor",
                "<executor>",
                "--shim-dir",
                "<dir>",
                "--project-root",
                "<project-root>",
                "--apply",
                "--approved-by",
                "<operator>",
                "--output",
                "json",
            ],
            true,
            "medium",
        ),
        slash(
            "/runtimes",
            "Runtimes",
            "List runtime policy.",
            &["forge", "runtimes"],
            false,
            "low",
        ),
        slash(
            "/validate",
            "Validate",
            "Run validation gate projection.",
            &["forge", "validate", "--workflow", "<workflow-id>"],
            false,
            "medium",
        ),
        slash(
            "/approve",
            "Approve",
            "Approve a pending human gate.",
            &[
                "forge",
                "workflow",
                "update-goal",
                "--workflow",
                "<workflow-id>",
            ],
            true,
            "high",
        ),
        slash(
            "/reject",
            "Reject",
            "Reject or return a gate to work.",
            &[
                "forge",
                "workflow",
                "update-goal",
                "--workflow",
                "<workflow-id>",
            ],
            true,
            "high",
        ),
        slash(
            "/goal",
            "Goal",
            "Mutate a workflow goal with revision trace.",
            &[
                "forge",
                "workflow",
                "update-goal",
                "--workflow",
                "<workflow-id>",
            ],
            true,
            "medium",
        ),
        slash(
            "/attach",
            "Attach",
            "Attach an artifact to a workflow.",
            &[
                "forge",
                "workflow",
                "attach-artifact",
                "--workflow",
                "<workflow-id>",
            ],
            true,
            "medium",
        ),
        slash(
            "/resume",
            "Resume",
            "Resume an async run.",
            &["forge", "request", "resume", "--run", "<run-id>"],
            true,
            "medium",
        ),
        slash(
            "/pause",
            "Pause",
            "Pause a loop node.",
            &[
                "forge",
                "schedule",
                "pause",
                "--workflow",
                "<workflow-id>",
                "--task",
                "<task-id>",
            ],
            true,
            "medium",
        ),
        slash(
            "/stop",
            "Stop",
            "Stop a loop node or run.",
            &[
                "forge",
                "schedule",
                "stop",
                "--workflow",
                "<workflow-id>",
                "--task",
                "<task-id>",
            ],
            true,
            "high",
        ),
        slash(
            "/delete",
            "Delete",
            "Request deletion under retention policy.",
            &[
                "forge",
                "interactive",
                "route",
                "--input",
                "delete workflow",
            ],
            true,
            "high",
        ),
        slash(
            "/export",
            "Export",
            "Export workflow state or artifacts.",
            &["forge", "artifacts", "--workflow", "<workflow-id>"],
            false,
            "low",
        ),
        slash(
            "/logs",
            "Logs",
            "Inspect run and validation logs.",
            &["forge", "request", "status", "--run", "<run-id>"],
            false,
            "low",
        ),
        slash(
            "/manifest",
            "Manifest",
            "Show Forge 0.5 milestone manifest with promotion decision.",
            &[
                "forge",
                "milestone",
                "manifest",
                "--version",
                "0.5",
                "--output",
                "json",
            ],
            false,
            "low",
        ),
        slash(
            "/milestone",
            "Milestone",
            "Show Forge 0.5 milestone status and boundary gates.",
            &[
                "forge",
                "milestone",
                "status",
                "--version",
                "0.5",
                "--output",
                "json",
            ],
            false,
            "low",
        ),
        slash(
            "/research",
            "Research",
            "Show Forge 0.5 milestone research artifact summary.",
            &[
                "forge",
                "milestone",
                "research",
                "--version",
                "0.5",
                "--output",
                "json",
            ],
            false,
            "low",
        ),
        slash(
            "/update",
            "Update",
            "Update/sync Forge surfaces.",
            &["forge", "sync", "all"],
            true,
            "medium",
        ),
        slash(
            "/workers",
            "Workers",
            "Show scheduler worker status.",
            &["forge", "schedule", "worker-status"],
            false,
            "low",
        ),
        slash(
            "/operating-context",
            "Operating Context",
            "Show tenant identity, memory policy, personality routing and prompt-packet gates before executor handoff.",
            &["forge", "interactive", "operating-context"],
            false,
            "low",
        ),
        slash(
            "/context-memory",
            "Context/Memory",
            "Show context readiness, routing quality and project memory governance without building a task packet.",
            &["forge", "interactive", "context-memory"],
            false,
            "low",
        ),
        slash(
            "/context",
            "Context",
            "Build a bounded, versioned task context package before executor handoff. Use: /context --workflow <id> --task <id> --budget 1200 --strict",
            &[
                "forge",
                "context",
                "--workflow",
                "<workflow-id>",
                "--task",
                "<task-id>",
                "--strict",
            ],
            false,
            "low",
        ),
        slash(
            "/handoff",
            "Handoff",
            "Acquire a task lease and prepare an executor handoff packet after explicit approval. Use: /handoff --workflow <id> --task <id> --executor codex",
            &[
                "forge",
                "task",
                "handoff",
                "--workflow",
                "<workflow-id>",
                "--task",
                "<task-id>",
                "--executor",
                "<executor>",
            ],
            true,
            "medium",
        ),
        slash(
            "/patch",
            "Patch",
            "File editing workflow: /patch plan --workflow <id> --task <id> --intent \"...\" --path <path>. Subcommands: plan, diff, review, apply, revert, restore.",
            &["forge", "patch", "plan", "--workflow", "<workflow-id>"],
            true,
            "high",
        ),
        slash(
            "/patch plan",
            "Patch Plan",
            "Plan a bounded file edit with permission gates, diff review and file snapshots. Use: /patch plan --workflow <id> --task <id> --intent \"...\" --path <path>",
            &["forge", "patch", "plan", "--workflow", "<workflow-id>", "--task", "<task-id>", "--intent", "...", "--path", "<path>"],
            false,
            "medium",
        ),
        slash(
            "/patch diff",
            "Patch Diff",
            "Navigate current multi-file diffs without editing files. Use: /patch diff --workflow <id> --task <id> --path <path> --file-index 0 --hunk-index 0",
            &["forge", "patch", "diff", "--workflow", "<workflow-id>", "--task", "<task-id>", "--path", "<path>"],
            false,
            "medium",
        ),
        slash(
            "/patch apply",
            "Patch Apply",
            "Apply a planned patch after diff review and human approval. Use: /patch apply --workflow <id> --task <id> --path <path>",
            &["forge", "patch", "apply", "--workflow", "<workflow-id>", "--task", "<task-id>", "--path", "<path>"],
            true,
            "high",
        ),
        slash(
            "/patch review",
            "Patch Review",
            "Review current file diffs for a bounded patch without editing files. Use: /patch review --workflow <id> --task <id> --path <path>",
            &["forge", "patch", "review", "--workflow", "<workflow-id>", "--task", "<task-id>", "--path", "<path>"],
            false,
            "medium",
        ),
        slash(
            "/patch revert",
            "Patch Revert",
            "Record a guarded revert proposal without silently restoring files. Use: /patch revert --workflow <id> --task <id> --apply-artifact <id>",
            &["forge", "patch", "revert", "--workflow", "<workflow-id>", "--task", "<task-id>", "--apply-artifact", "<artifact-id>"],
            true,
            "high",
        ),
        slash(
            "/patch restore",
            "Patch Restore",
            "Execute an explicitly approved file restore from a revert artifact. Use: /patch restore --workflow <id> --task <id> --revert-artifact <id> --approved-by <operator> --confirm-restore",
            &["forge", "patch", "restore", "--workflow", "<workflow-id>", "--task", "<task-id>", "--revert-artifact", "<artifact-id>", "--approved-by", "<operator>", "--confirm-restore"],
            true,
            "high",
        ),
        slash(
            "/pm",
            "PM Mode",
            "Start a human-guided product management session to clarify goals, risks and MVP boundaries.",
            &["forge", "interactive", "route", "--input", "start pm session"],
            true,
            "medium",
        ),
        slash(
            "/decision",
            "Product Decision",
            "Record a durable product decision with rationale and impact trace. Use: /decision --workflow <id> --title \"...\" --rationale \"...\" [--alternative \"...\"] [--trade-off \"...\"] [--success-metric \"...\"] [--backlog-mutation \"...\"]",
            &["forge", "workflow", "decision", "--workflow", "<workflow-id>", "--title", "...", "--rationale", "..."],
            true,
            "medium",
        ),
        slash(
            "/exit",
            "Exit",
            "Exit the interactive REPL.",
            &[],
            false,
            "low",
        ),
        slash(
            "/quit",
            "Quit",
            "Exit the interactive REPL.",
            &[],
            false,
            "low",
        ),
    ]
}

fn slash(
    name: &str,
    title: &str,
    description: &str,
    equivalent_command: &[&str],
    mutates_workflow: bool,
    risk_level: &str,
) -> SlashCommandSpec {
    SlashCommandSpec {
        name: name.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        equivalent_command: equivalent_command
            .iter()
            .map(|item| (*item).to_string())
            .collect(),
        scriptable: true,
        mutates_workflow,
        risk_level: risk_level.to_string(),
    }
}

fn anvil_mark() -> &'static str {
    "    ▄███████████████▄\n  ▄██▓▓▓▓▓▓▓▓▓▓▓▓▓▓██▄\n ▄█▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓█▄\n ██▓▓▓▓▓▓▓   ████   ▓▓▓▓▓▓▓██\n ██▓▓▓▓▓▓▓▓████████▓▓▓▓▓▓▓▓██\n ▀█▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓█▀\n  ▀██▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓██▀\n    ▀████████████████████▀\n      ██  ████████  ██\n      ██    ████    ██\n      ██    ████    ██"
}

fn render_interactive_tui_frame(
    report: &InteractiveHomeReport,
    state: &InteractiveReplState,
) -> String {
    let width = tui_terminal_width();
    let dashboard = &report.dashboard;
    let guided = &dashboard.guided_cockpit_panel;
    let schedule = &dashboard.schedule_panel;
    let cost = &dashboard.cost_panel;
    let addon = &dashboard.addon_capability_panel;
    let improvement = &dashboard.improvement_loop_panel;
    let mutation = &dashboard.workflow_mutation_panel;
    let sidebar = &dashboard.workflow_sidebar_panel;
    let event_runtime = &dashboard.event_runtime_panel;
    let title = format!(
        "Forge advanced operational TUI | Forge operational TUI | {} | {}/{} steps | current {}",
        guided.status, guided.completed_step_count, guided.total_step_count, guided.current_step_id
    );
    let focus_line = state.focus_status_line();
    let key_line = "j/k focus | enter open | r refresh | m mode | t theme | /help | q quit";
    let compatibility_lines = vec![
        format!(
            "Guided cockpit: {}; visual {}; steps {}/{}; current {}; next {}",
            guided.status,
            guided.visual_mode,
            guided.completed_step_count,
            guided.total_step_count,
            guided.current_step_id,
            guided.next_command
        ),
        format!(
            "Guided steps: create_workflow to close_outcome; total {}; blocked {}; confirmations {}",
            guided.total_step_count, guided.blocked_step_count, guided.confirmation_step_count
        ),
        "Safe actions: read-only panels open directly; mutating actions require preview, confirmation and visible rollback.".to_string(),
        format!(
            "Active workflows: {}; Active runs: {}; focus {}",
            sidebar.workflow_count, dashboard.active_runs, dashboard.workflow_focus.len()
        ),
        format!(
            "Events/schedules: events {}, scheduled {}, due {}; Addons/capabilities: addons {}, caps {}",
            dashboard.event_panel.total_event_count,
            dashboard.scheduled_workflows,
            schedule.due_workflows,
            addon.enabled_addon_count,
            addon.capability_count
        ),
        format!(
            "Costs: estimated ${:.4}, observed ${:.4}; Handoffs/approvals: ready {}, pending {}",
            cost.estimated_task_cost_total_usd,
            cost.observed_event_cost_total_usd,
            dashboard.task_board_panel.ready_handoffs,
            dashboard.pending_approvals
        ),
        format!(
            "Operational cockpit: {}; {}; active work {}, ready handoffs {}, human waits {}",
            dashboard.operational_cockpit_panel.attention_level,
            dashboard.operational_cockpit_panel.priority_summary,
            dashboard.operational_cockpit_panel.active_work_count,
            dashboard.operational_cockpit_panel.ready_handoff_count,
            dashboard.operational_cockpit_panel.pending_human_wait_count
        ),
        format!(
            "Task board: workflows {}, tasks {}, ready handoffs {}, human waits {}",
            dashboard.task_board_panel.workflow_count,
            dashboard.task_board_panel.task_count,
            dashboard.task_board_panel.ready_handoffs,
            dashboard.task_board_panel.pending_human_interactions
        ),
        format!(
            "Architecture compass: {}; tracks {}, docs {}, conflicts {}",
            dashboard.architecture_compass_panel.status,
            dashboard.architecture_compass_panel.tracks.len(),
            dashboard.architecture_compass_panel.source_documents.len(),
            dashboard.architecture_compass_panel.conflicts.len()
        ),
        format!(
            "Architecture execution plan: {}; increments {}, next {}",
            dashboard
                .architecture_compass_panel
                .execution_plan
                .status,
            dashboard
                .architecture_compass_panel
                .execution_plan
                .increments
                .len(),
            dashboard
                .architecture_compass_panel
                .execution_plan
                .next_command
        ),
        format!(
            "Useful next commands: {}",
            tui_join_limited(&dashboard.useful_next_commands, 3, "none")
        ),
        "Smoke test: forge smoke operational-tui --output json | Quick actions: /status /cockpit /task-board".to_string(),
    ];

    let workflows = vec![
        format!(
            "workflows {} | active {} | attention {}",
            sidebar.workflow_count, sidebar.active_count, sidebar.attention_count
        ),
        format!(
            "runs {} | selected {}",
            dashboard.active_runs,
            empty_as(sidebar.selected_workflow_id.as_str(), "none")
        ),
        format!(
            "groups {} | event {} | scheduled {} | done {}",
            sidebar.group_count,
            sidebar.event_driven_count,
            sidebar.scheduled_count,
            sidebar.completed_count
        ),
        format!(
            "release {} | replacement {}%",
            dashboard.release_gates_panel.status, dashboard.replacement_cli_panel.readiness_percent
        ),
        format!("core boundary {}", dashboard.core_boundary_panel.status),
        format!(
            "shells {} | brain {}",
            dashboard.shell_entrypoints.join(", "),
            dashboard.brain_router
        ),
    ];
    let execution = vec![
        format!(
            "guided {} | blocked {} | confirms {}",
            guided.visual_mode, guided.blocked_step_count, guided.confirmation_step_count
        ),
        format!(
            "tasks {} | ready handoffs {} | human waits {}",
            dashboard.task_board_panel.task_count,
            dashboard.task_board_panel.ready_handoffs,
            dashboard.task_board_panel.pending_human_interactions
        ),
        format!(
            "approvals {} | validation failures {}",
            dashboard.pending_approvals, dashboard.validation_failures
        ),
        format!(
            "mutation proposals {}/{} | mutable {}",
            mutation.pending_modifier_proposal_count,
            mutation.applied_modifier_proposal_count,
            mutation.mutable_workflow_count
        ),
        format!("next {}", guided.next_command),
        "open focused pane with enter".to_string(),
    ];
    let realtime = vec![
        format!(
            "events visible {}/{} | runtime pending {}",
            dashboard.event_panel.visible_event_count,
            dashboard.event_panel.total_event_count,
            event_runtime.pending_event_count
        ),
        format!(
            "schedules due {} | runnable {} | next {}",
            schedule.due_workflows,
            schedule.runnable_due_workflows,
            schedule.next_wakeup_at.as_deref().unwrap_or("none")
        ),
        format!(
            "cost est ${:.4} | observed ${:.4} | ai {} | deterministic {}",
            cost.estimated_task_cost_total_usd,
            cost.observed_event_cost_total_usd,
            cost.ai_node_count,
            cost.deterministic_node_count
        ),
        format!(
            "addons {} enabled | caps {} | queued {}",
            addon.enabled_addon_count, addon.capability_count, addon.queued_dispatch_count
        ),
        format!(
            "improve {} candidates | high {} | parallel {}",
            improvement.candidate_count,
            improvement.high_candidate_count,
            improvement.parallel_ready_candidate_count
        ),
        format!("logs {}", dashboard.structured_logs_panel.log_count),
    ];
    let actions = vec![
        format!("focus {}", state.focused_panel().title),
        "slash /cockpit /task-board /workflow-mutation /schedules /addons /costs".to_string(),
        format!(
            "quick {}",
            tui_join_limited(&dashboard.quick_actions, 3, "none")
        ),
        format!(
            "attention {}",
            tui_join_limited(&dashboard.attention_actions, 2, "none")
        ),
        format!(
            "commands {}",
            tui_join_limited(&dashboard.useful_next_commands, 2, "none")
        ),
    ];

    let mut lines = vec![
        "\x1b[2J\x1b[H".to_string(),
        tui_full_line(&title, width),
        tui_full_line(&focus_line, width),
        tui_full_line(key_line, width),
        tui_full_line(
            "Type an objective to route it, or use slash commands for exact panels.",
            width,
        ),
    ];
    for line in compatibility_lines {
        lines.push(tui_full_line(&line, width));
    }

    if width >= 110 {
        let gap = " ";
        let column_width = ((width - 2) / 3).max(32);
        let boxes = [
            tui_box("Workflows", &workflows, column_width, 8),
            tui_box("Execution", &execution, column_width, 8),
            tui_box("Realtime", &realtime, column_width, 8),
        ];
        for ((left, center), right) in boxes[0].iter().zip(&boxes[1]).zip(&boxes[2]) {
            lines.push(format!("{}{}{}{}{}", left, gap, center, gap, right));
        }
    } else {
        lines.extend(tui_box("Workflows", &workflows, width, 8));
        lines.extend(tui_box("Execution", &execution, width, 8));
        lines.extend(tui_box("Realtime", &realtime, width, 8));
    }

    lines.extend(tui_box("Safe Actions", &actions, width, 7));
    lines.join("\n")
}

fn tui_terminal_width() -> usize {
    env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .map(|width| width.clamp(80, 160))
        .unwrap_or(120)
}

fn tui_full_line(text: &str, width: usize) -> String {
    format!("| {} |", tui_pad(text, width.saturating_sub(4)))
}

fn tui_box(title: &str, lines: &[String], width: usize, height: usize) -> Vec<String> {
    let width = width.max(24);
    let inner = width.saturating_sub(4);
    let mut rendered = Vec::with_capacity(height + 2);
    rendered.push(tui_border(title, width));
    for line in lines.iter().take(height) {
        rendered.push(format!("| {} |", tui_pad(line, inner)));
    }
    for _ in lines.len()..height {
        rendered.push(format!("| {} |", " ".repeat(inner)));
    }
    rendered.push(format!("+{}+", "-".repeat(width.saturating_sub(2))));
    rendered
}

fn tui_border(title: &str, width: usize) -> String {
    let label = format!(" {title} ");
    let fill = width.saturating_sub(label.len() + 2);
    format!("+{}{}+", label, "-".repeat(fill))
}

fn tui_pad(text: &str, width: usize) -> String {
    let clipped = tui_clip(text, width);
    format!("{clipped:<width$}")
}

fn tui_clip(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    if width <= 3 {
        return text.chars().take(width).collect();
    }
    let mut clipped = text.chars().take(width - 3).collect::<String>();
    clipped.push_str("...");
    clipped
}

fn tui_join_limited(items: &[String], limit: usize, fallback: &str) -> String {
    if items.is_empty() {
        return fallback.to_string();
    }
    let mut selected = items.iter().take(limit).cloned().collect::<Vec<_>>();
    if items.len() > limit {
        selected.push(format!("+{} more", items.len() - limit));
    }
    selected.join(" | ")
}

fn empty_as<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.trim().is_empty() {
        fallback
    } else {
        value
    }
}

pub fn run_interactive_repl(store_path: &std::path::Path) -> Result<i32> {
    if !std::io::stdin().is_terminal() {
        let store = ForgeStore::open(store_path)?;
        let report = build_interactive_home(&store)?;
        let repl_state = InteractiveReplState::from_home(&report);
        println!("{}", render_interactive_tui_frame(&report, &repl_state));
        return Ok(0);
    }

    let store = ForgeStore::open(store_path)?;
    let report = build_interactive_home(&store)?;
    let mut repl_state = InteractiveReplState::from_home(&report);
    println!("{}", render_interactive_tui_frame(&report, &repl_state));

    loop {
        print!("forge> ");
        std::io::Write::flush(&mut std::io::stdout())?;

        let mut line = String::new();
        let bytes = std::io::stdin().read_line(&mut line)?;
        if bytes == 0 {
            println!();
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if is_repl_exit_command(trimmed) {
            println!("goodbye");
            break;
        }

        if let Some(output) = dispatch_repl_navigation_key(&store, &mut repl_state, trimmed)? {
            println!("{output}");
            continue;
        }

        if trimmed.starts_with('/') {
            let result = route_slash_command(trimmed);
            let route = result.slash_command.unwrap_or(SlashCommandRoute {
                name: trimmed.to_string(),
                recognized: false,
                input_arguments: Vec::new(),
                input_argument_text: String::new(),
                equivalent_command: Vec::new(),
                mutates_workflow: false,
                risk_level: "unknown".to_string(),
                execution_boundary: "catalog_lookup_not_executed".to_string(),
            });

            if trimmed.starts_with("/patch ") {
                dispatch_patch_command(&store, trimmed, store_path)?;
                continue;
            }
            if trimmed == "/context" || trimmed.starts_with("/context ") {
                dispatch_context_command(trimmed, store_path)?;
                continue;
            }
            if trimmed == "/handoff" || trimmed.starts_with("/handoff ") {
                dispatch_handoff_command(trimmed, store_path)?;
                continue;
            }
            if trimmed.starts_with("/pm ") {
                dispatch_pm_command(&store, trimmed)?;
                continue;
            }
            if trimmed.starts_with("/decision ") {
                dispatch_decision_command(&store, trimmed, store_path)?;
                continue;
            }
            if trimmed == "/actions" || trimmed.starts_with("/actions ") {
                dispatch_actions_command(&store, trimmed)?;
                continue;
            }
            if trimmed == "/action" || trimmed.starts_with("/action ") {
                dispatch_action_command(&store, trimmed)?;
                continue;
            }
            if dispatch_read_only_panel_command(&store, trimmed)? {
                continue;
            }

            if route.recognized {
                println!(
                    "  {name}: {explanation}",
                    name = route.name,
                    explanation = result.routing_explanation
                );
                if !route.equivalent_command.is_empty() {
                    println!("  Equivalent: {}", route.equivalent_command.join(" "));
                }
            } else {
                println!(
                    "  Unknown command: {name}. Type /help for available commands.",
                    name = route.name
                );
            }
            continue;
        }

        let route_result = route_interactive_input(&store, trimmed, "forge_repl")?;
        println!(
            "  Routing: {decision}",
            decision = route_result.routing_decision
        );
        if let Some(answer) = &route_result.answer {
            println!("  {answer}");
        }
        if route_result.workflow_created {
            if let Some(run_id) = &route_result.run_id {
                println!("  Run ID: {run_id}");
            }
            if let Some(wf_id) = &route_result.workflow_id {
                println!("  Workflow ID: {wf_id}");
            }
            println!(
                "  Retention: {action}",
                action = route_result.retention_decision.action
            );
        }
    }

    Ok(0)
}

#[derive(Debug, Clone)]
struct InteractiveReplFocusPanel {
    panel_id: &'static str,
    title: &'static str,
}

#[derive(Debug, Clone)]
struct InteractiveReplState {
    panels: Vec<InteractiveReplFocusPanel>,
    focused_index: usize,
    display_modes: Vec<String>,
    display_mode_index: usize,
    themes: Vec<String>,
    theme_index: usize,
}

impl InteractiveReplState {
    fn from_home(report: &InteractiveHomeReport) -> Self {
        let navigation = &report.dashboard.navigation_panel;
        let display_modes = if navigation.display_modes.is_empty() {
            vec![
                "compact".to_string(),
                "detailed".to_string(),
                "focus".to_string(),
            ]
        } else {
            navigation.display_modes.clone()
        };
        let display_mode_index = display_modes
            .iter()
            .position(|mode| mode == &navigation.default_display_mode)
            .unwrap_or(0);
        let themes = if navigation.themes.is_empty() {
            vec![
                "forge_dark".to_string(),
                "forge_light".to_string(),
                "high_contrast".to_string(),
            ]
        } else {
            navigation.themes.clone()
        };
        let theme_index = themes
            .iter()
            .position(|theme| theme == &navigation.active_theme)
            .unwrap_or(0);

        Self {
            panels: repl_focus_panels(),
            focused_index: 0,
            display_modes,
            display_mode_index,
            themes,
            theme_index,
        }
    }

    fn focused_panel(&self) -> &InteractiveReplFocusPanel {
        &self.panels[self.focused_index]
    }

    fn focus_next(&mut self) {
        self.focused_index = (self.focused_index + 1) % self.panels.len();
    }

    fn focus_previous(&mut self) {
        self.focused_index = if self.focused_index == 0 {
            self.panels.len() - 1
        } else {
            self.focused_index - 1
        };
    }

    fn cycle_display_mode(&mut self) -> String {
        self.display_mode_index = (self.display_mode_index + 1) % self.display_modes.len();
        format!(
            "Display mode: {}",
            self.display_modes[self.display_mode_index]
        )
    }

    fn cycle_theme(&mut self) -> String {
        self.theme_index = (self.theme_index + 1) % self.themes.len();
        format!("Theme: {}", self.themes[self.theme_index])
    }

    fn focus_status_line(&self) -> String {
        let panel = self.focused_panel();
        format!(
            "Focus: {} ({}) [{}/{}]; mode {}; theme {}",
            panel.panel_id,
            panel.title,
            self.focused_index + 1,
            self.panels.len(),
            self.display_modes[self.display_mode_index],
            self.themes[self.theme_index],
        )
    }
}

fn repl_focus_panels() -> Vec<InteractiveReplFocusPanel> {
    vec![
        InteractiveReplFocusPanel {
            panel_id: "guided_cockpit_panel",
            title: "Guided cockpit",
        },
        InteractiveReplFocusPanel {
            panel_id: "operational_cockpit_panel",
            title: "Operational cockpit",
        },
        InteractiveReplFocusPanel {
            panel_id: "task_board_panel",
            title: "Task board",
        },
        InteractiveReplFocusPanel {
            panel_id: "workflow_mutation_panel",
            title: "Workflow mutation",
        },
        InteractiveReplFocusPanel {
            panel_id: "architecture_compass_panel",
            title: "Architecture compass",
        },
        InteractiveReplFocusPanel {
            panel_id: "core_boundary_panel",
            title: "Core boundary",
        },
        InteractiveReplFocusPanel {
            panel_id: "artifact_panel",
            title: "Artifacts",
        },
        InteractiveReplFocusPanel {
            panel_id: "readiness_panel",
            title: "Readiness",
        },
        InteractiveReplFocusPanel {
            panel_id: "addon_capability_panel",
            title: "Addons/capabilities",
        },
        InteractiveReplFocusPanel {
            panel_id: "sessions_panel",
            title: "Sessions",
        },
        InteractiveReplFocusPanel {
            panel_id: "harness_panel",
            title: "Harness",
        },
        InteractiveReplFocusPanel {
            panel_id: "token_usage_panel",
            title: "Token usage",
        },
        InteractiveReplFocusPanel {
            panel_id: "structured_logs_panel",
            title: "Structured logs",
        },
        InteractiveReplFocusPanel {
            panel_id: "improvement_loop_panel",
            title: "Improvement loop",
        },
        InteractiveReplFocusPanel {
            panel_id: "schedule_panel",
            title: "Schedules",
        },
        InteractiveReplFocusPanel {
            panel_id: "context_memory_panel",
            title: "Context/memory",
        },
        InteractiveReplFocusPanel {
            panel_id: "operating_context_panel",
            title: "Operating context",
        },
        InteractiveReplFocusPanel {
            panel_id: "permissions_panel",
            title: "Permissions",
        },
        InteractiveReplFocusPanel {
            panel_id: "identity_panel",
            title: "Identity",
        },
        InteractiveReplFocusPanel {
            panel_id: "workflow_dag_panel",
            title: "Workflow DAG",
        },
        InteractiveReplFocusPanel {
            panel_id: "patch_workbench_panel",
            title: "Patch workbench",
        },
    ]
}

fn dispatch_repl_navigation_key(
    store: &ForgeStore,
    state: &mut InteractiveReplState,
    input: &str,
) -> Result<Option<String>> {
    match input {
        "j" => {
            state.focus_next();
            Ok(Some(render_interactive_tui_frame(
                &build_interactive_home(store)?,
                state,
            )))
        }
        "k" => {
            state.focus_previous();
            Ok(Some(render_interactive_tui_frame(
                &build_interactive_home(store)?,
                state,
            )))
        }
        "m" => {
            let message = state.cycle_display_mode();
            let frame = render_interactive_tui_frame(&build_interactive_home(store)?, state);
            Ok(Some(format!("{message}\n{frame}")))
        }
        "t" => {
            let message = state.cycle_theme();
            let frame = render_interactive_tui_frame(&build_interactive_home(store)?, state);
            Ok(Some(format!("{message}\n{frame}")))
        }
        "r" => Ok(Some(render_interactive_tui_frame(
            &build_interactive_home(store)?,
            state,
        ))),
        "enter" => {
            let panel_id = state.focused_panel().panel_id;
            let rendered = render_repl_focused_panel(store, panel_id)?;
            Ok(Some(format!(
                "Opened focused panel: {panel_id}\n{rendered}"
            )))
        }
        _ => Ok(None),
    }
}

fn render_repl_focused_panel(store: &ForgeStore, panel_id: &str) -> Result<String> {
    match panel_id {
        "guided_cockpit_panel" => {
            let panel = build_interactive_guided_cockpit(store)?;
            Ok(render_interactive_guided_cockpit(&panel))
        }
        "operational_cockpit_panel" => {
            let panel = build_interactive_operational_cockpit(store)?;
            Ok(render_interactive_operational_cockpit(&panel))
        }
        "architecture_compass_panel" => {
            let panel = build_interactive_architecture_compass(store, None)?;
            Ok(render_interactive_architecture_compass(&panel))
        }
        "core_boundary_panel" => {
            let panel = build_interactive_core_boundary(store);
            Ok(render_interactive_core_boundary(&panel))
        }
        "task_board_panel" => {
            let panel = build_interactive_task_board(store)?;
            Ok(render_interactive_task_board(&panel))
        }
        "workflow_mutation_panel" => {
            let panel = build_interactive_workflow_mutation(store)?;
            Ok(render_interactive_workflow_mutation(&panel))
        }
        "artifact_panel" => {
            let panel = build_interactive_artifacts(store)?;
            Ok(render_interactive_artifacts(&panel))
        }
        "readiness_panel" => {
            let panel = build_interactive_readiness(store)?;
            Ok(render_interactive_readiness(&panel))
        }
        "addon_capability_panel" => {
            let panel = build_interactive_addon_capabilities_default(store);
            Ok(render_interactive_addon_capabilities(&panel))
        }
        "ui_composition_panel" => {
            let panel = build_interactive_ui_composition(store, None)?;
            Ok(render_interactive_ui_composition(&panel))
        }
        "sessions_panel" => {
            let panel = build_interactive_sessions(store, InteractiveSessionsOptions::default())?;
            Ok(render_interactive_sessions(&panel))
        }
        "harness_panel" => {
            let panel = build_interactive_harness(
                store,
                InteractiveHarnessOptions::default_for_current_dir(),
            )?;
            Ok(render_interactive_harness(&panel))
        }
        "token_usage_panel" => {
            let panel = build_interactive_token_usage(store)?;
            Ok(render_interactive_token_usage(&panel))
        }
        "structured_logs_panel" => {
            let panel = build_interactive_structured_logs(store)?;
            Ok(render_interactive_structured_logs(&panel))
        }
        "improvement_loop_panel" => {
            let panel = build_interactive_improvement_loop(store)?;
            Ok(render_interactive_improvement_loop(&panel))
        }
        "schedule_panel" => {
            let panel = build_interactive_schedules(store);
            Ok(render_interactive_schedules(&panel))
        }
        "context_memory_panel" => {
            let project_root = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let panel = build_interactive_context_memory(store, &project_root)?;
            Ok(render_interactive_context_memory(&panel))
        }
        "operating_context_panel" => {
            let project_root = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let panel = build_interactive_operating_context(store, &project_root)?;
            Ok(render_interactive_operating_context(&panel))
        }
        "permissions_panel" => {
            let panel = build_interactive_permissions(store)?;
            Ok(render_interactive_permissions(&panel))
        }
        "identity_panel" => {
            let panel = build_interactive_identity(store, std::path::Path::new("."))?;
            Ok(render_interactive_identity(&panel))
        }
        "workflow_dag_panel" => {
            let panel = build_interactive_workflow_dag(store)?;
            Ok(render_interactive_workflow_dag(&panel))
        }
        "workflow_sidebar_panel" => {
            let panel = build_interactive_workflow_sidebar(store)?;
            Ok(render_interactive_workflow_sidebar(&panel))
        }
        "replacement_cli_panel" => {
            let panel = build_interactive_replacement_cli(store)?;
            Ok(render_interactive_replacement_cli(&panel))
        }
        "multimodal_runtime_panel" => {
            let project_root = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let panel = build_interactive_multimodal_runtime(store, &project_root, false)?;
            Ok(render_interactive_multimodal_runtime(&panel))
        }
        "patch_workbench_panel" => {
            let panel = build_interactive_patch_workbench(store)?;
            Ok(render_interactive_patch_workbench(&panel))
        }
        _ => Ok(format!("Focused panel {panel_id} has no renderer")),
    }
}

fn dispatch_read_only_panel_command(store: &ForgeStore, input: &str) -> Result<bool> {
    match input.trim() {
        "/guided-cockpit" | "/guide" => {
            let panel = build_interactive_guided_cockpit(store)?;
            println!("{}", render_interactive_guided_cockpit(&panel));
            Ok(true)
        }
        "/cockpit" => {
            let panel = build_interactive_operational_cockpit(store)?;
            println!("{}", render_interactive_operational_cockpit(&panel));
            Ok(true)
        }
        "/architecture" | "/compass" => {
            let panel = build_interactive_architecture_compass(store, None)?;
            println!("{}", render_interactive_architecture_compass(&panel));
            Ok(true)
        }
        "/core-boundary" | "/boundary" => {
            let panel = build_interactive_core_boundary(store);
            println!("{}", render_interactive_core_boundary(&panel));
            Ok(true)
        }
        "/task-board" => {
            let panel = build_interactive_task_board(store)?;
            println!("{}", render_interactive_task_board(&panel));
            Ok(true)
        }
        "/workflow-mutation" | "/replan" | "/replanning" => {
            let panel = build_interactive_workflow_mutation(store)?;
            println!("{}", render_interactive_workflow_mutation(&panel));
            Ok(true)
        }
        "/workflows" | "/workflow-sidebar" | "/sidebar" => {
            let panel = build_interactive_workflow_sidebar(store)?;
            println!("{}", render_interactive_workflow_sidebar(&panel));
            Ok(true)
        }
        "/replacement-cli" => {
            let panel = build_interactive_replacement_cli(store)?;
            println!("{}", render_interactive_replacement_cli(&panel));
            Ok(true)
        }
        "/multimodal-runtime" => {
            let project_root = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let panel = build_interactive_multimodal_runtime(store, &project_root, false)?;
            println!("{}", render_interactive_multimodal_runtime(&panel));
            Ok(true)
        }
        "/artifacts" => {
            let panel = build_interactive_artifacts(store)?;
            println!("{}", render_interactive_artifacts(&panel));
            Ok(true)
        }
        "/readiness" => {
            let panel = build_interactive_readiness(store)?;
            println!("{}", render_interactive_readiness(&panel));
            Ok(true)
        }
        "/addons" => {
            let panel = build_interactive_addon_capabilities_default(store);
            println!("{}", render_interactive_addon_capabilities(&panel));
            Ok(true)
        }
        "/ui-composition" | "/ui" => {
            let panel = build_interactive_ui_composition(store, None)?;
            println!("{}", render_interactive_ui_composition(&panel));
            Ok(true)
        }
        "/sessions" => {
            let panel = build_interactive_sessions(store, InteractiveSessionsOptions::default())?;
            println!("{}", render_interactive_sessions(&panel));
            Ok(true)
        }
        "/harness" => {
            let panel = build_interactive_harness(
                store,
                InteractiveHarnessOptions::default_for_current_dir(),
            )?;
            println!("{}", render_interactive_harness(&panel));
            Ok(true)
        }
        "/tokens" | "/token-usage" => {
            let panel = build_interactive_token_usage(store)?;
            println!("{}", render_interactive_token_usage(&panel));
            Ok(true)
        }
        "/logs" => {
            let panel = build_interactive_structured_logs(store)?;
            println!("{}", render_interactive_structured_logs(&panel));
            Ok(true)
        }
        "/improvement-loop" | "/improve" => {
            let panel = build_interactive_improvement_loop(store)?;
            println!("{}", render_interactive_improvement_loop(&panel));
            Ok(true)
        }
        "/schedules" => {
            let panel = build_interactive_schedules(store);
            println!("{}", render_interactive_schedules(&panel));
            Ok(true)
        }
        "/context-memory" => {
            let project_root = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let panel = build_interactive_context_memory(store, &project_root)?;
            println!("{}", render_interactive_context_memory(&panel));
            Ok(true)
        }
        "/operating-context" => {
            let project_root = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let panel = build_interactive_operating_context(store, &project_root)?;
            println!("{}", render_interactive_operating_context(&panel));
            Ok(true)
        }
        "/permissions" => {
            let panel = build_interactive_permissions(store)?;
            println!("{}", render_interactive_permissions(&panel));
            Ok(true)
        }
        "/identity" => {
            let panel = build_interactive_identity(store, std::path::Path::new("."))?;
            println!("{}", render_interactive_identity(&panel));
            Ok(true)
        }
        "/workflow-dag" | "/dag" => {
            let panel = build_interactive_workflow_dag(store)?;
            println!("{}", render_interactive_workflow_dag(&panel));
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn dispatch_actions_command(store: &ForgeStore, input: &str) -> Result<()> {
    let rest = input.trim().strip_prefix("/actions").unwrap_or("").trim();
    let query = rest
        .strip_prefix("--query")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| (!rest.is_empty()).then_some(rest));
    let report = build_interactive_action_registry(store, query)?;
    println!("{}", render_interactive_action_registry(&report));
    Ok(())
}

fn dispatch_action_command(store: &ForgeStore, input: &str) -> Result<()> {
    let rest = input.trim().strip_prefix("/action").unwrap_or("").trim();
    let mut tokens = rest.split_whitespace();
    let action_id = match tokens.next() {
        Some("--action") => tokens.next(),
        Some(value) => Some(value),
        None => None,
    };

    let Some(action_id) = action_id else {
        println!("  Usage: /action <action-id>");
        println!(
            "  Discover actions with: /actions [query] or forge interactive action-registry --output json"
        );
        return Ok(());
    };

    let report = build_interactive_action_invocation(store, action_id)?;
    println!("{}", render_interactive_action_invocation(&report));
    Ok(())
}

fn dispatch_patch_command(
    _store: &ForgeStore,
    input: &str,
    store_path: &std::path::Path,
) -> Result<()> {
    let rest = input.trim().strip_prefix("/patch ").unwrap_or("").trim();
    let subcommand = rest.split_whitespace().next().unwrap_or("");
    let store_str = store_path.to_string_lossy();

    match subcommand {
        "plan" => {
            println!("  Patch Plan: planning a bounded file edit...");
            let plan_output = Command::new(
                std::env::args()
                    .next()
                    .unwrap_or_else(|| "forge".to_string()),
            )
            .args(["--store", &store_str, "patch", "plan"])
            .args(rest.split_whitespace().skip(1).collect::<Vec<_>>())
            .arg("--output")
            .arg("json")
            .output()?;
            if plan_output.status.success() {
                let stdout = String::from_utf8_lossy(&plan_output.stdout);
                if let Ok(plan) = serde_json::from_str::<serde_json::Value>(&stdout) {
                    println!("  Status: {}", plan["status"].as_str().unwrap_or("ok"));
                    println!(
                        "  Permission gate: {}",
                        plan["permission_gate"]["policy"]
                            .as_str()
                            .unwrap_or("check")
                    );
                    if let Some(snapshots) = plan["file_snapshots"].as_array() {
                        for snap in snapshots {
                            println!(
                                "  File: {} ({} bytes, sha256: {})",
                                snap["path"].as_str().unwrap_or("?"),
                                snap["bytes"].as_u64().unwrap_or(0),
                                snap["sha256"].as_str().unwrap_or("none")
                            );
                        }
                    }
                    println!(
                        "  Diff review required: {}",
                        plan["diff_review"]["required_before_apply"]
                    );
                    println!("  Review commands:");
                    for cmd in plan["diff_review"]["review_commands"]
                        .as_array()
                        .unwrap_or(&vec![])
                    {
                        println!("    $ {}", cmd.as_str().unwrap_or(""));
                    }
                } else {
                    println!("  Plan created. Use '/patch apply' after reviewing.");
                }
            } else {
                let stderr = String::from_utf8_lossy(&plan_output.stderr);
                println!("  Patch plan failed: {stderr}");
            }
        }
        "apply" => {
            println!("  Patch Apply: you are about to apply a file edit.");
            print!("  Approve apply? (y/N): ");
            std::io::Write::flush(&mut std::io::stdout())?;
            let mut confirm = String::new();
            std::io::stdin().read_line(&mut confirm)?;
            let confirmed = confirm.trim().eq_ignore_ascii_case("y")
                || confirm.trim().eq_ignore_ascii_case("yes");

            if !confirmed {
                println!("  Apply cancelled by user.");
                return Ok(());
            }

            let apply_output = Command::new(
                std::env::args()
                    .next()
                    .unwrap_or_else(|| "forge".to_string()),
            )
            .args(["--store", &store_str, "patch", "apply"])
            .args(rest.split_whitespace().skip(1).collect::<Vec<_>>())
            .arg("--output")
            .arg("json")
            .output()?;
            if apply_output.status.success() {
                let stdout = String::from_utf8_lossy(&apply_output.stdout);
                if let Ok(apply) = serde_json::from_str::<serde_json::Value>(&stdout) {
                    println!(
                        "  Status: {}",
                        apply["status"].as_str().unwrap_or("applied")
                    );
                    println!("  Apply recorded as artifact.");
                    if let Some(artifact) = apply["artifact"].as_object() {
                        println!(
                            "  Artifact: {} ({})",
                            artifact.get("path").and_then(|v| v.as_str()).unwrap_or("?"),
                            artifact
                                .get("sha256")
                                .and_then(|v| v.as_str())
                                .unwrap_or("?")
                        );
                    }
                } else {
                    println!("  Apply completed.");
                }
            } else {
                let stderr = String::from_utf8_lossy(&apply_output.stderr);
                println!("  Patch apply failed: {stderr}");
            }
        }
        "review" => {
            println!("  Patch Review: collecting current diff evidence...");
            let review_output = Command::new(
                std::env::args()
                    .next()
                    .unwrap_or_else(|| "forge".to_string()),
            )
            .args(["--store", &store_str, "patch", "review"])
            .args(rest.split_whitespace().skip(1).collect::<Vec<_>>())
            .arg("--output")
            .arg("json")
            .output()?;
            if review_output.status.success() {
                let stdout = String::from_utf8_lossy(&review_output.stdout);
                if let Ok(review) = serde_json::from_str::<serde_json::Value>(&stdout) {
                    println!(
                        "  Status: {}",
                        review["status"].as_str().unwrap_or("reviewed")
                    );
                    println!(
                        "  Changed paths: {}",
                        review["summary"]["changed_path_count"]
                            .as_u64()
                            .unwrap_or(0)
                    );
                    println!(
                        "  Diff check passed: {}",
                        review["summary"]["diff_check_passed"]
                            .as_bool()
                            .unwrap_or(false)
                    );
                    println!(
                        "  Recommendation: {}",
                        review["summary"]["approval_recommendation"]
                            .as_str()
                            .unwrap_or("review_required")
                    );
                    if let Some(paths) = review["path_reviews"].as_array() {
                        for path in paths {
                            println!(
                                "  File: {} changed={}",
                                path["path"].as_str().unwrap_or("?"),
                                path["changed"].as_bool().unwrap_or(false)
                            );
                        }
                    }
                } else {
                    println!("  Patch review recorded.");
                }
            } else {
                let stderr = String::from_utf8_lossy(&review_output.stderr);
                println!("  Patch review failed: {stderr}");
            }
        }
        "diff" => {
            println!("  Patch Diff: building multi-file diff navigation...");
            let diff_output = Command::new(
                std::env::args()
                    .next()
                    .unwrap_or_else(|| "forge".to_string()),
            )
            .args(["--store", &store_str, "patch", "diff"])
            .args(rest.split_whitespace().skip(1).collect::<Vec<_>>())
            .arg("--output")
            .arg("json")
            .output()?;
            if diff_output.status.success() {
                let stdout = String::from_utf8_lossy(&diff_output.stdout);
                if let Ok(diff) = serde_json::from_str::<serde_json::Value>(&stdout) {
                    println!(
                        "  Status: {}",
                        diff["status"].as_str().unwrap_or("diff_ready")
                    );
                    println!(
                        "  Changed files: {}",
                        diff["summary"]["changed_file_count"].as_u64().unwrap_or(0)
                    );
                    println!(
                        "  Hunks: {}",
                        diff["summary"]["hunk_count"].as_u64().unwrap_or(0)
                    );
                    if let Some(path) = diff["selection"]["selected_path"].as_str() {
                        println!(
                            "  Selected: file={} hunk={} path={}",
                            diff["selection"]["selected_file_index"]
                                .as_u64()
                                .unwrap_or(0),
                            diff["selection"]["selected_hunk_index"]
                                .as_u64()
                                .unwrap_or(0),
                            path
                        );
                    }
                    if let Some(command) = diff["navigation"]["next_file_command"].as_str() {
                        println!("  Next file: {command}");
                    }
                    if let Some(command) = diff["navigation"]["next_hunk_command"].as_str() {
                        println!("  Next hunk: {command}");
                    }
                } else {
                    println!("  Patch diff navigation recorded.");
                }
            } else {
                let stderr = String::from_utf8_lossy(&diff_output.stderr);
                println!("  Patch diff failed: {stderr}");
            }
        }
        "revert" => {
            println!("  Patch Revert: recording guarded revert proposal.");
            println!("  WARNING: Revert does NOT silently restore files. It records intent.");
            print!("  Continue? (y/N): ");
            std::io::Write::flush(&mut std::io::stdout())?;
            let mut confirm = String::new();
            std::io::stdin().read_line(&mut confirm)?;
            let confirmed = confirm.trim().eq_ignore_ascii_case("y")
                || confirm.trim().eq_ignore_ascii_case("yes");
            if !confirmed {
                println!("  Revert cancelled by user.");
                return Ok(());
            }

            let revert_output = Command::new(
                std::env::args()
                    .next()
                    .unwrap_or_else(|| "forge".to_string()),
            )
            .args(["--store", &store_str, "patch", "revert"])
            .args(rest.split_whitespace().skip(1).collect::<Vec<_>>())
            .arg("--output")
            .arg("json")
            .output()?;
            if revert_output.status.success() {
                println!("  Revert proposal recorded.");
            } else {
                let stderr = String::from_utf8_lossy(&revert_output.stderr);
                println!("  Patch revert failed: {stderr}");
            }
        }
        "restore" => {
            println!("  Patch Restore: you are about to restore repository files.");
            println!(
                "  WARNING: this executes git checkout for paths recorded in a revert artifact."
            );
            print!("  Approve restore? (y/N): ");
            std::io::Write::flush(&mut std::io::stdout())?;
            let mut confirm = String::new();
            std::io::stdin().read_line(&mut confirm)?;
            let confirmed = confirm.trim().eq_ignore_ascii_case("y")
                || confirm.trim().eq_ignore_ascii_case("yes");
            if !confirmed {
                println!("  Restore cancelled by user.");
                return Ok(());
            }

            let mut args = rest
                .split_whitespace()
                .skip(1)
                .map(str::to_string)
                .collect::<Vec<_>>();
            if !args
                .iter()
                .any(|arg| arg == "--approved-by" || arg.starts_with("--approved-by="))
            {
                args.push("--approved-by".to_string());
                args.push("human".to_string());
            }
            if !args.iter().any(|arg| arg == "--confirm-restore") {
                args.push("--confirm-restore".to_string());
            }
            let restore_output = Command::new(
                std::env::args()
                    .next()
                    .unwrap_or_else(|| "forge".to_string()),
            )
            .args(["--store", &store_str, "patch", "restore"])
            .args(args.iter().map(String::as_str))
            .arg("--output")
            .arg("json")
            .output()?;
            if restore_output.status.success() {
                let stdout = String::from_utf8_lossy(&restore_output.stdout);
                if let Ok(restore) = serde_json::from_str::<serde_json::Value>(&stdout) {
                    println!(
                        "  Status: {}",
                        restore["status"].as_str().unwrap_or("restored")
                    );
                    println!(
                        "  Restored paths: {}",
                        restore["restored_paths"].as_array().map_or(0, Vec::len)
                    );
                    println!(
                        "  Approved by: {}",
                        restore["approved_by"].as_str().unwrap_or("unknown")
                    );
                } else {
                    println!("  Restore executed.");
                }
            } else {
                let stderr = String::from_utf8_lossy(&restore_output.stderr);
                println!("  Patch restore failed: {stderr}");
            }
        }
        "" => {
            println!(
                "  Usage: /patch plan --workflow <id> --task <id> --intent \"...\" --path <path>"
            );
            println!("         /patch diff --workflow <id> --task <id> --path <path> --file-index 0 --hunk-index 0");
            println!("         /patch review --workflow <id> --task <id> --path <path>");
            println!("         /patch apply --workflow <id> --task <id> --path <path>");
            println!("         /patch revert --workflow <id> --task <id> --apply-artifact <id>");
            println!("         /patch restore --workflow <id> --task <id> --revert-artifact <id> --approved-by <operator> --confirm-restore");
        }
        other => {
            println!(
                "  Unknown patch subcommand: {other}. Use plan, diff, review, apply, revert, or restore."
            );
        }
    }

    Ok(())
}

fn dispatch_context_command(input: &str, store_path: &std::path::Path) -> Result<()> {
    let rest = input.trim().strip_prefix("/context").unwrap_or("").trim();
    if rest.is_empty() {
        println!("  Usage: /context --workflow <id> --task <id> --budget 1200 --strict");
        return Ok(());
    }

    println!("  Context: building bounded task-local package...");
    let store_str = store_path.to_string_lossy();
    let args = cli_args_without_output(rest);
    let output = Command::new(
        std::env::args()
            .next()
            .unwrap_or_else(|| "forge".to_string()),
    )
    .args(["--store", &store_str, "context"])
    .args(args.iter().map(String::as_str))
    .arg("--output")
    .arg("json")
    .output()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Ok(context) = serde_json::from_str::<serde_json::Value>(&stdout) {
            println!(
                "  Status: context_ready={}",
                context["context_ready"].as_bool().unwrap_or(false)
            );
            println!(
                "  Handoff: {}",
                context["handoff_status"].as_str().unwrap_or("unknown")
            );
            println!(
                "  Route key: {}",
                context["routing_fingerprint"]["cache_key"]
                    .as_str()
                    .unwrap_or("unknown")
            );
            println!(
                "  Bytes: {} / budget {}",
                context["context_bytes"].as_u64().unwrap_or(0),
                context["effective_budget"].as_u64().unwrap_or(0)
            );
            println!(
                "  Quality: {}",
                context["routing_quality"]["status"]
                    .as_str()
                    .unwrap_or("unknown")
            );
            println!(
                "  Next action: {}",
                context["next_action"]["action"]
                    .as_str()
                    .unwrap_or("inspect_context")
            );
        } else {
            println!("  Context package generated.");
        }
    } else {
        print_command_failure("context", &output);
    }

    Ok(())
}

fn dispatch_pm_command(store: &ForgeStore, input: &str) -> Result<()> {
    let objective = input.trim().strip_prefix("/pm ").unwrap_or("").trim();
    if objective.is_empty() {
        println!("  Usage: /pm <broad objective>");
        return Ok(());
    }

    println!("  PM Mode: starting human-guided product management session...");
    let report = crate::request::start_pm_session(store, objective, "forge_repl")?;
    println!("  Status: {}", report.status);
    println!("  Run ID: {}", report.run_id);
    println!("  Workflow ID: {}", report.workflow_id);
    println!("  Goal: {}", report.goal);
    println!("  Handoff: PM agent will now clarify the challenge, identify users and risks.");
    Ok(())
}

fn dispatch_decision_command(
    _store: &ForgeStore,
    input: &str,
    store_path: &std::path::Path,
) -> Result<()> {
    let rest = input.trim().strip_prefix("/decision ").unwrap_or("").trim();
    if rest.is_empty() {
        println!("  Usage: /decision --workflow <id> --title \"...\" --rationale \"...\"");
        return Ok(());
    }

    println!("  Decision: recording durable product decision...");
    let store_str = store_path.to_string_lossy();
    let decision_output = Command::new(
        std::env::args()
            .next()
            .unwrap_or_else(|| "forge".to_string()),
    )
    .args(["--store", &store_str, "workflow", "decision"])
    .args(parse_repl_args(rest)?)
    .arg("--output")
    .arg("json")
    .output()?;

    if decision_output.status.success() {
        let stdout = String::from_utf8_lossy(&decision_output.stdout);
        if let Ok(report) = serde_json::from_str::<serde_json::Value>(&stdout) {
            println!("  Status: {}", report["status"].as_str().unwrap_or("ok"));
            println!(
                "  Decision ID: {}",
                report["decision_id"].as_str().unwrap_or("?")
            );
            println!("  Revision: {}", report["revision"]);
            let decision = &report["decision"];
            println!("  Title: {}", decision["title"].as_str().unwrap_or("?"));
            println!("  Author: {}", decision["author"].as_str().unwrap_or("?"));
            println!(
                "  Rationale: {}",
                decision["rationale"].as_str().unwrap_or("?")
            );
        } else {
            println!("  Decision recorded successfully.");
        }
    } else {
        let stderr = String::from_utf8_lossy(&decision_output.stderr);
        println!("  Error: {}", stderr.trim());
    }
    Ok(())
}

fn parse_repl_args(input: &str) -> Result<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut quote: Option<char> = None;

    while let Some(ch) = chars.next() {
        match (quote, ch) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), '\\') => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            (Some(_), c) => current.push(c),
            (None, '"' | '\'') => quote = Some(ch),
            (None, c) if c.is_whitespace() => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            (None, '\\') => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            (None, c) => current.push(c),
        }
    }

    if let Some(q) = quote {
        anyhow::bail!("unterminated quoted argument starting with {q}");
    }
    if !current.is_empty() {
        args.push(current);
    }
    Ok(args)
}

fn dispatch_handoff_command(input: &str, store_path: &std::path::Path) -> Result<()> {
    let rest = input.trim().strip_prefix("/handoff").unwrap_or("").trim();
    if rest.is_empty() {
        println!("  Usage: /handoff --workflow <id> --task <id> --executor codex --budget 1200");
        return Ok(());
    }

    println!("  Handoff: this may acquire a task lease for the selected executor.");
    print!("  Approve handoff lease acquisition? (y/N): ");
    std::io::Write::flush(&mut std::io::stdout())?;
    let mut confirm = String::new();
    std::io::stdin().read_line(&mut confirm)?;
    let confirmed =
        confirm.trim().eq_ignore_ascii_case("y") || confirm.trim().eq_ignore_ascii_case("yes");
    if !confirmed {
        println!("  Handoff cancelled by user.");
        return Ok(());
    }

    let store_str = store_path.to_string_lossy();
    let args = cli_args_without_output(rest);
    let output = Command::new(
        std::env::args()
            .next()
            .unwrap_or_else(|| "forge".to_string()),
    )
    .args(["--store", &store_str, "task", "handoff"])
    .args(args.iter().map(String::as_str))
    .arg("--output")
    .arg("json")
    .output()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Ok(handoff) = serde_json::from_str::<serde_json::Value>(&stdout) {
            println!("  Status: {}", handoff["status"].as_str().unwrap_or("ok"));
            println!(
                "  Allowed: {}",
                handoff["allowed"].as_bool().unwrap_or(false)
            );
            println!(
                "  Lease status: {}",
                handoff["packet"]["lease_status"]
                    .as_str()
                    .unwrap_or("unknown")
            );
            println!(
                "  Route key: {}",
                handoff["packet"]["context_routing_cache_key"]
                    .as_str()
                    .unwrap_or("unknown")
            );
        } else {
            println!("  Handoff packet generated.");
        }
    } else {
        print_command_failure("handoff", &output);
    }

    Ok(())
}

fn cli_args_without_output(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut iter = input.split_whitespace();
    while let Some(arg) = iter.next() {
        if arg == "--output" {
            let _ = iter.next();
            continue;
        }
        if let Some((name, _value)) = arg.split_once('=') {
            if name == "--output" {
                continue;
            }
        }
        args.push(arg.to_string());
    }
    args
}

fn print_command_failure(label: &str, output: &std::process::Output) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let message = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    println!("  {label} failed: {message}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_terminal_entrypoint_renders_advanced_cockpit_frame() {
        let temp = tempfile::tempdir().unwrap();
        let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();
        let report = build_interactive_home(&store).unwrap();
        let state = InteractiveReplState::from_home(&report);
        let frame = render_interactive_tui_frame(&report, &state);

        assert!(frame.contains("Forge advanced operational TUI"));
        assert!(frame.contains("Workflows"));
        assert!(frame.contains("Execution"));
        assert!(frame.contains("Realtime"));
        assert!(frame.contains("Safe Actions"));
        assert!(frame.contains("j/k focus"));
        assert!(frame.contains("enter open"));
        assert!(frame.contains("Type an objective"));
        assert!(frame.contains("slash /cockpit /task-board"));
    }

    #[test]
    fn can_answer_questions_about_current_state() {
        assert!(can_answer_directly("What is the current Forge status?"));
        assert!(can_answer_directly("Show me the current help"));
        assert!(can_answer_directly("what is happening right now"));
        assert!(can_answer_directly("status please"));
        assert!(!can_answer_directly("Research upcoming events"));
        assert!(!can_answer_directly("implement a scheduler"));
        assert!(!can_answer_directly("deploy to production"));
        assert!(!can_answer_directly("validate the workflow"));
    }

    #[test]
    fn requires_workflow_detects_execution_keywords() {
        assert!(requires_workflow("research this topic"));
        assert!(requires_workflow("implement a feature"));
        assert!(requires_workflow("code a solution"));
        assert!(requires_workflow("create artifact"));
        assert!(requires_workflow("run the analysis"));
        assert!(requires_workflow("deploy to server"));
        assert!(requires_workflow("delete workflow"));
        assert!(requires_workflow("schedule daily report"));
        assert!(requires_workflow("cron every hour"));
        assert!(!requires_workflow("what is the weather"));
        assert!(!requires_workflow("current status"));
        assert!(!requires_workflow("help me understand"));
    }

    #[test]
    fn decide_retention_keeps_recurring_workflows() {
        let decision = decide_retention("Research hackathons every day", true);
        assert_eq!(decision.action, "retain");
        assert!(!decision.requires_human_approval);
        assert_eq!(
            decision.schema_version,
            "forge.interactive.retention_decision.v1"
        );

        let decision = decide_retention("Daily report with cron", true);
        assert_eq!(decision.action, "retain");

        let decision = decide_retention("Send artifact via telegram", true);
        assert_eq!(decision.action, "retain");
    }

    #[test]
    fn decide_retention_keeps_workflows_with_artifacts_or_side_effects() {
        let decision = decide_retention("Generate PDF report", true);
        assert_eq!(decision.action, "retain");

        let decision = decide_retention("Send notification externally", true);
        assert_eq!(decision.action, "retain");

        let decision = decide_retention("Deploy the new version", true);
        assert_eq!(decision.action, "retain");
    }

    #[test]
    fn decide_retention_archives_simple_execution_backed_workflows() {
        let decision = decide_retention("Run a quick calculation", true);
        assert_eq!(decision.action, "archive");
        assert!(!decision.requires_human_approval);
        assert_eq!(decision.confidence, 0.68);
    }

    #[test]
    fn decide_retention_blocks_deletion_of_artifact_or_side_effect_workflows() {
        let decision = decide_retention("Create a PDF artifact then delete", true);
        assert_eq!(decision.action, "keep_until_approved");
        assert!(decision.requires_human_approval);
        assert_eq!(decision.confidence, 0.94);

        let decision = decide_retention("delete the deploy workflow", true);
        assert_eq!(decision.action, "keep_until_approved");
    }

    #[test]
    fn decide_retention_noops_when_no_workflow_created() {
        let decision = decide_retention("anything", false);
        assert_eq!(decision.action, "none");
        assert!(decision.confidence > 0.99);
    }

    #[test]
    fn route_slash_command_recognizes_known_commands() {
        let report = route_slash_command("/status");
        assert_eq!(report.input_kind, "slash_command");
        assert_eq!(report.routing_decision, "slash_command");
        assert!(!report.workflow_created);
        let route = report.slash_command.unwrap();
        assert_eq!(route.name, "/status");
        assert!(route.recognized);
        assert!(!route.mutates_workflow);
        assert_eq!(route.risk_level, "low");
    }

    #[test]
    fn route_slash_command_recognizes_harness_audit() {
        let report = route_slash_command("/harness");
        assert_eq!(report.input_kind, "slash_command");
        assert_eq!(report.routing_decision, "slash_command");
        let route = report.slash_command.unwrap();
        assert_eq!(route.name, "/harness");
        assert!(route.recognized);
        assert!(!route.mutates_workflow);
        assert_eq!(route.risk_level, "low");
        assert_eq!(
            route.equivalent_command,
            vec![
                "forge".to_string(),
                "harness".to_string(),
                "mode".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ]
        );
    }

    #[test]
    fn route_slash_command_recognizes_harness_doctor_audit() {
        let report = route_slash_command(
            "/harness doctor --executor codex --shim-dir /tmp/forge-bin --project-root /repo",
        );
        assert_eq!(report.input_kind, "slash_command");
        assert_eq!(report.routing_decision, "slash_command");
        let route = report.slash_command.unwrap();
        assert_eq!(route.name, "/harness doctor");
        assert!(route.recognized);
        assert!(!route.mutates_workflow);
        assert_eq!(route.risk_level, "low");
        assert_eq!(
            route.equivalent_command,
            vec![
                "forge".to_string(),
                "harness".to_string(),
                "doctor".to_string(),
                "--executor".to_string(),
                "<executor>".to_string(),
                "--shim-dir".to_string(),
                "<dir>".to_string(),
                "--project-root".to_string(),
                "<project-root>".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ]
        );
    }

    #[test]
    fn interactive_home_surfaces_sessions_quick_action() {
        let temp = tempfile::tempdir().unwrap();
        let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();
        let report = build_interactive_home(&store).unwrap();
        assert!(report
            .dashboard
            .quick_actions
            .contains(&"/sessions".to_string()));
    }

    #[test]
    fn slash_sessions_is_recognized_as_read_only_provider_state() {
        let report = route_slash_command("/sessions");
        assert_eq!(report.input_kind, "slash_command");
        assert_eq!(report.routing_decision, "slash_command");
        let route = report.slash_command.unwrap();
        assert_eq!(route.name, "/sessions");
        assert!(route.recognized);
        assert!(!route.mutates_workflow);
        assert_eq!(route.risk_level, "low");
        assert_eq!(
            route.equivalent_command,
            vec![
                "forge".to_string(),
                "sessions".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ]
        );
    }

    #[test]
    fn slash_sessions_lifecycle_is_recognized_as_audited_mutation() {
        let report = route_slash_command(
            "/sessions lifecycle --session codex-shell --state opened --origin operator",
        );
        assert_eq!(report.input_kind, "slash_command");
        assert_eq!(report.routing_decision, "slash_command");
        let route = report.slash_command.unwrap();
        assert_eq!(route.name, "/sessions lifecycle");
        assert!(route.recognized);
        assert!(route.mutates_workflow);
        assert_eq!(route.risk_level, "medium");
        assert_eq!(
            route.equivalent_command,
            vec![
                "forge".to_string(),
                "sessions".to_string(),
                "lifecycle".to_string(),
                "--session".to_string(),
                "<session-id>".to_string(),
                "--state".to_string(),
                "opened".to_string(),
            ]
        );
    }

    #[test]
    fn slash_sessions_history_is_recognized_as_read_only_audit_view() {
        let report = route_slash_command("/sessions history --session codex-shell");
        assert_eq!(report.input_kind, "slash_command");
        assert_eq!(report.routing_decision, "slash_command");
        let route = report.slash_command.unwrap();
        assert_eq!(route.name, "/sessions history");
        assert!(route.recognized);
        assert!(!route.mutates_workflow);
        assert_eq!(route.risk_level, "low");
        assert_eq!(
            route.equivalent_command,
            vec![
                "forge".to_string(),
                "sessions".to_string(),
                "history".to_string(),
                "--session".to_string(),
                "<session-id>".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ]
        );
    }

    #[test]
    fn route_slash_command_reports_unknown_commands() {
        let report = route_slash_command("/nonexistent");
        assert_eq!(report.input_kind, "slash_command");
        let route = report.slash_command.unwrap();
        assert_eq!(route.name, "/nonexistent");
        assert!(!route.recognized);
        assert_eq!(route.risk_level, "unknown");
    }

    #[test]
    fn route_slash_command_recognizes_milestone_subcommands() {
        let report = route_slash_command("/milestone");
        assert_eq!(report.input_kind, "slash_command");
        let route = report.slash_command.unwrap();
        assert_eq!(route.name, "/milestone");
        assert!(route.recognized);
        assert!(route.equivalent_command.contains(&"milestone".to_string()));
        assert!(!route.mutates_workflow);

        let manifest = route_slash_command("/manifest");
        let mr = manifest.slash_command.unwrap();
        assert!(mr.recognized);
        assert_eq!(mr.name, "/manifest");

        let research = route_slash_command("/research");
        let rr = research.slash_command.unwrap();
        assert!(rr.recognized);
        assert_eq!(rr.name, "/research");
    }

    #[test]
    fn route_slash_command_preserves_arguments() {
        let report = route_slash_command("/stop --workflow wf_demo --task task_1");
        assert_eq!(report.input_kind, "slash_command");
        let route = report.slash_command.unwrap();
        assert_eq!(route.name, "/stop");
        assert!(route.recognized);
        assert!(route.mutates_workflow);
        assert_eq!(route.risk_level, "high");
        assert_eq!(
            route.input_arguments,
            vec![
                "--workflow".to_string(),
                "wf_demo".to_string(),
                "--task".to_string(),
                "task_1".to_string()
            ]
        );
        assert_eq!(
            route.input_argument_text,
            "--workflow wf_demo --task task_1"
        );
        assert_eq!(route.execution_boundary, "slash_command_not_executed");
    }

    #[test]
    fn can_answer_supports_help_questions() {
        assert!(can_answer_directly("help"));
        assert!(can_answer_directly("Help me understand Forge"));
        assert!(!can_answer_directly("help me implement a workflow"));
    }

    #[test]
    fn routing_classification_pure_simple_question() {
        assert!(can_answer_directly("What is the current status?"));
        assert!(!can_answer_directly(
            "What is the best way to implement a cron job?"
        ));
    }

    #[test]
    fn executor_aware_routing_detects_codex_and_opencode() {
        assert!(executor_or_runtime_required("run this with codex"));
        assert!(executor_or_runtime_required("opencode can handle this"));
        assert!(executor_or_runtime_required("deploy via docker"));
        assert!(executor_or_runtime_required("run on kubernetes"));
        assert!(executor_or_runtime_required("k8s deployment"));
        assert!(executor_or_runtime_required("knative service"));
        assert!(requires_workflow("codex implement feature"));
        assert!(requires_workflow("opencode research topic"));
        assert!(requires_workflow("docker run analysis"));
        assert!(!executor_or_runtime_required("what is the status"));
        assert!(!executor_or_runtime_required("help me understand"));
    }

    #[test]
    fn cost_sensitive_routing_detects_expensive_actions() {
        assert!(cost_sensitive("what is the cost of deploy"));
        assert!(cost_sensitive("expensive external delivery"));
        assert!(cost_sensitive("budget for external notification"));
        assert!(cost_sensitive("cost of external delivery"));
        assert!(requires_workflow("cost of deploy"));
        assert!(!cost_sensitive("what is the cost"));
        assert!(!cost_sensitive("help"));
        assert!(!cost_sensitive("current status"));
    }

    #[test]
    fn classify_workflow_reason_includes_executor_and_cost_reasons() {
        let reason = classify_workflow_reason("codex analysis");
        assert!(
            reason.contains("executor"),
            "expected executor reason, got: {reason}"
        );

        let reason = classify_workflow_reason("expensive deploy");
        assert!(
            reason.contains("cost"),
            "expected cost reason, got: {reason}"
        );

        let reason = classify_workflow_reason("docker run analysis");
        assert!(
            reason.contains("executor"),
            "expected executor reason, got: {reason}"
        );
    }

    #[test]
    fn executor_and_cost_terms_prevent_direct_answer() {
        assert!(!can_answer_directly("What is the cost of deploying?"));
        assert!(!can_answer_directly("What is the status of my codex run?"));
        assert!(!can_answer_directly("Help me use opencode for research"));
    }

    #[test]
    fn slash_patch_plan_is_recognized() {
        let report = route_slash_command(
            "/patch plan --workflow wf_1 --task task_1 --intent test --path Cargo.toml",
        );
        assert_eq!(report.input_kind, "slash_command");
        let route = report.slash_command.unwrap();
        assert_eq!(route.name, "/patch plan");
        assert!(route.recognized);
        assert!(route.equivalent_command.contains(&"forge".to_string()));
    }

    #[test]
    fn slash_patch_diff_is_recognized() {
        let report =
            route_slash_command("/patch diff --workflow wf_1 --task task_1 --path Cargo.toml");
        assert_eq!(report.input_kind, "slash_command");
        let route = report.slash_command.unwrap();
        assert_eq!(route.name, "/patch diff");
        assert!(route.recognized);
        assert!(!route.mutates_workflow);
        assert_eq!(route.risk_level, "medium");
    }

    #[test]
    fn slash_patch_apply_is_recognized() {
        let report =
            route_slash_command("/patch apply --workflow wf_1 --task task_1 --path Cargo.toml");
        assert_eq!(report.input_kind, "slash_command");
        let route = report.slash_command.unwrap();
        assert_eq!(route.name, "/patch apply");
        assert!(route.recognized);
    }

    #[test]
    fn slash_patch_review_is_recognized() {
        let report =
            route_slash_command("/patch review --workflow wf_1 --task task_1 --path Cargo.toml");
        assert_eq!(report.input_kind, "slash_command");
        let route = report.slash_command.unwrap();
        assert_eq!(route.name, "/patch review");
        assert!(route.recognized);
        assert!(!route.mutates_workflow);
        assert_eq!(route.risk_level, "medium");
    }

    #[test]
    fn slash_patch_revert_is_recognized() {
        let report = route_slash_command(
            "/patch revert --workflow wf_1 --task task_1 --apply-artifact art_1",
        );
        assert_eq!(report.input_kind, "slash_command");
        let route = report.slash_command.unwrap();
        assert_eq!(route.name, "/patch revert");
        assert!(route.recognized);
        assert!(route.mutates_workflow);
        assert_eq!(route.risk_level, "high");
    }

    #[test]
    fn slash_patch_restore_is_recognized() {
        let report = route_slash_command(
            "/patch restore --workflow wf_1 --task task_1 --revert-artifact art_1 --approved-by tester --confirm-restore",
        );
        assert_eq!(report.input_kind, "slash_command");
        let route = report.slash_command.unwrap();
        assert_eq!(route.name, "/patch restore");
        assert!(route.recognized);
        assert!(route.mutates_workflow);
        assert_eq!(route.risk_level, "high");
    }

    #[test]
    fn slash_patch_standalone_is_recognized() {
        let report = route_slash_command("/patch");
        assert_eq!(report.input_kind, "slash_command");
        let route = report.slash_command.unwrap();
        assert_eq!(route.name, "/patch");
        assert!(route.recognized);
        assert_eq!(route.risk_level, "high");
    }

    #[test]
    fn slash_patch_unknown_subcommand_is_not_recognized() {
        let report = route_slash_command("/patch unknown");
        // With subcommand, route_slash_command looks for exact match on "/patch unknown"
        // which does not exist as a spec; it falls back to the "/patch" spec
        let route = report.slash_command.unwrap();
        // First token is always the base command in the current parser
        assert_eq!(route.name, "/patch");
        assert!(route.recognized);
    }

    #[test]
    fn slash_context_and_handoff_commands_are_recognized() {
        let context = route_slash_command("/context --workflow wf_1 --task task-001");
        let route = context.slash_command.unwrap();
        assert_eq!(route.name, "/context");
        assert!(route.recognized);
        assert!(!route.mutates_workflow);
        assert_eq!(route.risk_level, "low");
        assert!(route.equivalent_command.contains(&"context".to_string()));

        let handoff =
            route_slash_command("/handoff --workflow wf_1 --task task-001 --executor codex");
        let route = handoff.slash_command.unwrap();
        assert_eq!(route.name, "/handoff");
        assert!(route.recognized);
        assert!(route.mutates_workflow);
        assert_eq!(route.risk_level, "medium");
        assert!(route.equivalent_command.contains(&"handoff".to_string()));
    }

    #[test]
    fn parse_repl_args_preserves_quoted_product_decision_fields() {
        let args = parse_repl_args(
            "--workflow wf_1 --title \"Serve operators first\" --rationale 'Repeated workflow pain'",
        )
        .unwrap();
        assert_eq!(
            args,
            vec![
                "--workflow",
                "wf_1",
                "--title",
                "Serve operators first",
                "--rationale",
                "Repeated workflow pain"
            ]
        );
    }
}
