use crate::graph::{TaskStatus, Workflow};
use crate::interaction::{blocking_human_interaction, HumanInteractionBlocker};
use crate::scheduler::{plan_parallel_execution, ParallelSchedulePlan};
use chrono::Utc;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::thread;

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_interaction: Option<HumanInteractionBlocker>,
    #[serde(default)]
    pub concurrent_wave_count: usize,
    pub max_concurrent_tasks: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ConcurrentWaveReport {
    pub wave_index: usize,
    pub task_count: usize,
    pub completed_count: usize,
    pub failed_count: usize,
    pub execution_order: Vec<String>,
}

pub fn run_simulated(workflow: &mut Workflow) -> ExecutionReport {
    if let Some(blocker) = blocking_human_interaction(workflow) {
        workflow.status = "blocked".to_string();
        if let Some(task) = workflow
            .tasks
            .iter_mut()
            .find(|task| task.id == blocker.task_id)
        {
            task.status = TaskStatus::Blocked;
            task.work_item.backlog_state = "blocked_on_human_interaction".to_string();
        }
        return ExecutionReport {
            workflow_id: workflow.id.clone(),
            status: "blocked_on_human_interaction".to_string(),
            mode: "simulate".to_string(),
            completed_tasks: 0,
            cost_report: CostReport {
                total_estimated_cost_usd: 0.0,
                by_task: Vec::new(),
            },
            notifications: Vec::new(),
            trace: vec![format!(
                "{} blocked on human interaction {}",
                blocker.task_id, blocker.interaction_id
            )],
            daily_goal_research: None,
            parallel_plan: Some(plan_parallel_execution(workflow)),
            blocked_interaction: Some(blocker),
            concurrent_wave_count: 0,
            max_concurrent_tasks: 0,
        };
    }

    run_simulated_parallel(workflow)
}

fn run_simulated_parallel(workflow: &mut Workflow) -> ExecutionReport {
    let parallel_plan = plan_parallel_execution(workflow);
    let total_tasks = workflow.tasks.len();

    if total_tasks == 0 {
        workflow.status = "completed".to_string();
        return ExecutionReport {
            workflow_id: workflow.id.clone(),
            status: "completed".to_string(),
            mode: "simulate_parallel".to_string(),
            completed_tasks: 0,
            cost_report: CostReport {
                total_estimated_cost_usd: 0.0,
                by_task: Vec::new(),
            },
            notifications: Vec::new(),
            trace: Vec::new(),
            daily_goal_research: None,
            parallel_plan: Some(parallel_plan),
            blocked_interaction: None,
            concurrent_wave_count: 0,
            max_concurrent_tasks: 0,
        };
    }

    let completed = Arc::new(Mutex::new(BTreeSet::<String>::new()));
    let cancelled = Arc::new(Mutex::new(false));
    let max_concurrent = Arc::new(Mutex::new(0usize));

    for wave in &parallel_plan.waves {
        let c = cancelled.lock().unwrap();
        if *c {
            break;
        }
        drop(c);

        let wave_ids: Vec<String> = wave.task_ids.clone();
        let mut handles = Vec::new();
        let wave_concurrent = wave_ids.len();

        {
            let mut mc = max_concurrent.lock().unwrap();
            if wave_concurrent > *mc {
                *mc = wave_concurrent;
            }
        }

        for task_id in &wave_ids {
            let task_id = task_id.clone();
            let completed = Arc::clone(&completed);
            let cancelled = Arc::clone(&cancelled);

            let handle = thread::spawn(move || {
                let c = cancelled.lock().unwrap();
                if *c {
                    return;
                }
                drop(c);

                let mut comp = completed.lock().unwrap();
                comp.insert(task_id);
            });

            handles.push(handle);
        }

        for handle in handles {
            if handle.join().is_err() {
                let mut c = cancelled.lock().unwrap();
                *c = true;
            }
        }
    }

    let max_conc = *max_concurrent.lock().unwrap();
    let completed_set = completed.lock().unwrap().clone();
    let wave_count = parallel_plan.waves.len();
    let all_cancelled = *cancelled.lock().unwrap();

    let mut by_task = Vec::new();
    let mut trace = Vec::new();
    let mut notifications_final = Vec::new();
    let mut total_estimated_cost_usd = 0.0;

    for task in &mut workflow.tasks {
        if completed_set.contains(&task.id) {
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
            total_estimated_cost_usd += task.cost.estimated_cost_usd;
            trace.push(format!(
                "{} completed at {}",
                task.id,
                Utc::now().to_rfc3339()
            ));
        } else if all_cancelled {
            task.status = TaskStatus::Blocked;
            task.work_item.backlog_state = "blocked_cancelled".to_string();
        }

        if let Some(notification) = &task.notification {
            if completed_set.contains(&task.id) {
                notifications_final.push(NotificationDelivery {
                    task_id: task.id.clone(),
                    channel: notification.channel.clone(),
                    to: notification.to.clone(),
                    subject: notification.subject.clone(),
                    body: format!(
                        "Forge workflow {} completed. total_estimated_cost_usd={:.6}",
                        workflow.id, total_estimated_cost_usd
                    ),
                    simulated: true,
                });
            }
        }
    }

    let all_completed = completed_set.len() == total_tasks;
    workflow.status = if all_completed {
        "completed".to_string()
    } else {
        "cancelled".to_string()
    };

    ExecutionReport {
        workflow_id: workflow.id.clone(),
        status: workflow.status.clone(),
        mode: "simulate_parallel".to_string(),
        completed_tasks: completed_set.len(),
        cost_report: CostReport {
            total_estimated_cost_usd,
            by_task,
        },
        notifications: notifications_final,
        trace,
        daily_goal_research: None,
        parallel_plan: Some(parallel_plan),
        blocked_interaction: None,
        concurrent_wave_count: wave_count,
        max_concurrent_tasks: max_conc,
    }
}
