use crate::graph::{TaskStatus, Workflow};
use crate::scheduler::{plan_parallel_execution, ParallelSchedulePlan};
use chrono::Utc;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct TaskCost {
    pub task_id: String,
    pub title: String,
    pub executor: String,
    pub estimated_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CostReport {
    pub total_estimated_cost_usd: f64,
    pub by_task: Vec<TaskCost>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NotificationDelivery {
    pub task_id: String,
    pub channel: String,
    pub to: String,
    pub subject: String,
    pub body: String,
    pub simulated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutionReport {
    pub workflow_id: String,
    pub status: String,
    pub mode: String,
    pub completed_tasks: usize,
    pub cost_report: CostReport,
    pub notifications: Vec<NotificationDelivery>,
    pub trace: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub daily_goal_research: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_plan: Option<ParallelSchedulePlan>,
}

pub fn run_simulated(workflow: &mut Workflow) -> ExecutionReport {
    let mut trace = Vec::new();
    let mut by_task = Vec::new();
    let mut notifications = Vec::new();

    for task in &mut workflow.tasks {
        task.status = TaskStatus::Completed;
        task.work_item.backlog_state = "done".to_string();
        task.work_item.goal_validation.definitively_ready = true;
        for subtask in &mut task.work_item.subtasks {
            subtask.status = TaskStatus::Completed;
        }
        by_task.push(TaskCost {
            task_id: task.id.clone(),
            title: task.title.clone(),
            executor: serde_json::to_value(&task.executor)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_else(|| "unknown".to_string()),
            estimated_cost_usd: task.cost.estimated_cost_usd,
        });
        trace.push(format!(
            "{} completed at {}",
            task.id,
            Utc::now().to_rfc3339()
        ));

        if let Some(notification) = &task.notification {
            let total: f64 = by_task.iter().map(|cost| cost.estimated_cost_usd).sum();
            let body = format!(
                "Forge workflow {} completed. total_estimated_cost_usd={:.6}",
                workflow.id, total
            );
            notifications.push(NotificationDelivery {
                task_id: task.id.clone(),
                channel: notification.channel.clone(),
                to: notification.to.clone(),
                subject: notification.subject.clone(),
                body,
                simulated: true,
            });
        }
    }
    workflow.status = "completed".to_string();
    let total_estimated_cost_usd = by_task.iter().map(|cost| cost.estimated_cost_usd).sum();

    let parallel_plan = Some(plan_parallel_execution(workflow));

    ExecutionReport {
        workflow_id: workflow.id.clone(),
        status: "completed".to_string(),
        mode: "simulate".to_string(),
        completed_tasks: workflow.tasks.len(),
        cost_report: CostReport {
            total_estimated_cost_usd,
            by_task,
        },
        notifications,
        trace,
        daily_goal_research: None,
        parallel_plan,
    }
}
