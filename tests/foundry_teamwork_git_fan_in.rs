use foundry_core::graph::TaskStatus;
use foundry_core::storage::FoundryStore;
use foundry_core::teamwork::{
    plan_teamwork_workflow_with_config, prepare_teamwork_worktrees, TeamworkLaneConfig,
    TeamworkParallelConfig, TeamworkWorktreePreparationReport, TeamworkWorktreePrepareOptions,
};
use foundry_core::teamwork_fan_in::{
    current_teamwork_fan_in_status, integrate_worktree_dependencies, IntegrateDependenciesOptions,
};
use foundry_core::worktree::{bound_worktree_context, inspect_registered_worktree};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::{tempdir, TempDir};

const FRONTEND_JOIN: &str = "task-005-frontend-join";
const BACKEND_JOIN: &str = "task-005-backend-join";
const FINAL_AUDITOR: &str = "task-006";

struct Fixture {
    _temporary: TempDir,
    store: FoundryStore,
    workflow_id: String,
    paths: HashMap<String, PathBuf>,
}

impl Fixture {
    fn path(&self, task_id: &str) -> &Path {
        self.paths
            .get(task_id)
            .unwrap_or_else(|| panic!("missing prepared worktree for {task_id}"))
    }
}

fn git(repository: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed in {}: {}",
        args,
        repository.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
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
        &["config", "user.name", "Foundry Git Fan-In Tests"],
    );
    git(repository, &["config", "core.autocrlf", "false"]);
    fs::write(repository.join("README.md"), "fan-in fixture\n").unwrap();
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

fn config(frontend_agents: usize, backend_agents: usize) -> TeamworkParallelConfig {
    TeamworkParallelConfig {
        lanes: vec![
            TeamworkLaneConfig {
                id: "frontend".to_string(),
                brain: "agy".to_string(),
                agent_count: frontend_agents,
                parallel_group: "implementation-wave-001".to_string(),
                responsibility: "Implement isolated frontend slices".to_string(),
            },
            TeamworkLaneConfig {
                id: "backend".to_string(),
                brain: "codex".to_string(),
                agent_count: backend_agents,
                parallel_group: "implementation-wave-001".to_string(),
                responsibility: "Implement isolated backend slices".to_string(),
            },
        ],
        max_parallel_agents: frontend_agents + backend_agents,
    }
}

fn fixture(frontend_agents: usize, backend_agents: usize) -> Fixture {
    let temporary = tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let worktree_root = temporary.path().join("teamwork-worktrees");
    initialize_repository(&repository);
    let store = FoundryStore::open(temporary.path().join("foundry.sqlite")).unwrap();
    let workflow_id = plan_teamwork_workflow_with_config(
        &store,
        "Deliver frontend with Agy and backend with Codex in parallel",
        false,
        false,
        config(frontend_agents, backend_agents),
    )
    .unwrap()
    .workflow_id;
    let report = prepare_teamwork_worktrees(
        &store,
        TeamworkWorktreePrepareOptions {
            workflow_id: workflow_id.clone(),
            repository,
            worktree_root,
            branch_prefix: "foundry/git-fan-in-test".to_string(),
            origin: "git-fan-in-test".to_string(),
            allow_repository_mutation: true,
        },
    )
    .unwrap();
    Fixture {
        _temporary: temporary,
        store,
        workflow_id,
        paths: preparation_paths(&report),
    }
}

fn preparation_paths(report: &TeamworkWorktreePreparationReport) -> HashMap<String, PathBuf> {
    report
        .entries
        .iter()
        .map(|entry| (entry.task_id.clone(), PathBuf::from(&entry.path)))
        .collect()
}

fn worker_id(lane: &str, index: usize) -> String {
    format!("task-005-{lane}-{index:03}")
}

fn commit_file(worktree: &Path, path: &str, contents: &str) -> String {
    fs::write(worktree.join(path), contents).unwrap();
    git(worktree, &["add", "--", path]);
    git(
        worktree,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            &format!("implement {path}"),
        ],
    );
    git(worktree, &["rev-parse", "HEAD"])
}

fn complete_tasks(fixture: &Fixture, task_ids: &[String]) {
    let mut workflow = fixture.store.load_workflow(&fixture.workflow_id).unwrap();
    for task_id in task_ids {
        workflow
            .tasks
            .iter_mut()
            .find(|task| task.id == *task_id)
            .unwrap_or_else(|| panic!("missing task {task_id}"))
            .status = TaskStatus::Completed;
    }
    fixture.store.save_workflow(&workflow).unwrap();
}

fn integration_options<'a>(
    workflow_id: &'a str,
    task_id: &'a str,
    allow_repository_mutation: bool,
) -> IntegrateDependenciesOptions<'a> {
    IntegrateDependenciesOptions {
        workflow_id,
        task_id,
        allow_repository_mutation,
        approved_by: if allow_repository_mutation {
            "integration-test"
        } else {
            ""
        },
        reason: if allow_repository_mutation {
            "converge dependency worktrees"
        } else {
            ""
        },
        origin: "git-fan-in-test",
    }
}

fn first_parent_commit_count(worktree: &Path, before: &str, after: &str) -> usize {
    git(
        worktree,
        &[
            "rev-list",
            "--first-parent",
            "--count",
            &format!("{before}..{after}"),
        ],
    )
    .parse()
    .unwrap()
}

fn parent_count(worktree: &Path, commit: &str) -> usize {
    git(worktree, &["rev-list", "--parents", "-n", "1", commit])
        .split_whitespace()
        .count()
        - 1
}

#[test]
fn git_fan_in_converges_parallel_lanes_rebinds_and_replays_without_a_new_commit() {
    let fixture = fixture(2, 2);
    let frontend_workers = [worker_id("frontend", 1), worker_id("frontend", 2)];
    let backend_workers = [worker_id("backend", 1), worker_id("backend", 2)];
    let frontend_files = ["frontend-001.txt", "frontend-002.txt"];
    let backend_files = ["backend-001.txt", "backend-002.txt"];

    for (task_id, path) in frontend_workers.iter().zip(frontend_files) {
        commit_file(fixture.path(task_id), path, &format!("{task_id}\n"));
    }
    for (task_id, path) in backend_workers.iter().zip(backend_files) {
        commit_file(fixture.path(task_id), path, &format!("{task_id}\n"));
    }
    complete_tasks(
        &fixture,
        &frontend_workers
            .iter()
            .chain(&backend_workers)
            .cloned()
            .collect::<Vec<_>>(),
    );

    let frontend_join = fixture.path(FRONTEND_JOIN);
    let frontend_pre_head = git(frontend_join, &["rev-parse", "HEAD"]);
    let events_before_dry_run = fixture
        .store
        .load_workflow_events(&fixture.workflow_id)
        .unwrap()
        .len();
    let revisions_before_dry_run = fixture
        .store
        .load_workflow(&fixture.workflow_id)
        .unwrap()
        .revisions
        .len();
    let dry_run = integrate_worktree_dependencies(
        &fixture.store,
        &integration_options(&fixture.workflow_id, FRONTEND_JOIN, false),
    )
    .unwrap();
    assert!(dry_run.success);
    assert!(dry_run.dry_run);
    assert!(!dry_run.repository_mutation_attempted);
    assert_eq!(
        git(frontend_join, &["rev-parse", "HEAD"]),
        frontend_pre_head
    );
    assert_eq!(
        fixture
            .store
            .load_workflow_events(&fixture.workflow_id)
            .unwrap()
            .len(),
        events_before_dry_run
    );
    assert_eq!(
        fixture
            .store
            .load_workflow(&fixture.workflow_id)
            .unwrap()
            .revisions
            .len(),
        revisions_before_dry_run
    );

    let frontend = integrate_worktree_dependencies(
        &fixture.store,
        &integration_options(&fixture.workflow_id, FRONTEND_JOIN, true),
    )
    .unwrap();
    assert!(frontend.success);
    assert!(frontend.commit_created);
    assert!(frontend.destination_rebound);
    assert_eq!(frontend.destination.binding_head, frontend.result_head);
    assert_eq!(
        first_parent_commit_count(frontend_join, &frontend_pre_head, &frontend.result_head),
        1
    );
    assert_eq!(parent_count(frontend_join, &frontend.result_head), 3);
    assert_eq!(
        frontend
            .integrated_paths
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>(),
        frontend_files.iter().map(|path| path.to_string()).collect()
    );
    let frontend_status =
        current_teamwork_fan_in_status(&fixture.store, &fixture.workflow_id, FRONTEND_JOIN)
            .unwrap();
    assert!(frontend_status.current);
    assert!(frontend_status.destination_binding_current);
    let frontend_context =
        bound_worktree_context(&fixture.store, &fixture.workflow_id, Some(FRONTEND_JOIN))
            .unwrap()
            .unwrap();
    assert!(!frontend_context.binding_drifted);
    let revision_after_frontend = fixture
        .store
        .load_workflow(&fixture.workflow_id)
        .unwrap()
        .revisions
        .len();

    let replay = integrate_worktree_dependencies(
        &fixture.store,
        &integration_options(&fixture.workflow_id, FRONTEND_JOIN, true),
    )
    .unwrap();
    assert!(replay.success);
    assert!(replay.replay);
    assert!(!replay.commit_created);
    assert!(!replay.destination_rebound);
    assert_eq!(replay.result_head, frontend.result_head);
    assert_eq!(
        fixture
            .store
            .load_workflow(&fixture.workflow_id)
            .unwrap()
            .revisions
            .len(),
        revision_after_frontend
    );
    assert_eq!(
        git(frontend_join, &["rev-parse", "HEAD"]),
        frontend.result_head
    );

    let backend_join = fixture.path(BACKEND_JOIN);
    let backend_pre_head = git(backend_join, &["rev-parse", "HEAD"]);
    let backend = integrate_worktree_dependencies(
        &fixture.store,
        &integration_options(&fixture.workflow_id, BACKEND_JOIN, true),
    )
    .unwrap();
    assert!(backend.success);
    assert_eq!(parent_count(backend_join, &backend.result_head), 3);
    assert_eq!(
        first_parent_commit_count(backend_join, &backend_pre_head, &backend.result_head),
        1
    );

    complete_tasks(
        &fixture,
        &[FRONTEND_JOIN.to_string(), BACKEND_JOIN.to_string()],
    );
    let final_worktree = fixture.path(FINAL_AUDITOR);
    let final_pre_head = git(final_worktree, &["rev-parse", "HEAD"]);
    let final_report = integrate_worktree_dependencies(
        &fixture.store,
        &integration_options(&fixture.workflow_id, FINAL_AUDITOR, true),
    )
    .unwrap();
    assert!(final_report.success);
    assert!(final_report.destination_rebound);
    assert_eq!(parent_count(final_worktree, &final_report.result_head), 3);
    assert_eq!(
        first_parent_commit_count(final_worktree, &final_pre_head, &final_report.result_head),
        1
    );
    let frontend_lineage = final_report
        .sources
        .iter()
        .find(|source| source.worktree.task_id == FRONTEND_JOIN)
        .unwrap();
    let backend_lineage = final_report
        .sources
        .iter()
        .find(|source| source.worktree.task_id == BACKEND_JOIN)
        .unwrap();
    assert_eq!(
        frontend_lineage
            .changed_paths
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>(),
        frontend_files.iter().map(|path| path.to_string()).collect()
    );
    assert_eq!(
        backend_lineage
            .changed_paths
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>(),
        backend_files.iter().map(|path| path.to_string()).collect()
    );
    for path in frontend_files.iter().chain(&backend_files) {
        assert!(final_worktree.join(path).is_file(), "missing {path}");
    }
    let final_status =
        current_teamwork_fan_in_status(&fixture.store, &fixture.workflow_id, FINAL_AUDITOR)
            .unwrap();
    assert!(final_status.current);
    assert!(final_status.destination_binding_current);
}

#[test]
fn git_fan_in_conflict_restores_the_destination_and_preserves_every_source_head() {
    let fixture = fixture(2, 1);
    let frontend_workers = [worker_id("frontend", 1), worker_id("frontend", 2)];
    for (index, task_id) in frontend_workers.iter().enumerate() {
        commit_file(
            fixture.path(task_id),
            "README.md",
            &format!("conflicting frontend value {index}\n"),
        );
    }
    complete_tasks(&fixture, &frontend_workers);

    let destination = fixture.path(FRONTEND_JOIN);
    let pre_head = git(destination, &["rev-parse", "HEAD"]);
    let source_heads = frontend_workers
        .iter()
        .map(|task_id| {
            (
                task_id.clone(),
                git(fixture.path(task_id), &["rev-parse", "HEAD"]),
            )
        })
        .collect::<HashMap<_, _>>();
    let revisions_before = fixture
        .store
        .load_workflow(&fixture.workflow_id)
        .unwrap()
        .revisions
        .len();

    let report = integrate_worktree_dependencies(
        &fixture.store,
        &integration_options(&fixture.workflow_id, FRONTEND_JOIN, true),
    )
    .unwrap();
    assert!(!report.success);
    assert_eq!(report.status, "integration_conflict");
    assert!(!report.commit_created);
    assert!(!report.destination_rebound);
    assert!(report.atomicity.rollback_required);
    assert!(report.atomicity.rollback_verified);
    assert_eq!(report.conflict_paths, vec!["README.md".to_string()]);
    assert_eq!(git(destination, &["rev-parse", "HEAD"]), pre_head);
    assert!(git(destination, &["status", "--porcelain"]).is_empty());
    assert_eq!(
        fixture
            .store
            .load_workflow(&fixture.workflow_id)
            .unwrap()
            .revisions
            .len(),
        revisions_before
    );
    for task_id in &frontend_workers {
        assert_eq!(
            git(fixture.path(task_id), &["rev-parse", "HEAD"]),
            source_heads[task_id]
        );
        assert!(git(fixture.path(task_id), &["status", "--porcelain"]).is_empty());
    }
    let destination_record =
        inspect_registered_worktree(&fixture.store, &report.destination.worktree_id).unwrap();
    assert_eq!(destination_record.head, pre_head);
    assert!(!destination_record.dirty);
    let status =
        current_teamwork_fan_in_status(&fixture.store, &fixture.workflow_id, FRONTEND_JOIN)
            .unwrap();
    assert!(!status.current);
    assert!(!status.receipt_successful);
}
