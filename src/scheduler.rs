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
    pub max_task_cost_usd: f64,
    pub parallel_duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParallelSchedulePlan {
    pub schema_version: String,
    pub workflow_id: String,
    pub status: String,
    pub total_tasks: usize,
    pub total_waves: usize,
    pub theoretical_min_waves: usize,
    pub max_parallel_tasks: usize,
    pub sequential_cost_usd: f64,
    pub parallel_cost_usd: f64,
    pub duration_estimate_status: String,
    pub duration_estimate_basis: String,
    pub missing_duration_task_ids: Vec<String>,
    pub sequential_duration_ms: Option<u64>,
    pub parallel_duration_ms: Option<u64>,
    pub latency_reduction_bps: Option<u32>,
    pub parallel_opportunity: bool,
    pub waves: Vec<ScheduleWave>,
}

pub fn plan_parallel_execution(workflow: &Workflow) -> ParallelSchedulePlan {
    let dependency_map = build_dependency_map(workflow);
    let max_parallel_tasks = workflow.core_orchestration.max_parallel_tasks.max(1);
    let waves = compute_execution_waves(workflow, &dependency_map, max_parallel_tasks);
    let total_tasks = workflow.tasks.len();
    let total_waves = waves.len();
    let theoretical_min_waves = compute_min_waves(&dependency_map);
    let scheduled_task_count: usize = waves.iter().map(|wave| wave.task_count).sum();
    let schedule_complete = scheduled_task_count == total_tasks;

    let sequential_cost_usd = workflow
        .tasks
        .iter()
        .map(|task| task.cost.estimated_cost_usd)
        .sum();
    let missing_duration_task_ids: Vec<String> = workflow
        .tasks
        .iter()
        .filter(|task| task.cost.estimated_duration_ms.is_none())
        .map(|task| task.id.clone())
        .collect();
    let sequential_duration_ms =
        if total_tasks == 0 || !schedule_complete || !missing_duration_task_ids.is_empty() {
            None
        } else {
            workflow.tasks.iter().try_fold(0_u64, |total, task| {
                total.checked_add(task.cost.estimated_duration_ms?)
            })
        };
    let parallel_duration_ms =
        if total_tasks == 0 || !schedule_complete || !missing_duration_task_ids.is_empty() {
            None
        } else {
            waves.iter().try_fold(0_u64, |total, wave| {
                total.checked_add(wave.parallel_duration_ms?)
            })
        };
    let duration_estimate_status = if total_tasks == 0 {
        "unavailable_no_tasks"
    } else if !schedule_complete {
        "unavailable_incomplete_schedule"
    } else if !missing_duration_task_ids.is_empty() {
        "unavailable_missing_task_duration"
    } else if sequential_duration_ms.is_none() || parallel_duration_ms.is_none() {
        "unavailable_duration_overflow"
    } else {
        "available"
    }
    .to_string();
    let latency_reduction_bps = latency_reduction_bps(sequential_duration_ms, parallel_duration_ms);

    let parallel_opportunity = total_waves < total_tasks && total_tasks > 1;

    let schema_version = "foundry.scheduler.parallel_plan.v2".to_string();

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
        max_parallel_tasks,
        sequential_cost_usd,
        parallel_cost_usd: sequential_cost_usd,
        duration_estimate_status,
        duration_estimate_basis:
            "dependency_and_max_parallel_tasks_estimate_no_additional_runtime_wait_modeled"
                .to_string(),
        missing_duration_task_ids,
        sequential_duration_ms,
        parallel_duration_ms,
        latency_reduction_bps,
        parallel_opportunity,
        waves,
    }
}

fn latency_reduction_bps(
    sequential_duration_ms: Option<u64>,
    parallel_duration_ms: Option<u64>,
) -> Option<u32> {
    let sequential_duration_ms = sequential_duration_ms?;
    let parallel_duration_ms = parallel_duration_ms?;
    if sequential_duration_ms == 0 || parallel_duration_ms > sequential_duration_ms {
        return None;
    }

    let reduction_ms = sequential_duration_ms - parallel_duration_ms;
    let reduction_bps = (u128::from(reduction_ms) * 10_000) / u128::from(sequential_duration_ms);
    u32::try_from(reduction_bps).ok()
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
    waves
}

fn compute_execution_waves(
    workflow: &Workflow,
    dependency_map: &BTreeMap<String, BTreeSet<String>>,
    max_parallel_tasks: usize,
) -> Vec<ScheduleWave> {
    let task_cost_usd: BTreeMap<&str, f64> = workflow
        .tasks
        .iter()
        .map(|task| (task.id.as_str(), task.cost.estimated_cost_usd))
        .collect();
    let task_duration_ms: BTreeMap<&str, u64> = workflow
        .tasks
        .iter()
        .filter_map(|task| {
            task.cost
                .estimated_duration_ms
                .map(|duration_ms| (task.id.as_str(), duration_ms))
        })
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
            .take(max_parallel_tasks.max(1))
            .collect();

        if ready.is_empty() {
            break;
        }

        let wave_cost_usd: f64 = ready.iter().filter_map(|id| task_cost_usd.get(id)).sum();
        let max_task_cost_usd: f64 = ready
            .iter()
            .filter_map(|id| task_cost_usd.get(id))
            .cloned()
            .fold(0.0_f64, f64::max);
        let parallel_duration_ms = if ready.iter().all(|id| task_duration_ms.contains_key(id)) {
            ready
                .iter()
                .filter_map(|id| task_duration_ms.get(id))
                .copied()
                .max()
        } else {
            None
        };
        let wave_titles: Vec<String> = ready
            .iter()
            .filter_map(|id| task_title.get(id).map(|t| (*t).to_string()))
            .collect();
        let wave_ids: Vec<String> = ready.iter().map(|id| (*id).to_string()).collect();

        waves.push(ScheduleWave {
            level: waves.len() + 1,
            task_ids: wave_ids,
            task_titles: wave_titles,
            estimated_cost_usd: wave_cost_usd,
            task_count: ready.len(),
            concurrent: ready.len() > 1,
            max_task_cost_usd,
            parallel_duration_ms,
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
    use crate::graph::{
        create_workflow, node_brain_routing_for_executor, CostEstimate, ExecutorKind, TaskStatus,
    };
    use crate::intent::IntentSpec;

    fn make_workflow_with_deps(dependency_chains: &[(&str, &[&str])]) -> Workflow {
        let intent = IntentSpec {
            goal: "test parallel scheduling".to_string(),
            constraints: vec![],
            deliverables: vec![],
            risks: vec![],
            unknowns: vec![],
            ..IntentSpec::default()
        };
        let mut workflow = create_workflow(intent);
        workflow.tasks.clear();

        for (task_id, deps) in dependency_chains {
            workflow.tasks.push(crate::graph::AtomicTask {
                id: task_id.to_string(),
                title: format!("Task {task_id}"),
                goal: format!("Goal {task_id}"),
                dependencies: deps.iter().map(|d| d.to_string()).collect(),
                active_impediments: vec![],
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
                    estimated_duration_ms: None,
                },
                notification: None,
                persona: None,
                work_item: crate::graph::WorkItemSpec {
                    item_type: "execution_story".to_string(),
                    backlog_state: "ready".to_string(),
                    priority: "p1".to_string(),
                    owner_role: "foundry_runtime".to_string(),
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
                node_brain_routing: node_brain_routing_for_executor(&ExecutorKind::Command),
                child_subflows: vec![],
                human_interaction: None,
                status: TaskStatus::Pending,
                version: 1,
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
    fn scheduler_duration_respects_declared_parallel_capacity() {
        let mut workflow = make_workflow_with_deps(&[
            ("task-001", &[] as &[&str]),
            ("task-002", &[] as &[&str]),
            ("task-003", &[] as &[&str]),
        ]);
        workflow.core_orchestration.max_parallel_tasks = 2;
        for task in &mut workflow.tasks {
            task.cost.estimated_duration_ms = Some(1_000);
        }

        let plan = plan_parallel_execution(&workflow);

        assert_eq!(plan.max_parallel_tasks, 2);
        assert_eq!(plan.theoretical_min_waves, 1);
        assert_eq!(plan.total_waves, 2);
        assert_eq!(plan.parallel_duration_ms, Some(2_000));
        assert_eq!(plan.latency_reduction_bps, Some(3_333));
        assert!(plan
            .duration_estimate_basis
            .contains("no_additional_runtime_wait_modeled"));
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
    fn complex_dag_reports_latency_reduction_from_explicit_duration_ms() {
        let mut workflow = make_workflow_with_deps(&[
            ("task-001", &[] as &[&str]),
            ("task-002", &[] as &[&str]),
            ("task-003", &["task-001"]),
            ("task-004", &["task-001", "task-002"]),
            ("task-005", &["task-003", "task-004"]),
        ]);
        for task in &mut workflow.tasks {
            task.cost.estimated_duration_ms = Some(1_000);
        }

        let plan = plan_parallel_execution(&workflow);
        assert_eq!(plan.total_waves, 3);
        assert!(plan.parallel_opportunity);
        assert_eq!(plan.duration_estimate_status, "available");
        assert_eq!(plan.sequential_duration_ms, Some(5_000));
        assert_eq!(plan.parallel_duration_ms, Some(3_000));
        assert_eq!(plan.latency_reduction_bps, Some(4_000));
    }

    #[test]
    fn missing_duration_never_reuses_cost_as_time_or_claims_reduction() {
        let mut workflow =
            make_workflow_with_deps(&[("task-001", &[] as &[&str]), ("task-002", &[] as &[&str])]);
        workflow.tasks[0].cost.estimated_cost_usd = 250.0;
        workflow.tasks[1].cost.estimated_cost_usd = 2.0;

        let plan = plan_parallel_execution(&workflow);

        assert_eq!(plan.schema_version, "foundry.scheduler.parallel_plan.v2");
        assert_eq!(plan.sequential_cost_usd, 252.0);
        assert_eq!(plan.parallel_cost_usd, 252.0);
        assert_eq!(
            plan.duration_estimate_status,
            "unavailable_missing_task_duration"
        );
        assert_eq!(plan.missing_duration_task_ids, vec!["task-001", "task-002"]);
        assert_eq!(plan.sequential_duration_ms, None);
        assert_eq!(plan.parallel_duration_ms, None);
        assert_eq!(plan.latency_reduction_bps, None);
        assert_eq!(plan.waves[0].estimated_cost_usd, 252.0);
        assert_eq!(plan.waves[0].max_task_cost_usd, 250.0);
        assert_eq!(plan.waves[0].parallel_duration_ms, None);

        let serialized = serde_json::to_value(&plan).unwrap();
        assert_eq!(
            serialized["schema_version"],
            "foundry.scheduler.parallel_plan.v2"
        );
        assert!(serialized["sequential_duration_ms"].is_null());
        assert!(serialized["parallel_duration_ms"].is_null());
        assert!(serialized["latency_reduction_bps"].is_null());
        assert!(serialized.get("sequential_duration_estimate").is_none());
        assert!(serialized.get("parallel_duration_estimate").is_none());
    }

    #[test]
    fn incomplete_schedule_does_not_claim_latency_reduction() {
        let mut workflow = make_workflow_with_deps(&[("task-001", &["missing-task"])]);
        workflow.tasks[0].cost.estimated_duration_ms = Some(1_000);

        let plan = plan_parallel_execution(&workflow);

        assert_eq!(plan.total_waves, 0);
        assert_eq!(
            plan.duration_estimate_status,
            "unavailable_incomplete_schedule"
        );
        assert_eq!(plan.sequential_duration_ms, None);
        assert_eq!(plan.parallel_duration_ms, None);
        assert_eq!(plan.latency_reduction_bps, None);
    }

    #[test]
    fn zero_duration_does_not_claim_a_percentage_reduction() {
        let mut workflow =
            make_workflow_with_deps(&[("task-001", &[] as &[&str]), ("task-002", &[] as &[&str])]);
        for task in &mut workflow.tasks {
            task.cost.estimated_duration_ms = Some(0);
        }

        let plan = plan_parallel_execution(&workflow);

        assert_eq!(plan.duration_estimate_status, "available");
        assert_eq!(plan.sequential_duration_ms, Some(0));
        assert_eq!(plan.parallel_duration_ms, Some(0));
        assert_eq!(plan.latency_reduction_bps, None);
    }

    #[test]
    fn cost_estimate_serde_accepts_legacy_and_explicit_duration_contracts() {
        let legacy: CostEstimate = serde_json::from_value(serde_json::json!({
            "estimated_cost_usd": 1.25,
            "cost_model": "legacy"
        }))
        .unwrap();
        assert_eq!(legacy.estimated_duration_ms, None);
        assert!(serde_json::to_value(&legacy).unwrap()["estimated_duration_ms"].is_null());

        let explicit: CostEstimate = serde_json::from_value(serde_json::json!({
            "estimated_cost_usd": 1.25,
            "cost_model": "duration_aware",
            "estimated_duration_ms": 750
        }))
        .unwrap();
        assert_eq!(explicit.estimated_duration_ms, Some(750));
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
        assert_eq!(plan.schema_version, "foundry.scheduler.parallel_plan.v2");
        assert_eq!(plan.status, "parallel_opportunity_detected");
        assert_eq!(
            plan.duration_estimate_status,
            "unavailable_missing_task_duration"
        );
        assert!(plan.workflow_id.starts_with("wf_"));
    }
}
