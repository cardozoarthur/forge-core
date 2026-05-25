use crate::intent::IntentSpec;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Blocked,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRule {
    pub kind: String,
    pub command: Option<String>,
    pub expected: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutorKind {
    Ai,
    Command,
    Wait,
    Notification,
    Mixed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleSpec {
    #[serde(default = "schedule_schema_version")]
    pub schema_version: String,
    #[serde(default = "schedule_kind_cron")]
    pub kind: String,
    pub cron: String,
    pub timezone: String,
    #[serde(default)]
    pub next_run_at: Option<DateTime<Utc>>,
    #[serde(default = "default_missed_run_policy")]
    pub missed_run_policy: String,
    #[serde(default)]
    pub run_history: Vec<ScheduleRunRecord>,
    #[serde(default = "default_scale_to_zero_when_idle")]
    pub scale_to_zero_when_idle: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleRunRecord {
    pub run_id: String,
    pub scheduled_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub status: String,
    pub missed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopSpec {
    #[serde(default = "loop_schema_version")]
    pub schema_version: String,
    pub kind: String,
    #[serde(default)]
    pub items: Vec<String>,
    #[serde(default)]
    pub max_iterations: Option<u32>,
    #[serde(default)]
    pub condition: Option<String>,
    #[serde(default)]
    pub backoff_policy: Option<String>,
    pub subflow_mode: String,
    pub stop_policy: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NativeSubflowSpec {
    #[serde(default = "native_subflow_schema_version")]
    pub schema_version: String,
    pub subflow_id: String,
    pub goal: String,
    pub mode: String,
    pub triggered_by: String,
    pub lineage: SubflowLineageSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubflowLineageSpec {
    pub workflow_id_policy: String,
    pub run_id_policy: String,
    pub artifact_lineage_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEstimate {
    pub estimated_cost_usd: f64,
    pub cost_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationSpec {
    pub channel: String,
    pub to: String,
    pub subject: String,
    pub include_cost_report: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaRoutingSpec {
    pub mode: String,
    pub scope: String,
    pub instruction_source: String,
    pub voice: String,
    pub tone: String,
    pub validation_gate: String,
    pub source_models: Vec<String>,
    pub auditable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeRuntimeSpec {
    pub language: String,
    pub entrypoint: String,
    pub sandbox: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPolicySpec {
    pub mode: String,
    pub ai_allowed: bool,
    pub deterministic: bool,
    pub code_runtime: Option<CodeRuntimeSpec>,
    pub reuse_hint: String,
    pub selection_reason: String,
    pub validation_gate: String,
}

impl Default for ExecutionPolicySpec {
    fn default() -> Self {
        Self {
            mode: "executor_adapter".to_string(),
            ai_allowed: true,
            deterministic: false,
            code_runtime: None,
            reuse_hint: "task_local".to_string(),
            selection_reason: "default executor policy".to_string(),
            validation_gate: "task_validation_rules".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildSubflowRef {
    pub workflow_id: String,
    pub task_id: String,
    pub title: String,
    pub binding_status: String,
    pub lifecycle_state: String,
    pub reuse_key: String,
    pub context_lineage_sha256: String,
    pub validation_gate: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtaskSpec {
    pub id: String,
    pub title: String,
    pub goal: String,
    pub definition_of_done: Vec<String>,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalValidationSpec {
    pub goal: String,
    pub evidence_required: Vec<String>,
    pub definitively_ready: bool,
    pub rework_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsyncPolicy {
    pub mode: String,
    pub resume_strategy: String,
    pub run_substrates: Vec<String>,
}

impl Default for AsyncPolicy {
    fn default() -> Self {
        Self {
            mode: "sync".to_string(),
            resume_strategy: "inline".to_string(),
            run_substrates: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItemSpec {
    pub item_type: String,
    pub backlog_state: String,
    pub priority: String,
    pub owner_role: String,
    pub parent_id: Option<String>,
    pub subtasks: Vec<SubtaskSpec>,
    pub impediments: Vec<String>,
    pub acceptance_criteria: Vec<String>,
    pub goal_validation: GoalValidationSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomicTask {
    pub id: String,
    pub title: String,
    pub goal: String,
    pub dependencies: Vec<String>,
    pub context_requirements: Vec<String>,
    pub validation_rules: Vec<ValidationRule>,
    pub expected_output: String,
    pub executor: ExecutorKind,
    pub human_required: bool,
    pub schedule: Option<ScheduleSpec>,
    #[serde(default)]
    pub loop_control: Option<LoopSpec>,
    #[serde(default)]
    pub native_subflow: Option<NativeSubflowSpec>,
    pub cost: CostEstimate,
    pub notification: Option<NotificationSpec>,
    #[serde(default)]
    pub persona: Option<PersonaRoutingSpec>,
    pub work_item: WorkItemSpec,
    #[serde(default)]
    pub async_policy: AsyncPolicy,
    #[serde(default)]
    pub execution_policy: ExecutionPolicySpec,
    #[serde(default)]
    pub child_subflows: Vec<ChildSubflowRef>,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactLineageRecord {
    #[serde(default = "artifact_lineage_schema_version")]
    pub schema_version: String,
    pub workflow_id: String,
    pub run_id: String,
    pub schedule_task_id: String,
    pub loop_task_id: String,
    pub goal: String,
    pub subflow_id: String,
    pub triggered_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub id: String,
    pub kind: String,
    pub path: String,
    pub sha256: String,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineage: Option<ArtifactLineageRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRevision {
    pub revision: u64,
    pub origin: String,
    pub change_type: String,
    pub summary: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub goal: String,
    #[serde(default)]
    pub initial_goal: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub intent: IntentSpec,
    pub tasks: Vec<AtomicTask>,
    #[serde(default)]
    pub artifacts: Vec<ArtifactRecord>,
    #[serde(default)]
    pub revisions: Vec<WorkflowRevision>,
}

pub fn create_workflow(intent: IntentSpec) -> Workflow {
    let id = format!("wf_{}", Uuid::new_v4().to_string().replace('-', ""));
    let tasks = build_tasks(&intent);
    Workflow {
        id,
        goal: intent.goal.clone(),
        initial_goal: Some(intent.goal.clone()),
        status: "pending".to_string(),
        created_at: Utc::now(),
        intent,
        tasks,
        artifacts: Vec::new(),
        revisions: Vec::new(),
    }
}

fn rule(kind: &str, expected: &str, command: Option<&str>) -> ValidationRule {
    ValidationRule {
        kind: kind.to_string(),
        expected: expected.to_string(),
        command: command.map(str::to_string),
    }
}

fn schedule_schema_version() -> String {
    "forge.schedule.v1".to_string()
}

fn schedule_kind_cron() -> String {
    "cron".to_string()
}

fn default_missed_run_policy() -> String {
    "run_once_then_resume".to_string()
}

fn default_scale_to_zero_when_idle() -> bool {
    true
}

fn loop_schema_version() -> String {
    "forge.loop.v1".to_string()
}

fn native_subflow_schema_version() -> String {
    "forge.native_subflow.v1".to_string()
}

fn artifact_lineage_schema_version() -> String {
    "forge.artifact_lineage.v1".to_string()
}

fn task(
    id: &str,
    title: &str,
    dependencies: &[&str],
    context_requirements: &[&str],
    validation_rules: Vec<ValidationRule>,
    expected_output: &str,
    execution: (ExecutorKind, f64),
) -> AtomicTask {
    let (executor, estimated_cost_usd) = execution;
    let work_item = work_item(id, title, dependencies, &validation_rules);
    let execution_policy = default_execution_policy(&executor);
    AtomicTask {
        id: id.to_string(),
        title: title.to_string(),
        goal: format!("{title}: produce {expected_output}"),
        dependencies: dependencies
            .iter()
            .map(|item| (*item).to_string())
            .collect(),
        context_requirements: context_requirements
            .iter()
            .map(|item| (*item).to_string())
            .collect(),
        validation_rules,
        expected_output: expected_output.to_string(),
        executor,
        human_required: false,
        schedule: None,
        loop_control: None,
        native_subflow: None,
        cost: CostEstimate {
            estimated_cost_usd,
            cost_model: "static_v0_estimate".to_string(),
        },
        notification: None,
        persona: None,
        work_item,
        async_policy: AsyncPolicy::default(),
        execution_policy,
        child_subflows: Vec::new(),
        status: TaskStatus::Pending,
    }
}

fn work_item(
    id: &str,
    title: &str,
    dependencies: &[&str],
    validation_rules: &[ValidationRule],
) -> WorkItemSpec {
    let item_type = if dependencies.is_empty() {
        "capability".to_string()
    } else if title.to_lowercase().contains("validate") {
        "validation_story".to_string()
    } else {
        "execution_story".to_string()
    };
    let parent_id = dependencies
        .first()
        .map(|dependency| (*dependency).to_string());
    let subtasks = vec![
        SubtaskSpec {
            id: format!("{id}-subtask-001"),
            title: "Prepare bounded context".to_string(),
            goal: format!("Gather the minimal context required to complete {title}"),
            definition_of_done: vec![
                "Context references are task-local".to_string(),
                "Context stays within the declared budget".to_string(),
            ],
            status: TaskStatus::Pending,
        },
        SubtaskSpec {
            id: format!("{id}-subtask-002"),
            title: "Execute work".to_string(),
            goal: format!("Produce the expected output for {title}"),
            definition_of_done: vec![
                "Execution trace is recorded".to_string(),
                "Output is persisted or attached to the workflow state".to_string(),
            ],
            status: TaskStatus::Pending,
        },
        SubtaskSpec {
            id: format!("{id}-subtask-003"),
            title: "Validate readiness".to_string(),
            goal: format!("Prove that {title} is definitively ready"),
            definition_of_done: vec![
                "Validation rules pass".to_string(),
                "No unresolved impediment blocks promotion".to_string(),
            ],
            status: TaskStatus::Pending,
        },
    ];
    let impediments = vec![
        "missing executor authorization".to_string(),
        "failed validation gate".to_string(),
        "blocked dependency task".to_string(),
    ];
    let mut acceptance_criteria = validation_rules
        .iter()
        .map(|rule| format!("Validation rules pass for {}: {}", rule.kind, rule.expected))
        .collect::<Vec<_>>();
    acceptance_criteria
        .push("Task output is persisted as replayable operational evidence".to_string());

    WorkItemSpec {
        item_type,
        backlog_state: "ready".to_string(),
        priority: "p1".to_string(),
        owner_role: "forge_runtime".to_string(),
        parent_id,
        subtasks,
        impediments,
        acceptance_criteria,
        goal_validation: GoalValidationSpec {
            goal: format!("{title}: reach a definitively ready state before promotion"),
            evidence_required: vec![
                "completed task status".to_string(),
                "completed subtasks".to_string(),
                "passing validation rules".to_string(),
                "no blocking impediments".to_string(),
            ],
            definitively_ready: false,
            rework_policy:
                "if goal evidence is missing, return the task to work instead of promoting"
                    .to_string(),
        },
    }
}

pub fn build_tasks(intent: &IntentSpec) -> Vec<AtomicTask> {
    let local_code_policy = local_code_execution_policy(&intent.goal);
    let windows_software_policy = windows_software_execution_policy(&intent.goal);
    let autonomous_extensions_required =
        requires_autonomous_extensions(&intent.goal) && !requires_daily_goal_research(&intent.goal);
    let mut tasks = vec![
        task(
            "task-001",
            "Parse intent",
            &[],
            &["human goal", "explicit constraints"],
            vec![rule(
                "schema",
                "intent contains goal, constraints, deliverables, risks and unknowns",
                None,
            )],
            "IntentSpec JSON",
            (ExecutorKind::Command, 0.0001),
        ),
        task(
            "task-002",
            "Extract requirements",
            &["task-001"],
            &["IntentSpec", "product definition"],
            vec![rule(
                "schema",
                "requirements are normalized and measurable",
                None,
            )],
            "Requirement summary artifact",
            (ExecutorKind::Ai, 0.02),
        ),
        task(
            "task-003",
            "Build atomic task graph",
            &["task-002"],
            &["requirements", "validation policy"],
            vec![rule("graph", "DAG has no missing dependencies", None)],
            "Atomic task graph",
            (ExecutorKind::Command, 0.0005),
        ),
        task(
            "task-004",
            "Route minimal context",
            &["task-003"],
            &["task dependencies", "artifact manifest"],
            vec![rule("context", "context is task-local and bounded", None)],
            "Context package",
            (ExecutorKind::Command, 0.0003),
        ),
        task(
            "task-005",
            "Execute isolated task",
            &["task-004"],
            &["context package", "execution target"],
            vec![rule("execution", "trace is recorded and retryable", None)],
            "Execution trace",
            (ExecutorKind::Mixed, 0.01),
        ),
        task(
            "task-006",
            "Validate build",
            &["task-005"],
            &["execution trace", "validation rules"],
            vec![rule(
                "command",
                "cargo test exits successfully when this task produces code",
                Some("cargo test"),
            )],
            "Validation report",
            (ExecutorKind::Command, 0.0005),
        ),
        task(
            "task-007",
            "Integrate artifacts",
            &["task-006"],
            &["validation report", "artifact outputs"],
            vec![rule(
                "artifact",
                "artifact manifest has stable paths and hashes",
                None,
            )],
            "Artifact manifest",
            (ExecutorKind::Command, 0.0004),
        ),
        with_persona(
            task(
                "task-008",
                "Generate documentation",
                &["task-007"],
                &["artifact manifest", "workflow summary"],
                vec![rule(
                    "documentation",
                    "operator can replay the workflow from documented state",
                    None,
                )],
                "Operational report",
                (ExecutorKind::Ai, 0.015),
            ),
            operator_report_persona(),
        ),
    ];

    if requires_n8n_research(&intent.goal) {
        let catalog_id = next_task_id(&tasks);
        let catalog = task(
            &catalog_id,
            "Catalog n8n workflow primitives",
            &["task-002"],
            &[
                "n8n source documentation",
                "n8n node package inventory",
                "workflow primitive taxonomy",
            ],
            vec![rule(
                "research_catalog",
                "catalog covers loop, condition, router, merge, wait, code, execute-subworkflow, trigger, retry, error, transform and human approval patterns",
                None,
            )],
            "n8n node and pattern catalog artifact",
            (ExecutorKind::Ai, 0.018),
        );
        tasks.push(catalog);

        let evaluation_id = next_task_id(&tasks);
        let catalog_dependency = [catalog_id.as_str()];
        let evaluation = task(
            &evaluation_id,
            "Evaluate Forge primitive candidates",
            &catalog_dependency,
            &[
                "n8n research catalog",
                "Forge DAG semantics",
                "context routing requirements",
                "resumability and observability goals",
            ],
            vec![
                rule(
                    "promotion_guard",
                    "only promote concepts that improve validated DAG execution, context routing, resumability, observability or operator clarity",
                    None,
                ),
                rule(
                    "license_guard",
                    "external source code and licenses are not copied blindly into Forge",
                    None,
                ),
            ],
            "Forge primitive promotion recommendation",
            (ExecutorKind::Ai, 0.012),
        );
        tasks.push(evaluation);

        if let Some(graph_task) = tasks.iter_mut().find(|task| task.id == "task-003") {
            if !graph_task.dependencies.contains(&evaluation_id) {
                graph_task.dependencies.push(evaluation_id);
            }
            graph_task
                .context_requirements
                .push("Forge primitive promotion recommendation".to_string());
        }
    }

    if requires_hackathon_factory(&intent.goal) {
        append_hackathon_factory_tasks(&mut tasks);
    }

    if requires_daily_goal_research(&intent.goal) {
        append_daily_goal_research_tasks(&mut tasks, intent);
    }

    if !autonomous_extensions_required {
        let loop_kind = detect_loop_kind(&intent.goal);
        if let Some(kind) = loop_kind {
            tasks.push(loop_node_task("task-009", &["task-003"], kind));
        } else if let Some(policy) = windows_software_policy.clone() {
            tasks.push(windows_software_task("task-009", &["task-003"], policy));
        } else if let Some(policy) = reusable_local_code_policy(local_code_policy.as_ref()) {
            tasks.push(deterministic_non_ai_task(
                "task-009",
                &["task-003"],
                policy.clone(),
            ));
        }

        if let Some(kind) = detect_loop_kind(&intent.goal) {
            let loop_node_id = format!("task-{:03}", tasks.len());
            let loop_id = tasks
                .iter()
                .find(|t| t.title.contains("Run explicit loop"))
                .map(|t| t.id.clone())
                .unwrap_or_else(|| "task-009".to_string());
            let mut infinite_task = task(
                &loop_node_id,
                &format!("Execute {kind} subflow"),
                &[&loop_id],
                &["loop control", "loop items", "subflow mode"],
                vec![rule(
                    "loop_execution",
                    &format!("{kind} subflow is executed with controlled stop/pause/mutate"),
                    None,
                )],
                "Loop subflow execution trace",
                (ExecutorKind::Command, 0.0002),
            );
            infinite_task.loop_control = Some(LoopSpec {
                schema_version: loop_schema_version(),
                kind: kind.clone(),
                items: vec!["goal_item".to_string()],
                max_iterations: match kind.as_str() {
                    "bounded_repeat" | "retry_backoff" => Some(3),
                    _ => None,
                },
                condition: match kind.as_str() {
                    "while_until" => Some("goal_is_definitively_ready".to_string()),
                    _ => None,
                },
                backoff_policy: match kind.as_str() {
                    "retry_backoff" => Some("exponential_2s_30s_max".to_string()),
                    _ => None,
                },
                subflow_mode: if kind == "infinite_recurring_subflow" {
                    "infinite_per_item".to_string()
                } else {
                    "finite_per_item".to_string()
                },
                stop_policy: "pause_mutate_or_stop".to_string(),
                state: "active".to_string(),
            });
            tasks.push(infinite_task);
        }
    }

    if autonomous_extensions_required {
        let mut immediate = task(
            "task-009",
            "Execute immediate workflow action",
            &["task-003"],
            &["goal", "current runtime state"],
            vec![rule(
                "execution",
                "immediate action trace is recorded",
                None,
            )],
            "Immediate action trace",
            (ExecutorKind::Ai, 0.012),
        );
        immediate.human_required = false;
        tasks.push(immediate);

        let mut wait = task(
            "task-010",
            "Wait for scheduled continuation",
            &["task-009"],
            &["schedule", "workflow state"],
            vec![rule("schedule", "cron trigger is persisted", None)],
            "Scheduled wakeup record",
            (ExecutorKind::Wait, 0.0),
        );
        wait.schedule = Some(ScheduleSpec {
            schema_version: schedule_schema_version(),
            kind: schedule_kind_cron(),
            cron: detect_cron(&intent.goal),
            timezone: "UTC".to_string(),
            next_run_at: Some(Utc::now() + Duration::hours(1)),
            missed_run_policy: default_missed_run_policy(),
            run_history: Vec::new(),
            scale_to_zero_when_idle: true,
        });
        tasks.push(wait);

        let deterministic = if let Some(policy) = windows_software_policy {
            windows_software_task("task-011", &["task-010"], policy)
        } else {
            let deterministic_policy = local_code_policy
                .unwrap_or_else(|| default_execution_policy(&ExecutorKind::Command));
            deterministic_non_ai_task("task-011", &["task-010"], deterministic_policy)
        };
        tasks.push(deterministic);

        if let Some(email) = extract_email(&intent.goal) {
            let mut notification = with_persona(
                task(
                    "task-012",
                    "Send workflow cost email",
                    &["task-011"],
                    &["workflow costs", "notification target"],
                    vec![rule(
                        "notification",
                        "email payload includes workflow cost report",
                        None,
                    )],
                    "Email notification payload",
                    (ExecutorKind::Notification, 0.0001),
                ),
                stakeholder_notice_persona(),
            );
            notification.notification = Some(NotificationSpec {
                channel: "email".to_string(),
                to: email,
                subject: "Forge workflow cost report".to_string(),
                include_cost_report: true,
            });
            tasks.push(notification);
        }
    }

    if requires_async_runtime(&intent.goal) {
        for task in &mut tasks {
            task.async_policy = AsyncPolicy {
                mode: "async".to_string(),
                resume_strategy: "event_or_poll".to_string(),
                run_substrates: vec![
                    "docker".to_string(),
                    "kubernetes".to_string(),
                    "knative".to_string(),
                ],
            };
        }
    }

    tasks
}

fn append_hackathon_factory_tasks(tasks: &mut Vec<AtomicTask>) {
    let regulation_id = next_task_id(tasks);
    let regulation = with_persona(
        task(
            &regulation_id,
            "Parse hackathon regulation",
            &["task-001"],
            &[
                "regulation text",
                "eligibility rules",
                "accepted themes",
                "deliverables",
                "judging weights",
                "schedule",
            ],
            vec![rule(
                "regulation_matrix",
                "matrix captures themes, eligibility, team format, required deliverables, final deadline, defense limits and judging weights",
                None,
            )],
            "Hackathon regulation compliance matrix",
            (ExecutorKind::Ai, 0.018),
        ),
        hackathon_operator_persona(),
    );
    tasks.push(regulation);

    let deadline_id = next_task_id(tasks);
    let regulation_dep = [regulation_id.as_str()];
    let deadline = task(
        &deadline_id,
        "Calculate buffered hackathon deadline",
        &regulation_dep,
        &[
            "official final delivery deadline",
            "preferred buffer hours",
            "team organization time",
            "timezone",
        ],
        vec![rule(
            "deadline_buffer",
            "buffered stop is computed before the official submission deadline and remains customizable per run",
            None,
        )],
        "Buffered deadline plan",
        (ExecutorKind::Command, 0.0002),
    );
    tasks.push(deadline);

    let viability_id = next_task_id(tasks);
    let viability_deps = [deadline_id.as_str(), "task-002"];
    let viability = with_persona(
        task(
            &viability_id,
            "Evaluate user idea viability against regulation",
            &viability_deps,
            &[
                "user build request",
                "regulation compliance matrix",
                "judging rubric",
                "technical feasibility constraints",
                "deadline buffer",
            ],
            vec![
                rule(
                    "viability_gate",
                    "decision is viable, viable_with_reframe or not_viable_with_alternative; off-theme ideas must receive a stronger aligned alternative",
                    None,
                ),
                rule(
                    "judging_fit",
                    "evaluation maps the idea to adherence, innovation, technical viability, social/environmental impact, business model and pitch quality",
                    None,
                ),
            ],
            "Idea viability decision",
            (ExecutorKind::Ai, 0.02),
        ),
        hackathon_judge_persona(),
    );
    tasks.push(viability);

    let brainstorm_id = next_task_id(tasks);
    let viability_dep = [viability_id.as_str()];
    let brainstorm = with_persona(
        task(
            &brainstorm_id,
            "Brainstorm and score hackathon MVP concepts",
            &viability_dep,
            &[
                "viability decision",
                "theme fit",
                "OSM/OSRM logistics concept",
                "energy inclusion angle",
                "rubric weights",
            ],
            vec![rule(
                "weighted_scoring",
                "concept scores use the hackathon weights: 10% adherence, 20% innovation, 20% technical viability, 20% social/environmental impact, 10% business/funding and 20% pitch quality",
                None,
            )],
            "Ranked concept shortlist",
            (ExecutorKind::Ai, 0.024),
        ),
        creative_strategy_persona(),
    );
    tasks.push(brainstorm);

    let final_idea_id = next_task_id(tasks);
    let brainstorm_dep = [brainstorm_id.as_str()];
    let final_idea = with_persona(
        task(
            &final_idea_id,
            "Select final idea and MVP scope",
            &brainstorm_dep,
            &[
                "ranked concept shortlist",
                "deadline buffer",
                "team size constraints",
                "MVP complexity limit",
            ],
            vec![rule(
                "mvp_scope",
                "selected idea is regulation-aligned, technically buildable as an MVP and pitchable in five minutes with up to ten slides",
                None,
            )],
            "Final hackathon idea and MVP scope",
            (ExecutorKind::Ai, 0.018),
        ),
        product_strategy_persona(),
    );
    tasks.push(final_idea);

    let pdf_id = next_task_id(tasks);
    let final_idea_dep = [final_idea_id.as_str()];
    let pdf = with_persona(
        task(
            &pdf_id,
            "Generate final idea PDF and explanation artifact",
            &final_idea_dep,
            &[
                "final idea",
                "problem statement",
                "MVP scope",
                "rubric fit",
                "pitch narrative",
            ],
            vec![rule(
                "pdf_artifact",
                "PDF artifact includes idea explanation, regulation fit, MVP scope, technical approach, impact, business model and pitch guidance",
                None,
            )],
            "Final idea PDF artifact",
            (ExecutorKind::Command, 0.0004),
        ),
        stakeholder_notice_persona(),
    );
    tasks.push(pdf);

    let telegram_id = next_task_id(tasks);
    let pdf_dep = [pdf_id.as_str()];
    let mut telegram = with_persona(
        task(
            &telegram_id,
            "Send final idea PDF to Telegram",
            &pdf_dep,
            &["final idea PDF", "explanation artifact", "Telegram delivery target"],
            vec![rule(
                "telegram_delivery",
                "Telegram document payload references the PDF artifact and does not expose bot token or raw chat id",
                None,
            )],
            "Telegram delivery payload",
            (ExecutorKind::Notification, 0.0001),
        ),
        stakeholder_notice_persona(),
    );
    telegram.notification = Some(NotificationSpec {
        channel: "telegram".to_string(),
        to: "configured_telegram_chat".to_string(),
        subject: "Forge Hackathon MVP Factory - final idea PDF".to_string(),
        include_cost_report: false,
    });
    tasks.push(telegram);

    let backlog_id = next_task_id(tasks);
    let backlog_deps = [final_idea_id.as_str(), deadline_id.as_str()];
    let backlog = task(
        &backlog_id,
        "Build hackathon MVP software factory backlog",
        &backlog_deps,
        &[
            "final MVP scope",
            "buffered deadline plan",
            "software factory stages",
            "team coordination needs",
        ],
        vec![rule(
            "backlog",
            "backlog covers problem understanding, brainstorm, artifacts, development, tests, pitch and continuous improvement",
            None,
        )],
        "Hackathon MVP backlog",
        (ExecutorKind::Command, 0.0006),
    );
    tasks.push(backlog);

    let build_plan_id = next_task_id(tasks);
    let backlog_dep = [backlog_id.as_str()];
    let build_plan = task(
        &build_plan_id,
        "Prepare OSM OSRM MVP build plan",
        &backlog_dep,
        &[
            "MVP backlog",
            "OSM map requirements",
            "OSRM routing requirements",
            "emissions and energy saving calculator",
            "collaborative capacity matching model",
        ],
        vec![rule(
            "technical_plan",
            "plan separates demo-safe MVP features from future production integrations and defines testable OSM/OSRM routing assumptions",
            None,
        )],
        "OSM/OSRM MVP technical plan",
        (ExecutorKind::Mixed, 0.012),
    );
    tasks.push(build_plan);

    let test_plan_id = next_task_id(tasks);
    let build_plan_dep = [build_plan_id.as_str()];
    let test_plan = task(
        &test_plan_id,
        "Validate MVP, pitch and judging package",
        &build_plan_dep,
        &[
            "technical plan",
            "pitch story",
            "rubric compliance",
            "test plan",
            "delivery checklist",
        ],
        vec![
            rule(
                "mvp_validation",
                "MVP package has smoke tests, core user flow, no deadline-blocking scope creep and clear demo data",
                None,
            ),
            rule(
                "pitch_validation",
                "pitch package fits five minutes and ten slides while addressing the weighted judging criteria",
                None,
            ),
        ],
        "Validated MVP and pitch package",
        (ExecutorKind::Command, 0.0008),
    );
    tasks.push(test_plan);

    let improvement_id = next_task_id(tasks);
    let test_plan_dep = [test_plan_id.as_str()];
    let mut improvement = task(
        &improvement_id,
        "Run continuous improvement until buffered deadline",
        &test_plan_dep,
        &[
            "validated MVP package",
            "buffered deadline",
            "open impediments",
            "rubric weak spots",
        ],
        vec![rule(
            "improvement_loop",
            "loop stops at the buffered deadline and prioritizes highest scoring rubric gaps before extra features",
            None,
        )],
        "Continuous improvement checkpoint",
        (ExecutorKind::Wait, 0.0),
    );
    improvement.schedule = Some(ScheduleSpec {
        schema_version: schedule_schema_version(),
        kind: schedule_kind_cron(),
        cron: "0 */6 * * *".to_string(),
        timezone: "America/Sao_Paulo".to_string(),
        next_run_at: Some(Utc::now() + Duration::hours(6)),
        missed_run_policy: default_missed_run_policy(),
        run_history: Vec::new(),
        scale_to_zero_when_idle: true,
    });
    tasks.push(improvement);
}

fn append_daily_goal_research_tasks(tasks: &mut Vec<AtomicTask>, intent: &IntentSpec) {
    let goals = daily_goal_research_goals(&intent.goal);
    let timezone = detect_timezone(&intent.goal);
    let cron = detect_daily_goal_cron(&intent.goal);

    let schedule_id = next_task_id(tasks);
    let mut schedule = task(
        &schedule_id,
        "Schedule daily Goal research",
        &["task-003"],
        &[
            "configured Goals",
            "timezone",
            "missed-run policy",
            "durable workflow state",
        ],
        vec![rule(
            "schedule",
            "cron node persists timezone, next_run_at, missed-run policy and run history",
            None,
        )],
        "Durable daily schedule state",
        (ExecutorKind::Wait, 0.0),
    );
    schedule.schedule = Some(ScheduleSpec {
        schema_version: schedule_schema_version(),
        kind: schedule_kind_cron(),
        cron: cron.clone(),
        timezone: timezone.clone(),
        next_run_at: Some(Utc::now() + Duration::days(1)),
        missed_run_policy: default_missed_run_policy(),
        run_history: Vec::new(),
        scale_to_zero_when_idle: true,
    });
    schedule.async_policy = AsyncPolicy {
        mode: "async".to_string(),
        resume_strategy: "schedule_due_or_manual_resume".to_string(),
        run_substrates: Vec::new(),
    };
    tasks.push(schedule);

    let loop_id = next_task_id(tasks);
    let schedule_dep = [schedule_id.as_str()];
    let mut loop_node = task(
        &loop_id,
        "Loop over configured Goals",
        &schedule_dep,
        &[
            "Goal configuration list",
            "per-Goal subflow template",
            "pause/stop/mutate controls",
        ],
        vec![rule(
            "loop_control",
            "loop node explicitly records loop-over-items semantics and controlled stop/pause/mutate behavior",
            None,
        )],
        "Goal iteration plan",
        (ExecutorKind::Command, 0.0001),
    );
    loop_node.loop_control = Some(LoopSpec {
        schema_version: loop_schema_version(),
        kind: "loop_over_items".to_string(),
        items: goals.clone(),
        max_iterations: Some(goals.len() as u32),
        condition: None,
        backoff_policy: None,
        subflow_mode: "finite_per_item".to_string(),
        stop_policy: "pause_mutate_or_stop".to_string(),
        state: "waiting_for_schedule".to_string(),
    });
    tasks.push(loop_node);

    for goal in goals {
        append_goal_research_subflow_tasks(tasks, &goal, &loop_id);
    }
}

fn append_goal_research_subflow_tasks(tasks: &mut Vec<AtomicTask>, goal: &str, loop_id: &str) {
    let subflow = native_goal_research_subflow(goal, loop_id);
    let search_id = next_task_id(tasks);
    let loop_dep = [loop_id];
    let mut search = task(
        &search_id,
        &format!("Search {goal} opportunities with DuckDuckGo"),
        &loop_dep,
        &[
            "DuckDuckGo query plan",
            "upcoming hackathon and marathon keywords",
            "online first-phase eligibility criteria",
        ],
        vec![rule(
            "search",
            "deterministic discovery captures candidate URLs, dates and source snippets without a model call",
            None,
        )],
        "DuckDuckGo discovery results",
        (ExecutorKind::Command, 0.0002),
    );
    search.execution_policy = daily_goal_deterministic_policy("duckduckgo_discovery");
    search.native_subflow = Some(subflow.clone());
    tasks.push(search);

    let inspect_id = next_task_id(tasks);
    let search_dep = [search_id.as_str()];
    let mut inspect = task(
        &inspect_id,
        &format!("Inspect {goal} pages and regulations with Playwright"),
        &search_dep,
        &[
            "candidate URLs",
            "Playwright browser inspection",
            "regulation and eligibility pages",
        ],
        vec![rule(
            "playwright_inspection",
            "page inspection extracts regulation clarity, deadlines, cost and location evidence",
            None,
        )],
        "Inspected regulation evidence",
        (ExecutorKind::Command, 0.0004),
    );
    inspect.execution_policy = daily_goal_deterministic_policy("playwright_regulation_inspection");
    inspect.native_subflow = Some(subflow.clone());
    tasks.push(inspect);

    let evaluation_id = next_task_id(tasks);
    let inspect_dep = [inspect_id.as_str()];
    let mut evaluation = task(
        &evaluation_id,
        &format!("Evaluate {goal} Goal fit"),
        &inspect_dep,
        &[
            "regulation evidence",
            "Pelotas/RS geography",
            "Engineering Production plus ADS fit",
            "cost and first-phase online constraints",
            "user ambitions",
        ],
        vec![rule(
            "goal_fit",
            "judgment step scores eligibility, geography, academic fit, cost, regulation clarity and ambition alignment",
            None,
        )],
        "Goal fit evaluation matrix",
        (ExecutorKind::Ai, 0.012),
    );
    evaluation.native_subflow = Some(subflow.clone());
    tasks.push(evaluation);

    let markdown_id = next_task_id(tasks);
    let evaluation_dep = [evaluation_id.as_str()];
    let mut markdown = task(
        &markdown_id,
        &format!("Generate {goal} Markdown report"),
        &evaluation_dep,
        &[
            "Goal fit evaluation matrix",
            "source evidence",
            "report template",
        ],
        vec![rule(
            "markdown_artifact",
            "Markdown report is structured, source-aware and attached to the parent workflow lineage",
            None,
        )],
        "Structured Markdown Goal report",
        (ExecutorKind::Command, 0.0002),
    );
    markdown.execution_policy = daily_goal_deterministic_policy("markdown_report_generation");
    markdown.native_subflow = Some(subflow.clone());
    tasks.push(markdown);

    let pdf_id = next_task_id(tasks);
    let markdown_dep = [markdown_id.as_str()];
    let mut pdf = task(
        &pdf_id,
        &format!("Generate {goal} PDF report"),
        &markdown_dep,
        &["Markdown report", "PDF renderer", "artifact manifest"],
        vec![rule(
            "pdf_artifact",
            "PDF report is generated from the Markdown report and attached to the same Goal lineage",
            None,
        )],
        "PDF Goal report",
        (ExecutorKind::Command, 0.0002),
    );
    pdf.execution_policy = daily_goal_deterministic_policy("pdf_report_generation");
    pdf.native_subflow = Some(subflow.clone());
    tasks.push(pdf);

    let telegram_id = next_task_id(tasks);
    let pdf_dep = [pdf_id.as_str()];
    let mut telegram = task(
        &telegram_id,
        &format!("Record {goal} Telegram delivery"),
        &pdf_dep,
        &[
            "PDF Goal report",
            "Markdown Goal report",
            "configured Telegram destination",
        ],
        vec![rule(
            "telegram_delivery",
            "Telegram delivery record references report artifacts and redacts raw secrets",
            None,
        )],
        "Telegram delivery record",
        (ExecutorKind::Notification, 0.0001),
    );
    telegram.notification = Some(NotificationSpec {
        channel: "telegram".to_string(),
        to: "configured_telegram_chat_ref".to_string(),
        subject: format!("Daily Goal research report: {goal}"),
        include_cost_report: false,
    });
    telegram.native_subflow = Some(subflow);
    tasks.push(telegram);
}

fn native_goal_research_subflow(goal: &str, loop_id: &str) -> NativeSubflowSpec {
    NativeSubflowSpec {
        schema_version: native_subflow_schema_version(),
        subflow_id: format!("goal_research:{goal}"),
        goal: goal.to_string(),
        mode: "finite".to_string(),
        triggered_by: format!("loop:{loop_id}"),
        lineage: SubflowLineageSpec {
            workflow_id_policy: "inherit_parent_workflow_id".to_string(),
            run_id_policy: "inherit_parent_run_id".to_string(),
            artifact_lineage_policy: "attach_to_parent_run_and_goal".to_string(),
        },
    }
}

fn daily_goal_deterministic_policy(entrypoint: &str) -> ExecutionPolicySpec {
    ExecutionPolicySpec {
        mode: "local_code_node".to_string(),
        ai_allowed: false,
        deterministic: true,
        code_runtime: Some(CodeRuntimeSpec {
            language: "rust".to_string(),
            entrypoint: format!("forge_daily_goal_{entrypoint}"),
            sandbox: "forge_owned_artifacts_no_external_mutation".to_string(),
        }),
        reuse_hint: "reuse_compatible_code_node".to_string(),
        selection_reason:
            "daily Goal research uses deterministic code for stable repeated search/report/PDF/Telegram work"
                .to_string(),
        validation_gate: "daily_goal_deterministic_node_validation_required".to_string(),
    }
}

fn deterministic_non_ai_task(
    id: &str,
    dependencies: &[&str],
    execution_policy: ExecutionPolicySpec,
) -> AtomicTask {
    let mut deterministic = task(
        id,
        "Run deterministic non-AI step",
        dependencies,
        &["workflow metrics", "artifact state"],
        vec![rule(
            "deterministic",
            "step does not require live model call",
            None,
        )],
        "Non-AI execution result",
        (ExecutorKind::Command, 0.0002),
    );
    deterministic.execution_policy = execution_policy;
    if deterministic.execution_policy.mode == "local_code_node" {
        deterministic
            .context_requirements
            .push("local deterministic code-node policy".to_string());
    }
    deterministic
}

fn windows_software_task(
    id: &str,
    dependencies: &[&str],
    execution_policy: ExecutionPolicySpec,
) -> AtomicTask {
    let mut software_task = task(
        id,
        "Run MetaTrader 5 deterministic step",
        dependencies,
        &[
            "MetaTrader 5 terminal",
            "Windows desktop user session",
            "cluster node registry",
            "artifact state",
        ],
        vec![rule(
            "software_runtime",
            "MetaTrader 5 work is placed on a registered Windows node with explicit trust and sandbox permissions",
            None,
        )],
        "Windows software execution result",
        (ExecutorKind::Command, 0.0002),
    );
    software_task.execution_policy = execution_policy;
    software_task
        .context_requirements
        .push("windows software-node policy".to_string());
    software_task
}

fn with_persona(mut task: AtomicTask, persona: PersonaRoutingSpec) -> AtomicTask {
    task.persona = Some(persona);
    task
}

fn operator_report_persona() -> PersonaRoutingSpec {
    persona(
        "operator_report",
        "operational reporter",
        "direct, auditable and evidence-bound",
    )
}

fn stakeholder_notice_persona() -> PersonaRoutingSpec {
    persona(
        "stakeholder_notice",
        "stakeholder communicator",
        "concise, traceable and action-oriented",
    )
}

fn hackathon_operator_persona() -> PersonaRoutingSpec {
    persona(
        "hackathon_operator",
        "regulation-aware hackathon operator",
        "deadline-conscious, practical and evidence-bound",
    )
}

fn hackathon_judge_persona() -> PersonaRoutingSpec {
    persona(
        "hackathon_judge",
        "strict hackathon judge",
        "rubric-weighted, skeptical and constructive",
    )
}

fn creative_strategy_persona() -> PersonaRoutingSpec {
    persona(
        "creative_strategy",
        "creative product strategist",
        "inventive, focused and feasibility-aware",
    )
}

fn product_strategy_persona() -> PersonaRoutingSpec {
    persona(
        "product_strategy",
        "MVP product strategist",
        "clear, scoped and execution-oriented",
    )
}

fn persona(mode: &str, voice: &str, tone: &str) -> PersonaRoutingSpec {
    PersonaRoutingSpec {
        mode: mode.to_string(),
        scope: "node".to_string(),
        instruction_source: "forge_personality_soul_routing_v1".to_string(),
        voice: voice.to_string(),
        tone: tone.to_string(),
        validation_gate: "persona_routing_required".to_string(),
        source_models: vec![
            "codex_developer_personality_instructions".to_string(),
            "paperclip_soul_voice_tone_persona".to_string(),
        ],
        auditable: true,
    }
}

fn default_execution_policy(executor: &ExecutorKind) -> ExecutionPolicySpec {
    match executor {
        ExecutorKind::Ai => ExecutionPolicySpec {
            mode: "model_executor".to_string(),
            ai_allowed: true,
            deterministic: false,
            code_runtime: None,
            reuse_hint: "task_local".to_string(),
            selection_reason: "task requires model reasoning".to_string(),
            validation_gate: "task_validation_rules".to_string(),
        },
        ExecutorKind::Command | ExecutorKind::Wait | ExecutorKind::Notification => {
            ExecutionPolicySpec {
                mode: "deterministic_executor".to_string(),
                ai_allowed: false,
                deterministic: true,
                code_runtime: None,
                reuse_hint: "task_local".to_string(),
                selection_reason: "task can run without a live model call".to_string(),
                validation_gate: "task_validation_rules".to_string(),
            }
        }
        ExecutorKind::Mixed => ExecutionPolicySpec {
            mode: "bounded_mixed_executor".to_string(),
            ai_allowed: true,
            deterministic: false,
            code_runtime: None,
            reuse_hint: "task_local".to_string(),
            selection_reason: "task may combine deterministic work and bounded model reasoning"
                .to_string(),
            validation_gate: "task_validation_rules".to_string(),
        },
    }
}

fn local_code_execution_policy(goal: &str) -> Option<ExecutionPolicySpec> {
    let lower = goal.to_lowercase();
    let runtime = if lower.contains("python") {
        CodeRuntimeSpec {
            language: "python".to_string(),
            entrypoint: "forge_local_python_code_node".to_string(),
            sandbox: "local_process_no_network".to_string(),
        }
    } else if lower.contains("node.js") || lower.contains("node ") || lower.contains("javascript") {
        CodeRuntimeSpec {
            language: "nodejs".to_string(),
            entrypoint: "forge_local_node_code_node".to_string(),
            sandbox: "local_process_no_network".to_string(),
        }
    } else {
        return None;
    };

    let reuse_hint = if lower.contains("repeated")
        || lower.contains("frequent")
        || lower.contains("recurring")
        || lower.contains("frequente")
        || lower.contains("recorrente")
    {
        "reuse_compatible_code_node".to_string()
    } else {
        "task_local_code_node".to_string()
    };

    Some(ExecutionPolicySpec {
        mode: "local_code_node".to_string(),
        ai_allowed: false,
        deterministic: true,
        selection_reason: format!(
            "goal requested {} deterministic work without routing the repeated step through a model",
            runtime.language
        ),
        code_runtime: Some(runtime),
        reuse_hint,
        validation_gate: "deterministic_code_node_validation_required".to_string(),
    })
}

fn windows_software_execution_policy(goal: &str) -> Option<ExecutionPolicySpec> {
    let lower = goal.to_lowercase();
    if !(lower.contains("metatrader 5") || lower.contains("metatrader5") || lower.contains("mt5")) {
        return None;
    }

    let reuse_hint = if lower.contains("repeated")
        || lower.contains("frequent")
        || lower.contains("recurring")
        || lower.contains("frequente")
        || lower.contains("recorrente")
    {
        "reuse_compatible_windows_software_node"
    } else {
        "task_local_windows_software_node"
    };

    Some(ExecutionPolicySpec {
        mode: "windows_software_node".to_string(),
        ai_allowed: false,
        deterministic: true,
        code_runtime: Some(CodeRuntimeSpec {
            language: "metatrader5".to_string(),
            entrypoint: "metatrader5_terminal".to_string(),
            sandbox: "windows_desktop_user_session".to_string(),
        }),
        reuse_hint: reuse_hint.to_string(),
        selection_reason:
            "goal requires MetaTrader 5 on a real Windows desktop session without model execution"
                .to_string(),
        validation_gate: "windows_software_node_validation_required".to_string(),
    })
}

fn reusable_local_code_policy(
    policy: Option<&ExecutionPolicySpec>,
) -> Option<&ExecutionPolicySpec> {
    policy.filter(|policy| policy.reuse_hint == "reuse_compatible_code_node")
}

fn requires_n8n_research(goal: &str) -> bool {
    goal.to_lowercase().contains("n8n")
}

fn requires_hackathon_factory(goal: &str) -> bool {
    let lower = goal.to_lowercase();
    (lower.contains("hackathon") || lower.contains("ideathon") || lower.contains("maratona"))
        && (lower.contains("mvp")
            || lower.contains("software factory")
            || lower.contains("fábrica")
            || lower.contains("factory"))
}

fn requires_daily_goal_research(goal: &str) -> bool {
    let lower = goal.to_lowercase();
    (lower.contains("daily goal research")
        || lower.contains("daily goal")
        || lower.contains("goal research workflow"))
        && (lower.contains("goal") || lower.contains("goals"))
}

fn daily_goal_research_goals(goal: &str) -> Vec<String> {
    let lower = goal.to_lowercase();
    let mut goals = Vec::new();

    if let Some(goals_start) = lower.find("goals:") {
        let after = &lower[goals_start + "goals:".len()..];
        let goals_str = after.split(['.', ';']).next().unwrap_or(after);
        let in_pos = goals_str.find(" in ");
        let cron_pos = goals_str.find(" cron ");
        let timezone_pos = goals_str.find(" timezone ");
        let end = in_pos
            .or(cron_pos)
            .or(timezone_pos)
            .unwrap_or(goals_str.len());
        for part in goals_str[..end].split(',') {
            let trimmed = part.trim().trim_matches(|c: char| c == ',' || c == ' ');
            if !trimmed.is_empty() {
                goals.push(trimmed.to_string());
            }
        }
    }

    if goals.is_empty() && (lower.contains("hackathon") || lower.contains("maratona")) {
        goals.push("hackathon".to_string());
    }

    if goals.is_empty() {
        goals.push("hackathon".to_string());
    }
    goals
}

fn detect_timezone(goal: &str) -> String {
    goal.split_whitespace()
        .map(|token| token.trim_matches(|ch: char| ch == ',' || ch == ';' || ch == '.'))
        .find(|token| token.contains('/') && token.chars().any(|ch| ch == '_'))
        .unwrap_or("UTC")
        .to_string()
}

fn detect_daily_goal_cron(goal: &str) -> String {
    let cron = detect_cron(goal);
    if cron == "0 * * * *" {
        "0 8 * * *".to_string()
    } else {
        cron
    }
}

fn next_task_id(tasks: &[AtomicTask]) -> String {
    format!("task-{:03}", tasks.len() + 1)
}

fn requires_async_runtime(goal: &str) -> bool {
    let lower = goal.to_lowercase();
    lower.contains("async")
        || lower.contains("assíncron")
        || lower.contains("asynchronous")
        || lower.contains("docker")
        || lower.contains("kubernetes")
        || lower.contains("knative")
}

fn requires_autonomous_extensions(goal: &str) -> bool {
    let lower = goal.to_lowercase();
    lower.contains("cron")
        || lower.contains("friday")
        || lower.contains("sexta")
        || lower.contains("email")
        || lower.contains("without ai")
        || lower.contains("sem ia")
        || lower.contains("não dependa de ia")
}

fn detect_cron(goal: &str) -> String {
    let tokens: Vec<&str> = goal.split_whitespace().collect();
    if let Some(index) = tokens
        .iter()
        .position(|token| token.eq_ignore_ascii_case("cron"))
    {
        if tokens.len() >= index + 6 {
            return tokens[index + 1..index + 6].join(" ");
        }
    }

    let lower = goal.to_lowercase();
    if lower.contains("friday") || lower.contains("sexta") {
        return "0 9 * * 5".to_string();
    }
    "0 * * * *".to_string()
}

fn extract_email(goal: &str) -> Option<String> {
    goal.split_whitespace()
        .map(|token| {
            token.trim_matches(|char: char| {
                !char.is_ascii_alphanumeric()
                    && char != '@'
                    && char != '.'
                    && char != '_'
                    && char != '-'
            })
        })
        .find(|token| token.contains('@') && token.contains('.'))
        .map(str::to_string)
}

fn detect_loop_kind(goal: &str) -> Option<String> {
    let lower = goal.to_lowercase();
    if lower.contains("loop over items")
        || lower.contains("loop-over-items")
        || lower.contains("loop_over_items")
    {
        Some("loop_over_items".to_string())
    } else if lower.contains("bounded repeat")
        || lower.contains("bounded_repeat")
        || lower.contains("bounded-repeat")
    {
        Some("bounded_repeat".to_string())
    } else if lower.contains("retry backoff")
        || lower.contains("retry_backoff")
        || lower.contains("retry-backoff")
        || lower.contains("retry with backoff")
    {
        Some("retry_backoff".to_string())
    } else if lower.contains("while condition")
        || lower.contains("while_until")
        || lower.contains("while/until")
        || lower.contains("while-until")
        || lower.contains("condition loop")
    {
        Some("while_until".to_string())
    } else if lower.contains("infinite recurring")
        || lower.contains("infinite_recurring_subflow")
        || lower.contains("infinite-recurring")
        || lower.contains("recurring subflow")
    {
        Some("infinite_recurring_subflow".to_string())
    } else {
        None
    }
}

fn loop_node_task(id: &str, dependencies: &[&str], kind: String) -> AtomicTask {
    let title = format!("Run explicit {kind} loop");
    let mut t = task(
        id,
        &title,
        dependencies,
        &["loop control", "loop items"],
        vec![rule(
            "loop_kind",
            &format!("loop node is type {kind} with controlled stop/pause/mutate"),
            None,
        )],
        "Loop node execution trace",
        (ExecutorKind::Command, 0.0002),
    );
    t.loop_control = Some(LoopSpec {
        schema_version: loop_schema_version(),
        kind,
        items: Vec::new(),
        max_iterations: None,
        condition: None,
        backoff_policy: None,
        subflow_mode: "finite_per_item".to_string(),
        stop_policy: "pause_mutate_or_stop".to_string(),
        state: "active".to_string(),
    });
    t
}
