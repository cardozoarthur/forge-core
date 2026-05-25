use crate::graph::Workflow;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize)]
pub struct ScheduleWave {
    pub level: usize,
    pub task_ids: Vec<String>,
    pub task_titles: Vec<String>,
    pub estimated_cost_usd: f64,
    pub task_count: usize,
    pub concurrent: bool,
    pub max_task_cost: f64,
    pub parallel_duration_estimate: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParallelSchedulePlan {
    pub schema_version: String,
    pub workflow_id: String,
    pub status: String,
    pub total_tasks: usize,
    pub total_waves: usize,
    pub theoretical_min_waves: usize,
    pub sequential_cost_usd: f64,
    pub parallel_cost_usd: f64,
    pub sequential_duration_estimate: f64,
    pub parallel_duration_estimate: f64,
    pub latency_reduction_bps: u32,
    pub parallel_opportunity: bool,
    pub waves: Vec<ScheduleWave>,
}

pub fn plan_parallel_execution(workflow: &Workflow) -> ParallelSchedulePlan {
    let dependency_map = build_dependency_map(workflow);
    let waves = compute_execution_waves(workflow, &dependency_map);
    let total_tasks = workflow.tasks.len();
    let total_waves = waves.len();
    let theoretical_min_waves = compute_min_waves(&dependency_map);

    let mut sequential_duration = 0.0;
    let mut parallel_duration = 0.0;
    let mut sequential_cost = 0.0;
    for task in &workflow.tasks {
        sequential_duration += task.cost.estimated_cost_usd;
        sequential_cost += task.cost.estimated_cost_usd;
    }
    for wave in &waves {
        parallel_duration += wave.parallel_duration_estimate;
    }

    let latency_reduction_bps = if sequential_duration > 0.0 {
        let reduction = sequential_duration - parallel_duration;
        ((reduction / sequential_duration) * 10_000.0) as u32
    } else {
        0
    };

    let parallel_opportunity = total_waves < total_tasks && total_tasks > 1;

    let schema_version = "forge.scheduler.parallel_plan.v1".to_string();

    ParallelSchedulePlan {
        schema_version,
        workflow_id: workflow.id.clone(),
        status: if parallel_opportunity {
            "parallel_opportunity_detected"
        } else {
            "sequential_only"
        }
        .to_string(),
        total_tasks,
        total_waves,
        theoretical_min_waves,
        sequential_cost_usd: sequential_cost,
        parallel_cost_usd: sequential_cost,
        sequential_duration_estimate: sequential_duration,
        parallel_duration_estimate: parallel_duration,
        latency_reduction_bps,
        parallel_opportunity,
        waves,
    }
}

fn build_dependency_map(workflow: &Workflow) -> BTreeMap<String, BTreeSet<String>> {
    let mut map: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for task in &workflow.tasks {
        map.entry(task.id.clone())
            .or_default()
            .extend(task.dependencies.iter().cloned());
    }
    map
}

fn compute_min_waves(dependency_map: &BTreeMap<String, BTreeSet<String>>) -> usize {
    let mut in_degree: BTreeMap<&str, usize> = BTreeMap::new();
    for (task_id, deps) in dependency_map {
        in_degree.entry(task_id).or_insert(0);
        for dep in deps {
            *in_degree.entry(dep).or_insert(0) += 0;
        }
    }
    for deps in dependency_map.values() {
        for dep in deps {
            if let Some(degree) = in_degree.get_mut(dep.as_str()) {
                *degree += 0;
            }
        }
    }
    for (task_id, deps) in dependency_map {
        let entry = in_degree.entry(task_id).or_insert(0);
        *entry = deps.len();
    }

    let mut remaining: BTreeSet<&str> = dependency_map.keys().map(|k| k.as_str()).collect();
    let mut waves = 0;
    while !remaining.is_empty() {
        let ready: Vec<&str> = remaining
            .iter()
            .filter(|task_id| {
                let deps = &dependency_map[**task_id];
                deps.iter().all(|dep| !remaining.contains(dep.as_str()))
            })
            .copied()
            .collect();
        if ready.is_empty() {
            break;
        }
        for task_id in ready {
            remaining.remove(task_id);
        }
        waves += 1;
    }
    waves.max(1)
}

fn compute_execution_waves(
    workflow: &Workflow,
    dependency_map: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<ScheduleWave> {
    let task_cost: BTreeMap<&str, f64> = workflow
        .tasks
        .iter()
        .map(|task| (task.id.as_str(), task.cost.estimated_cost_usd))
        .collect();
    let task_title: BTreeMap<&str, &str> = workflow
        .tasks
        .iter()
        .map(|task| (task.id.as_str(), task.title.as_str()))
        .collect();

    let mut completed: BTreeSet<&str> = BTreeSet::new();
    let mut all_task_ids: BTreeSet<&str> = workflow.tasks.iter().map(|t| t.id.as_str()).collect();
    let mut waves = Vec::new();

    while !all_task_ids.is_empty() {
        let ready: Vec<&str> = all_task_ids
            .iter()
            .filter(|task_id| {
                let Some(deps) = dependency_map.get(**task_id) else {
                    return true;
                };
                deps.iter().all(|dep| completed.contains(dep.as_str()))
            })
            .copied()
            .collect();

        if ready.is_empty() {
            break;
        }

        let wave_cost: f64 = ready.iter().filter_map(|id| task_cost.get(id)).sum();
        let max_cost: f64 = ready
            .iter()
            .filter_map(|id| task_cost.get(id))
            .cloned()
            .fold(0.0_f64, f64::max);
        let wave_titles: Vec<String> = ready
            .iter()
            .filter_map(|id| task_title.get(id).map(|t| (*t).to_string()))
            .collect();
        let wave_ids: Vec<String> = ready.iter().map(|id| (*id).to_string()).collect();

        waves.push(ScheduleWave {
            level: waves.len() + 1,
            task_ids: wave_ids,
            task_titles: wave_titles,
            estimated_cost_usd: wave_cost,
            task_count: ready.len(),
            concurrent: ready.len() > 1,
            max_task_cost: max_cost,
            parallel_duration_estimate: max_cost,
        });

        for task_id in ready {
            completed.insert(task_id);
            all_task_ids.remove(task_id);
        }
    }

    waves
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{create_workflow, CostEstimate, ExecutorKind, TaskStatus};
    use crate::intent::IntentSpec;

    fn make_workflow_with_deps(dependency_chains: &[(&str, &[&str])]) -> Workflow {
        let intent = IntentSpec {
            goal: "test parallel scheduling".to_string(),
            constraints: vec![],
            deliverables: vec![],
            risks: vec![],
            unknowns: vec![],
        };
        let mut workflow = create_workflow(intent);
        workflow.tasks.clear();

        for (task_id, deps) in dependency_chains {
            workflow.tasks.push(crate::graph::AtomicTask {
                id: task_id.to_string(),
                title: format!("Task {task_id}"),
                goal: format!("Goal {task_id}"),
                dependencies: deps.iter().map(|d| d.to_string()).collect(),
                context_requirements: vec![],
                validation_rules: vec![],
                expected_output: "output".to_string(),
                executor: ExecutorKind::Command,
                human_required: false,
                schedule: None,
                loop_control: None,
                native_subflow: None,
                cost: CostEstimate {
                    estimated_cost_usd: 1.0,
                    cost_model: "test".to_string(),
                },
                notification: None,
                persona: None,
                work_item: crate::graph::WorkItemSpec {
                    item_type: "execution_story".to_string(),
                    backlog_state: "ready".to_string(),
                    priority: "p1".to_string(),
                    owner_role: "forge_runtime".to_string(),
                    parent_id: None,
                    subtasks: vec![],
                    impediments: vec![],
                    acceptance_criteria: vec![],
                    goal_validation: crate::graph::GoalValidationSpec {
                        goal: "test".to_string(),
                        evidence_required: vec![],
                        definitively_ready: false,
                        rework_policy: "default".to_string(),
                    },
                },
                async_policy: crate::graph::AsyncPolicy::default(),
                execution_policy: crate::graph::ExecutionPolicySpec::default(),
                child_subflows: vec![],
                human_interaction: None,
                status: TaskStatus::Pending,
            });
        }

        workflow
    }

    #[test]
    fn sequential_chain_produces_one_task_per_wave() {
        let workflow = make_workflow_with_deps(&[
            ("task-001", &[] as &[&str]),
            ("task-002", &["task-001"]),
            ("task-003", &["task-002"]),
        ]);
        let plan = plan_parallel_execution(&workflow);
        assert_eq!(plan.total_waves, 3);
        assert_eq!(plan.total_tasks, 3);
        assert!(!plan.parallel_opportunity);
        assert_eq!(plan.waves[0].task_ids, vec!["task-001"]);
        assert_eq!(plan.waves[1].task_ids, vec!["task-002"]);
        assert_eq!(plan.waves[2].task_ids, vec!["task-003"]);
    }

    #[test]
    fn independent_tasks_are_scheduled_in_one_wave() {
        let workflow = make_workflow_with_deps(&[
            ("task-001", &[] as &[&str]),
            ("task-002", &[] as &[&str]),
            ("task-003", &[] as &[&str]),
        ]);
        let plan = plan_parallel_execution(&workflow);
        assert_eq!(plan.total_waves, 1);
        assert_eq!(plan.total_tasks, 3);
        assert!(plan.parallel_opportunity);
        assert_eq!(plan.waves[0].task_count, 3);
        assert!(plan.waves[0].concurrent);
    }

    #[test]
    fn diamond_dag_schedules_two_independent_waves_with_merge() {
        let workflow = make_workflow_with_deps(&[
            ("task-001", &[] as &[&str]),
            ("task-002", &["task-001"]),
            ("task-003", &["task-001"]),
            ("task-004", &["task-002", "task-003"]),
        ]);
        let plan = plan_parallel_execution(&workflow);
        assert_eq!(plan.total_waves, 3);
        assert_eq!(plan.waves[0].task_count, 1);
        assert_eq!(plan.waves[1].task_count, 2);
        assert!(plan.waves[1].concurrent);
        assert_eq!(plan.waves[2].task_count, 1);
        assert!(plan.parallel_opportunity);
    }

    #[test]
    fn complex_dag_reports_latency_reduction() {
        let workflow = make_workflow_with_deps(&[
            ("task-001", &[] as &[&str]),
            ("task-002", &[] as &[&str]),
            ("task-003", &["task-001"]),
            ("task-004", &["task-001", "task-002"]),
            ("task-005", &["task-003", "task-004"]),
        ]);
        let plan = plan_parallel_execution(&workflow);
        assert_eq!(plan.total_waves, 3);
        assert!(plan.parallel_opportunity);
        assert!(plan.latency_reduction_bps > 0);
        assert!(plan.sequential_duration_estimate > plan.parallel_duration_estimate);
    }

    #[test]
    fn single_task_plan_is_sequential() {
        let workflow = make_workflow_with_deps(&[("task-001", &[] as &[&str])]);
        let plan = plan_parallel_execution(&workflow);
        assert_eq!(plan.total_waves, 1);
        assert_eq!(plan.total_tasks, 1);
        assert!(!plan.parallel_opportunity);
    }

    #[test]
    fn plan_schema_includes_version_and_status() {
        let workflow =
            make_workflow_with_deps(&[("task-001", &[] as &[&str]), ("task-002", &[] as &[&str])]);
        let plan = plan_parallel_execution(&workflow);
        assert!(plan.schema_version.starts_with("forge.scheduler"));
        assert_eq!(plan.status, "parallel_opportunity_detected");
        assert!(plan.workflow_id.starts_with("wf_"));
    }
}
