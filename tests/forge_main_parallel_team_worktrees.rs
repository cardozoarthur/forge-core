use assert_cmd::Command as AssertCommand;
use forge_core::storage::ForgeStore;
use forge_core::teamwork::{prepare_teamwork_worktrees, TeamworkWorktreePrepareOptions};
use serde_json::Value;
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
        .args(["init", "--initial-branch=main"])
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
        &["config", "user.email", "forge@example.invalid"],
    );
    git(
        repository,
        &["config", "user.name", "Forge Parallel Team Tests"],
    );
    fs::write(repository.join("README.md"), "parallel team fixture\n").unwrap();
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

fn forge() -> AssertCommand {
    AssertCommand::cargo_bin("forge").expect("forge binary should build")
}

fn workflow_id(output: &[u8]) -> String {
    let report: Value = serde_json::from_slice(output).unwrap();
    report["workflow_id"].as_str().unwrap().to_string()
}

fn assert_explicit_topology_prepares_twelve_worktrees(
    store: &ForgeStore,
    workflow_id: &str,
    repository: &Path,
    worktree_root: &Path,
    branch_prefix: &str,
) {
    let workflow = store.load_workflow(workflow_id).unwrap();
    let workflow_final_join = workflow
        .tasks
        .iter()
        .find(|task| task.id == "task-006")
        .unwrap();
    assert!(workflow_final_join
        .node_brain_routing
        .default_brain
        .is_none());
    assert!(workflow_final_join
        .node_brain_routing
        .agent_slots
        .is_empty());

    let report = prepare_teamwork_worktrees(
        store,
        TeamworkWorktreePrepareOptions {
            workflow_id: workflow_id.to_string(),
            repository: repository.to_path_buf(),
            worktree_root: worktree_root.to_path_buf(),
            branch_prefix: branch_prefix.to_string(),
            origin: "main-parallel-team-worktree-test".to_string(),
            allow_repository_mutation: false,
        },
    )
    .unwrap();

    assert_eq!(report.planned_worktrees, 12);
    assert_eq!(report.parallel_branch_worktrees, 8);
    let task_ids = report
        .entries
        .iter()
        .map(|entry| entry.task_id.as_str())
        .collect::<BTreeSet<_>>();
    let branch_ids = (1..=3)
        .map(|index| format!("task-005-frontend-{index:03}"))
        .chain((1..=5).map(|index| format!("task-005-backend-{index:03}")))
        .collect::<BTreeSet<_>>();
    assert!(branch_ids
        .iter()
        .all(|task_id| task_ids.contains(task_id.as_str())));
    for supporting_task in [
        "task-005-frontend-join",
        "task-005-backend-join",
        "task-006",
        "task-008",
    ] {
        assert!(task_ids.contains(supporting_task));
    }
    let final_join = report
        .entries
        .iter()
        .find(|entry| entry.task_id == "task-006")
        .unwrap();
    assert!(
        final_join.brain.is_none(),
        "the generic final join must not invent a cognitive executor"
    );
}

#[test]
fn plan_and_request_explicit_topologies_each_prepare_twelve_task_scoped_worktrees() {
    let temporary = tempdir().unwrap();
    let repository = temporary.path().join("repository");
    initialize_repository(&repository);
    let store_path = temporary.path().join("forge.sqlite");

    let plan_output = forge()
        .current_dir(&repository)
        .arg("--store")
        .arg(&store_path)
        .args([
            "plan",
            "--goal",
            "Deliver independent frontend and backend lanes",
            "--lane",
            "frontend=agy:3",
            "--lane",
            "backend=codex:5",
            "--max-parallel-agents",
            "8",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let request_output = forge()
        .current_dir(&repository)
        .arg("--store")
        .arg(&store_path)
        .args([
            "request",
            "start",
            "--goal",
            "Deliver independent frontend and backend lanes",
            "--origin",
            "parallel-team-worktree-test",
            "--lane",
            "frontend=agy:3",
            "--lane",
            "backend=codex:5",
            "--max-parallel-agents",
            "8",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let store = ForgeStore::open(&store_path).unwrap();
    assert_explicit_topology_prepares_twelve_worktrees(
        &store,
        &workflow_id(&plan_output),
        &repository,
        &temporary.path().join("plan-worktrees"),
        "forge/plan-explicit",
    );
    assert_explicit_topology_prepares_twelve_worktrees(
        &store,
        &workflow_id(&request_output),
        &repository,
        &temporary.path().join("request-worktrees"),
        "forge/request-explicit",
    );
}
