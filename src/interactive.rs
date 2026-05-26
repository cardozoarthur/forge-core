use crate::executor::load_executors;
use crate::registry::{
    list_workflows_with_filters, WorkflowLifecycleFilter, WorkflowRegistryFilters,
};
use crate::request::start_async_request;
use crate::runtime::load_runtimes;
use crate::schedule::build_schedule_worker_status;
use crate::storage::ForgeStore;
use anyhow::Result;
use serde::Serialize;
use std::env;

const INTERACTIVE_HOME_SCHEMA_VERSION: &str = "forge.interactive.home.v1";
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
    pub scheduled_workflows: usize,
    pub looping_workflows: usize,
    pub paused_idle_workflows: usize,
    pub recent_artifacts: usize,
    pub pending_approvals: usize,
    pub validation_failures: usize,
    pub executor_availability: String,
    pub runtime_node_status: String,
    pub repository_context: String,
    pub estimated_costs: String,
    pub scheduler_worker_status: String,
    pub useful_next_commands: Vec<String>,
    pub quick_actions: Vec<String>,
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

    let active_runs = requests
        .runs
        .iter()
        .filter(|run| run.activity.active || matches!(run.status.as_str(), "accepted" | "resumed"))
        .count();
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
    let runtime_node_status = if runtimes.usable.is_empty() {
        "no allowed async run substrates".to_string()
    } else {
        format!("usable runtimes: {}", runtimes.usable.join(", "))
    };

    let scheduler_worker_status = build_schedule_worker_status(store, "forge-scheduler", 1, 300)
        .ok()
        .map(|ws| {
            let s = ws.summary;
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

    Ok(InteractiveHomeReport {
        status: "interactive_home_ready".to_string(),
        schema_version: INTERACTIVE_HOME_SCHEMA_VERSION.to_string(),
        banner: InteractiveBanner {
            mark: anvil_mark().to_string(),
            name: "forge".to_string(),
        },
        dashboard: InteractiveDashboard {
            active_runs,
            scheduled_workflows,
            looping_workflows,
            paused_idle_workflows: workflows.summary.non_running,
            recent_artifacts,
            pending_approvals,
            validation_failures,
            executor_availability,
            runtime_node_status,
            repository_context: env::current_dir()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
            estimated_costs: "available per workflow via /costs or forge run --simulate"
                .to_string(),
            scheduler_worker_status,
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
                "/sync".to_string(),
                "/validate".to_string(),
                "/logs".to_string(),
                "/workers".to_string(),
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

pub fn route_interactive_input(
    store: &ForgeStore,
    input: &str,
    origin: &str,
) -> Result<InteractiveRouteReport> {
    let trimmed = input.trim();
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
        retention_decision,
    })
}

pub fn render_interactive_home(report: &InteractiveHomeReport) -> String {
    let d = &report.dashboard;
    let quick_actions = d.quick_actions.join(" ");
    let next_commands = d.useful_next_commands.join(" | ");
    format!(
        "{mark}\n{name}\n\n\
         Active runs: {active_runs}\n\
         Scheduled workflows: {scheduled_workflows}\n\
         Looping workflows: {looping_workflows}\n\
         Paused/idle workflows: {paused_idle_workflows}\n\
         Recent artifacts: {recent_artifacts}\n\
         Pending approvals: {pending_approvals}\n\
         Validation failures: {validation_failures}\n\
         Executor availability: {executor_availability}\n\
         Runtime/node status: {runtime_node_status}\n\
         Scheduler worker status: {scheduler_worker_status}\n\
         Repository context: {repository_context}\n\
         Estimated costs: {estimated_costs}\n\
         Quick actions: {quick_actions}\n\
         Useful next commands: {next_commands}\n",
        mark = report.banner.mark,
        name = report.banner.name,
        active_runs = d.active_runs,
        scheduled_workflows = d.scheduled_workflows,
        looping_workflows = d.looping_workflows,
        paused_idle_workflows = d.paused_idle_workflows,
        recent_artifacts = d.recent_artifacts,
        pending_approvals = d.pending_approvals,
        validation_failures = d.validation_failures,
        executor_availability = d.executor_availability,
        runtime_node_status = d.runtime_node_status,
        scheduler_worker_status = d.scheduler_worker_status,
        repository_context = d.repository_context,
        estimated_costs = d.estimated_costs,
        quick_actions = quick_actions,
        next_commands = next_commands,
    )
}

fn route_slash_command(trimmed: &str) -> InteractiveRouteReport {
    let name = trimmed
        .split_whitespace()
        .next()
        .unwrap_or("/")
        .to_ascii_lowercase();
    let command = slash_commands()
        .into_iter()
        .find(|command| command.name == name);
    let recognized = command.is_some();
    let route = command
        .map(|command| SlashCommandRoute {
            name: command.name,
            recognized: true,
            equivalent_command: command.equivalent_command,
            mutates_workflow: command.mutates_workflow,
            risk_level: command.risk_level,
        })
        .unwrap_or_else(|| SlashCommandRoute {
            name,
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

fn requires_workflow(lower: &str) -> bool {
    [
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
    ]
    .iter()
    .any(|needle| lower.contains(needle))
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
    "      _________\n  ___/         \\___\n |_______________|\n       |  |\n       |__|"
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
    fn route_slash_command_reports_unknown_commands() {
        let report = route_slash_command("/nonexistent");
        assert_eq!(report.input_kind, "slash_command");
        let route = report.slash_command.unwrap();
        assert_eq!(route.name, "/nonexistent");
        assert!(!route.recognized);
        assert_eq!(route.risk_level, "unknown");
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
}
