#![cfg(unix)]

use assert_cmd::Command as AssertCommand;
use foundry_core::graph::TaskStatus;
use foundry_core::storage::FoundryStore;
use foundry_core::teamwork::{
    plan_teamwork_workflow_with_config, prepare_teamwork_worktrees, TeamworkLaneConfig,
    TeamworkParallelConfig, TeamworkWorktreePreparationReport, TeamworkWorktreePrepareOptions,
};
use foundry_core::teamwork_fan_in::{
    current_teamwork_fan_in_status, integrate_worktree_dependencies, IntegrateDependenciesOptions,
};
use foundry_core::worktree::{
    bind_worktree, bound_worktree_context, create_worktree, WorktreeBinding, WorktreeCreateOptions,
};
use rusqlite::Connection;
use serde_json::json;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::{tempdir, TempDir};

const FRONTEND_JOIN: &str = "task-005-frontend-join";

struct Fixture {
    _temporary: TempDir,
    store: FoundryStore,
    store_path: PathBuf,
    repository: PathBuf,
    worktree_root: PathBuf,
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

fn git_succeeds(repository: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(args)
        .output()
        .unwrap()
        .status
        .success()
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
        &["config", "user.name", "Foundry Git Fan-In Safety Tests"],
    );
    fs::write(repository.join("README.md"), "fan-in safety fixture\n").unwrap();
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

fn teamwork_config() -> TeamworkParallelConfig {
    TeamworkParallelConfig {
        lanes: vec![
            TeamworkLaneConfig {
                id: "frontend".to_string(),
                brain: "agy".to_string(),
                agent_count: 1,
                parallel_group: "implementation-wave-001".to_string(),
                responsibility: "Implement an isolated frontend slice".to_string(),
            },
            TeamworkLaneConfig {
                id: "backend".to_string(),
                brain: "codex".to_string(),
                agent_count: 1,
                parallel_group: "implementation-wave-001".to_string(),
                responsibility: "Implement an isolated backend slice".to_string(),
            },
        ],
        max_parallel_agents: 2,
    }
}

fn fixture() -> Fixture {
    let temporary = tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let worktree_root = temporary.path().join("teamwork-worktrees");
    let store_path = temporary.path().join("foundry.sqlite");
    initialize_repository(&repository);
    let store = FoundryStore::open(&store_path).unwrap();
    let workflow_id = plan_teamwork_workflow_with_config(
        &store,
        "Deliver frontend with Agy and backend with Codex in parallel",
        false,
        false,
        teamwork_config(),
    )
    .unwrap()
    .workflow_id;
    let report = prepare_teamwork_worktrees(
        &store,
        TeamworkWorktreePrepareOptions {
            workflow_id: workflow_id.clone(),
            repository: repository.clone(),
            worktree_root: worktree_root.clone(),
            branch_prefix: "foundry/git-fan-in-safety".to_string(),
            origin: "git-fan-in-safety-test".to_string(),
            allow_repository_mutation: true,
        },
    )
    .unwrap();
    Fixture {
        _temporary: temporary,
        store,
        store_path,
        repository,
        worktree_root,
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

fn frontend_worker() -> String {
    "task-005-frontend-001".to_string()
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

fn complete_task(fixture: &Fixture, task_id: &str) {
    let mut workflow = fixture.store.load_workflow(&fixture.workflow_id).unwrap();
    workflow
        .tasks
        .iter_mut()
        .find(|task| task.id == task_id)
        .unwrap_or_else(|| panic!("missing task {task_id}"))
        .status = TaskStatus::Completed;
    fixture.store.save_workflow(&workflow).unwrap();
}

fn integration_options<'a>(
    workflow_id: &'a str,
    task_id: &'a str,
) -> IntegrateDependenciesOptions<'a> {
    IntegrateDependenciesOptions {
        workflow_id,
        task_id,
        allow_repository_mutation: true,
        approved_by: "integration-test",
        reason: "converge dependency worktrees safely",
        origin: "git-fan-in-safety-test",
    }
}

fn task_binding(fixture: &Fixture, task_id: &str) -> (String, WorktreeBinding) {
    let context = bound_worktree_context(&fixture.store, &fixture.workflow_id, Some(task_id))
        .unwrap()
        .unwrap_or_else(|| panic!("missing bound worktree for {task_id}"));
    let binding = context
        .bindings
        .iter()
        .find(|binding| {
            binding.workflow_id == fixture.workflow_id
                && binding.task_id.as_deref() == Some(task_id)
        })
        .unwrap_or_else(|| panic!("missing task-scoped binding for {task_id}"))
        .clone();
    (context.id, binding)
}

fn prepare_frontend_source(fixture: &Fixture, filename: &str) -> (String, String) {
    let task_id = frontend_worker();
    let source_head = commit_file(
        fixture.path(&task_id),
        filename,
        "frontend safety regression\n",
    );
    complete_task(fixture, &task_id);
    (task_id, source_head)
}

#[test]
fn replay_repairs_stale_destination_binding_and_second_replay_is_idempotent() {
    let fixture = fixture();
    let (_source_task, source_head) =
        prepare_frontend_source(&fixture, "frontend-replay-safety.txt");
    let destination = fixture.path(FRONTEND_JOIN);

    git(
        destination,
        &[
            "-c",
            "commit.gpgsign=false",
            "merge",
            "--no-ff",
            "-q",
            "-m",
            "manual dependency integration",
            &source_head,
        ],
    );
    let integrated_head = git(destination, &["rev-parse", "HEAD"]);
    let (_, binding_before_replay) = task_binding(&fixture, FRONTEND_JOIN);
    assert_ne!(binding_before_replay.head_at_binding, integrated_head);

    let first = integrate_worktree_dependencies(
        &fixture.store,
        &integration_options(&fixture.workflow_id, FRONTEND_JOIN),
    )
    .unwrap();
    assert!(first.success);
    assert!(first.replay);
    assert!(!first.commit_created);
    assert!(first.destination_rebound);
    assert_eq!(first.result_head, integrated_head);
    assert_eq!(git(destination, &["rev-parse", "HEAD"]), integrated_head);

    let (_, binding_after_first) = task_binding(&fixture, FRONTEND_JOIN);
    assert_eq!(binding_after_first.head_at_binding, integrated_head);
    let revisions_after_first = fixture
        .store
        .load_workflow(&fixture.workflow_id)
        .unwrap()
        .revisions
        .len();

    let second = integrate_worktree_dependencies(
        &fixture.store,
        &integration_options(&fixture.workflow_id, FRONTEND_JOIN),
    )
    .unwrap();
    assert!(second.success);
    assert!(second.replay);
    assert!(!second.commit_created);
    assert!(!second.destination_rebound);
    assert_eq!(second.result_head, integrated_head);
    assert_eq!(git(destination, &["rev-parse", "HEAD"]), integrated_head);
    assert_eq!(
        fixture
            .store
            .load_workflow(&fixture.workflow_id)
            .unwrap()
            .revisions
            .len(),
        revisions_after_first
    );
    assert!(
        current_teamwork_fan_in_status(&fixture.store, &fixture.workflow_id, FRONTEND_JOIN)
            .unwrap()
            .current
    );
}

#[test]
fn rebinding_a_source_to_another_worktree_makes_the_receipt_stale() {
    let fixture = fixture();
    let (source_task, source_head) =
        prepare_frontend_source(&fixture, "frontend-source-ownership.txt");
    let report = integrate_worktree_dependencies(
        &fixture.store,
        &integration_options(&fixture.workflow_id, FRONTEND_JOIN),
    )
    .unwrap();
    assert!(report.success);
    assert!(
        current_teamwork_fan_in_status(&fixture.store, &fixture.workflow_id, FRONTEND_JOIN)
            .unwrap()
            .current
    );

    let alternate_path = fixture.worktree_root.join("rebound-frontend-source");
    let alternate = create_worktree(
        &fixture.store,
        WorktreeCreateOptions {
            repository: fixture.repository.clone(),
            path: alternate_path,
            branch: "foundry/git-fan-in-safety-rebound-source".to_string(),
            start_point: Some(source_head),
            allow_repository_mutation: true,
            origin: "git-fan-in-safety-test".to_string(),
        },
    )
    .unwrap();
    bind_worktree(
        &fixture.store,
        &alternate.worktree.id,
        &fixture.workflow_id,
        Some(&source_task),
        "git-fan-in-safety-test",
    )
    .unwrap();

    let status =
        current_teamwork_fan_in_status(&fixture.store, &fixture.workflow_id, FRONTEND_JOIN)
            .unwrap();
    assert!(!status.current);
    assert_eq!(status.status, "stale");
    assert!(!status.source_heads_current);
}

#[test]
fn integrated_event_failure_rolls_back_git_and_destination_binding() {
    let fixture = fixture();
    prepare_frontend_source(&fixture, "frontend-transaction-safety.txt");
    let destination = fixture.path(FRONTEND_JOIN);
    let pre_head = git(destination, &["rev-parse", "HEAD"]);
    let (worktree_id_before, binding_before) = task_binding(&fixture, FRONTEND_JOIN);
    let revisions_before = fixture
        .store
        .load_workflow(&fixture.workflow_id)
        .unwrap()
        .revisions
        .len();

    let connection = Connection::open(&fixture.store_path).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER fail_integrated_fan_in_receipt
             BEFORE INSERT ON events
             WHEN NEW.kind = 'worktree_dependencies_integrated'
             BEGIN
                 SELECT RAISE(ABORT, 'forced integrated receipt failure');
             END;",
        )
        .unwrap();

    let error = integrate_worktree_dependencies(
        &fixture.store,
        &integration_options(&fixture.workflow_id, FRONTEND_JOIN),
    )
    .unwrap_err();
    connection
        .execute_batch("DROP TRIGGER fail_integrated_fan_in_receipt;")
        .unwrap();
    drop(connection);

    assert!(
        format!("{error:#}").contains("forced integrated receipt failure"),
        "unexpected integration failure: {error:#}"
    );
    assert_eq!(git(destination, &["rev-parse", "HEAD"]), pre_head);
    assert!(git(destination, &["status", "--porcelain"]).is_empty());
    assert!(!git_succeeds(
        destination,
        &["rev-parse", "--verify", "-q", "MERGE_HEAD"]
    ));

    let (worktree_id_after, binding_after) = task_binding(&fixture, FRONTEND_JOIN);
    assert_eq!(worktree_id_after, worktree_id_before);
    assert_eq!(
        binding_after.head_at_binding,
        binding_before.head_at_binding
    );
    assert_eq!(
        binding_after.workflow_revision,
        binding_before.workflow_revision
    );
    assert_eq!(
        binding_after.worktree_identity_sha256,
        binding_before.worktree_identity_sha256
    );
    assert_eq!(
        binding_after.config_sha256_at_binding,
        binding_before.config_sha256_at_binding
    );
    assert_eq!(
        fixture
            .store
            .load_workflow(&fixture.workflow_id)
            .unwrap()
            .revisions
            .len(),
        revisions_before
    );
    assert!(!fixture
        .store
        .load_workflow_events(&fixture.workflow_id)
        .unwrap()
        .iter()
        .any(|event| event.kind == "worktree_dependencies_integrated"));
    assert_eq!(
        current_teamwork_fan_in_status(&fixture.store, &fixture.workflow_id, FRONTEND_JOIN)
            .unwrap()
            .status,
        "missing"
    );

    let retry = integrate_worktree_dependencies(
        &fixture.store,
        &integration_options(&fixture.workflow_id, FRONTEND_JOIN),
    )
    .unwrap();
    assert!(retry.success);
    assert!(
        current_teamwork_fan_in_status(&fixture.store, &fixture.workflow_id, FRONTEND_JOIN)
            .unwrap()
            .current
    );
}

#[test]
fn invalid_latest_receipt_fails_closed_instead_of_falling_back_to_an_older_receipt() {
    let fixture = fixture();
    prepare_frontend_source(&fixture, "frontend-invalid-receipt.txt");
    let report = integrate_worktree_dependencies(
        &fixture.store,
        &integration_options(&fixture.workflow_id, FRONTEND_JOIN),
    )
    .unwrap();
    assert!(report.success);
    let valid_status =
        current_teamwork_fan_in_status(&fixture.store, &fixture.workflow_id, FRONTEND_JOIN)
            .unwrap();
    assert!(valid_status.current);

    fixture
        .store
        .record_event(
            &fixture.workflow_id,
            "worktree_dependencies_integrated",
            &json!({
                "task_id": FRONTEND_JOIN,
                "status": "tampered-newer-receipt"
            }),
        )
        .unwrap();
    let invalid_event_id = fixture
        .store
        .load_workflow_events(&fixture.workflow_id)
        .unwrap()
        .into_iter()
        .rev()
        .find(|event| event.kind == "worktree_dependencies_integrated")
        .unwrap()
        .id;

    let status =
        current_teamwork_fan_in_status(&fixture.store, &fixture.workflow_id, FRONTEND_JOIN)
            .unwrap();
    assert!(!status.current);
    assert_eq!(status.status, "invalid_receipt");
    assert_eq!(status.latest_event_id, Some(invalid_event_id));
    assert_ne!(status.latest_event_id, valid_status.latest_event_id);
    assert!(status.reason.contains("structurally invalid"));
}

#[test]
fn destination_lock_rejects_a_concurrent_fan_in_without_mutation_and_retry_succeeds() {
    let fixture = fixture();
    prepare_frontend_source(&fixture, "frontend-lock-safety.txt");
    let destination = fixture.path(FRONTEND_JOIN);
    let pre_head = git(destination, &["rev-parse", "HEAD"]);
    let git_dir = PathBuf::from(git(destination, &["rev-parse", "--absolute-git-dir"]));
    let lock_file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(git_dir.join("foundry-dependency-fan-in.lock"))
        .unwrap();
    lock_file.try_lock().unwrap();

    let error = integrate_worktree_dependencies(
        &fixture.store,
        &integration_options(&fixture.workflow_id, FRONTEND_JOIN),
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("already running"));
    assert_eq!(git(destination, &["rev-parse", "HEAD"]), pre_head);
    assert!(git(destination, &["status", "--porcelain"]).is_empty());
    assert_eq!(
        current_teamwork_fan_in_status(&fixture.store, &fixture.workflow_id, FRONTEND_JOIN)
            .unwrap()
            .status,
        "missing"
    );

    drop(lock_file);
    let retry = integrate_worktree_dependencies(
        &fixture.store,
        &integration_options(&fixture.workflow_id, FRONTEND_JOIN),
    )
    .unwrap();
    assert!(retry.success);
}

#[test]
fn external_git_driver_is_rejected_and_commit_hooks_remain_disabled() {
    let fixture = fixture();
    prepare_frontend_source(&fixture, "frontend-git-process-safety.txt");
    let destination = fixture.path(FRONTEND_JOIN);
    let pre_head = git(destination, &["rev-parse", "HEAD"]);
    let driver_sentinel = fixture._temporary.path().join("merge-driver-ran");
    git(
        &fixture.repository,
        &[
            "config",
            "merge.foundry-unsafe.driver",
            &format!("touch {}", driver_sentinel.display()),
        ],
    );

    let error = integrate_worktree_dependencies(
        &fixture.store,
        &integration_options(&fixture.workflow_id, FRONTEND_JOIN),
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("external Git drivers"));
    assert!(!driver_sentinel.exists());
    assert_eq!(git(destination, &["rev-parse", "HEAD"]), pre_head);
    assert!(git(destination, &["status", "--porcelain"]).is_empty());
    git(
        &fixture.repository,
        &["config", "--unset-all", "merge.foundry-unsafe.driver"],
    );

    let hook_sentinel = fixture._temporary.path().join("post-commit-hook-ran");
    let hook = fixture.repository.join(".git/hooks/post-commit");
    fs::write(
        &hook,
        format!("#!/bin/sh\ntouch '{}'\n", hook_sentinel.display()),
    )
    .unwrap();
    let mut permissions = fs::metadata(&hook).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&hook, permissions).unwrap();

    let report = integrate_worktree_dependencies(
        &fixture.store,
        &integration_options(&fixture.workflow_id, FRONTEND_JOIN),
    )
    .unwrap();
    assert!(report.success);
    assert!(!hook_sentinel.exists());
}

#[test]
fn poisoned_git_redirection_environment_cannot_redirect_cli_fan_in() {
    let fixture = fixture();
    prepare_frontend_source(&fixture, "frontend-git-env-safety.txt");
    let destination = fixture.path(FRONTEND_JOIN);
    let destination_pre_head = git(destination, &["rev-parse", "HEAD"]);
    let poison_repository = fixture._temporary.path().join("poison-repository");
    initialize_repository(&poison_repository);
    let poison_head = git(&poison_repository, &["rev-parse", "HEAD"]);
    let poison_index = fixture._temporary.path().join("poison-index");
    let poison_config = fixture._temporary.path().join("poison-gitconfig");
    fs::write(
        &poison_config,
        "[core]\n\thooksPath = /definitely-not-a-safe-hook-directory\n",
    )
    .unwrap();

    let output = AssertCommand::cargo_bin("foundry")
        .unwrap()
        .arg("--store")
        .arg(&fixture.store_path)
        .args([
            "worktree",
            "integrate-dependencies",
            "--workflow",
            &fixture.workflow_id,
            "--task",
            FRONTEND_JOIN,
            "--allow-repository-mutation",
            "--approved-by",
            "git-env-safety-test",
            "--reason",
            "prove Git redirection variables are ignored",
            "--origin",
            "git-env-safety-test",
            "--output",
            "json",
        ])
        .env("GIT_DIR", poison_repository.join(".git"))
        .env("GIT_WORK_TREE", &poison_repository)
        .env("GIT_INDEX_FILE", &poison_index)
        .env("GIT_CONFIG_GLOBAL", &poison_config)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["success"], true);
    assert_ne!(
        git(destination, &["rev-parse", "HEAD"]),
        destination_pre_head
    );
    assert_eq!(git(&poison_repository, &["rev-parse", "HEAD"]), poison_head);
    assert!(git(&poison_repository, &["status", "--porcelain"]).is_empty());
    assert!(!poison_index.exists());
}
