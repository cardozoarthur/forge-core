use assert_cmd::Command;
use forge_core::storage::ForgeStore;
use rusqlite::Connection;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command as ProcessCommand;
use tempfile::tempdir;

fn forge() -> Command {
    Command::cargo_bin("forge").expect("forge binary should build")
}

fn run_json(command: &mut Command) -> Value {
    let output = command.output().expect("forge command should run");
    assert!(
        output.status.success(),
        "forge command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("forge output should be JSON")
}

fn run_json_with_failure(command: &mut Command) -> Value {
    let output = command.output().expect("forge command should run");
    assert!(
        !output.status.success(),
        "forge command unexpectedly succeeded\nstdout:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).expect("blocked forge output should be JSON")
}

fn init_repository(path: &Path) {
    let init = ProcessCommand::new("git")
        .arg("init")
        .arg("--initial-branch=main")
        .arg(path)
        .output()
        .expect("git init should run");
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    for (key, value) in [
        ("user.email", "forge-worktree-test@example.invalid"),
        ("user.name", "Forge Worktree Test"),
    ] {
        assert!(ProcessCommand::new("git")
            .arg("-C")
            .arg(path)
            .arg("config")
            .arg(key)
            .arg(value)
            .status()
            .expect("git config should run")
            .success());
    }
    assert!(ProcessCommand::new("git")
        .arg("-C")
        .arg(path)
        .args([
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--allow-empty",
            "-m",
            "initial"
        ])
        .status()
        .expect("git commit should run")
        .success());
}

fn create_registered_worktree(
    store: &Path,
    repository: &Path,
    worktree_path: &Path,
    branch: &str,
) -> Value {
    run_json(
        forge()
            .arg("--store")
            .arg(store)
            .args(["worktree", "create", "--repository"])
            .arg(repository)
            .arg("--path")
            .arg(worktree_path)
            .args([
                "--branch",
                branch,
                "--allow-repository-mutation",
                "--output",
                "json",
            ]),
    )
}

#[test]
fn worktree_binding_routes_context_and_runs_internal_test_sandbox() {
    let temp = tempdir().unwrap();
    let repository = temp.path().join("repository");
    let worktree_path = temp.path().join("feature-preview");
    let store = temp.path().join("forge.sqlite");
    init_repository(&repository);

    let created =
        create_registered_worktree(&store, &repository, &worktree_path, "feature/preview");
    assert_eq!(created["status"], "worktree_created");
    assert_eq!(created["worktree"]["created_by_forge"], true);
    let worktree_id = created["worktree"]["id"].as_str().unwrap();

    let initialized = run_json(
        forge()
            .arg("--store")
            .arg(&store)
            .args(["worktree", "init", "--worktree", worktree_id])
            .args(["--allow-worktree-write", "--output", "json"]),
    );
    assert_eq!(initialized["status"], "worktree_initialized");
    assert_eq!(initialized["worktree"]["config"]["status"], "configured");
    assert_eq!(
        initialized["worktree"]["config"]["config"]["sandbox"]["enabled"],
        true
    );

    let workflow = run_json(
        forge()
            .current_dir(&repository)
            .arg("--store")
            .arg(&store)
            .args([
                "plan",
                "--goal",
                "Validate worktree preview and internal tests",
                "--output",
                "json",
            ]),
    );
    let workflow_id = workflow["workflow_id"].as_str().unwrap();
    let binding = run_json(
        forge()
            .arg("--store")
            .arg(&store)
            .args(["worktree", "bind", "--worktree", worktree_id])
            .args([
                "--workflow",
                workflow_id,
                "--origin",
                "test",
                "--output",
                "json",
            ]),
    );
    assert_eq!(binding["status"], "worktree_bound");
    assert_eq!(binding["binding"]["workflow_revision"], 1);

    let context_before = run_json(
        forge()
            .current_dir(&repository)
            .arg("--store")
            .arg(&store)
            .args(["context", "--workflow", workflow_id, "--task", "task-001"])
            .args(["--budget", "4096", "--output", "json"]),
    );
    assert_eq!(
        context_before["worktree"]["worktree_root"],
        worktree_path.display().to_string()
    );
    assert!(context_before["content"]
        .as_str()
        .unwrap()
        .contains("Execution worktree:"));

    let handoff_output = forge()
        .current_dir(&repository)
        .arg("--store")
        .arg(&store)
        .args([
            "task",
            "handoff",
            "--workflow",
            workflow_id,
            "--task",
            "task-001",
            "--executor",
            "codex",
            "--budget",
            "4096",
            "--output",
            "json",
        ])
        .output()
        .unwrap();
    let handoff: Value = serde_json::from_slice(&handoff_output.stdout).unwrap();
    assert_eq!(handoff["context"]["worktree"]["id"], worktree_id);
    assert_eq!(
        handoff["packet"]["worktree"]["worktree_root"],
        worktree_path.display().to_string()
    );

    let sandbox_plan = run_json(forge().arg("--store").arg(&store).args([
        "worktree",
        "sandbox",
        "plan",
        "--worktree",
        worktree_id,
        "--purpose",
        "test",
        "--workflow",
        workflow_id,
        "--task",
        "task-001",
        "--output",
        "json",
        "--",
        "cargo",
        "--version",
    ]));
    assert_eq!(sandbox_plan["status"], "sandbox_ready");
    assert_eq!(sandbox_plan["allowed"], true);
    assert_eq!(sandbox_plan["filesystem_isolation_enforced"], false);

    let sandbox_run = run_json(forge().arg("--store").arg(&store).args([
        "worktree",
        "sandbox",
        "run",
        "--worktree",
        worktree_id,
        "--purpose",
        "test",
        "--workflow",
        workflow_id,
        "--task",
        "task-001",
        "--allow-exec",
        "--output",
        "json",
        "--",
        "cargo",
        "--version",
    ]));
    assert_eq!(sandbox_run["status"], "sandbox_completed");
    assert_eq!(sandbox_run["exit_code"], 0);
    assert!(sandbox_run["stdout"]["content"]
        .as_str()
        .unwrap()
        .contains("cargo"));
    assert!(!worktree_path.join(".forge/forge.sqlite").exists());

    let config_path = worktree_path.join(".forge/worktree.toml");
    let config = fs::read_to_string(&config_path).unwrap();
    fs::write(
        &config_path,
        config.replace("max_command_seconds = 900", "max_command_seconds = 901"),
    )
    .unwrap();
    let context_after = run_json(
        forge()
            .current_dir(&repository)
            .arg("--store")
            .arg(&store)
            .args(["context", "--workflow", workflow_id, "--task", "task-001"])
            .args(["--budget", "4096", "--output", "json"]),
    );
    assert_ne!(
        context_before["context_sha256"],
        context_after["context_sha256"]
    );
    assert_ne!(
        context_before["worktree"]["config_sha256"],
        context_after["worktree"]["config_sha256"]
    );
    assert_eq!(context_after["worktree"]["binding_drifted"], true);
    assert!(context_after["worktree"]["binding_drift_reasons"]
        .as_array()
        .is_some_and(|reasons| reasons
            .iter()
            .any(|reason| reason == "config_changed_since_binding")));

    let drifted_plan = run_json_with_failure(forge().arg("--store").arg(&store).args([
        "worktree",
        "sandbox",
        "plan",
        "--worktree",
        worktree_id,
        "--purpose",
        "test",
        "--workflow",
        workflow_id,
        "--task",
        "task-001",
        "--output",
        "json",
        "--",
        "cargo",
        "--version",
    ]));
    assert_eq!(drifted_plan["allowed"], false);
    assert!(drifted_plan["blockers"]
        .as_array()
        .is_some_and(|blockers| blockers.iter().any(|blocker| {
            blocker
                .as_str()
                .is_some_and(|value| value.contains("binding_fingerprint"))
        })));

    let request = run_json(
        forge()
            .current_dir(&repository)
            .arg("--store")
            .arg(&store)
            .args([
                "request",
                "start",
                "--goal",
                "Run inside the registered worktree",
                "--worktree",
                worktree_id,
                "--output",
                "json",
            ]),
    );
    assert_eq!(
        request["worktree"]["worktree_root"],
        worktree_path.display().to_string()
    );
    assert_eq!(
        request["handoff_contract"]["allowed_context"]["command"][0],
        "forge"
    );
    assert_eq!(
        request["handoff_contract"]["allowed_context"]["command"][1],
        "--store"
    );
    assert_eq!(
        request["handoff_contract"]["allowed_context"]["command"][2],
        store.display().to_string()
    );
}

#[test]
fn task_binding_precedes_workflow_binding() {
    let temp = tempdir().unwrap();
    let repository = temp.path().join("repository");
    let first_path = temp.path().join("workflow-default");
    let second_path = temp.path().join("task-specific");
    let store = temp.path().join("forge.sqlite");
    init_repository(&repository);
    let first = create_registered_worktree(&store, &repository, &first_path, "feature/default");
    let second = create_registered_worktree(&store, &repository, &second_path, "feature/task");
    let first_id = first["worktree"]["id"].as_str().unwrap();
    let second_id = second["worktree"]["id"].as_str().unwrap();

    let workflow = run_json(
        forge()
            .current_dir(&repository)
            .arg("--store")
            .arg(&store)
            .args([
                "plan",
                "--goal",
                "Test scoped worktree bindings",
                "--output",
                "json",
            ]),
    );
    let workflow_id = workflow["workflow_id"].as_str().unwrap();
    run_json(
        forge()
            .arg("--store")
            .arg(&store)
            .args(["worktree", "bind", "--worktree", first_id])
            .args(["--workflow", workflow_id, "--output", "json"]),
    );
    run_json(
        forge()
            .arg("--store")
            .arg(&store)
            .args(["worktree", "bind", "--worktree", second_id])
            .args([
                "--workflow",
                workflow_id,
                "--task",
                "task-001",
                "--output",
                "json",
            ]),
    );

    let task_specific = run_json(
        forge()
            .arg("--store")
            .arg(&store)
            .args(["context", "--workflow", workflow_id, "--task", "task-001"])
            .args(["--budget", "4096", "--output", "json"]),
    );
    let workflow_default = run_json(
        forge()
            .arg("--store")
            .arg(&store)
            .args(["context", "--workflow", workflow_id, "--task", "task-002"])
            .args(["--budget", "4096", "--output", "json"]),
    );
    assert_eq!(
        task_specific["worktree"]["worktree_root"],
        second_path.display().to_string()
    );
    assert_eq!(
        workflow_default["worktree"]["worktree_root"],
        first_path.display().to_string()
    );

    let status = run_json(forge().arg("--store").arg(&store).args([
        "status",
        "--workflow",
        workflow_id,
        "--output",
        "json",
    ]));
    assert_eq!(status["worktrees"]["count"], 2);
    let status_ids = status["worktrees"]["worktrees"]
        .as_array()
        .unwrap()
        .iter()
        .map(|worktree| worktree["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(status_ids.contains(&first_id));
    assert!(status_ids.contains(&second_id));

    let project_root_conflict = forge()
        .arg("--store")
        .arg(&store)
        .args([
            "harness",
            "exec",
            "--executor",
            "codex",
            "--workflow",
            workflow_id,
            "--task",
            "task-001",
            "--project-root",
        ])
        .arg(&first_path)
        .args(["--output", "json"])
        .output()
        .unwrap();
    assert!(!project_root_conflict.status.success());
    assert!(
        String::from_utf8_lossy(&project_root_conflict.stderr).contains("explicit project root")
    );

    let cwd_conflict = forge()
        .arg("--store")
        .arg(&store)
        .args([
            "harness",
            "exec",
            "--executor",
            "codex",
            "--workflow",
            workflow_id,
            "--task",
            "task-001",
            "--cwd",
        ])
        .arg(&first_path)
        .args(["--output", "json"])
        .output()
        .unwrap();
    assert!(!cwd_conflict.status.success());
    assert!(String::from_utf8_lossy(&cwd_conflict.stderr).contains("explicit cwd"));
}

#[test]
fn custom_worktree_id_survives_binding_context_and_handoff() {
    let temp = tempdir().unwrap();
    let repository = temp.path().join("repository");
    let store = temp.path().join("forge.sqlite");
    init_repository(&repository);

    let registered = run_json(
        forge()
            .arg("--store")
            .arg(&store)
            .args(["worktree", "register", "--path"])
            .arg(&repository)
            .args(["--id", "custom_worktree", "--output", "json"]),
    );
    assert_eq!(registered["worktree"]["id"], "custom_worktree");

    let workflow = run_json(
        forge()
            .current_dir(&repository)
            .arg("--store")
            .arg(&store)
            .args([
                "plan",
                "--goal",
                "Preserve a custom registered worktree identity",
                "--output",
                "json",
            ]),
    );
    let workflow_id = workflow["workflow_id"].as_str().unwrap();
    run_json(forge().arg("--store").arg(&store).args([
        "worktree",
        "bind",
        "--worktree",
        "custom_worktree",
        "--workflow",
        workflow_id,
        "--output",
        "json",
    ]));

    let context = run_json(
        forge()
            .arg("--store")
            .arg(&store)
            .args(["context", "--workflow", workflow_id, "--task", "task-001"])
            .args(["--budget", "4096", "--output", "json"]),
    );
    assert_eq!(context["worktree"]["id"], "custom_worktree");

    let handoff = run_json(forge().arg("--store").arg(&store).args([
        "task",
        "handoff",
        "--workflow",
        workflow_id,
        "--task",
        "task-001",
        "--executor",
        "codex",
        "--budget",
        "4096",
        "--output",
        "json",
    ]));
    assert_eq!(handoff["context"]["worktree"]["id"], "custom_worktree");
    assert_eq!(handoff["packet"]["worktree"]["id"], "custom_worktree");
}

#[test]
fn worktree_creation_requires_authorization_and_migration_is_additive() {
    let temp = tempdir().unwrap();
    let repository = temp.path().join("repository");
    let worktree_path = temp.path().join("blocked-worktree");
    let store = temp.path().join("forge.sqlite");
    init_repository(&repository);
    ForgeStore::open(&store).unwrap();

    let output = forge()
        .arg("--store")
        .arg(&store)
        .args(["worktree", "create", "--repository"])
        .arg(&repository)
        .arg("--path")
        .arg(&worktree_path)
        .args(["--branch", "feature/blocked", "--output", "json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--allow-repository-mutation"));
    assert!(!worktree_path.exists());

    let connection = Connection::open(&store).unwrap();
    let table_count: i64 = connection
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='worktree_states'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let index_count: i64 = connection
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='index' AND name='idx_worktree_states_repository'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(table_count, 1);
    assert_eq!(index_count, 1);
}

#[test]
fn missing_manifest_returns_a_versioned_blocked_plan() {
    let temp = tempdir().unwrap();
    let repository = temp.path().join("repository");
    let store = temp.path().join("forge.sqlite");
    init_repository(&repository);
    let registered = run_json(
        forge()
            .arg("--store")
            .arg(&store)
            .args(["worktree", "register", "--path"])
            .arg(&repository)
            .args(["--output", "json"]),
    );
    assert_eq!(registered["worktree"]["created_by_forge"], false);
    let worktree_id = registered["worktree"]["id"].as_str().unwrap();

    let blocked = run_json_with_failure(forge().arg("--store").arg(&store).args([
        "worktree",
        "sandbox",
        "plan",
        "--worktree",
        worktree_id,
        "--purpose",
        "test",
        "--output",
        "json",
        "--",
        "cargo",
        "--version",
    ]));
    assert_eq!(blocked["schema_version"], "forge.worktree.sandbox_plan.v1");
    assert_eq!(blocked["status"], "sandbox_blocked");
    assert!(blocked["blockers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|blocker| blocker.as_str().unwrap().contains("config_present")));
}
