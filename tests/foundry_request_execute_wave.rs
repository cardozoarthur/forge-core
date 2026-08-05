#![cfg(unix)]

use assert_cmd::Command;
use chrono::Utc;
use foundry_core::executor::ExecutorState;
use foundry_core::graph::{
    create_workflow, task, ExecutorKind, NodeBrainAgentSlotSpec, TaskStatus,
};
use foundry_core::intent::parse_intent;
use foundry_core::request::{
    create_run_record, drive_request_with_context_budget, save_run_record,
};
use foundry_core::storage::FoundryStore;
use foundry_core::teamwork::{
    plan_teamwork_workflow_with_config, prepare_teamwork_worktrees, TeamworkLaneConfig,
    TeamworkParallelConfig, TeamworkWorktreePrepareOptions,
};
use foundry_core::teamwork_fan_in::{
    current_teamwork_fan_in_status, integrate_worktree_dependencies, IntegrateDependenciesOptions,
    TeamworkFanInReport,
};
use foundry_core::worktree::{
    register_worktree, resolve_bound_worktree_root, WorktreeRegisterOptions,
};
use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command as ProcessCommand;
use tempfile::tempdir;

fn foundry() -> Command {
    Command::cargo_bin("foundry").expect("foundry binary should build")
}

#[test]
fn execute_wave_requires_explicit_process_authorization_before_drive() {
    let temp = tempdir().unwrap();
    foundry()
        .arg("--store")
        .arg(temp.path().join("foundry.sqlite"))
        .args([
            "request",
            "execute-wave",
            "--run",
            "missing-run",
            "--approved-by",
            "wave-test",
            "--output",
            "json",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "requires explicit --allow-exec authorization",
        ));
}

#[test]
fn execute_wave_runs_admitted_task_and_keeps_promotion_separate() {
    let temp = tempdir().unwrap();
    let repository = temp.path().join("repository");
    initialize_repository(&repository);
    let store_path = temp.path().join("foundry.sqlite");
    let stub_path = temp.path().join("codex-stub");
    write_codex_stub(&stub_path);

    let store = FoundryStore::open(&store_path).unwrap();
    save_ready_executor(&store, "codex", &stub_path);
    let mut workflow = create_workflow(parse_intent(
        "Execute one isolated Codex task through the productive request wave",
    ));
    let mut agent_task = task(
        "task-codex-wave",
        "Implement isolated fixture change",
        &[],
        &["task-bound repository context"],
        vec![],
        "validated fixture change",
        (ExecutorKind::Mixed, 0.001),
    );
    agent_task.node_brain_routing.scope = "agentic_ai_node".to_string();
    agent_task.node_brain_routing.default_brain = Some("codex".to_string());
    agent_task.node_brain_routing.allowed_brains = vec!["codex".to_string()];
    agent_task.node_brain_routing.agent_slots = vec![NodeBrainAgentSlotSpec {
        slot_id: "slot-codex-wave".to_string(),
        brain_id: Some("codex".to_string()),
        role: "BackendWorker".to_string(),
        parallel_group: "wave-test".to_string(),
        state_owner: "foundry".to_string(),
    }];
    agent_task.node_brain_routing.max_parallel_agents = 1;
    workflow.tasks = vec![agent_task];
    workflow.core_orchestration.max_parallel_tasks = 1;
    store.save_workflow(&workflow).unwrap();
    let run = create_run_record(&workflow, "execute-wave-test", "accepted");
    save_run_record(&store, &run).unwrap();
    register_worktree(
        &store,
        WorktreeRegisterOptions {
            path: repository.clone(),
            id: None,
            workflow_id: Some(workflow.id.clone()),
            task_id: Some("task-codex-wave".to_string()),
            origin: "execute-wave-test".to_string(),
            created_by_foundry: false,
        },
    )
    .unwrap();
    fs::write(
        repository.join("README.md"),
        "executor wave fixture\nworktree changed after binding\n",
    )
    .unwrap();
    git(&repository, &["add", "README.md"]);
    git(
        &repository,
        &["commit", "-q", "-m", "change bound worktree head"],
    );
    drop(store);

    let output = foundry()
        .arg("--store")
        .arg(&store_path)
        .args([
            "request",
            "execute-wave",
            "--run",
            &run.run_id,
            "--executor",
            "auto",
            "--ttl-seconds",
            "60",
            "--timeout-seconds",
            "5",
            "--context-budget",
            "4096",
            "--allow-exec",
            "--approved-by",
            "wave-test",
            "--reason",
            "exercise the productive request wave without auto-promotion",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["schema_version"], "foundry.request_executor_wave.v1");
    assert_eq!(report["status"], "executor_wave_succeeded");
    assert_eq!(report["success"], true);
    assert_eq!(report["executor_wave"]["request_count"], 1);
    assert_eq!(
        report["executor_wave"]["receipts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        report["executor_wave"]["receipts"][0]["dispatch"]["wave_id"],
        report["wave_id"]
    );
    assert_eq!(
        report["executor_wave"]["receipts"][0]["workspace_binding_scope"],
        "task"
    );
    assert_eq!(
        report["executor_wave"]["receipts"][0]["lease_extended_for_runtime"],
        true
    );
    assert_eq!(report["task_completion_attempted"], false);
    assert_eq!(report["output_accepted_as_validation"], false);
    assert_eq!(report["validation_commands"].as_array().unwrap().len(), 1);

    let args = fs::read_to_string(temp.path().join("codex-stub.args")).unwrap();
    assert!(args.lines().last().is_some_and(|argument| argument == "-"));
    let prompt = fs::read_to_string(temp.path().join("codex-stub.stdin")).unwrap();
    assert!(prompt.contains("Bounded task context"));
    assert!(prompt.contains("task-codex-wave"));

    let store = FoundryStore::open(&store_path).unwrap();
    let restored = store.load_workflow(&workflow.id).unwrap();
    assert_eq!(restored.tasks[0].status, TaskStatus::Pending);
    let events = store.load_workflow_events(&workflow.id).unwrap();
    assert!(events
        .iter()
        .any(|event| event.kind == "request_executor_wave_started"));
    assert!(events
        .iter()
        .any(|event| event.kind == "request_executor_wave_finished"));
    drop(store);

    let completion_output = foundry()
        .arg("--store")
        .arg(&store_path)
        .args([
            "request",
            "complete-task",
            "--run",
            &run.run_id,
            "--task",
            "task-codex-wave",
            "--executor",
            "codex",
            "--summary",
            "Executor wave output reviewed and accepted",
            "--evidence-command",
            "codex-stub",
            "--evidence-exit-code",
            "0",
            "--origin",
            "execute-wave-test",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let completion: serde_json::Value = serde_json::from_slice(&completion_output).unwrap();
    assert_eq!(
        completion["schema_version"],
        "foundry.request_task_completion.v1"
    );
    assert_eq!(completion["status"], "completed");
    assert_eq!(completion["task_id"], "task-codex-wave");
    assert_eq!(completion["validation"]["accepted"], true);

    let store = FoundryStore::open(&store_path).unwrap();
    let completed = store.load_workflow(&workflow.id).unwrap();
    assert_eq!(completed.tasks[0].status, TaskStatus::Completed);
}

#[test]
fn execute_wave_progresses_three_agy_five_codex_then_joins_and_auditor() {
    let temp = tempdir().unwrap();
    let repository = temp.path().join("repository");
    let worktree_root = temp.path().join("worktrees");
    let store_path = temp.path().join("foundry.sqlite");
    let codex_path = temp.path().join("codex-parallel-stub");
    let agy_path = temp.path().join("agy-parallel-stub");
    initialize_repository(&repository);
    write_parallel_stub(&codex_path);
    write_parallel_stub(&agy_path);

    let store = FoundryStore::open(&store_path).unwrap();
    save_ready_executor(&store, "codex", &codex_path);
    save_ready_executor(&store, "agy", &agy_path);
    let fixture = seed_parallel_teamwork_run(&store, &repository, &worktree_root);
    let preview = drive_request_with_context_budget(
        &store,
        &fixture.run_id,
        "auto",
        60,
        "parallel-wave-preview-test",
        Some(4096),
    )
    .unwrap();
    let preview_frontier = preview.dispatch_frontier.as_ref().unwrap();
    assert_eq!(preview_frontier.wave.assignments.len(), 8);
    assert!(preview_frontier
        .wave
        .assignments
        .iter()
        .all(|assignment| assignment.lease_state == "acquired"));
    drop(store);

    let report = execute_parallel_wave(&store_path, &fixture.run_id, 8);
    assert_eq!(report["schema_version"], "foundry.request_executor_wave.v1");
    assert_eq!(report["status"], "executor_wave_succeeded");
    assert_eq!(report["success"], true);
    assert_eq!(
        report["dispatch_frontier"]["admission"]["status"],
        "admitted"
    );
    assert_eq!(
        report["dispatch_frontier"]["wave"]["assignments"]
            .as_array()
            .unwrap()
            .len(),
        8
    );
    assert_eq!(report["executor_wave"]["request_count"], 8);
    assert_eq!(report["executor_wave"]["unique_request_count"], 8);
    assert_eq!(report["executor_wave"]["worker_count"], 8);
    assert_eq!(report["executor_wave"]["max_parallel"], 8);
    assert!(report["dispatch_frontier"]["wave"]["assignments"]
        .as_array()
        .unwrap()
        .iter()
        .all(|assignment| assignment["lease_state"] == "reused_active"));

    let receipts = report["executor_wave"]["receipts"].as_array().unwrap();
    assert_eq!(receipts.len(), 8);
    assert_eq!(
        receipts
            .iter()
            .filter(|receipt| receipt["executor"] == "agy")
            .count(),
        3
    );
    assert_eq!(
        receipts
            .iter()
            .filter(|receipt| receipt["executor"] == "codex")
            .count(),
        5
    );
    assert!(receipts.iter().all(|receipt| {
        let task_id = receipt["task_id"].as_str().unwrap();
        match receipt["executor"].as_str().unwrap() {
            "agy" => task_id.starts_with("task-005-frontend-"),
            "codex" => task_id.starts_with("task-005-backend-"),
            _ => false,
        }
    }));
    assert!(receipts.iter().all(|receipt| receipt["success"] == true));
    assert!(receipts
        .iter()
        .all(|receipt| receipt["workspace_binding_scope"] == "task"));
    assert_eq!(unique_receipt_field(receipts, "task_id").len(), 8);
    assert_eq!(unique_receipt_field(receipts, "lease_id").len(), 8);
    assert_eq!(unique_receipt_field(receipts, "worktree_id").len(), 8);
    assert_eq!(
        unique_receipt_field(receipts, "task_id"),
        fixture.branch_task_ids
    );
    assert_eq!(report["task_completion_attempted"], false);
    assert_eq!(report["output_accepted_as_validation"], false);
    assert_eq!(report["validation_commands"].as_array().unwrap().len(), 8);
    let mut all_lease_ids = BTreeSet::new();
    let mut all_worktree_ids = BTreeSet::new();
    extend_unique_receipt_identity(receipts, &mut all_lease_ids, &mut all_worktree_ids);

    let store = FoundryStore::open(&store_path).unwrap();
    assert_branches_remain_pending(&store, &fixture);
    for task_id in &fixture.branch_task_ids {
        let lease = store
            .load_task_lease(&fixture.workflow_id, task_id)
            .unwrap()
            .unwrap();
        assert!(lease["lease_id"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));
        assert_eq!(lease["workspace_claim"]["binding_scope"], "task");
    }
    assert_eq!(parallel_stub_invocations(temp.path()), 8);
    drop(store);

    complete_wave_tasks(&store_path, &fixture.run_id, receipts);
    let store = FoundryStore::open(&store_path).unwrap();
    assert_task_statuses(
        &store,
        &fixture.workflow_id,
        &fixture.branch_task_ids,
        TaskStatus::Completed,
    );
    drop(store);

    let blocked_join_report = execute_parallel_wave(&store_path, &fixture.run_id, 2);
    assert_eq!(blocked_join_report["status"], "execution_not_started");
    assert_eq!(blocked_join_report["success"], false);
    let blocked_joins = blocked_join_report["dispatch_frontier"]["wave"]["deferred"]
        .as_array()
        .unwrap();
    assert_eq!(
        blocked_joins
            .iter()
            .filter(|task| task["status"] == "deferred_git_fan_in_required")
            .count(),
        2
    );

    for join_task_id in ["task-005-frontend-join", "task-005-backend-join"] {
        let fan_in = apply_dependency_fan_in(
            &store_path,
            &fixture.workflow_id,
            join_task_id,
            "parallel-wave-test",
        );
        assert_eq!(fan_in.status, "dependencies_integrated");
        assert!(fan_in.success);
        assert!(fan_in.commit_created);
        let store = FoundryStore::open(&store_path).unwrap();
        let status =
            current_teamwork_fan_in_status(&store, &fixture.workflow_id, join_task_id).unwrap();
        assert!(status.current, "{}", status.reason);
    }

    let join_report = execute_parallel_wave(&store_path, &fixture.run_id, 2);
    assert_eq!(join_report["status"], "executor_wave_succeeded");
    assert_eq!(join_report["task_completion_attempted"], false);
    assert_eq!(join_report["output_accepted_as_validation"], false);
    let join_receipts = join_report["executor_wave"]["receipts"].as_array().unwrap();
    assert_eq!(join_receipts.len(), 2);
    assert_eq!(
        join_receipts
            .iter()
            .filter(|receipt| receipt["executor"] == "agy")
            .count(),
        1
    );
    assert_eq!(
        join_receipts
            .iter()
            .filter(|receipt| receipt["executor"] == "codex")
            .count(),
        1
    );
    let join_task_ids = [
        "task-005-frontend-join".to_string(),
        "task-005-backend-join".to_string(),
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    assert_eq!(
        unique_receipt_field(join_receipts, "task_id"),
        join_task_ids
    );
    extend_unique_receipt_identity(join_receipts, &mut all_lease_ids, &mut all_worktree_ids);
    assert_eq!(all_lease_ids.len(), 10);
    assert_eq!(all_worktree_ids.len(), 10);
    let store = FoundryStore::open(&store_path).unwrap();
    assert_task_statuses(
        &store,
        &fixture.workflow_id,
        &join_task_ids,
        TaskStatus::Pending,
    );
    assert_eq!(parallel_stub_invocations(temp.path()), 10);
    drop(store);

    complete_wave_tasks(&store_path, &fixture.run_id, join_receipts);
    let store = FoundryStore::open(&store_path).unwrap();
    assert_task_statuses(
        &store,
        &fixture.workflow_id,
        &join_task_ids,
        TaskStatus::Completed,
    );
    drop(store);

    let blocked_auditor_report = execute_parallel_wave(&store_path, &fixture.run_id, 1);
    assert_eq!(blocked_auditor_report["status"], "execution_not_started");
    assert_eq!(blocked_auditor_report["success"], false);
    assert_eq!(
        blocked_auditor_report["dispatch_frontier"]["wave"]["deferred"][0]["status"],
        "deferred_git_fan_in_required"
    );
    let auditor_fan_in = apply_dependency_fan_in(
        &store_path,
        &fixture.workflow_id,
        "task-006",
        "parallel-wave-test",
    );
    assert_eq!(auditor_fan_in.status, "dependencies_integrated");
    assert!(auditor_fan_in.success);
    assert!(auditor_fan_in.commit_created);

    let auditor_report = execute_parallel_wave(&store_path, &fixture.run_id, 1);
    assert_eq!(auditor_report["status"], "executor_wave_succeeded");
    assert_eq!(auditor_report["task_completion_attempted"], false);
    assert_eq!(auditor_report["output_accepted_as_validation"], false);
    let auditor_receipts = auditor_report["executor_wave"]["receipts"]
        .as_array()
        .unwrap();
    assert_eq!(auditor_receipts.len(), 1);
    assert_eq!(auditor_receipts[0]["task_id"], "task-006");
    assert_eq!(auditor_receipts[0]["executor"], "codex");
    assert_eq!(auditor_receipts[0]["workspace_binding_scope"], "task");
    extend_unique_receipt_identity(auditor_receipts, &mut all_lease_ids, &mut all_worktree_ids);
    assert_eq!(all_lease_ids.len(), 11);
    assert_eq!(all_worktree_ids.len(), 11);
    let auditor_task_ids = ["task-006".to_string()]
        .into_iter()
        .collect::<BTreeSet<_>>();
    let store = FoundryStore::open(&store_path).unwrap();
    assert_task_statuses(
        &store,
        &fixture.workflow_id,
        &auditor_task_ids,
        TaskStatus::Pending,
    );
    assert_eq!(parallel_stub_invocations(temp.path()), 11);
    let auditor_root = resolve_bound_worktree_root(&store, &fixture.workflow_id, Some("task-006"))
        .unwrap()
        .unwrap();
    for worker_task_id in &fixture.branch_task_ids {
        assert!(
            auditor_root
                .join(format!("foundry-runtime-{worker_task_id}.txt"))
                .is_file(),
            "auditor worktree must contain output from {worker_task_id}"
        );
    }
    for join_task_id in &join_task_ids {
        assert!(auditor_root
            .join(format!("foundry-runtime-{join_task_id}.txt"))
            .is_file());
    }
}

#[test]
fn blocked_agy_quota_does_not_prevent_five_independent_codex_tasks() {
    let temp = tempdir().unwrap();
    let repository = temp.path().join("repository");
    let worktree_root = temp.path().join("worktrees");
    let store_path = temp.path().join("foundry.sqlite");
    let codex_path = temp.path().join("codex-parallel-stub");
    let agy_path = temp.path().join("agy-parallel-stub");
    initialize_repository(&repository);
    write_parallel_stub(&codex_path);
    write_parallel_stub(&agy_path);

    let store = FoundryStore::open(&store_path).unwrap();
    save_ready_executor(&store, "codex", &codex_path);
    save_ready_executor(&store, "agy", &agy_path);
    save_executor_quota_observation(&store, "agy", "exhausted", "blocked");
    let fixture = seed_parallel_teamwork_run(&store, &repository, &worktree_root);
    drop(store);

    let report = execute_parallel_wave(&store_path, &fixture.run_id, 8);
    assert_eq!(report["status"], "executor_wave_succeeded");
    assert_eq!(report["success"], true);
    assert_eq!(
        report["dispatch_frontier"]["admission"]["status"],
        "degraded"
    );
    assert_eq!(
        report["dispatch_frontier"]["admission"]["executor_quota_statuses"]["agy"],
        "blocked"
    );
    assert_eq!(
        report["dispatch_frontier"]["admission"]["executor_quota_statuses"]["codex"],
        "fresh"
    );

    let assignments = report["dispatch_frontier"]["wave"]["assignments"]
        .as_array()
        .unwrap();
    assert_eq!(assignments.len(), 5);
    assert!(assignments
        .iter()
        .all(|assignment| assignment["selected_executor"] == "codex"));
    let deferred = report["dispatch_frontier"]["wave"]["deferred"]
        .as_array()
        .unwrap();
    assert_eq!(
        deferred
            .iter()
            .filter(|task| task["status"] == "deferred_executor_quota")
            .count(),
        3
    );

    let receipts = report["executor_wave"]["receipts"].as_array().unwrap();
    assert_eq!(report["executor_wave"]["request_count"], 5);
    assert_eq!(report["executor_wave"]["worker_count"], 5);
    assert_eq!(report["executor_wave"]["max_parallel"], 8);
    assert_eq!(receipts.len(), 5);
    assert!(receipts
        .iter()
        .all(|receipt| receipt["executor"] == "codex"));
    assert_eq!(unique_receipt_field(receipts, "lease_id").len(), 5);
    assert_eq!(unique_receipt_field(receipts, "worktree_id").len(), 5);
    assert!(unique_receipt_field(receipts, "task_id")
        .iter()
        .all(|task_id| task_id.starts_with("task-005-backend-")));
    assert_eq!(report["task_completion_attempted"], false);
    assert_eq!(report["output_accepted_as_validation"], false);

    let store = FoundryStore::open(&store_path).unwrap();
    assert_branches_remain_pending(&store, &fixture);
    for task_id in fixture
        .branch_task_ids
        .iter()
        .filter(|task_id| task_id.starts_with("task-005-frontend-"))
    {
        assert!(store
            .load_task_lease(&fixture.workflow_id, task_id)
            .unwrap()
            .is_none());
    }
    assert_eq!(parallel_stub_invocations(temp.path()), 5);
}

struct ParallelTeamworkFixture {
    workflow_id: String,
    run_id: String,
    branch_task_ids: BTreeSet<String>,
}

fn seed_parallel_teamwork_run(
    store: &FoundryStore,
    repository: &Path,
    worktree_root: &Path,
) -> ParallelTeamworkFixture {
    let response = plan_teamwork_workflow_with_config(
        store,
        "Deliver independent frontend and backend slices in parallel",
        false,
        true,
        TeamworkParallelConfig {
            lanes: vec![
                TeamworkLaneConfig {
                    id: "frontend".to_string(),
                    brain: "agy".to_string(),
                    agent_count: 3,
                    parallel_group: "implementation-wave-001".to_string(),
                    responsibility: "Implement isolated frontend slices".to_string(),
                },
                TeamworkLaneConfig {
                    id: "backend".to_string(),
                    brain: "codex".to_string(),
                    agent_count: 5,
                    parallel_group: "implementation-wave-001".to_string(),
                    responsibility: "Implement isolated backend slices".to_string(),
                },
            ],
            max_parallel_agents: 8,
        },
    )
    .unwrap();
    let preparation = prepare_teamwork_worktrees(
        store,
        TeamworkWorktreePrepareOptions {
            workflow_id: response.workflow_id.clone(),
            repository: repository.to_path_buf(),
            worktree_root: worktree_root.to_path_buf(),
            branch_prefix: "foundry/request-wave-test".to_string(),
            origin: "request_execute_wave_test".to_string(),
            allow_repository_mutation: true,
        },
    )
    .unwrap();
    assert_eq!(preparation.parallel_branch_worktrees, 8);
    assert_eq!(preparation.created_worktrees, preparation.planned_worktrees);

    let workflow = store.load_workflow(&response.workflow_id).unwrap();
    let branch_task_ids = workflow
        .tasks
        .iter()
        .filter(|task| {
            (task.id.starts_with("task-005-frontend-") || task.id.starts_with("task-005-backend-"))
                && !task.id.ends_with("-join")
        })
        .map(|task| task.id.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(branch_task_ids.len(), 8);
    assert!(workflow
        .tasks
        .iter()
        .filter(|task| branch_task_ids.contains(&task.id))
        .all(|task| task.status == TaskStatus::Pending));

    let run = create_run_record(&workflow, "parallel-teamwork-wave-test", "accepted");
    save_run_record(store, &run).unwrap();
    ParallelTeamworkFixture {
        workflow_id: workflow.id,
        run_id: run.run_id,
        branch_task_ids,
    }
}

fn execute_parallel_wave(
    store_path: &Path,
    run_id: &str,
    max_parallel: usize,
) -> serde_json::Value {
    execute_parallel_wave_with_skip(store_path, run_id, max_parallel, None)
}

fn apply_dependency_fan_in(
    store_path: &Path,
    workflow_id: &str,
    task_id: &str,
    approved_by: &str,
) -> TeamworkFanInReport {
    let store = FoundryStore::open(store_path).unwrap();
    integrate_worktree_dependencies(
        &store,
        &IntegrateDependenciesOptions {
            workflow_id,
            task_id,
            allow_repository_mutation: true,
            approved_by,
            reason: "converge validated dependency commits before executor dispatch",
            origin: "parallel-wave-test",
        },
    )
    .unwrap()
}

fn execute_parallel_wave_with_skip(
    store_path: &Path,
    run_id: &str,
    max_parallel: usize,
    skip_commit_task: Option<&str>,
) -> serde_json::Value {
    let resource_snapshot = serde_json::json!({
        "cpu_count": 16,
        "load_one": 0.0,
        "memory_available_bytes": 34_359_738_368_u64,
        "swap_free_bytes": 8_589_934_592_u64,
        "disk_free_bytes": 536_870_912_000_u64,
        "disk_total_bytes": 1_073_741_824_000_u64
    })
    .to_string();
    let mut command = foundry();
    command.env(
        "FOUNDRY_TEST_HOST_RESOURCE_SNAPSHOT_JSON",
        resource_snapshot,
    );
    if let Some(task_id) = skip_commit_task {
        command.env("FOUNDRY_TEST_SKIP_COMMIT_TASK", task_id);
    }
    let output = command
        .arg("--store")
        .arg(store_path)
        .args([
            "request",
            "execute-wave",
            "--run",
            run_id,
            "--executor",
            "auto",
            "--ttl-seconds",
            "60",
            "--timeout-seconds",
            "5",
            "--context-budget",
            "4096",
            "--max-parallel",
            &max_parallel.to_string(),
            "--allow-exec",
            "--approved-by",
            "parallel-wave-test",
            "--reason",
            "exercise mixed Agy and Codex request wave without auto-promotion",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).unwrap()
}

fn unique_receipt_field(receipts: &[serde_json::Value], field: &str) -> BTreeSet<String> {
    receipts
        .iter()
        .map(|receipt| receipt[field].as_str().unwrap().to_string())
        .collect()
}

fn extend_unique_receipt_identity(
    receipts: &[serde_json::Value],
    lease_ids: &mut BTreeSet<String>,
    worktree_ids: &mut BTreeSet<String>,
) {
    for receipt in receipts {
        let task_id = receipt["task_id"].as_str().unwrap();
        let lease_id = receipt["lease_id"].as_str().unwrap().to_string();
        let worktree_id = receipt["worktree_id"].as_str().unwrap().to_string();
        assert!(
            lease_ids.insert(lease_id),
            "task {task_id} reused another wave lease"
        );
        assert!(
            worktree_ids.insert(worktree_id),
            "task {task_id} reused another task worktree"
        );
        assert_eq!(receipt["workspace_binding_scope"], "task");
    }
}

fn complete_wave_tasks(store_path: &Path, run_id: &str, receipts: &[serde_json::Value]) {
    for receipt in receipts {
        let task_id = receipt["task_id"].as_str().unwrap();
        let executor = receipt["executor"].as_str().unwrap();
        let execution_id = receipt["execution_id"].as_str().unwrap();
        let summary = format!("Reviewed executor receipt {execution_id} for {task_id}");
        let evidence_summary = format!("Stub execution {execution_id} exited successfully");
        let output = foundry()
            .arg("--store")
            .arg(store_path)
            .args(["request", "complete-task", "--run", run_id, "--task"])
            .arg(task_id)
            .args(["--executor", executor, "--summary"])
            .arg(summary)
            .args([
                "--evidence-command",
                "parallel-executor-stub",
                "--evidence-exit-code",
                "0",
                "--evidence-summary",
            ])
            .arg(evidence_summary)
            .args(["--origin", "parallel-wave-test", "--output", "json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let completion: serde_json::Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(completion["status"], "completed");
        assert_eq!(completion["task_id"], task_id);
        assert_eq!(completion["validation"]["accepted"], true);
    }
}

fn assert_task_statuses(
    store: &FoundryStore,
    workflow_id: &str,
    task_ids: &BTreeSet<String>,
    expected: TaskStatus,
) {
    let workflow = store.load_workflow(workflow_id).unwrap();
    for task_id in task_ids {
        let task = workflow
            .tasks
            .iter()
            .find(|task| task.id == *task_id)
            .unwrap();
        assert_eq!(task.status, expected, "unexpected status for {task_id}");
    }
}

fn assert_branches_remain_pending(store: &FoundryStore, fixture: &ParallelTeamworkFixture) {
    let workflow = store.load_workflow(&fixture.workflow_id).unwrap();
    assert!(workflow
        .tasks
        .iter()
        .filter(|task| fixture.branch_task_ids.contains(&task.id))
        .all(|task| task.status == TaskStatus::Pending));
}

fn initialize_repository(repository: &Path) {
    fs::create_dir_all(repository).unwrap();
    git(repository, &["init", "-q"]);
    git(
        repository,
        &["config", "user.email", "foundry-tests@example.invalid"],
    );
    git(repository, &["config", "user.name", "Foundry Tests"]);
    fs::write(repository.join("README.md"), "executor wave fixture\n").unwrap();
    git(repository, &["add", "README.md"]);
    git(repository, &["commit", "-q", "-m", "fixture"]);
}

fn git(repository: &Path, args: &[&str]) {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repository)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn save_ready_executor(store: &FoundryStore, executor: &str, command_path: &Path) {
    let state = ExecutorState {
        id: executor.to_string(),
        display_name: format!("{executor} stub"),
        command: executor.to_string(),
        installed: true,
        configured: true,
        command_path: Some(command_path.display().to_string()),
        config_evidence: vec!["test executable".to_string()],
        non_interactive_ready: true,
        probe_evidence: vec!["test non-interactive probe".to_string()],
        foundry_first_ready: false,
        foundry_first_entrypoint: None,
        harness_status: None,
        allowed: true,
        decision_source: "execute_wave_test".to_string(),
        synced_at: Utc::now().to_rfc3339(),
    };
    store
        .save_executor_state(executor, &serde_json::to_value(state).unwrap())
        .unwrap();
    save_executor_quota_observation(store, executor, "available", "low");
}

fn save_executor_quota_observation(
    store: &FoundryStore,
    executor: &str,
    remaining_quota: &str,
    rate_limit_risk: &str,
) {
    store
        .save_executor_quota(
            executor,
            executor,
            "test",
            &serde_json::json!({
                "executor": executor,
                "provider": executor,
                "model": "test",
                "local_vs_non_local": "non_local",
                "free_vs_paid_if_known": "unknown",
                "remaining_quota": remaining_quota,
                "rate_limit_risk": rate_limit_risk,
                "monetary_or_token_cost": "unknown",
                "latency": "test",
                "expected_quality": "test",
                "suitability": "test",
                "source": "execute_wave_test",
                "observed_at": Utc::now().to_rfc3339()
            }),
        )
        .unwrap();
}

fn write_parallel_stub(path: &Path) {
    fs::write(
        path,
        r#"#!/bin/sh
marker_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
mkdir -p "$marker_dir/parallel-invocations"
printf '%s\n' "$@" > "$marker_dir/parallel-invocations/$FOUNDRY_TASK_ID.args"
cat > "$marker_dir/parallel-invocations/$FOUNDRY_TASK_ID.stdin"
if [ "${FOUNDRY_TEST_SKIP_COMMIT_TASK:-}" != "$FOUNDRY_TASK_ID" ]; then
  task_file="foundry-runtime-$FOUNDRY_TASK_ID.txt"
  printf 'committed by %s\n' "$FOUNDRY_TASK_ID" > "$task_file"
  git add -- "$task_file"
  git commit -q -m "foundry runtime $FOUNDRY_TASK_ID"
fi
printf '{"stub":"parallel","task":"%s"}\n' "$FOUNDRY_TASK_ID"
"#,
    )
    .unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

fn parallel_stub_invocations(root: &Path) -> usize {
    fs::read_dir(root.join("parallel-invocations"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|value| value == "stdin")
        })
        .count()
}

fn write_codex_stub(path: &Path) {
    fs::write(
        path,
        r#"#!/bin/sh
marker_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
printf '%s\n' "$@" > "$marker_dir/codex-stub.args"
cat > "$marker_dir/codex-stub.stdin"
printf '{"stub":"codex"}\n'
"#,
    )
    .unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}
