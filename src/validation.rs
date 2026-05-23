use crate::graph::{TaskStatus, Workflow};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct FailedRule {
    pub task_id: String,
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationReport {
    pub workflow_id: String,
    pub status: String,
    pub promotable: bool,
    pub failed_rules: Vec<FailedRule>,
    pub rework_tasks: Vec<ReworkTask>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReworkTask {
    pub task_id: String,
    pub goal: String,
    pub reason: String,
}

pub fn validate_workflow(workflow: &Workflow) -> ValidationReport {
    let mut failed_rules = Vec::new();
    let mut rework_tasks = Vec::new();

    for task in &workflow.tasks {
        for dependency in &task.dependencies {
            if !workflow
                .tasks
                .iter()
                .any(|candidate| &candidate.id == dependency)
            {
                failed_rules.push(FailedRule {
                    task_id: task.id.clone(),
                    kind: "graph".to_string(),
                    message: format!("missing dependency {dependency}"),
                });
            }
        }
        if task.status != TaskStatus::Completed {
            failed_rules.push(FailedRule {
                task_id: task.id.clone(),
                kind: "task_status".to_string(),
                message: format!("task {} is {:?}", task.id, task.status),
            });
        }
        if !task.work_item.goal_validation.definitively_ready {
            failed_rules.push(FailedRule {
                task_id: task.id.clone(),
                kind: "goal_readiness".to_string(),
                message: format!("task {} goal is not definitively ready", task.id),
            });
            rework_tasks.push(ReworkTask {
                task_id: task.id.clone(),
                goal: task.goal.clone(),
                reason: "goal evidence is missing or not definitively ready; return to work"
                    .to_string(),
            });
        }
    }

    let promotable = failed_rules.is_empty();
    ValidationReport {
        workflow_id: workflow.id.clone(),
        status: if promotable { "passed" } else { "blocked" }.to_string(),
        promotable,
        failed_rules,
        rework_tasks,
    }
}
