use assert_cmd::Command as AssertCommand;
use foundry_core::graph::TaskStatus;
use foundry_core::storage::FoundryStore;
use foundry_core::teamwork::{
    plan_teamwork_workflow_with_config, prepare_teamwork_worktrees, TeamworkLaneConfig,
    TeamworkParallelConfig, TeamworkWorktreePreparationReport, TeamworkWorktreePrepareOptions,
};
use foundry_core::worktree::{discover_worktrees, list_registered_worktrees};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

fn git(repository: &Path, args: &[&str]) {
    let output = Command::new("git")
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

fn initialize_repository(repository: &Path) {
    fs::create_dir_all(repository).unwrap();
    let output = Command::new("git")
        .arg("init")
        .arg("--initial-branch=main")
        .arg(repository)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    git(
        repository,
        &["config", "user.email", "foundry@example.invalid"],
    );
    git(
        repository,
        &["config", "user.name", "Foundry Teamwork Tests"],
    );
    fs::write(repository.join("README.md"), "teamwork fixture\n").unwrap();
    git(repository, &["add", "README.md"]);
    git(
        repository,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            "fixture",
        ],
    );
}

fn frontend_backend_config() -> TeamworkParallelConfig {
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
    }
}

fn plan_teamwork(store: &FoundryStore) -> String {
    plan_teamwork_workflow_with_config(
        store,
        "Deliver frontend with Agy and backend with Codex in parallel",
        false,
        false,
        frontend_backend_config(),
    )
    .unwrap()
    .workflow_id
}

fn options(
    workflow_id: &str,
    repository: &Path,
    worktree_root: &Path,
    allow_repository_mutation: bool,
) -> TeamworkWorktreePrepareOptions {
    TeamworkWorktreePrepareOptions {
        workflow_id: workflow_id.to_string(),
        repository: repository.to_path_buf(),
        worktree_root: worktree_root.to_path_buf(),
        branch_prefix: "foundry/teamwork-test".to_string(),
        origin: "teamwork-worktree-test".to_string(),
        allow_repository_mutation,
    }
}

fn cli_prepare_teamwork(
    store_path: &Path,
    workflow_id: &str,
    repository: &Path,
    worktree_root: &Path,
    allow_repository_mutation: bool,
) -> TeamworkWorktreePreparationReport {
    let mut command = AssertCommand::cargo_bin("foundry").expect("foundry binary should build");
    command
        .arg("--store")
        .arg(store_path)
        .args(["worktree", "prepare-teamwork", "--workflow", workflow_id])
        .arg("--repository")
        .arg(repository)
        .arg("--worktree-root")
        .arg(worktree_root)
        .args([
            "--branch-prefix",
            "foundry/teamwork-cli-test",
            "--origin",
            "teamwork-cli-test",
            "--output",
            "json",
        ]);
    if allow_repository_mutation {
        command.arg("--allow-repository-mutation");
    }
    let output = command.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&output).unwrap()
}

fn stored_workflow_ids(store: &FoundryStore) -> BTreeSet<String> {
    store
        .load_workflows()
        .unwrap()
        .into_iter()
        .map(|workflow| workflow.id)
        .collect()
}

fn stored_run_ids(store: &FoundryStore) -> BTreeSet<String> {
    store
        .load_runs()
        .unwrap()
        .into_iter()
        .filter_map(|run| run["run_id"].as_str().map(str::to_string))
        .collect()
}

#[test]
fn teamwork_plan_completes_only_materialized_meta_nodes_and_exposes_eight_ready_branches() {
    let temporary = tempdir().unwrap();
    let store = FoundryStore::open(temporary.path().join("foundry.sqlite")).unwrap();
    let workflow_id = plan_teamwork(&store);
    let workflow = store.load_workflow(&workflow_id).unwrap();

    let completed = workflow
        .tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Completed)
        .map(|task| task.id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        completed,
        BTreeSet::from(["task-001", "task-002", "task-003", "task-004"])
    );
    for task_id in ["task-001", "task-002", "task-003", "task-004"] {
        let task = workflow
            .tasks
            .iter()
            .find(|task| task.id == task_id)
            .unwrap();
        assert!(task.work_item.goal_validation.definitively_ready);
        assert!(task.work_item.impediments.is_empty());
        assert!(task.active_impediments.is_empty());
        assert!(task
            .work_item
            .subtasks
            .iter()
            .all(|subtask| subtask.status == TaskStatus::Completed));
    }

    let branches = workflow
        .tasks
        .iter()
        .filter(|task| {
            task.node_brain_routing
                .agent_slots
                .iter()
                .any(|slot| slot.parallel_group == "implementation-wave-001")
        })
        .collect::<Vec<_>>();
    assert_eq!(branches.len(), 8);
    assert!(branches.iter().all(|task| {
        task.status == TaskStatus::Pending
            && task.dependencies.iter().all(|dependency| {
                workflow.tasks.iter().any(|candidate| {
                    candidate.id == *dependency && candidate.status == TaskStatus::Completed
                })
            })
    }));
    assert!(workflow
        .tasks
        .iter()
        .filter(|task| task.id.contains("join") || task.id == "task-006")
        .all(|task| task.status != TaskStatus::Completed));
    assert!(workflow.revisions.iter().any(|revision| {
        revision.change_type == "teamwork_planning_nodes_materialized"
            && revision.summary.contains("task-001")
            && revision
                .summary
                .contains("branches and joins remain pending")
    }));
    let planned_event = store
        .load_workflow_events(&workflow_id)
        .unwrap()
        .into_iter()
        .find(|event| event.kind == "teamwork_planned")
        .unwrap();
    assert_eq!(
        planned_event.data["planning_evidence"]["status"],
        "materialized"
    );
    assert_eq!(
        planned_event.data["planning_evidence"]["completed_task_ids"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
}

#[test]
fn worktree_dry_run_is_read_only_and_returns_explicit_commands_for_all_agentic_tasks() {
    let temporary = tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let worktree_root = temporary.path().join("teamwork-worktrees");
    initialize_repository(&repository);
    let store = FoundryStore::open(temporary.path().join("foundry.sqlite")).unwrap();
    let workflow_id = plan_teamwork(&store);
    let revision_count = store.load_workflow(&workflow_id).unwrap().revisions.len();
    let event_count = store.load_workflow_events(&workflow_id).unwrap().len();

    let report = prepare_teamwork_worktrees(
        &store,
        options(&workflow_id, &repository, &worktree_root, false),
    )
    .unwrap();

    assert_eq!(report.status, "teamwork_worktrees_planned");
    assert!(!report.mutation_authorized);
    assert_eq!(report.parallel_branch_worktrees, 8);
    assert_eq!(report.planned_worktrees, 12);
    assert_eq!(report.supporting_agent_worktrees, 4);
    assert_eq!(report.created_worktrees, 0);
    assert_eq!(report.bound_existing_worktrees, 0);
    assert_eq!(report.reused_worktrees, 0);
    assert_eq!(report.commands.len(), report.planned_worktrees * 2);
    assert!(report.commands.iter().all(|command| {
        command.first().map(String::as_str) == Some("foundry")
            && command.iter().any(|argument| argument == "--output")
            && command.iter().any(|argument| argument == "json")
    }));
    assert!(!worktree_root.exists());
    assert_eq!(discover_worktrees(&repository).unwrap().worktree_count, 1);
    assert_eq!(
        list_registered_worktrees(&store, Some(&repository), None)
            .unwrap()
            .count,
        0
    );
    assert_eq!(
        store.load_workflow(&workflow_id).unwrap().revisions.len(),
        revision_count
    );
    assert_eq!(
        store.load_workflow_events(&workflow_id).unwrap().len(),
        event_count
    );
    let entry_ids = report
        .entries
        .iter()
        .map(|entry| entry.task_id.as_str())
        .collect::<BTreeSet<_>>();
    assert!(!entry_ids.contains("task-003"));
    assert!(entry_ids.contains("task-005-frontend-join"));
    assert!(entry_ids.contains("task-005-backend-join"));
    assert!(entry_ids.contains("task-006"));
    assert!(entry_ids.contains("task-008"));
}

#[test]
fn worktree_apply_creates_unique_bindings_and_rerun_is_idempotent() {
    let temporary = tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let worktree_root = temporary.path().join("teamwork-worktrees");
    initialize_repository(&repository);
    let store = FoundryStore::open(temporary.path().join("foundry.sqlite")).unwrap();
    let workflow_id = plan_teamwork(&store);

    let report = prepare_teamwork_worktrees(
        &store,
        options(&workflow_id, &repository, &worktree_root, true),
    )
    .unwrap();
    assert_eq!(report.status, "teamwork_worktrees_prepared");
    assert!(report.mutation_authorized);
    assert_eq!(report.parallel_branch_worktrees, 8);
    assert_eq!(report.planned_worktrees, 12);
    assert_eq!(report.created_worktrees, 12);
    assert_eq!(report.bound_existing_worktrees, 0);
    assert_eq!(report.reused_worktrees, 0);
    assert_eq!(
        report
            .entries
            .iter()
            .filter(|entry| entry.parallel_branch)
            .count(),
        8
    );
    assert!(report.entries.iter().all(|entry| {
        entry.status == "created_and_bound"
            && entry
                .claim
                .as_ref()
                .is_some_and(|claim| claim.binding_scope == "task")
    }));
    assert_eq!(
        report
            .entries
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        12
    );
    assert_eq!(
        report
            .entries
            .iter()
            .map(|entry| entry.branch.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        12
    );
    assert_eq!(
        report
            .entries
            .iter()
            .filter_map(|entry| entry.worktree_id.as_deref())
            .collect::<BTreeSet<_>>()
            .len(),
        12
    );
    let registered =
        list_registered_worktrees(&store, Some(&repository), Some(&workflow_id)).unwrap();
    assert_eq!(registered.count, 12);
    assert!(registered.worktrees.iter().all(|worktree| {
        worktree.bindings.len() == 1
            && worktree.bindings[0].workflow_id == workflow_id
            && worktree.bindings[0].task_id.is_some()
    }));
    assert_eq!(discover_worktrees(&repository).unwrap().worktree_count, 13);

    let revision_count = store.load_workflow(&workflow_id).unwrap().revisions.len();
    let event_count = store.load_workflow_events(&workflow_id).unwrap().len();
    let rerun = prepare_teamwork_worktrees(
        &store,
        options(&workflow_id, &repository, &worktree_root, true),
    )
    .unwrap();
    assert_eq!(rerun.status, "teamwork_worktrees_already_prepared");
    assert_eq!(rerun.created_worktrees, 0);
    assert_eq!(rerun.bound_existing_worktrees, 0);
    assert_eq!(rerun.reused_worktrees, 12);
    assert!(rerun.commands.is_empty());
    assert!(rerun.entries.iter().all(|entry| entry.status == "reused"));
    assert_eq!(discover_worktrees(&repository).unwrap().worktree_count, 13);
    assert_eq!(
        store.load_workflow(&workflow_id).unwrap().revisions.len(),
        revision_count
    );
    assert_eq!(
        store.load_workflow_events(&workflow_id).unwrap().len(),
        event_count
    );
}

#[test]
fn cli_prepare_teamwork_dry_run_apply_and_replay_reuse_the_same_workflow_and_run() {
    let temporary = tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let worktree_root = temporary.path().join("teamwork-worktrees");
    let store_path = temporary.path().join("foundry.sqlite");
    initialize_repository(&repository);

    let store = FoundryStore::open(&store_path).unwrap();
    let workflow_id = plan_teamwork(&store);
    let workflow_ids_before = stored_workflow_ids(&store);
    let run_ids_before = stored_run_ids(&store);
    assert_eq!(workflow_ids_before, BTreeSet::from([workflow_id.clone()]));
    assert_eq!(run_ids_before.len(), 1);
    drop(store);

    let dry_run = cli_prepare_teamwork(
        &store_path,
        &workflow_id,
        &repository,
        &worktree_root,
        false,
    );
    assert_eq!(dry_run.status, "teamwork_worktrees_planned");
    assert_eq!(dry_run.workflow_id, workflow_id);
    assert!(!dry_run.mutation_authorized);
    assert_eq!(dry_run.planned_worktrees, 12);
    assert_eq!(dry_run.created_worktrees, 0);
    assert!(!worktree_root.exists());

    let store = FoundryStore::open(&store_path).unwrap();
    assert_eq!(stored_workflow_ids(&store), workflow_ids_before);
    assert_eq!(stored_run_ids(&store), run_ids_before);
    drop(store);

    let applied =
        cli_prepare_teamwork(&store_path, &workflow_id, &repository, &worktree_root, true);
    assert_eq!(applied.status, "teamwork_worktrees_prepared");
    assert_eq!(applied.workflow_id, workflow_id);
    assert!(applied.mutation_authorized);
    assert_eq!(applied.created_worktrees, 12);
    assert_eq!(applied.reused_worktrees, 0);

    let store = FoundryStore::open(&store_path).unwrap();
    assert_eq!(stored_workflow_ids(&store), workflow_ids_before);
    assert_eq!(stored_run_ids(&store), run_ids_before);
    let revision_count = store.load_workflow(&workflow_id).unwrap().revisions.len();
    let event_count = store.load_workflow_events(&workflow_id).unwrap().len();
    drop(store);

    let replay = cli_prepare_teamwork(&store_path, &workflow_id, &repository, &worktree_root, true);
    assert_eq!(replay.status, "teamwork_worktrees_already_prepared");
    assert_eq!(replay.workflow_id, workflow_id);
    assert_eq!(replay.created_worktrees, 0);
    assert_eq!(replay.bound_existing_worktrees, 0);
    assert_eq!(replay.reused_worktrees, 12);
    assert!(replay.commands.is_empty());

    let store = FoundryStore::open(&store_path).unwrap();
    assert_eq!(stored_workflow_ids(&store), workflow_ids_before);
    assert_eq!(stored_run_ids(&store), run_ids_before);
    assert_eq!(
        store.load_workflow(&workflow_id).unwrap().revisions.len(),
        revision_count
    );
    assert_eq!(
        store.load_workflow_events(&workflow_id).unwrap().len(),
        event_count
    );
}

#[test]
fn worktree_preflight_rejects_broad_roots_and_collisions_before_any_git_mutation() {
    let temporary = tempdir().unwrap();
    let repository = temporary.path().join("repository");
    initialize_repository(&repository);
    let store = FoundryStore::open(temporary.path().join("foundry.sqlite")).unwrap();
    let workflow_id = plan_teamwork(&store);

    let equal = prepare_teamwork_worktrees(
        &store,
        options(&workflow_id, &repository, &repository, false),
    )
    .unwrap_err();
    assert!(equal.to_string().contains("too broad"));
    let broad = prepare_teamwork_worktrees(
        &store,
        options(&workflow_id, &repository, temporary.path(), false),
    )
    .unwrap_err();
    assert!(broad.to_string().contains("too broad"));

    let worktree_root = temporary.path().join("teamwork-worktrees");
    fs::create_dir_all(worktree_root.join("task-005-backend-001")).unwrap();
    let collision = prepare_teamwork_worktrees(
        &store,
        options(&workflow_id, &repository, &worktree_root, true),
    )
    .unwrap_err();
    assert!(collision.to_string().contains("not registered"));
    assert_eq!(discover_worktrees(&repository).unwrap().worktree_count, 1);
    assert_eq!(
        list_registered_worktrees(&store, Some(&repository), None)
            .unwrap()
            .count,
        0
    );
}
