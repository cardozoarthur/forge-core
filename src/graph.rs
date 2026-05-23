use crate::intent::IntentSpec;
use chrono::{DateTime, Utc};
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
    pub cron: String,
    pub timezone: String,
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
pub struct AtomicTask {
    pub id: String,
    pub title: String,
    pub dependencies: Vec<String>,
    pub context_requirements: Vec<String>,
    pub validation_rules: Vec<ValidationRule>,
    pub expected_output: String,
    pub executor: ExecutorKind,
    pub human_required: bool,
    pub schedule: Option<ScheduleSpec>,
    pub cost: CostEstimate,
    pub notification: Option<NotificationSpec>,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub id: String,
    pub kind: String,
    pub path: String,
    pub sha256: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub goal: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub intent: IntentSpec,
    pub tasks: Vec<AtomicTask>,
    pub artifacts: Vec<ArtifactRecord>,
}

pub fn create_workflow(intent: IntentSpec) -> Workflow {
    let id = format!("wf_{}", Uuid::new_v4().to_string().replace('-', ""));
    let tasks = build_tasks(&intent);
    Workflow {
        id,
        goal: intent.goal.clone(),
        status: "pending".to_string(),
        created_at: Utc::now(),
        intent,
        tasks,
        artifacts: Vec::new(),
    }
}

fn rule(kind: &str, expected: &str, command: Option<&str>) -> ValidationRule {
    ValidationRule {
        kind: kind.to_string(),
        expected: expected.to_string(),
        command: command.map(str::to_string),
    }
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
    AtomicTask {
        id: id.to_string(),
        title: title.to_string(),
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
        cost: CostEstimate {
            estimated_cost_usd,
            cost_model: "static_v0_estimate".to_string(),
        },
        notification: None,
        status: TaskStatus::Pending,
    }
}

pub fn build_tasks(intent: &IntentSpec) -> Vec<AtomicTask> {
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
    ];

    if requires_autonomous_extensions(&intent.goal) {
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
            cron: detect_cron(&intent.goal),
            timezone: "UTC".to_string(),
        });
        tasks.push(wait);

        tasks.push(task(
            "task-011",
            "Run deterministic non-AI step",
            &["task-010"],
            &["workflow metrics", "artifact state"],
            vec![rule(
                "deterministic",
                "step does not require live model call",
                None,
            )],
            "Non-AI execution result",
            (ExecutorKind::Command, 0.0002),
        ));

        if let Some(email) = extract_email(&intent.goal) {
            let mut notification = task(
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

    tasks
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
