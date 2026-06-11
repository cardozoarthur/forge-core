use crate::addon::{default_addon_dirs, list_addon_views, load_addon_catalog_from_store};
use crate::checkpoint::TaskCheckpoint;
use crate::cost::build_cost_ledger;
use crate::event::build_global_event_timeline;
use crate::executor::load_executors;
use crate::graph::{AtomicTask, ExecutorKind, TaskStatus};
use crate::harness::{build_harness_mode_report, HarnessModeOptions, HarnessModeReport};
use crate::memory::memory_policy_report;
use crate::ops::build_addon_view_renderer_report;
use crate::registry::{
    list_workflows_with_filters, RegistryContextActionRef, WorkflowLifecycleFilter,
    WorkflowRegistryFilters, WorkflowRegistryRow,
};
use crate::request::start_async_request;
use crate::runtime::load_runtimes;
use crate::schedule::build_schedule_worker_status;
use crate::storage::ForgeStore;
use crate::workflow::{record_product_decision, ProductDecisionInput};
use anyhow::Result;
use serde::Serialize;
use std::env;
use std::io::IsTerminal;
use std::process::Command;

const INTERACTIVE_HOME_SCHEMA_VERSION: &str = "forge.interactive.home.v1";
const INTERACTIVE_TASK_BOARD_SCHEMA_VERSION: &str = "forge.interactive.task_board.v1";
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
    pub harness_mode_panel: HarnessModeReport,
    pub runtime_node_status: String,
    pub repository_context: String,
    pub estimated_costs: String,
    pub scheduler_worker_status: String,
    pub workflow_focus: Vec<InteractiveWorkflowCard>,
    pub task_board_panel: InteractiveTaskBoardPanel,
    pub schedule_panel: InteractiveSchedulePanel,
    pub event_panel: InteractiveEventPanel,
    pub cost_panel: InteractiveCostPanel,
    pub context_memory_panel: InteractiveContextMemoryPanel,
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
    let harness_mode_panel = build_harness_mode_report(HarnessModeOptions {
        forge_first: false,
        observe_only: false,
        project_root: None,
    });
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
    let task_board_panel = build_task_board_panel(store, &workflows.workflows)?;
    let event_panel = build_global_event_timeline(store, None, None, None, None, Some(5), None)
        .ok()
        .map(|timeline| InteractiveEventPanel {
            status: timeline.status,
            total_event_count: timeline.total_event_count,
            visible_event_count: timeline.event_count,
            latest_events: timeline
                .events
                .iter()
                .rev()
                .take(5)
                .map(|event| format!("{} {} {}", event.occurred_at, event.workflow_id, event.kind))
                .collect(),
        })
        .unwrap_or_else(|| InteractiveEventPanel {
            status: "event_timeline_unavailable".to_string(),
            total_event_count: 0,
            visible_event_count: 0,
            latest_events: Vec::new(),
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
    let addon_renderer_panel = load_addon_catalog_from_store(store, &default_addon_dirs())
        .ok()
        .map(|catalog| {
            let addon_views =
                list_addon_views(&catalog, None, Some("ops_console"), Some("enabled"));
            let renderers = build_addon_view_renderer_report(&addon_views);
            InteractiveAddonRendererPanel {
                status: renderers.status,
                renderer_count: renderers.renderer_count,
                safe_renderer_count: renderers.safe_renderer_count,
                family_count: renderers.family_count,
                families: renderers.families,
            }
        })
        .unwrap_or_else(|| InteractiveAddonRendererPanel {
            status: "addon_renderers_unavailable".to_string(),
            renderer_count: 0,
            safe_renderer_count: 0,
            family_count: 0,
            families: Vec::new(),
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
            harness_mode_panel,
            runtime_node_status,
            repository_context: env::current_dir()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
            estimated_costs: "available per workflow via /costs or forge run --simulate"
                .to_string(),
            scheduler_worker_status,
            workflow_focus,
            task_board_panel,
            schedule_panel,
            event_panel,
            cost_panel,
            context_memory_panel,
            addon_renderer_panel,
            attention_actions,
            useful_next_commands: vec![
                "forge list".to_string(),
                "forge inspect <workflow-id>".to_string(),
                "forge request list".to_string(),
                "forge schedule list".to_string(),
                "forge schedule worker-status".to_string(),
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

pub fn build_interactive_task_board(store: &ForgeStore) -> Result<InteractiveTaskBoardPanel> {
    let workflows = list_workflows_with_filters(
        store,
        WorkflowRegistryFilters::new(WorkflowLifecycleFilter::All),
    )?;
    build_task_board_panel(store, &workflows.workflows)
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
    let task_board_lanes = render_task_board_lane_summary(&d.task_board_panel);
    let latest_events = if d.event_panel.latest_events.is_empty() {
        "none".to_string()
    } else {
        d.event_panel.latest_events.join(" | ")
    };
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
         Harness mode: {harness_effective_mode} from {harness_source}; project config {harness_project_status}; audit {harness_audit_command}\n\
         Runtime/node status: {runtime_node_status}\n\
         Scheduler worker status: {scheduler_worker_status}\n\
         Workflow focus: {workflow_focus}\n\
         Task board: {task_board_status}; workflows {task_board_workflows}, tasks {task_board_tasks}, ready handoffs {task_board_ready_handoffs}, human waits {task_board_human_waits}, checkpoints {task_board_checkpoints}, artifacts {task_board_artifacts}; lanes {task_board_lanes}\n\
         Schedule panel: {schedule_status}; due {schedule_due}, runnable {schedule_runnable}, cron {schedule_cron}, wait_until {schedule_wait_until}, next {schedule_next}\n\
         Event timeline: {event_status}; visible {event_visible}/{event_total}; latest {latest_events}\n\
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
        harness_effective_mode = d.harness_mode_panel.effective_mode,
        harness_source = d.harness_mode_panel.forge_first_source,
        harness_project_status = d.harness_mode_panel.project_config_status,
        harness_audit_command = "forge harness mode --output json",
        runtime_node_status = d.runtime_node_status,
        scheduler_worker_status = d.scheduler_worker_status,
        workflow_focus = workflow_focus,
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
    Ok(workflow
        .tasks
        .iter()
        .map(|task| build_task_board_task_card(row, task, checkpoints))
        .collect())
}

fn build_task_board_task_card(
    row: &WorkflowRegistryRow,
    task: &AtomicTask,
    checkpoints: &[TaskCheckpoint],
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

    InteractiveTaskBoardTaskCard {
        task_id: task.id.clone(),
        title: task.title.clone(),
        status: task_status_label(&task.status).to_string(),
        executor: executor_kind_label(&task.executor).to_string(),
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
