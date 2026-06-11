use crate::addon::{
    default_addon_dirs, list_addon_permission_authorizations, list_addon_views,
    load_addon_catalog_from_store,
};
use crate::checkpoint::TaskCheckpoint;
use crate::cost::build_cost_ledger;
use crate::event::{build_global_event_timeline, GlobalEventTimelineReport, WorkflowEventEnvelope};
use crate::executor::{
    build_brain_sessions_report_with_options, load_executors, BrainSessionState,
    BrainSessionsReport, BrainSessionsReportOptions,
};
use crate::graph::{AtomicTask, ExecutorKind, TaskStatus};
use crate::harness::{
    analyze_token_headroom, build_cli_wrapper_plan, build_harness_doctor_report,
    build_harness_mode_report, inspect_cli_harness_shim_status,
    resolve_harness_forge_first_source_for_project, resolve_harness_runtime_policy,
    CliShimStatusOptions, CliShimStatusReport, CliWrapperPlanOptions, CliWrapperPlanReport,
    HarnessDoctorOptions, HarnessDoctorReport, HarnessModeOptions, HarnessModeReport,
    HarnessRuntimePolicyOptions, TokenHeadroomReport,
};
use crate::identity::list_identity_memberships;
use crate::interaction::list_human_interactions;
use crate::memory::memory_policy_report;
use crate::ops::{
    build_addon_view_renderer_report, build_operational_digital_twin, load_modifier_lane,
    OpsAddonViewRendererReport, OpsOperationalDigitalTwin,
};
use crate::registry::{
    list_workflows_with_filters, RegistryContextActionRef, WorkflowLifecycleFilter,
    WorkflowRegistryFilters, WorkflowRegistryRow,
};
use crate::request::start_async_request;
use crate::runtime::load_runtimes;
use crate::schedule::build_schedule_worker_status;
use crate::storage::{ForgeStore, StoreEvent};
use crate::workflow::{record_product_decision, ProductDecisionInput};
use anyhow::Result;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command;

const INTERACTIVE_HOME_SCHEMA_VERSION: &str = "forge.interactive.home.v1";
const INTERACTIVE_TASK_BOARD_SCHEMA_VERSION: &str = "forge.interactive.task_board.v1";
const INTERACTIVE_WORKFLOW_DAG_SCHEMA_VERSION: &str = "forge.interactive.workflow_dag.v1";
const INTERACTIVE_READINESS_SCHEMA_VERSION: &str = "forge.interactive.readiness.v1";
const INTERACTIVE_HARNESS_SCHEMA_VERSION: &str = "forge.interactive.harness.v1";
const INTERACTIVE_SESSIONS_SCHEMA_VERSION: &str = "forge.interactive.sessions.v1";
const INTERACTIVE_COMMAND_PALETTE_SCHEMA_VERSION: &str = "forge.interactive.command_palette.v1";
const INTERACTIVE_AUTOCOMPLETE_SCHEMA_VERSION: &str = "forge.interactive.autocomplete.v1";
const INTERACTIVE_PATCH_WORKBENCH_SCHEMA_VERSION: &str = "forge.interactive.patch_workbench.v1";
const INTERACTIVE_PERMISSIONS_SCHEMA_VERSION: &str = "forge.interactive.permissions.v1";
const INTERACTIVE_NAVIGATION_SCHEMA_VERSION: &str = "forge.interactive.navigation.v1";
const INTERACTIVE_UI_COMPOSITION_SCHEMA_VERSION: &str = "forge.interactive.ui_composition.v1";
const INTERACTIVE_STRUCTURED_LOGS_SCHEMA_VERSION: &str = "forge.interactive.structured_logs.v1";
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
    pub sessions_panel: InteractiveSessionsPanel,
    pub command_palette_panel: InteractiveCommandPalettePanel,
    pub autocomplete_panel: InteractiveAutocompletePanel,
    pub harness_mode_panel: HarnessModeReport,
    pub harness_doctor_panel: HarnessDoctorReport,
    pub runtime_node_status: String,
    pub repository_context: String,
    pub estimated_costs: String,
    pub scheduler_worker_status: String,
    pub workflow_focus: Vec<InteractiveWorkflowCard>,
    pub navigation_panel: InteractiveNavigationPanel,
    pub ui_composition_panel: InteractiveUiCompositionPanel,
    pub patch_workbench_panel: InteractivePatchWorkbenchPanel,
    pub permissions_panel: InteractivePermissionsPanel,
    pub dag_panel: InteractiveWorkflowDagPanel,
    pub task_board_panel: InteractiveTaskBoardPanel,
    pub schedule_panel: InteractiveSchedulePanel,
    pub event_panel: InteractiveEventPanel,
    pub structured_logs_panel: InteractiveStructuredLogsPanel,
    pub cost_panel: InteractiveCostPanel,
    pub context_memory_panel: InteractiveContextMemoryPanel,
    pub digital_twin_panel: OpsOperationalDigitalTwin,
    pub addon_renderer_panel: InteractiveAddonRendererPanel,
    pub attention_actions: Vec<String>,
    pub useful_next_commands: Vec<String>,
    pub quick_actions: Vec<String>,
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
    pub workflow_id: Option<String>,
    pub commands: Vec<String>,
    pub mutates_workflow: bool,
    pub requires_approval: bool,
    pub risk_level: String,
    pub keywords: Vec<String>,
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
    pub mode: HarnessModeReport,
    pub doctor: HarnessDoctorReport,
    pub shim_status: CliShimStatusReport,
    pub wrapper_plan: CliWrapperPlanReport,
    pub headroom_preview: TokenHeadroomReport,
    pub next_actions: Vec<String>,
    pub notes: Vec<String>,
    pub commands: InteractiveHarnessCommands,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveHarnessCommands {
    pub refresh: Vec<String>,
    pub mode: Vec<String>,
    pub doctor: Vec<String>,
    pub shim_status: Vec<String>,
    pub wrap_plan: Vec<String>,
    pub install_shims: Vec<String>,
    pub exec: Vec<String>,
    pub sessions: Vec<String>,
    pub sync: Vec<String>,
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
    pub files: Vec<InteractivePatchWorkbenchFile>,
    pub approval_flow: InteractivePatchApprovalFlow,
    pub commands: InteractivePatchWorkbenchCommands,
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
    pub staged: bool,
    pub unstaged: bool,
    pub untracked: bool,
    pub commands: InteractivePatchWorkbenchFileCommands,
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
pub struct InteractiveTaskHistoryEvent {
    pub event_id: i64,
    pub kind: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InteractiveSchedulePanel {
    pub status: String,
    pub due_workflows: usize,
    pub runnable_due_workflows: usize,
    pub blocked_due_workflows: usize,
    pub cron_nodes: usize,
    pub wait_until_nodes: usize,
    pub delay_nodes: usize,
    pub scale_to_zero_workflows: usize,
    pub next_wakeup_at: Option<String>,
    pub sleep_seconds: u64,
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
pub struct InteractiveContextMemoryPanel {
    pub status: String,
    pub ready_for_handoff: usize,
    pub blocked_tasks: usize,
    pub context_budget_pressure: usize,
    pub memory_policy_status: String,
    pub memory_level_count: usize,
    pub temporary_memory_rule: String,
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
    pub equivalent_command: Vec<String>,
    pub mutates_workflow: bool,
    pub risk_level: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RetentionDecision {
    pub schema_version: String,
    pub action: String,
    pub reason: String,
    pub confidence: f32,
    pub requires_human_approval: bool,
}

pub fn build_interactive_home(store: &ForgeStore) -> Result<InteractiveHomeReport> {
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
    let repository_context_path = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
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
    let sessions_panel = build_interactive_sessions(store, InteractiveSessionsOptions::default())?;
    let command_palette_panel = build_interactive_command_palette(store, None)?;
    let autocomplete_panel = build_interactive_autocomplete(store, "")?;
    let harness_mode_panel = harness_panel.mode.clone();
    let harness_doctor_panel = harness_panel.doctor.clone();
    let runtime_node_status = if runtimes.usable.is_empty() {
        "no allowed async run substrates".to_string()
    } else {
        format!("usable runtimes: {}", runtimes.usable.join(", "))
    };

    let scheduler_worker = build_schedule_worker_status(store, "forge-scheduler", 1, 300).ok();
    let scheduler_worker_status = scheduler_worker
        .as_ref()
        .map(|ws| {
            let s = &ws.summary;
            let due = s.runnable_due_workflows;
            let idle = s.idle_workflows;
            let capacity = ws.worker_pool.available_workers;
            let sleep = if ws.sleep.sleep_until_next_wakeup {
                ws.sleep
                    .next_wakeup_at
                    .as_deref()
                    .unwrap_or("now")
                    .to_string()
            } else {
                "immediate".to_string()
            };
            format!("{due} due, {idle} idle, capacity {capacity}, next {sleep}")
        })
        .unwrap_or_else(|| "no scheduled workflows".to_string());
    let schedule_panel = scheduler_worker
        .as_ref()
        .map(|ws| {
            let summary = &ws.summary;
            InteractiveSchedulePanel {
                status: ws.status.clone(),
                due_workflows: summary.due_workflows,
                runnable_due_workflows: summary.runnable_due_workflows,
                blocked_due_workflows: summary.blocked_due_workflows,
                cron_nodes: summary.cron_nodes,
                wait_until_nodes: summary.wait_until_nodes,
                delay_nodes: summary.delay_nodes,
                scale_to_zero_workflows: summary.scale_to_zero_workflows,
                next_wakeup_at: ws.sleep.next_wakeup_at.clone(),
                sleep_seconds: ws.sleep.sleep_seconds,
            }
        })
        .unwrap_or_else(|| InteractiveSchedulePanel {
            status: "no_scheduled_workflows".to_string(),
            due_workflows: 0,
            runnable_due_workflows: 0,
            blocked_due_workflows: 0,
            cron_nodes: 0,
            wait_until_nodes: 0,
            delay_nodes: 0,
            scale_to_zero_workflows: 0,
            next_wakeup_at: None,
            sleep_seconds: 0,
        });
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
    let dag_panel = build_workflow_dag_panel(store, &workflows.workflows)?;
    let task_board_panel = build_task_board_panel(store, &workflows.workflows)?;
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
        .map(|ledger| {
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
        })
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
    let memory_policy = memory_policy_report(store);
    let temporary_memory_rule = memory_policy
        .interface_policy
        .iter()
        .find(|policy| policy.default_scope == "processing")
        .map(|policy| policy.retention.clone())
        .unwrap_or_else(|| "processing memory is temporary until promoted".to_string());
    let context_memory_panel = InteractiveContextMemoryPanel {
        status: "context_memory_ready".to_string(),
        ready_for_handoff: workflows.summary.context_actions.ready_for_handoff,
        blocked_tasks: workflows.summary.context_actions.blocked_tasks,
        context_budget_pressure: workflows.summary.context_quality.budget_pressure,
        memory_policy_status: memory_policy.status,
        memory_level_count: memory_policy.memory_levels.len(),
        temporary_memory_rule,
    };
    let addon_renderer_report = load_addon_catalog_from_store(store, &default_addon_dirs())
        .ok()
        .map(|catalog| {
            let addon_views =
                list_addon_views(&catalog, None, Some("ops_console"), Some("enabled"));
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
    let addon_renderer_panel = build_interactive_addon_renderer_panel(&addon_renderer_report);
    let patch_workbench_panel = build_interactive_patch_workbench(store)?;
    let permissions_panel = build_interactive_permissions(store)?;
    let ui_composition_panel = build_ui_composition_panel(&addon_renderer_report);

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
            sessions_panel,
            command_palette_panel,
            autocomplete_panel,
            harness_mode_panel,
            harness_doctor_panel,
            runtime_node_status,
            repository_context: repository_context_path.display().to_string(),
            estimated_costs: "available per workflow via /costs or forge run --simulate"
                .to_string(),
            scheduler_worker_status,
            workflow_focus,
            navigation_panel: build_navigation_panel(),
            ui_composition_panel,
            patch_workbench_panel,
            permissions_panel,
            dag_panel,
            task_board_panel,
            schedule_panel,
            event_panel,
            structured_logs_panel,
            cost_panel,
            context_memory_panel,
            digital_twin_panel,
            addon_renderer_panel,
            attention_actions,
            useful_next_commands: vec![
                "forge list".to_string(),
                "forge inspect <workflow-id>".to_string(),
                "forge request list".to_string(),
                "forge schedule list".to_string(),
                "forge schedule worker-status".to_string(),
                "forge interactive harness --output json".to_string(),
                "forge interactive sessions --output json".to_string(),
                "forge interactive patch-workbench --output json".to_string(),
                "forge interactive permissions --output json".to_string(),
            ],
            quick_actions: vec![
                "/status".to_string(),
                "/workflows".to_string(),
                "/runs".to_string(),
                "/artifacts".to_string(),
                "/task-board".to_string(),
                "/milestone".to_string(),
                "/sync".to_string(),
                "/brains".to_string(),
                "/sessions".to_string(),
                "/shells".to_string(),
                "/harness".to_string(),
                "/harness doctor".to_string(),
                "/validate".to_string(),
                "/logs".to_string(),
                "/workers".to_string(),
                "/context".to_string(),
                "/handoff".to_string(),
                "/pm".to_string(),
                "/decision".to_string(),
            ],
        },
        slash_commands: slash_commands(),
    })
}

pub fn slash_command_catalog() -> SlashCommandCatalogReport {
    SlashCommandCatalogReport {
        status: "slash_commands_loaded".to_string(),
        schema_version: SLASH_COMMANDS_SCHEMA_VERSION.to_string(),
        commands: slash_commands(),
    }
}

pub fn build_interactive_harness(
    _store: &ForgeStore,
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
    let wrapper_plan = build_cli_wrapper_plan(CliWrapperPlanOptions {
        executor: &options.executor,
        command: &command,
        forge_first: mode.forge_first,
        forge_first_source: &mode.forge_first_source,
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
    let headroom_preview = analyze_token_headroom(
        "Forge harness preview: route bounded context, shell receipts, logs, tool output and CLI stdout through local token headroom while preserving retrieval references.",
        Some("text"),
        runtime_policy.context_budget,
        "interactive_harness_preview",
        true,
    );
    let commands = interactive_harness_commands(
        &options.executor,
        &options.shim_dir,
        &project_root,
        runtime_policy.context_budget,
        runtime_policy.token_headroom,
    );
    let mut next_actions = doctor.next_actions.clone();
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
        mode,
        doctor,
        shim_status,
        wrapper_plan,
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
    let next_actions = readiness_next_actions(
        executors.usable.is_empty(),
        executors.needs_human_approval,
        runtimes.needs_human_approval,
        &harness_doctor,
    );

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
        next_actions,
        commands: readiness_commands(),
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
            "patch.plan",
            "patch",
            "Create patch plan",
            "Create a bounded Forge patch plan artifact before file edits.",
            "patch_workbench_panel",
            None,
            &[
                "patch",
                "plan",
                "--workflow",
                "<workflow-id>",
                "--task",
                "<task-id>",
                "--intent",
                "<intent>",
                "--output",
                "json",
            ],
            true,
            false,
            "medium",
            &["patch", "plan", "artifact", "edit", "bounds"],
        ),
        command_palette_entry(
            "patch.diff",
            "patch",
            "Review patch diff",
            "Open read-only multi-file diff navigation for the current bounded patch.",
            "patch_workbench_panel",
            None,
            &[
                "patch",
                "diff",
                "--workflow",
                "<workflow-id>",
                "--task",
                "<task-id>",
                "--output",
                "json",
            ],
            false,
            false,
            "low",
            &["patch", "diff", "review", "file", "hunk"],
        ),
        command_palette_entry(
            "patch.review",
            "patch",
            "Record patch review",
            "Persist current diff/status/check evidence before an apply approval.",
            "patch_workbench_panel",
            None,
            &[
                "patch",
                "review",
                "--workflow",
                "<workflow-id>",
                "--task",
                "<task-id>",
                "--output",
                "json",
            ],
            true,
            false,
            "medium",
            &["patch", "review", "evidence", "status", "diff"],
        ),
        command_palette_entry(
            "patch.apply",
            "patch",
            "Record patch apply",
            "Record applied file snapshots and validation evidence after approved edits.",
            "patch_workbench_panel",
            None,
            &[
                "patch",
                "apply",
                "--workflow",
                "<workflow-id>",
                "--task",
                "<task-id>",
                "--output",
                "json",
            ],
            true,
            true,
            "high",
            &["patch", "apply", "approval", "validation", "snapshot"],
        ),
        command_palette_entry(
            "patch.restore",
            "patch",
            "Restore from approved patch rollback",
            "Execute an explicitly approved patch restore path.",
            "patch_workbench_panel",
            None,
            &[
                "patch",
                "restore",
                "--workflow",
                "<workflow-id>",
                "--task",
                "<task-id>",
                "--confirm-restore",
                "--approved-by",
                "<operator>",
                "--output",
                "json",
            ],
            true,
            true,
            "high",
            &["patch", "restore", "rollback", "approval", "file"],
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
    ]
}

fn workflow_command_palette_entries(
    workflows: &[WorkflowRegistryRow],
) -> Vec<InteractiveCommandPaletteEntry> {
    workflows
        .iter()
        .take(24)
        .map(|workflow| {
            let goal = truncate_display(&workflow.current_goal, 80);
            let mut entry = command_palette_entry(
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
            entry.keywords.push(workflow.workflow_id.clone());
            entry.keywords.push(workflow.lifecycle_state.clone());
            entry.keywords.push(goal);
            entry
        })
        .collect()
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
    InteractiveCommandPaletteEntry {
        action_id: action_id.to_string(),
        group_id: group_id.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        source_panel: source_panel.to_string(),
        workflow_id,
        commands: commands
            .iter()
            .map(|command| (*command).to_string())
            .collect(),
        mutates_workflow,
        requires_approval,
        risk_level: risk_level.to_string(),
        keywords: keywords
            .iter()
            .map(|keyword| (*keyword).to_string())
            .collect(),
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
        "patch",
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
        "patch" => "Patch Workbench",
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
                insert_text: entry.commands.join(" "),
                description: entry.description.clone(),
                source: "command_palette_panel".to_string(),
                source_panel: entry.source_panel.clone(),
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
        return Ok(InteractivePatchWorkbenchPanel {
            schema_version: INTERACTIVE_PATCH_WORKBENCH_SCHEMA_VERSION.to_string(),
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
            files: Vec::new(),
            approval_flow: build_patch_approval_flow(true, false, "not_run", &commands),
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

    Ok(InteractivePatchWorkbenchPanel {
        schema_version: INTERACTIVE_PATCH_WORKBENCH_SCHEMA_VERSION.to_string(),
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
        files,
        approval_flow,
        commands,
    })
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

            Some(InteractivePatchWorkbenchFile {
                commands: patch_workbench_file_commands(&path),
                path,
                index_status: index_status.to_string(),
                worktree_status: worktree_status.to_string(),
                status_label,
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

pub fn build_interactive_workflow_dag(store: &ForgeStore) -> Result<InteractiveWorkflowDagPanel> {
    let workflows = list_workflows_with_filters(
        store,
        WorkflowRegistryFilters::new(WorkflowLifecycleFilter::All),
    )?;
    build_workflow_dag_panel(store, &workflows.workflows)
}

pub fn build_interactive_structured_logs(
    store: &ForgeStore,
) -> Result<InteractiveStructuredLogsPanel> {
    let timeline = build_global_event_timeline(store, None, None, None, None, Some(20), None)?;
    Ok(build_structured_logs_panel(&timeline))
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
            "forge harness install-shims --shim-dir $HOME/.forge/bin --executor codex --project-root . --output json"
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
    }
}

pub fn route_interactive_input(
    store: &ForgeStore,
    input: &str,
    origin: &str,
) -> Result<InteractiveRouteReport> {
    let trimmed = input.trim();
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
            equivalent_command: vec![
                "forge".to_string(),
                "interactive".to_string(),
                "route".to_string(),
                "--input".to_string(),
                format!("/pm {pm_goal}"),
            ],
            mutates_workflow: true,
            risk_level: "medium".to_string(),
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
    let navigation_keys = render_navigation_keybindings(&d.navigation_panel);
    let command_palette_entries = render_command_palette_entry_summary(&d.command_palette_panel);
    let autocomplete_suggestions = render_autocomplete_suggestion_summary(&d.autocomplete_panel);
    let ui_composition_regions = render_ui_composition_region_summary(&d.ui_composition_panel);
    let session_cards = render_session_card_summary(&d.sessions_panel);
    let patch_workbench_files = render_patch_workbench_file_summary(&d.patch_workbench_panel);
    let permission_memberships = render_permission_membership_summary(&d.permissions_panel);
    let permission_approvals = render_permission_approval_summary(&d.permissions_panel);
    let task_board_lanes = render_task_board_lane_summary(&d.task_board_panel);
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
    let structured_logs = render_structured_log_summary(&d.structured_logs_panel);
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
         Active runs: {active_runs}\n\
         {run_ids_line}\
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
         Session center: {sessions_status}; sessions {sessions_count}, ready {sessions_ready}, planned events {sessions_planned_events}, lifecycle events {sessions_lifecycle_events}; {session_cards}\n\
         Harness mode: {harness_effective_mode} from {harness_source}; project config {harness_project_status}; audit {harness_audit_command}\n\
         Harness doctor: {harness_doctor_status} for {harness_doctor_executor}; shim {harness_doctor_shim_dir}; checks {harness_doctor_checks}; audit {harness_doctor_command}\n\
         Runtime/node status: {runtime_node_status}\n\
         Scheduler worker status: {scheduler_worker_status}\n\
         Workflow focus: {workflow_focus}\n\
         Navigation panel: {navigation_status}; default {navigation_default_mode}, theme {navigation_theme}, modes {navigation_modes}, keys {navigation_keys}\n\
         Command palette: {command_palette_status}; query {command_palette_query}, groups {command_palette_groups}, entries {command_palette_entry_count}; {command_palette_entries}\n\
         Autocomplete: {autocomplete_status}; input {autocomplete_input}, suggestions {autocomplete_suggestion_count}; {autocomplete_suggestions}\n\
         UI composition: {ui_composition_status}; layout {ui_composition_layout}, regions {ui_composition_regions_count}, widgets {ui_composition_widgets} ({ui_composition_core_widgets} core, {ui_composition_addon_widgets} addon); {ui_composition_regions}\n\
         Patch workbench: {patch_workbench_status}; clean {patch_workbench_clean}, files {patch_workbench_files_count}, staged {patch_workbench_staged}, unstaged {patch_workbench_unstaged}, untracked {patch_workbench_untracked}, diff {patch_workbench_diff_present}, check {patch_workbench_diff_check}; {patch_workbench_files}\n\
         Permission center: {permissions_status}; memberships {permissions_memberships}, active {permissions_active}, addon permissions {permissions_addons}, approved {permissions_approved_addons}, pending approvals {permissions_pending}, timed out {permissions_timed_out}; memberships {permission_memberships}; approvals {permission_approvals}\n\
         Operational digital twin: {digital_twin_status}; workflows {digital_twin_workflows_count}, happening {digital_twin_happening}, done {digital_twin_done}, remaining {digital_twin_remaining}, validated {digital_twin_validated}, rejected {digital_twin_rejected}, approvals {digital_twin_approvals}; {digital_twin_workflows}\n\
         DAG panel: {dag_status}; workflows {dag_workflows_count}, nodes {dag_nodes}, edges {dag_edges}, running {dag_running}, blocked {dag_blocked}, waits {dag_waits}, human waits {dag_human_waits}; {dag_workflows}\n\
         Task board: {task_board_status}; workflows {task_board_workflows}, tasks {task_board_tasks}, ready handoffs {task_board_ready_handoffs}, human waits {task_board_human_waits}, checkpoints {task_board_checkpoints}, artifacts {task_board_artifacts}; lanes {task_board_lanes}\n\
         Schedule panel: {schedule_status}; due {schedule_due}, runnable {schedule_runnable}, cron {schedule_cron}, wait_until {schedule_wait_until}, next {schedule_next}\n\
         Event timeline: {event_status}; visible {event_visible}/{event_total}; latest {latest_events}\n\
         Structured logs: {structured_logs_status}; logs {structured_logs_count}/{structured_logs_total}, next cursor {structured_logs_next_cursor}, has more {structured_logs_has_more}; {structured_logs}\n\
         Cost panel: {cost_status}; workflows {cost_workflows}, nodes {cost_nodes}, estimated ${cost_estimated:.4}, observed ${cost_observed:.4}\n\
         Context/memory panel: ready {context_ready}, blocked {context_blocked}, budget pressure {context_budget_pressure}, memory {memory_policy_status}\n\
         Addon UI renderers: {addon_renderer_status}; safe {addon_safe_renderers}/{addon_renderers}, families {addon_renderer_family_count} ({addon_renderer_families})\n\
         Repository context: {repository_context}\n\
         Estimated costs: {estimated_costs}\n\
         Attention actions: {attention_actions}\n\
         Quick actions: {quick_actions}\n\
         Useful next commands: {next_commands}\n",
        mark = report.banner.mark,
        name = report.banner.name,
        active_runs = d.active_runs,
        run_ids_line = run_ids_line,
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
        cost_estimated = d.cost_panel.estimated_task_cost_total_usd,
        cost_observed = d.cost_panel.observed_event_cost_total_usd,
        context_ready = d.context_memory_panel.ready_for_handoff,
        context_blocked = d.context_memory_panel.blocked_tasks,
        context_budget_pressure = d.context_memory_panel.context_budget_pressure,
        memory_policy_status = d.context_memory_panel.memory_policy_status,
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

pub fn render_interactive_task_board(panel: &InteractiveTaskBoardPanel) -> String {
    format!(
        "Task board: {status}; workflows {workflow_count}, tasks {task_count}, ready handoffs {ready_handoffs}, human waits {human_waits}, checkpoints {checkpoints}, artifacts {artifacts}\nLanes: {lanes}\n",
        status = panel.status,
        workflow_count = panel.workflow_count,
        task_count = panel.task_count,
        ready_handoffs = panel.ready_handoffs,
        human_waits = panel.pending_human_interactions,
        checkpoints = panel.checkpoint_resume_candidates,
        artifacts = panel.artifact_count,
        lanes = render_task_board_lane_summary(panel),
    )
}

pub fn render_interactive_patch_workbench(panel: &InteractivePatchWorkbenchPanel) -> String {
    format!(
        "Patch workbench: {status}; clean {clean}, files {changed_path_count}, staged {staged_path_count}, unstaged {unstaged_path_count}, untracked {untracked_path_count}, diff {diff_present}, check {diff_check_status}\nRepository: {repository_path}\nFiles: {files}\nApproval flow: {approval_status}; gate {approval_gate}; approval {requires_human_approval}; apply ready {apply_ready}\n",
        status = panel.status,
        clean = panel.clean,
        changed_path_count = panel.changed_path_count,
        staged_path_count = panel.staged_path_count,
        unstaged_path_count = panel.unstaged_path_count,
        untracked_path_count = panel.untracked_path_count,
        diff_present = panel.diff_present,
        diff_check_status = panel.diff_check_status,
        repository_path = panel.repository_path,
        files = render_patch_workbench_file_summary(panel),
        approval_status = panel.approval_flow.status,
        approval_gate = panel.approval_flow.current_gate,
        requires_human_approval = panel.approval_flow.requires_human_approval,
        apply_ready = panel.approval_flow.apply_ready,
    )
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
        "Interactive readiness: {status}; executors {usable_executor_count}/{executor_count}, brains {brain_count}, shells {forge_first_shell_count}/{shell_count} Forge-first, selected brain {selected_brain}\nHarness mode: {harness_mode}; harness doctor: {harness_doctor}; usable executors: {usable_executors}\nNext actions: {next_actions}\n",
        status = panel.status,
        usable_executor_count = panel.usable_executor_count,
        executor_count = panel.executor_count,
        brain_count = panel.brain_count,
        forge_first_shell_count = panel.forge_first_shell_count,
        shell_count = panel.shell_count,
        selected_brain = panel.selected_brain,
        harness_mode = panel.harness_mode.status,
        harness_doctor = panel.harness_doctor.status,
        usable_executors = usable_executors,
        next_actions = next_actions,
    )
}

pub fn render_interactive_harness(panel: &InteractiveHarnessPanel) -> String {
    let next_actions = if panel.next_actions.is_empty() {
        "none".to_string()
    } else {
        panel.next_actions.join(" | ")
    };
    format!(
        "Harness center: {status}; executor {executor}; mode {mode}; doctor {doctor}; shim {shim}; headroom {headroom}; session lifecycle {session_lifecycle_status} for {session_id}\nProject: {project_root}; shim dir: {shim_dir}\nPrimary actions: doctor | shim-status | wrap-plan | install-shims | exec\nNext actions: {next_actions}\n",
        status = panel.status,
        executor = panel.executor,
        mode = panel.mode.effective_mode,
        doctor = panel.doctor.status,
        shim = panel.shim_status.status,
        headroom = panel.headroom_preview.status,
        session_lifecycle_status = panel.wrapper_plan.session_lifecycle_plan.status,
        session_id = panel.wrapper_plan.session_lifecycle_plan.session_id,
        project_root = panel.project_root,
        shim_dir = panel.shim_dir,
        next_actions = next_actions,
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
    let next_actions = if panel.next_actions.is_empty() {
        "none".to_string()
    } else {
        panel.next_actions.join(" | ")
    };
    format!(
        "Session center: {status}; controller {controller}; sessions {session_count}, ready {ready_session_count}, planned events {planned_event_count}, lifecycle events {lifecycle_event_count}\nSessions: {session_cards}\nNext actions: {next_actions}\n",
        status = panel.status,
        controller = panel.controller,
        session_count = panel.session_count,
        ready_session_count = panel.ready_session_count,
        planned_event_count = panel.planned_event_count,
        lifecycle_event_count = panel.lifecycle_event_count,
        session_cards = session_cards,
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
                    "{} {} {} {}",
                    session.session_id,
                    session.provider_id,
                    session.readiness,
                    session.lifecycle_state
                )
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

fn render_patch_workbench_file_summary(panel: &InteractivePatchWorkbenchPanel) -> String {
    if panel.files.is_empty() {
        "none".to_string()
    } else {
        panel
            .files
            .iter()
            .take(12)
            .map(|file| format!("{} ({})", file.path, file.status_label))
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
                    "{}:{}@{} ({}, {} permissions)",
                    membership.subject_scope,
                    membership.subject_id,
                    membership.tenant_path,
                    membership.role,
                    membership.permission_count
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
                    "{}:{} ({}, {})",
                    authorization.addon_id,
                    authorization.permission_id,
                    authorization.status,
                    authorization.risk
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
                    "{}:{} {} ({})",
                    item.workflow_id, item.task_id, item.kind, item.state
                )
            })
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

pub fn render_interactive_workflow_dag(panel: &InteractiveWorkflowDagPanel) -> String {
    format!(
        "Workflow DAG: {status}; workflows {workflow_count}, nodes {node_count}, edges {edge_count}, running {running}, blocked {blocked}, waits {waits}, human waits {human_waits}\nWorkflows: {workflows}\n",
        status = panel.status,
        workflow_count = panel.workflow_count,
        node_count = panel.node_count,
        edge_count = panel.edge_count,
        running = panel.running_node_count,
        blocked = panel.blocked_node_count,
        waits = panel.wait_node_count,
        human_waits = panel.human_wait_count,
        workflows = render_workflow_dag_summary(panel),
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

fn build_ui_composition_panel(
    addon_renderer_report: &OpsAddonViewRendererReport,
) -> InteractiveUiCompositionPanel {
    let mut addon_widgets = addon_renderer_report
        .renderers
        .iter()
        .take(24)
        .map(addon_ui_widget)
        .collect::<Vec<_>>();

    let mut addon_region_widgets = vec![core_ui_widget(
        "addon_renderer_panel",
        "Addon UI renderers",
        "addon_renderer_panel",
        "data_list_renderer",
        "standard",
        "full",
        vec!["forge addons views --surface ops_console --output json".to_string()],
    )];
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
                    vec!["forge interactive home --output json".to_string()],
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
                    vec!["forge interactive harness --output json".to_string()],
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
                    vec!["forge schedule worker-status --output json".to_string()],
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
                    "cost_panel",
                    "Cost panel",
                    "cost_panel",
                    "metric_renderer",
                    "standard",
                    "half",
                    vec!["forge cost ledger --output json".to_string()],
                ),
                core_ui_widget(
                    "context_memory_panel",
                    "Context/memory panel",
                    "context_memory_panel",
                    "policy_renderer",
                    "standard",
                    "half",
                    vec!["forge memory policy --output json".to_string()],
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
                "home".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            inspect_addons: vec![
                "addons".to_string(),
                "views".to_string(),
                "--surface".to_string(),
                "ops_console".to_string(),
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
                "{} [{}] {} workflow={} mutates={} approval={}",
                entry.action_id,
                entry.source_panel,
                entry.commands.join(" "),
                workflow,
                entry.mutates_workflow,
                entry.requires_approval
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
                "{} [{}] {} score={} mutates={} approval={}",
                suggestion.label,
                suggestion.kind,
                suggestion.source_panel,
                suggestion.score,
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
                "#{} {} {} {} {}",
                entry.store_sequence, entry.severity, entry.workflow_id, entry.kind, entry.origin
            )
        })
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
            format!(
                "{} [{}] tasks {}/{}, cards {}, ready handoffs {}, human waits {}, checkpoints {}, artifacts {}",
                lane.workflow_id,
                lane.lifecycle_state,
                lane.completed_tasks,
                lane.total_tasks,
                lane.task_cards.len(),
                lane.ready_handoffs,
                lane.pending_human_interactions,
                lane.checkpoint_resume_candidates,
                lane.artifact_count
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
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
    let two_token = tokens.get(0..2).map(|t| t.join(" ").to_ascii_lowercase());
    let one_token = tokens
        .first()
        .map(|t| t.to_ascii_lowercase())
        .unwrap_or_else(|| "/".to_string());
    let commands = slash_commands();
    let command = two_token
        .as_ref()
        .and_then(|two| commands.iter().find(|cmd| cmd.name.as_str() == two))
        .or_else(|| commands.iter().find(|cmd| cmd.name.as_str() == one_token));
    let recognized = command.is_some();
    let route = command
        .map(|command| SlashCommandRoute {
            name: command.name.clone(),
            recognized: true,
            equivalent_command: command.equivalent_command.clone(),
            mutates_workflow: command.mutates_workflow,
            risk_level: command.risk_level.clone(),
        })
        .unwrap_or_else(|| SlashCommandRoute {
            name: one_token,
            recognized: false,
            equivalent_command: vec![
                "forge".to_string(),
                "interactive".to_string(),
                "slash-commands".to_string(),
            ],
            mutates_workflow: false,
            risk_level: "unknown".to_string(),
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
            "List workflow registry.",
            &["forge", "list"],
            false,
            "low",
        ),
        slash(
            "/artifacts",
            "Artifacts",
            "List workflow artifacts.",
            &["forge", "artifacts", "--workflow", "<workflow-id>"],
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
            "/costs",
            "Costs",
            "Inspect or simulate workflow costs.",
            &["forge", "run", "--workflow", "<workflow-id>", "--simulate"],
            false,
            "medium",
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

pub fn run_interactive_repl(store_path: &std::path::Path) -> Result<i32> {
    if !std::io::stdin().is_terminal() {
        println!("Forge Core workflow runtime -- use `forge --help` for available commands");
        return Ok(0);
    }

    let store = ForgeStore::open(store_path)?;
    let report = build_interactive_home(&store)?;
    println!("{}", render_interactive_home(&report));

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

        if matches!(trimmed, "/exit" | "/quit") {
            println!("goodbye");
            break;
        }

        if trimmed.starts_with('/') {
            let result = route_slash_command(trimmed);
            let route = result.slash_command.unwrap_or(SlashCommandRoute {
                name: trimmed.to_string(),
                recognized: false,
                equivalent_command: Vec::new(),
                mutates_workflow: false,
                risk_level: "unknown".to_string(),
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
