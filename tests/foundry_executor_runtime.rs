#![cfg(unix)]

use chrono::Utc;
use foundry_core::artifact::hex_sha256;
use foundry_core::executor::ExecutorState;
use foundry_core::executor_runtime::{
    execute_executor_runtime, execute_executor_wave, ExecutorRuntimeAuthorization,
    ExecutorRuntimeDispatchCorrelation, ExecutorRuntimeRequest,
    EXECUTOR_RUNTIME_LEASE_GRACE_SECONDS,
};
use foundry_core::graph::{create_workflow, TaskStatus};
use foundry_core::intent::parse_intent;
use foundry_core::lease::acquire_task_lease;
use foundry_core::request::{create_run_record, save_run_record};
use foundry_core::storage::FoundryStore;
use foundry_core::worktree::{bind_worktree, create_worktree, WorktreeCreateOptions};
use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::tempdir;

#[derive(Clone)]
struct SeededExecution {
    workflow_id: String,
    run_id: String,
    task_id: String,
    lease_id: String,
    executor: String,
    cwd: PathBuf,
    dispatch: Option<ExecutorRuntimeDispatchCorrelation>,
}

#[test]
fn wave_overlaps_three_agy_and_five_codex_workers_in_eight_task_worktrees() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("foundry.sqlite");
    let repository = temp.path().join("repository");
    let worktrees = temp.path().join("worktrees");
    let codex_path = temp.path().join("codex-stub");
    let agy_path = temp.path().join("agy-stub");
    initialize_repository(&repository);
    write_waiting_stub(&codex_path);
    write_waiting_stub(&agy_path);

    let store = FoundryStore::open(&store_path).unwrap();
    save_executor_policy(&store, "codex", &codex_path, true, true);
    save_executor_policy(&store, "agy", &agy_path, true, true);
    let executors = [
        "agy", "agy", "agy", "codex", "codex", "codex", "codex", "codex",
    ];
    let executions = seed_parallel_executions(
        &store,
        &repository,
        &worktrees,
        &executors,
        "wave-eight-workers",
    );
    let expected_task_order = executions
        .iter()
        .map(|execution| execution.task_id.clone())
        .collect::<Vec<_>>();
    let requests = executions
        .iter()
        .enumerate()
        .map(|(index, execution)| {
            let executor = if index == 0 {
                "Antigravity"
            } else {
                execution.executor.as_str()
            };
            runtime_request(
                execution,
                executor,
                &format!("bounded parallel branch {index}"),
                true,
            )
        })
        .collect::<Vec<_>>();
    let wave_store_path = store_path.clone();
    let handle = thread::spawn(move || {
        let store = FoundryStore::open(wave_store_path).unwrap();
        execute_executor_wave(&store, requests, 8)
    });

    let overlap_observed = wait_for_started_count(temp.path(), 8, Duration::from_secs(8));
    fs::write(temp.path().join("release"), b"release").unwrap();
    let report = handle.join().unwrap().unwrap();

    assert!(
        overlap_observed,
        "all eight executor processes must start before any is released"
    );
    assert!(report.success, "{report:#?}");
    assert_eq!(report.status, "executor_wave_succeeded");
    assert_eq!(report.wave_id, "wave-eight-workers");
    assert_eq!(report.request_count, 8);
    assert_eq!(report.unique_request_count, 8);
    assert_eq!(report.deduplicated_request_count, 0);
    assert_eq!(report.worker_count, 8);
    assert_eq!(report.initialized_worker_count, 8);
    assert!(report.worker_errors.is_empty());
    assert_eq!(report.receipt_order, "request_order");
    assert_eq!(report.receipts.len(), 8);
    assert!(report.errors.is_empty());
    assert_eq!(
        report
            .receipts
            .iter()
            .map(|receipt| receipt.task_id.clone())
            .collect::<Vec<_>>(),
        expected_task_order
    );
    assert_eq!(
        report
            .receipts
            .iter()
            .filter(|receipt| receipt.executor == "agy")
            .count(),
        3
    );
    assert_eq!(
        report
            .receipts
            .iter()
            .filter(|receipt| receipt.executor == "codex")
            .count(),
        5
    );
    assert_eq!(
        report
            .receipts
            .iter()
            .map(|receipt| receipt.worktree_id.clone())
            .collect::<BTreeSet<_>>()
            .len(),
        8
    );

    for (index, (execution, receipt)) in executions.iter().zip(&report.receipts).enumerate() {
        assert!(receipt.success, "{receipt:#?}");
        assert_eq!(
            receipt.prompt_transport,
            if receipt.executor == "agy" {
                "argument"
            } else {
                "stdin"
            }
        );
        assert_eq!(receipt.workspace_binding_scope, "task");
        assert_eq!(receipt.dispatch, execution.dispatch);
        assert!(!receipt.idempotent_replay);
        assert!(receipt.lease_extended_for_runtime);
        assert!(receipt.lease_preserved_for_validation);
        assert_eq!(
            receipt.lease_grace_seconds,
            EXECUTOR_RUNTIME_LEASE_GRACE_SECONDS
        );
        assert!(
            (receipt.lease_expires_at - receipt.finished_at).num_seconds()
                >= EXECUTOR_RUNTIME_LEASE_GRACE_SECONDS as i64 - 5
        );
        assert!(!receipt.task_completion_attempted);
        assert!(!receipt.output_accepted_as_validation);
        assert_eq!(receipt.stdout.sha256.len(), 64);
        assert!(receipt.stdout.excerpt.contains("stub"));
        let prompt = format!("bounded parallel branch {index}");
        let stdin = fs::read_to_string(temp.path().join("stdin").join(&execution.task_id)).unwrap();
        if receipt.executor == "agy" {
            assert!(stdin.is_empty());
            let args =
                fs::read_to_string(temp.path().join("args").join(&execution.task_id)).unwrap();
            let args = args.lines().collect::<Vec<_>>();
            assert_eq!(args.first(), Some(&"--print"));
            assert_eq!(args.get(1), Some(&prompt.as_str()));
            assert_eq!(args.get(2), Some(&"--print-timeout"));
            assert_eq!(args.get(3), Some(&"5s"));
        } else {
            assert_eq!(stdin, prompt);
            let args =
                fs::read_to_string(temp.path().join("args").join(&execution.task_id)).unwrap();
            let args = args.lines().collect::<Vec<_>>();
            let add_dir_index = args
                .iter()
                .position(|argument| *argument == "--add-dir")
                .expect("Codex worktree runtime must grant its validated Git common dir");
            let expected_git_common_dir = fs::canonicalize(repository.join(".git")).unwrap();
            assert_eq!(
                args.get(add_dir_index + 1).copied(),
                expected_git_common_dir.to_str()
            );
        }
        let persisted = store
            .load_task_lease(&execution.workflow_id, &execution.task_id)
            .unwrap()
            .unwrap();
        assert_eq!(persisted["lease_id"], execution.lease_id);
    }

    let workflow_id = &executions[0].workflow_id;
    let events = store.load_workflow_events(workflow_id).unwrap();
    for kind in [
        "executor_runtime_claimed",
        "executor_runtime_started",
        "executor_runtime_finished",
    ] {
        assert_eq!(
            events.iter().filter(|event| event.kind == kind).count(),
            8,
            "event count for {kind}"
        );
    }
    let workflow = store.load_workflow(workflow_id).unwrap();
    assert!(workflow
        .tasks
        .iter()
        .take(8)
        .all(|task| task.status == TaskStatus::Pending));
    assert_eq!(invocation_count(temp.path()), 8);
    assert!(!temp.path().join("duplicate-invocation").exists());
}

#[test]
fn symlinked_volta_shim_is_invoked_through_the_executor_name() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("foundry.sqlite");
    let repository = temp.path().join("repository");
    let worktrees = temp.path().join("worktrees");
    let bin_dir = temp.path().join("bin");
    let volta_shim = bin_dir.join("volta-shim");
    let codex_path = bin_dir.join("codex");
    fs::create_dir_all(&bin_dir).unwrap();
    initialize_repository(&repository);
    write_argv0_sensitive_volta_shim(&volta_shim);
    symlink(&volta_shim, &codex_path).unwrap();

    let store = FoundryStore::open(&store_path).unwrap();
    save_executor_policy(&store, "codex", &codex_path, true, true);
    let execution = seed_parallel_executions(
        &store,
        &repository,
        &worktrees,
        &["codex"],
        "wave-volta-argv0",
    )
    .remove(0);
    let request = runtime_request(
        &execution,
        "codex",
        "preserve the executor shim invocation name",
        true,
    );

    let receipt = execute_executor_runtime(&store, request).unwrap();

    assert!(receipt.success, "{receipt:#?}");
    assert_eq!(receipt.exit_code, Some(0));
    assert_eq!(receipt.command_path, codex_path.display().to_string());
    assert!(execution.cwd.join("argv0-codex").exists());
}

#[test]
fn concurrent_same_lease_executes_one_child_and_replays_one_receipt() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("foundry.sqlite");
    let repository = temp.path().join("repository");
    let worktrees = temp.path().join("worktrees");
    let stub_path = temp.path().join("codex-stub");
    initialize_repository(&repository);
    write_waiting_stub(&stub_path);

    let store = FoundryStore::open(&store_path).unwrap();
    save_executor_policy(&store, "codex", &stub_path, true, true);
    let mut execution = seed_parallel_executions(
        &store,
        &repository,
        &worktrees,
        &["codex"],
        "wave-direct-dedupe",
    )
    .remove(0);
    execution.dispatch = None;
    let request = runtime_request(&execution, "codex", "execute exactly once", true);

    let first_store_path = store_path.clone();
    let first_request = request.clone();
    let first = thread::spawn(move || {
        let store = FoundryStore::open(first_store_path).unwrap();
        execute_executor_runtime(&store, first_request)
    });
    let second_store_path = store_path.clone();
    let second = thread::spawn(move || {
        let store = FoundryStore::open(second_store_path).unwrap();
        execute_executor_runtime(&store, request)
    });

    let started = wait_for_started_count(temp.path(), 1, Duration::from_secs(5));
    fs::write(temp.path().join("release"), b"release").unwrap();
    let first = first.join().unwrap().unwrap();
    let second = second.join().unwrap().unwrap();

    assert!(started);
    assert!(first.success && second.success);
    assert_eq!(first.execution_id, second.execution_id);
    assert_eq!(first.request_sha256, second.request_sha256);
    assert_eq!(first.stdout.sha256, second.stdout.sha256);
    assert_eq!(first.git, second.git);
    assert_eq!(
        [first.idempotent_replay, second.idempotent_replay]
            .into_iter()
            .filter(|replay| *replay)
            .count(),
        1
    );
    let git = first.git.as_ref().expect("Git observation");
    assert_eq!(git.status, "observed");
    assert_eq!(git.head.as_deref(), Some(git.base_head.as_str()));
    assert_eq!(git.base_is_ancestor, Some(true));
    assert_eq!(git.commit_count, Some(0));
    assert!(git.changed_paths.is_empty());
    assert_eq!(git.dirty, Some(false));
    assert_eq!(git.clean, Some(true));
    assert_eq!(invocation_count(temp.path()), 1);
    assert!(!temp.path().join("duplicate-invocation").exists());

    let events = store.load_workflow_events(&execution.workflow_id).unwrap();
    for kind in [
        "executor_runtime_claimed",
        "executor_runtime_started",
        "executor_runtime_finished",
    ] {
        assert_eq!(events.iter().filter(|event| event.kind == kind).count(), 1);
    }
    let claim = store
        .load_executor_runtime_claim(
            &execution.workflow_id,
            &execution.task_id,
            &execution.lease_id,
        )
        .unwrap()
        .unwrap();
    assert_eq!(claim.state, "finished");
    assert!(claim.receipt_json.is_some());
    assert!(store
        .load_task_lease(&execution.workflow_id, &execution.task_id)
        .unwrap()
        .is_some());
    let workflow = store.load_workflow(&execution.workflow_id).unwrap();
    assert_eq!(workflow.tasks[0].status, TaskStatus::Pending);
}

#[test]
fn runtime_receipt_observes_clean_committed_git_delta() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("foundry.sqlite");
    let repository = temp.path().join("repository");
    let worktrees = temp.path().join("worktrees");
    let stub_path = temp.path().join("codex-commit-stub");
    initialize_repository(&repository);
    write_committing_stub(&stub_path);

    let store = FoundryStore::open(&store_path).unwrap();
    save_executor_policy(&store, "codex", &stub_path, true, true);
    let mut execution = seed_parallel_executions(
        &store,
        &repository,
        &worktrees,
        &["codex"],
        "wave-git-commit",
    )
    .remove(0);
    execution.dispatch = None;
    let base_head = git_stdout(&execution.cwd, &["rev-parse", "--verify", "HEAD^{commit}"]);

    let receipt = execute_executor_runtime(
        &store,
        runtime_request(&execution, "codex", "commit two deterministic files", true),
    )
    .unwrap();

    assert!(receipt.success, "{receipt:#?}");
    let observation = receipt.git.as_ref().expect("Git observation");
    assert_eq!(
        observation.schema_version,
        "foundry.executor_runtime.git_observation.v1"
    );
    assert_eq!(observation.status, "observed");
    assert_eq!(
        observation.repository_root,
        fs::canonicalize(&repository).unwrap().display().to_string()
    );
    assert_eq!(
        observation.branch.as_deref(),
        Some("executor-runtime-worker-0")
    );
    assert_eq!(observation.base_head, base_head);
    assert_ne!(
        observation.head.as_deref(),
        Some(observation.base_head.as_str())
    );
    assert_eq!(observation.base_is_ancestor, Some(true));
    assert_eq!(observation.commit_count, Some(1));
    assert_eq!(
        observation.changed_paths,
        vec!["alpha.txt".to_string(), "nested/zeta.txt".to_string()]
    );
    assert_eq!(observation.dirty, Some(false));
    assert_eq!(observation.clean, Some(true));
    assert!(observation.observation_error.is_none());
}

#[test]
fn runtime_receipt_observes_uncommitted_dirty_worktree() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("foundry.sqlite");
    let repository = temp.path().join("repository");
    let worktrees = temp.path().join("worktrees");
    let stub_path = temp.path().join("agy-dirty-stub");
    initialize_repository(&repository);
    write_dirty_stub(&stub_path);

    let store = FoundryStore::open(&store_path).unwrap();
    save_executor_policy(&store, "agy", &stub_path, true, true);
    let mut execution =
        seed_parallel_executions(&store, &repository, &worktrees, &["agy"], "wave-git-dirty")
            .remove(0);
    execution.dispatch = None;
    let base_head = git_stdout(&execution.cwd, &["rev-parse", "--verify", "HEAD^{commit}"]);

    let receipt = execute_executor_runtime(
        &store,
        runtime_request(
            &execution,
            "agy",
            "leave one uncommitted file for observation",
            true,
        ),
    )
    .unwrap();

    assert!(receipt.success, "{receipt:#?}");
    let observation = receipt.git.as_ref().expect("Git observation");
    assert_eq!(observation.status, "observed");
    assert_eq!(observation.base_head, base_head);
    assert_eq!(observation.head.as_deref(), Some(base_head.as_str()));
    assert_eq!(observation.base_is_ancestor, Some(true));
    assert_eq!(observation.commit_count, Some(0));
    assert!(observation.changed_paths.is_empty());
    assert_eq!(observation.dirty, Some(true));
    assert_eq!(observation.clean, Some(false));
    assert!(execution.cwd.join("dirty.txt").is_file());
    assert!(observation.observation_error.is_none());
}

#[test]
fn wave_coalesces_identical_duplicate_lease_before_workers() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("foundry.sqlite");
    let repository = temp.path().join("repository");
    let worktrees = temp.path().join("worktrees");
    let stub_path = temp.path().join("agy-stub");
    initialize_repository(&repository);
    write_waiting_stub(&stub_path);

    let store = FoundryStore::open(&store_path).unwrap();
    save_executor_policy(&store, "agy", &stub_path, true, true);
    let execution = seed_parallel_executions(
        &store,
        &repository,
        &worktrees,
        &["agy"],
        "wave-coalesced-dedupe",
    )
    .remove(0);
    let request = runtime_request(&execution, "agy", "coalesced wave request", true);
    let wave_store_path = store_path.clone();
    let handle = thread::spawn(move || {
        let store = FoundryStore::open(wave_store_path).unwrap();
        execute_executor_wave(&store, vec![request.clone(), request], 2)
    });

    assert!(wait_for_started_count(
        temp.path(),
        1,
        Duration::from_secs(5)
    ));
    fs::write(temp.path().join("release"), b"release").unwrap();
    let report = handle.join().unwrap().unwrap();
    assert!(report.success, "{report:#?}");
    assert_eq!(report.request_count, 2);
    assert_eq!(report.unique_request_count, 1);
    assert_eq!(report.deduplicated_request_count, 1);
    assert_eq!(report.worker_count, 1);
    assert_eq!(report.initialized_worker_count, 1);
    assert!(report.worker_errors.is_empty());
    assert_eq!(report.receipts.len(), 2);
    assert_eq!(
        report.receipts[0].execution_id,
        report.receipts[1].execution_id
    );
    assert!(!report.receipts[0].idempotent_replay);
    assert!(report.receipts[1].idempotent_replay);
    assert_eq!(invocation_count(temp.path()), 1);
}

#[test]
fn runtime_fails_closed_before_invoking_stub_without_opt_in_or_ready_policy() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("foundry.sqlite");
    let stub_path = temp.path().join("codex-blocked-stub");
    write_immediate_stub(&stub_path);
    let cwd = temp.path().join("blocked-worktree");
    fs::create_dir_all(&cwd).unwrap();

    let store = FoundryStore::open(&store_path).unwrap();
    save_executor_policy(&store, "codex", &stub_path, false, false);
    let execution = seed_unbound_execution(&store, "Keep blocked executor inert", "codex", cwd);

    let missing_opt_in = execute_executor_runtime(
        &store,
        runtime_request(&execution, "codex", "must not run", false),
    )
    .unwrap_err();
    assert!(missing_opt_in.to_string().contains("explicit"));

    let blocked_policy = execute_executor_runtime(
        &store,
        runtime_request(&execution, "codex", "must still not run", true),
    )
    .unwrap_err();
    let blocked_policy = blocked_policy.to_string();
    assert!(blocked_policy.contains("allowed=false"));
    assert!(blocked_policy.contains("non_interactive_ready=false"));
    assert!(!temp.path().join("codex-blocked-stub.invoked").exists());
    assert!(store
        .load_workflow_events(&execution.workflow_id)
        .unwrap()
        .iter()
        .all(|event| !event.kind.starts_with("executor_runtime_")));
}

#[test]
fn runtime_requires_a_task_scoped_workspace_claim() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("foundry.sqlite");
    let stub_path = temp.path().join("codex-unbound-stub");
    let cwd = temp.path().join("unbound-directory");
    fs::create_dir_all(&cwd).unwrap();
    write_immediate_stub(&stub_path);

    let store = FoundryStore::open(&store_path).unwrap();
    save_executor_policy(&store, "codex", &stub_path, true, true);
    let execution = seed_unbound_execution(
        &store,
        "Reject executor mutation without task worktree",
        "codex",
        cwd,
    );
    let error = execute_executor_runtime(
        &store,
        runtime_request(&execution, "codex", "must remain inert", true),
    )
    .unwrap_err();
    assert!(error.to_string().contains("task-scoped workspace claim"));
    assert!(!temp.path().join("codex-unbound-stub.invoked").exists());
    assert!(store
        .load_executor_runtime_claim(
            &execution.workflow_id,
            &execution.task_id,
            &execution.lease_id,
        )
        .unwrap()
        .is_none());
}

#[test]
fn runtime_rejects_a_task_while_its_dependency_is_pending() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("foundry.sqlite");
    let stub_path = temp.path().join("agy-dependency-stub");
    let cwd = temp.path().join("dependency-directory");
    fs::create_dir_all(&cwd).unwrap();
    write_immediate_stub(&stub_path);

    let store = FoundryStore::open(&store_path).unwrap();
    save_executor_policy(&store, "agy", &stub_path, true, true);
    let execution = seed_dependency_blocked_execution(&store, "agy", cwd);
    let error = execute_executor_runtime(
        &store,
        runtime_request(&execution, "agy", "must wait for dependency", true),
    )
    .unwrap_err();
    assert!(error.to_string().contains("dependencies are not completed"));
    assert!(!temp.path().join("agy-dependency-stub.invoked").exists());
    assert!(store
        .load_executor_runtime_claim(
            &execution.workflow_id,
            &execution.task_id,
            &execution.lease_id,
        )
        .unwrap()
        .is_none());
}

fn seed_parallel_executions(
    store: &FoundryStore,
    repository: &Path,
    worktrees_root: &Path,
    executors: &[&str],
    wave_id: &str,
) -> Vec<SeededExecution> {
    let mut workflow = create_workflow(parse_intent(
        "Run independent executor branches in isolated task worktrees",
    ));
    assert!(workflow.tasks.len() >= executors.len());
    workflow.tasks.truncate(executors.len());
    for task in &mut workflow.tasks {
        task.dependencies.clear();
        task.status = TaskStatus::Pending;
    }
    let workflow_id = workflow.id.clone();
    let task_ids = workflow
        .tasks
        .iter()
        .map(|task| task.id.clone())
        .collect::<Vec<_>>();
    store.save_workflow(&workflow).unwrap();

    let mut roots = Vec::with_capacity(task_ids.len());
    for (index, task_id) in task_ids.iter().enumerate() {
        let root = worktrees_root.join(format!("worker-{index}"));
        let report = create_worktree(
            store,
            WorktreeCreateOptions {
                repository: repository.to_path_buf(),
                path: root.clone(),
                branch: format!("executor-runtime-worker-{index}"),
                start_point: Some("HEAD".to_string()),
                allow_repository_mutation: true,
                origin: "executor-runtime-test".to_string(),
            },
        )
        .unwrap();
        bind_worktree(
            store,
            &report.worktree.id,
            &workflow_id,
            Some(task_id),
            "executor-runtime-test",
        )
        .unwrap();
        roots.push(root);
    }

    let workflow = store.load_workflow(&workflow_id).unwrap();
    let workflow_revision = workflow
        .revisions
        .last()
        .map(|revision| revision.revision)
        .unwrap_or(0);
    let run = create_run_record(&workflow, "executor_runtime_test", "accepted");
    save_run_record(store, &run).unwrap();
    task_ids
        .into_iter()
        .zip(executors)
        .zip(roots)
        .map(|((task_id, executor), cwd)| {
            let lease = acquire_task_lease(store, &workflow_id, &task_id, executor, 60)
                .unwrap()
                .lease
                .unwrap();
            let task = workflow
                .tasks
                .iter()
                .find(|task| task.id == task_id)
                .unwrap();
            SeededExecution {
                workflow_id: workflow_id.clone(),
                run_id: run.run_id.clone(),
                task_id: task_id.clone(),
                lease_id: lease.lease_id,
                executor: (*executor).to_string(),
                cwd,
                dispatch: Some(ExecutorRuntimeDispatchCorrelation {
                    wave_id: wave_id.to_string(),
                    workflow_revision,
                    task_version: task.version,
                    context_sha256: hex_sha256(
                        format!("{workflow_id}:{task_id}:bounded-context").as_bytes(),
                    ),
                }),
            }
        })
        .collect()
}

fn seed_unbound_execution(
    store: &FoundryStore,
    goal: &str,
    executor: &str,
    cwd: PathBuf,
) -> SeededExecution {
    let workflow = create_workflow(parse_intent(goal));
    let task_id = workflow.tasks.first().unwrap().id.clone();
    store.save_workflow(&workflow).unwrap();
    let run = create_run_record(&workflow, "executor_runtime_test", "accepted");
    save_run_record(store, &run).unwrap();
    let lease = acquire_task_lease(store, &workflow.id, &task_id, executor, 60)
        .unwrap()
        .lease
        .unwrap();
    SeededExecution {
        workflow_id: workflow.id,
        run_id: run.run_id,
        task_id,
        lease_id: lease.lease_id,
        executor: executor.to_string(),
        cwd,
        dispatch: None,
    }
}

fn seed_dependency_blocked_execution(
    store: &FoundryStore,
    executor: &str,
    cwd: PathBuf,
) -> SeededExecution {
    let mut workflow = create_workflow(parse_intent(
        "Wait for a predecessor before executor runtime dispatch",
    ));
    assert!(workflow.tasks.len() >= 2);
    let dependency_id = workflow.tasks[0].id.clone();
    workflow.tasks[0].status = TaskStatus::Pending;
    workflow.tasks[1].status = TaskStatus::Pending;
    workflow.tasks[1].dependencies = vec![dependency_id];
    let task_id = workflow.tasks[1].id.clone();
    store.save_workflow(&workflow).unwrap();
    let run = create_run_record(&workflow, "executor_runtime_test", "accepted");
    save_run_record(store, &run).unwrap();
    let lease = acquire_task_lease(store, &workflow.id, &task_id, executor, 60)
        .unwrap()
        .lease
        .unwrap();
    SeededExecution {
        workflow_id: workflow.id,
        run_id: run.run_id,
        task_id,
        lease_id: lease.lease_id,
        executor: executor.to_string(),
        cwd,
        dispatch: None,
    }
}

fn runtime_request(
    execution: &SeededExecution,
    executor: &str,
    prompt: &str,
    allow: bool,
) -> ExecutorRuntimeRequest {
    ExecutorRuntimeRequest {
        workflow_id: execution.workflow_id.clone(),
        run_id: execution.run_id.clone(),
        task_id: execution.task_id.clone(),
        lease_id: execution.lease_id.clone(),
        executor: executor.to_string(),
        cwd: execution.cwd.clone(),
        prompt: prompt.to_string(),
        timeout_seconds: 5,
        authorization: ExecutorRuntimeAuthorization {
            allow_non_interactive_execution: allow,
            approved_by: "runtime-test-operator".to_string(),
            reason: "exercise the bounded real executor adapter".to_string(),
        },
        dispatch: execution.dispatch.clone(),
    }
}

fn save_executor_policy(
    store: &FoundryStore,
    executor: &str,
    command_path: &Path,
    allowed: bool,
    non_interactive_ready: bool,
) {
    let state = ExecutorState {
        id: executor.to_string(),
        display_name: format!("{executor} stub"),
        command: executor.to_string(),
        installed: true,
        configured: true,
        command_path: Some(command_path.display().to_string()),
        config_evidence: vec!["stub executable configured".to_string()],
        non_interactive_ready,
        probe_evidence: vec!["stub non-interactive probe".to_string()],
        foundry_first_ready: false,
        foundry_first_entrypoint: None,
        harness_status: None,
        allowed,
        decision_source: "executor_runtime_test".to_string(),
        synced_at: Utc::now().to_rfc3339(),
    };
    store
        .save_executor_state(executor, &serde_json::to_value(state).unwrap())
        .unwrap();
}

fn initialize_repository(repository: &Path) {
    fs::create_dir_all(repository).unwrap();
    git(repository, &["init", "-q", "--initial-branch=main"]);
    git(
        repository,
        &["config", "user.email", "foundry-runtime@example.invalid"],
    );
    git(repository, &["config", "user.name", "Foundry Runtime Test"]);
    fs::write(repository.join("README.md"), "executor runtime fixture\n").unwrap();
    git(repository, &["add", "README.md"]);
    git(repository, &["commit", "-q", "-m", "fixture"]);
}

fn git(repository: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_stdout(repository: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn write_waiting_stub(path: &Path) {
    fs::write(
        path,
        r#"#!/bin/sh
marker_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
name=$(basename -- "$0")
mkdir -p "$marker_dir/args" "$marker_dir/stdin" "$marker_dir/started" "$marker_dir/invocations"
printf '%s\n' "$@" > "$marker_dir/args/$FOUNDRY_TASK_ID"
cat > "$marker_dir/stdin/$FOUNDRY_TASK_ID"
if ! mkdir "$marker_dir/invocations/$FOUNDRY_TASK_LEASE_ID" 2>/dev/null; then
  : > "$marker_dir/duplicate-invocation"
fi
: > "$marker_dir/started/$FOUNDRY_TASK_ID"
while [ ! -f "$marker_dir/release" ]; do
  sleep 0.02
done
printf '{"executor":"%s","task":"%s"}\n' "$name" "$FOUNDRY_TASK_ID"
printf 'stderr-%s-%s\n' "$name" "$FOUNDRY_TASK_ID" >&2
"#,
    )
    .unwrap();
    make_executable(path);
}

fn write_immediate_stub(path: &Path) {
    fs::write(
        path,
        r#"#!/bin/sh
marker_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
name=$(basename -- "$0")
: > "$marker_dir/$name.invoked"
printf '{}\n'
"#,
    )
    .unwrap();
    make_executable(path);
}

fn write_argv0_sensitive_volta_shim(path: &Path) {
    fs::write(
        path,
        r#"#!/bin/sh
name=$(basename -- "$0")
if [ "$name" != "codex" ]; then
  printf "Volta error: '%s' should not be called directly. Please use existing shims.\n" "$name" >&2
  exit 126
fi
cat >/dev/null
: > "$PWD/argv0-codex"
printf '{}\n'
"#,
    )
    .unwrap();
    make_executable(path);
}

fn write_committing_stub(path: &Path) {
    fs::write(
        path,
        r#"#!/bin/sh
set -eu
cat >/dev/null
mkdir -p nested
printf 'zeta\n' > nested/zeta.txt
printf 'alpha\n' > alpha.txt
git add -- nested/zeta.txt alpha.txt
git -c user.name='Foundry Runtime Executor' \
    -c user.email='foundry-runtime-executor@example.invalid' \
    commit -q -m 'executor runtime fixture commit'
printf '{}\n'
"#,
    )
    .unwrap();
    make_executable(path);
}

fn write_dirty_stub(path: &Path) {
    fs::write(
        path,
        r#"#!/bin/sh
set -eu
cat >/dev/null
printf 'dirty\n' > dirty.txt
printf '{}\n'
"#,
    )
    .unwrap();
    make_executable(path);
}

fn make_executable(path: &Path) {
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

fn wait_for_started_count(root: &Path, expected: usize, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        let count = fs::read_dir(root.join("started"))
            .map(|entries| entries.filter_map(std::result::Result::ok).count())
            .unwrap_or(0);
        if count >= expected {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn invocation_count(root: &Path) -> usize {
    fs::read_dir(root.join("invocations"))
        .unwrap()
        .filter_map(std::result::Result::ok)
        .count()
}
