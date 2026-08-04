use forge_core::graph::{TaskStatus, Workflow};
use forge_core::storage::ForgeStore;
use forge_core::teamwork::{
    plan_teamwork_workflow_with_config, TeamworkLaneConfig, TeamworkParallelConfig,
};
use forge_core::validation::validate_workflow_structure;
use std::collections::BTreeSet;
use tempfile::tempdir;

const FRONTEND_JOIN: &str = "task-005-frontend-join";
const BACKEND_JOIN: &str = "task-005-backend-join";
const FINAL_AUDITOR: &str = "task-006";

#[test]
fn teamwork_fan_in_unlocks_each_join_only_after_its_entire_lane() {
    let temporary = tempdir().unwrap();
    let store = ForgeStore::open(temporary.path().join("forge.sqlite")).unwrap();
    let response = plan_teamwork_workflow_with_config(
        &store,
        "Deliver frontend and backend in independent parallel lanes",
        false,
        true,
        TeamworkParallelConfig {
            lanes: vec![
                TeamworkLaneConfig {
                    id: "frontend".to_string(),
                    brain: "agy".to_string(),
                    agent_count: 3,
                    parallel_group: "implementation-wave-001".to_string(),
                    responsibility: "Deliver isolated frontend slices".to_string(),
                },
                TeamworkLaneConfig {
                    id: "backend".to_string(),
                    brain: "codex".to_string(),
                    agent_count: 5,
                    parallel_group: "implementation-wave-001".to_string(),
                    responsibility: "Deliver isolated backend slices".to_string(),
                },
            ],
            max_parallel_agents: 8,
        },
    )
    .unwrap();
    let mut workflow = store.load_workflow(&response.workflow_id).unwrap();

    let frontend_workers = worker_ids("frontend", 3);
    let backend_workers = worker_ids("backend", 5);
    let all_workers = frontend_workers
        .iter()
        .chain(&backend_workers)
        .cloned()
        .collect::<BTreeSet<_>>();

    assert_eq!(workflow.core_orchestration.max_parallel_tasks, 8);
    assert_eq!(eligible_task_ids(&workflow), all_workers);
    for worker_id in &all_workers {
        let worker = task_by_id(&workflow, worker_id);
        assert_eq!(worker.dependencies, vec!["task-004".to_string()]);
        assert_eq!(
            worker.node_brain_routing.agent_slots[0].parallel_group,
            "implementation-wave-001"
        );
        assert!(worker
            .dependencies
            .iter()
            .all(|dependency| !all_workers.contains(dependency)));
    }
    assert_eq!(
        task_by_id(&workflow, FRONTEND_JOIN)
            .dependencies
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>(),
        frontend_workers.iter().cloned().collect()
    );
    assert_eq!(
        task_by_id(&workflow, BACKEND_JOIN)
            .dependencies
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>(),
        backend_workers.iter().cloned().collect()
    );
    assert_eq!(
        task_by_id(&workflow, FINAL_AUDITOR).dependencies,
        vec![FRONTEND_JOIN.to_string(), BACKEND_JOIN.to_string()]
    );
    assert_coherent(&workflow);

    complete_ready_task(&mut workflow, &frontend_workers[0]);
    complete_ready_task(&mut workflow, &frontend_workers[1]);
    assert!(!eligible_task_ids(&workflow).contains(FRONTEND_JOIN));
    assert!(eligible_convergence_tasks(&workflow).is_empty());
    assert_coherent(&workflow);

    complete_ready_task(&mut workflow, &frontend_workers[2]);
    assert_eq!(
        eligible_convergence_tasks(&workflow),
        BTreeSet::from([FRONTEND_JOIN.to_string()])
    );
    assert!(!eligible_task_ids(&workflow).contains(BACKEND_JOIN));
    assert!(!eligible_task_ids(&workflow).contains(FINAL_AUDITOR));
    assert_coherent(&workflow);

    complete_ready_task(&mut workflow, FRONTEND_JOIN);
    assert!(!eligible_task_ids(&workflow).contains(FINAL_AUDITOR));

    for worker_id in backend_workers.iter().take(4) {
        complete_ready_task(&mut workflow, worker_id);
    }
    assert!(!eligible_task_ids(&workflow).contains(BACKEND_JOIN));
    assert!(!eligible_task_ids(&workflow).contains(FINAL_AUDITOR));
    assert!(eligible_convergence_tasks(&workflow).is_empty());
    assert_coherent(&workflow);

    complete_ready_task(&mut workflow, &backend_workers[4]);
    assert_eq!(
        eligible_convergence_tasks(&workflow),
        BTreeSet::from([BACKEND_JOIN.to_string()])
    );
    assert!(!eligible_task_ids(&workflow).contains(FINAL_AUDITOR));
    assert_coherent(&workflow);

    complete_ready_task(&mut workflow, BACKEND_JOIN);
    assert_eq!(
        eligible_convergence_tasks(&workflow),
        BTreeSet::from([FINAL_AUDITOR.to_string()])
    );
    assert_eq!(
        eligible_task_ids(&workflow),
        BTreeSet::from([FINAL_AUDITOR.to_string()])
    );
    assert_coherent(&workflow);
}

fn worker_ids(lane: &str, count: usize) -> Vec<String> {
    (1..=count)
        .map(|index| format!("task-005-{lane}-{index:03}"))
        .collect()
}

fn task_by_id<'a>(workflow: &'a Workflow, task_id: &str) -> &'a forge_core::graph::AtomicTask {
    workflow
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .unwrap_or_else(|| panic!("missing task {task_id}"))
}

fn eligible_task_ids(workflow: &Workflow) -> BTreeSet<String> {
    workflow
        .tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Pending)
        .filter(|task| task.active_impediments.is_empty())
        .filter(|task| {
            task.dependencies.iter().all(|dependency_id| {
                task_by_id(workflow, dependency_id).status == TaskStatus::Completed
            })
        })
        .map(|task| task.id.clone())
        .collect()
}

fn eligible_convergence_tasks(workflow: &Workflow) -> BTreeSet<String> {
    eligible_task_ids(workflow)
        .into_iter()
        .filter(|task_id| {
            task_id == FRONTEND_JOIN || task_id == BACKEND_JOIN || task_id == FINAL_AUDITOR
        })
        .collect()
}

fn complete_ready_task(workflow: &mut Workflow, task_id: &str) {
    assert!(
        eligible_task_ids(workflow).contains(task_id),
        "task {task_id} must be dependency-ready before completion"
    );
    let task = workflow
        .tasks
        .iter_mut()
        .find(|task| task.id == task_id)
        .unwrap_or_else(|| panic!("missing task {task_id}"));
    task.status = TaskStatus::Completed;
    task.active_impediments.clear();
    task.work_item.backlog_state = "done".to_string();
    task.work_item.impediments.clear();
    task.work_item.goal_validation.definitively_ready = true;
    for subtask in &mut task.work_item.subtasks {
        subtask.status = TaskStatus::Completed;
    }
    task.version = task.version.saturating_add(1);
}

fn assert_coherent(workflow: &Workflow) {
    let structural_failures = validate_workflow_structure(workflow);
    assert!(
        structural_failures.is_empty(),
        "invalid teamwork graph: {structural_failures:?}"
    );
    for task in &workflow.tasks {
        match task.status {
            TaskStatus::Completed => {
                assert!(
                    task.work_item.goal_validation.definitively_ready,
                    "completed task {} must be definitively ready",
                    task.id
                );
                assert!(task
                    .work_item
                    .subtasks
                    .iter()
                    .all(|subtask| subtask.status == TaskStatus::Completed));
            }
            TaskStatus::Pending => assert!(
                !task.work_item.goal_validation.definitively_ready,
                "pending task {} cannot be definitively ready",
                task.id
            ),
            _ => panic!("unexpected task status for {}: {:?}", task.id, task.status),
        }
    }
}
