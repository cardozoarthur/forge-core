use assert_cmd::Command;
use foundry_core::graph::TaskStatus;
use foundry_core::storage::FoundryStore;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Output};
use tempfile::{tempdir, TempDir};

const GUARDED_GOAL: &str = "Prepare an approved update to .foundry/worktree.toml with auditable \
evidence.";

fn foundry() -> Command {
    Command::cargo_bin("foundry").expect("foundry binary should build")
}

fn command_output(command: &mut Command) -> Output {
    command.output().expect("foundry command should run")
}

fn success_json(command: &mut Command) -> Value {
    let output = command_output(command);
    assert!(
        output.status.success(),
        "foundry command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("successful foundry output should be JSON")
}

fn blocked_json(command: &mut Command) -> Value {
    let output = command_output(command);
    assert!(
        !output.status.success(),
        "blocked foundry command unexpectedly succeeded\nstdout:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "blocked foundry output should be JSON: {error}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
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
        "git init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    for (key, value) in [
        ("user.email", "foundry-worktree-guard@example.invalid"),
        ("user.name", "Foundry Worktree Guard Test"),
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
            "initial",
        ])
        .status()
        .expect("git commit should run")
        .success());
}

struct GuardFixture {
    _temp: TempDir,
    repository: PathBuf,
    store: PathBuf,
    worktree_root: PathBuf,
    worktree_id: String,
    workflow_id: String,
}

impl GuardFixture {
    fn new() -> Self {
        let temp = tempdir().unwrap();
        let repository = temp.path().join("repository");
        let store = temp.path().join("foundry.sqlite");
        let worktree_root = temp.path().join("guarded-worktree");
        init_repository(&repository);

        let created = success_json(
            foundry()
                .arg("--store")
                .arg(&store)
                .args(["worktree", "create", "--repository"])
                .arg(&repository)
                .arg("--path")
                .arg(&worktree_root)
                .args([
                    "--branch",
                    "feature/worktree-guard",
                    "--allow-repository-mutation",
                    "--output",
                    "json",
                ]),
        );
        let worktree_id = created["worktree"]["id"]
            .as_str()
            .expect("created worktree id")
            .to_string();

        let initialized = success_json(
            foundry()
                .arg("--store")
                .arg(&store)
                .args(["worktree", "init", "--worktree", &worktree_id])
                .args(["--allow-worktree-write", "--output", "json"]),
        );
        assert_eq!(initialized["status"], "worktree_initialized");

        let planned = success_json(
            foundry()
                .current_dir(&repository)
                .arg("--store")
                .arg(&store)
                .args([
                    "plan",
                    "--goal",
                    "Enforce guarded worktree mutations through objective predecessor tasks",
                    "--output",
                    "json",
                ]),
        );
        let workflow_id = planned["workflow_id"]
            .as_str()
            .expect("planned workflow id")
            .to_string();
        success_json(
            foundry()
                .arg("--store")
                .arg(&store)
                .args(["worktree", "bind", "--worktree", &worktree_id])
                .args([
                    "--workflow",
                    &workflow_id,
                    "--origin",
                    "test",
                    "--output",
                    "json",
                ]),
        );

        Self {
            _temp: temp,
            repository,
            store,
            worktree_root,
            worktree_id,
            workflow_id,
        }
    }

    fn write_guard_config(&self) {
        self.write_guard_config_without_approval();
        self.approve_guard_config(&self.worktree_id);
    }

    fn write_guard_config_without_approval(&self) {
        fs::write(
            self.worktree_root.join(".foundry/worktree.toml"),
            r#"schema_version = "foundry.worktree.config.v1"

[guardrails]
modifiable_paths = ["src/", "tests/safe.rs"]
protected_paths = ["src/secret/", ".foundry/worktree.toml"]
require_workflow_binding = true
"#,
        )
        .unwrap();
    }

    fn approve_guard_config(&self, worktree_id: &str) {
        let approved = success_json(foundry().arg("--store").arg(&self.store).args([
            "worktree",
            "approve-config",
            "--worktree",
            worktree_id,
            "--allow-guardrail-update",
            "--approved-by",
            "contract-auditor",
            "--origin",
            "test",
            "--output",
            "json",
        ]));
        assert_eq!(approved["status"], "worktree_config_approved");
    }

    fn guard_check(&self, path: &str, task: Option<&str>) -> Command {
        let mut command = foundry();
        command.arg("--store").arg(&self.store).args([
            "worktree",
            "guard",
            "check",
            "--worktree",
            &self.worktree_id,
            "--operation",
            "modify",
            "--path",
            path,
            "--reason",
            "contract test for guarded worktree mutation",
            "--workflow",
            &self.workflow_id,
        ]);
        if let Some(task) = task {
            command.args(["--task", task]);
        }
        command.args(["--output", "json"]);
        command
    }
}

fn assert_blocked_task_contract(
    report: &Value,
    store: &Path,
    workflow_id: &str,
    task_id: &str,
    guarded_path: &str,
) {
    assert_eq!(report["allowed"], false);
    assert!(
        report["status"]
            .as_str()
            .is_some_and(|status| status.contains("blocked")),
        "blocked guard status should be explicit: {report}"
    );
    assert_eq!(report["current_task_action"], "blocked");

    let task_spec = report["required_task_spec"]
        .as_object()
        .expect("blocked response should contain required_task_spec");
    assert!(task_spec
        .get("goal")
        .and_then(Value::as_str)
        .is_some_and(|goal| !goal.trim().is_empty()));
    assert!(task_spec
        .get("paths")
        .and_then(Value::as_array)
        .is_some_and(|paths| paths.iter().any(|path| path == guarded_path)));

    let next_command = report["next_command"]
        .as_array()
        .expect("blocked response should provide a next command");
    let expected_prefix = [
        Value::String("foundry".to_string()),
        Value::String("--store".to_string()),
        Value::String(store.display().to_string()),
        Value::String("worktree".to_string()),
        Value::String("guard".to_string()),
        Value::String("create-predecessor".to_string()),
    ];
    assert!(next_command.starts_with(&expected_prefix));
    for expected in [workflow_id, task_id, guarded_path, "--goal"] {
        assert!(
            next_command.iter().any(|value| value == expected),
            "next command should contain {expected}: {next_command:?}"
        );
    }
}

#[test]
fn guard_defaults_and_path_rules_allow_files_and_directories_with_protection_precedence() {
    let fixture = GuardFixture::new();

    let initialized = success_json(foundry().arg("--store").arg(&fixture.store).args([
        "worktree",
        "inspect",
        "--worktree",
        &fixture.worktree_id,
        "--output",
        "json",
    ]));
    assert_eq!(
        initialized["config"]["config"]["guardrails"]["modifiable_paths"],
        serde_json::json!(["."])
    );
    let protected_defaults = initialized["config"]["config"]["guardrails"]["protected_paths"]
        .as_array()
        .expect("default protected_paths");
    assert!(protected_defaults.iter().any(|path| path == ".git/"));
    assert!(protected_defaults
        .iter()
        .any(|path| path == ".foundry/worktree.toml"));

    fixture.write_guard_config();

    for allowed_path in ["src/lib.rs", "src/nested/module.rs", "tests/safe.rs"] {
        let allowed = success_json(&mut fixture.guard_check(allowed_path, Some("task-001")));
        assert_eq!(
            allowed["allowed"], true,
            "path should be allowed: {allowed_path}"
        );
        assert_eq!(allowed["current_task_action"], "continue");
        assert!(allowed["required_task_spec"].is_null());
    }

    for blocked_path in ["src/secret/key.rs", ".foundry/worktree.toml", "README.md"] {
        let blocked = blocked_json(&mut fixture.guard_check(blocked_path, Some("task-001")));
        assert_blocked_task_contract(
            &blocked,
            &fixture.store,
            &fixture.workflow_id,
            "task-001",
            blocked_path,
        );
    }
}

#[test]
fn predecessor_creation_rejects_mixed_delegable_and_unsafe_paths_atomically() {
    let fixture = GuardFixture::new();
    fixture.write_guard_config();
    let before_store = FoundryStore::open(&fixture.store).unwrap();
    let before = before_store.load_workflow(&fixture.workflow_id).unwrap();
    let before_task_count = before.tasks.len();
    let before_revision = before.revisions.last().map(|revision| revision.revision);
    drop(before_store);

    let output = command_output(foundry().arg("--store").arg(&fixture.store).args([
        "worktree",
        "guard",
        "create-predecessor",
        "--worktree",
        &fixture.worktree_id,
        "--workflow",
        &fixture.workflow_id,
        "--task",
        "task-001",
        "--path",
        ".foundry/worktree.toml",
        "--path",
        "../outside",
        "--goal",
        "Update .foundry/worktree.toml and ../outside with validated evidence",
        "--allow-workflow-mutation",
        "--approved-by",
        "contract-auditor",
        "--origin",
        "test",
        "--output",
        "json",
    ]));
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("every guard denial"));

    let store = FoundryStore::open(&fixture.store).unwrap();
    let after = store.load_workflow(&fixture.workflow_id).unwrap();
    assert_eq!(after.tasks.len(), before_task_count);
    assert_eq!(
        after.revisions.last().map(|revision| revision.revision),
        before_revision
    );
}

#[test]
fn blocked_guard_can_create_an_approved_predecessor_without_completing_current_task() {
    let fixture = GuardFixture::new();
    fixture.write_guard_config();
    let guarded_path = ".foundry/worktree.toml";

    let blocked = blocked_json(&mut fixture.guard_check(guarded_path, Some("task-001")));
    assert_blocked_task_contract(
        &blocked,
        &fixture.store,
        &fixture.workflow_id,
        "task-001",
        guarded_path,
    );

    let before_store = FoundryStore::open(&fixture.store).unwrap();
    let before = before_store.load_workflow(&fixture.workflow_id).unwrap();
    let before_task_count = before.tasks.len();
    let before_revision = before
        .revisions
        .last()
        .map(|revision| revision.revision)
        .unwrap_or(0);
    drop(before_store);

    let denied = command_output(foundry().arg("--store").arg(&fixture.store).args([
        "worktree",
        "guard",
        "create-predecessor",
        "--worktree",
        &fixture.worktree_id,
        "--workflow",
        &fixture.workflow_id,
        "--task",
        "task-001",
        "--path",
        guarded_path,
        "--goal",
        GUARDED_GOAL,
        "--approved-by",
        "contract-auditor",
        "--origin",
        "test",
        "--output",
        "json",
    ]));
    assert!(!denied.status.success());
    assert!(String::from_utf8_lossy(&denied.stderr).contains("--allow-workflow-mutation"));

    let unchanged_store = FoundryStore::open(&fixture.store).unwrap();
    let unchanged = unchanged_store.load_workflow(&fixture.workflow_id).unwrap();
    assert_eq!(unchanged.tasks.len(), before_task_count);
    assert_eq!(
        unchanged.revisions.last().map(|revision| revision.revision),
        Some(before_revision)
    );
    drop(unchanged_store);

    let created = success_json(foundry().arg("--store").arg(&fixture.store).args([
        "worktree",
        "guard",
        "create-predecessor",
        "--worktree",
        &fixture.worktree_id,
        "--workflow",
        &fixture.workflow_id,
        "--task",
        "task-001",
        "--path",
        guarded_path,
        "--goal",
        GUARDED_GOAL,
        "--allow-workflow-mutation",
        "--approved-by",
        "contract-auditor",
        "--origin",
        "test",
        "--output",
        "json",
    ]));
    assert!(created["status"]
        .as_str()
        .is_some_and(|status| status.contains("predecessor")));

    let store = FoundryStore::open(&fixture.store).unwrap();
    let workflow = store.load_workflow(&fixture.workflow_id).unwrap();
    assert_eq!(workflow.tasks.len(), before_task_count + 1);
    assert_eq!(
        workflow.revisions.last().map(|revision| revision.revision),
        Some(before_revision + 1)
    );
    let revision = workflow.revisions.last().unwrap();
    assert_eq!(revision.origin, "test");
    assert!(revision.change_type.contains("worktree_guard"));

    let predecessor = workflow
        .tasks
        .iter()
        .find(|task| task.goal == GUARDED_GOAL)
        .expect("guard should create the requested objective predecessor");
    assert_eq!(predecessor.status, TaskStatus::Pending);
    let current = workflow
        .tasks
        .iter()
        .find(|task| task.id == "task-001")
        .expect("current task should remain in the workflow");
    assert_eq!(current.status, TaskStatus::Blocked);
    assert_ne!(current.status, TaskStatus::Completed);
    assert!(current.dependencies.contains(&predecessor.id));
    assert_ne!(workflow.status, "completed");

    let events = store.load_workflow_events(&fixture.workflow_id).unwrap();
    assert!(events.iter().any(|event| {
        event.kind.contains("worktree_guard")
            && event.data.to_string().contains(&predecessor.id)
            && event.data.to_string().contains("task-001")
    }));
}

#[test]
fn retrying_predecessor_creation_reuses_the_incomplete_equivalent_task() {
    let fixture = GuardFixture::new();
    fixture.write_guard_config();
    let guarded_path = ".foundry/worktree.toml";

    let first = success_json(foundry().arg("--store").arg(&fixture.store).args([
        "worktree",
        "guard",
        "create-predecessor",
        "--worktree",
        &fixture.worktree_id,
        "--workflow",
        &fixture.workflow_id,
        "--task",
        "task-001",
        "--path",
        guarded_path,
        "--goal",
        GUARDED_GOAL,
        "--allow-workflow-mutation",
        "--approved-by",
        "contract-auditor",
        "--origin",
        "test",
        "--output",
        "json",
    ]));
    let first_id = first["predecessor_task_id"].as_str().unwrap().to_string();
    let store = FoundryStore::open(&fixture.store).unwrap();
    let after_first = store.load_workflow(&fixture.workflow_id).unwrap();
    let task_count = after_first.tasks.len();
    let revision = after_first.revisions.last().unwrap().revision;
    drop(store);

    let retried = success_json(foundry().arg("--store").arg(&fixture.store).args([
        "worktree",
        "guard",
        "create-predecessor",
        "--worktree",
        &fixture.worktree_id,
        "--workflow",
        &fixture.workflow_id,
        "--task",
        "task-001",
        "--path",
        guarded_path,
        "--goal",
        GUARDED_GOAL,
        "--allow-workflow-mutation",
        "--approved-by",
        "contract-auditor",
        "--origin",
        "test-retry",
        "--output",
        "json",
    ]));
    assert_eq!(retried["status"], "worktree_guard_predecessor_reused");
    assert_eq!(retried["predecessor_task_id"], first_id);
    assert_eq!(retried["dependency_added"], false);
    assert_eq!(retried["workflow_revision"], revision);

    let store = FoundryStore::open(&fixture.store).unwrap();
    let after_retry = store.load_workflow(&fixture.workflow_id).unwrap();
    assert_eq!(after_retry.tasks.len(), task_count);
    assert_eq!(after_retry.revisions.last().unwrap().revision, revision);
    assert_eq!(
        after_retry
            .tasks
            .iter()
            .filter(|task| task.id == first_id)
            .count(),
        1
    );
}

#[test]
fn validated_predecessor_completion_reactivates_the_guarded_task() {
    let fixture = GuardFixture::new();
    fixture.write_guard_config();
    let guarded_path = ".foundry/worktree.toml";

    let created = success_json(foundry().arg("--store").arg(&fixture.store).args([
        "worktree",
        "guard",
        "create-predecessor",
        "--worktree",
        &fixture.worktree_id,
        "--workflow",
        &fixture.workflow_id,
        "--task",
        "task-001",
        "--path",
        guarded_path,
        "--goal",
        GUARDED_GOAL,
        "--allow-workflow-mutation",
        "--approved-by",
        "contract-auditor",
        "--origin",
        "test",
        "--output",
        "json",
    ]));
    assert_eq!(
        created["current_task_action"],
        "blocked_by_predecessor_dependency"
    );
    let predecessor_id = created["predecessor_task_id"]
        .as_str()
        .expect("created predecessor id")
        .to_string();

    let blocked_store = FoundryStore::open(&fixture.store).unwrap();
    let blocked_workflow = blocked_store.load_workflow(&fixture.workflow_id).unwrap();
    let blocked_current = blocked_workflow
        .tasks
        .iter()
        .find(|task| task.id == "task-001")
        .unwrap();
    assert_eq!(blocked_current.status, TaskStatus::Blocked);
    assert_eq!(
        blocked_current.work_item.backlog_state,
        "blocked_by_worktree_guardrail"
    );
    assert!(blocked_current
        .work_item
        .impediments
        .iter()
        .any(|impediment| impediment.contains(&predecessor_id)));
    drop(blocked_store);

    let response_path = fixture._temp.path().join("guard-predecessor-response.json");
    fs::write(
        &response_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": "foundry.executor_response.v1",
            "task_id": predecessor_id,
            "status": "completed",
            "artifacts": [],
            "trace_ref": "traces/worktree-guard-predecessor.jsonl",
            "cost": {
                "estimated_usd": 0.0,
                "tokens_in": 0,
                "tokens_out": 0
            },
            "validation_evidence": [{
                "command": "worktree guard contract validation",
                "exit_code": 0,
                "summary": "protected path predecessor goal is definitively ready"
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    let validation = success_json(
        foundry()
            .arg("--store")
            .arg(&fixture.store)
            .args(["task", "validate-response", "--workflow"])
            .arg(&fixture.workflow_id)
            .args(["--task", &predecessor_id, "--response"])
            .arg(&response_path)
            .args(["--output", "json"]),
    );
    assert_eq!(validation["accepted"], true);
    assert_eq!(validation["response_status"], "completed");

    let store = FoundryStore::open(&fixture.store).unwrap();
    let workflow = store.load_workflow(&fixture.workflow_id).unwrap();
    let predecessor = workflow
        .tasks
        .iter()
        .find(|task| task.id == predecessor_id)
        .unwrap();
    assert_eq!(predecessor.status, TaskStatus::Completed);

    let current = workflow
        .tasks
        .iter()
        .find(|task| task.id == "task-001")
        .unwrap();
    assert_eq!(current.status, TaskStatus::Pending);
    assert_eq!(
        current.work_item.backlog_state,
        "ready_after_worktree_guard_predecessor"
    );
    assert!(current.dependencies.contains(&predecessor_id));
    assert!(current
        .work_item
        .impediments
        .iter()
        .all(|impediment| !impediment.starts_with("worktree guard predecessor ")));
}

#[test]
fn task_specific_binding_blocks_guard_checks_against_the_workflow_default_worktree() {
    let fixture = GuardFixture::new();
    fixture.write_guard_config();
    let second_root = fixture._temp.path().join("task-specific-worktree");
    let second = success_json(
        foundry()
            .arg("--store")
            .arg(&fixture.store)
            .args(["worktree", "create", "--repository"])
            .arg(&fixture.repository)
            .arg("--path")
            .arg(&second_root)
            .args([
                "--branch",
                "feature/task-specific-guard",
                "--allow-repository-mutation",
                "--output",
                "json",
            ]),
    );
    let second_id = second["worktree"]["id"].as_str().unwrap().to_string();
    success_json(
        foundry()
            .arg("--store")
            .arg(&fixture.store)
            .args(["worktree", "init", "--worktree", &second_id])
            .args(["--allow-worktree-write", "--output", "json"]),
    );
    fs::write(
        second_root.join(".foundry/worktree.toml"),
        fs::read_to_string(fixture.worktree_root.join(".foundry/worktree.toml")).unwrap(),
    )
    .unwrap();
    fixture.approve_guard_config(&second_id);
    success_json(
        foundry()
            .arg("--store")
            .arg(&fixture.store)
            .args(["worktree", "bind", "--worktree", &second_id])
            .args([
                "--workflow",
                &fixture.workflow_id,
                "--task",
                "task-001",
                "--origin",
                "test",
                "--output",
                "json",
            ]),
    );

    let wrong = blocked_json(&mut fixture.guard_check("src/lib.rs", Some("task-001")));
    assert_eq!(wrong["allowed"], false);
    assert_eq!(wrong["current_task_action"], "blocked");
    assert!(
        wrong["decisions"].as_array().is_some_and(|decisions| {
            decisions.iter().any(|decision| {
                decision["reason"]
                    .as_str()
                    .is_some_and(|reason| reason.contains("bound to a different worktree"))
            })
        }),
        "wrong-worktree response should identify the binding mismatch: {wrong}"
    );

    let wrong_protected =
        blocked_json(&mut fixture.guard_check(".foundry/worktree.toml", Some("task-001")));
    assert!(wrong_protected["required_task_spec"].is_null());
    assert!(wrong_protected["next_command"]
        .as_array()
        .is_some_and(Vec::is_empty));
    assert_eq!(
        wrong_protected["decisions"][0]["delegable_to_predecessor"],
        false
    );

    let correct_fixture = GuardFixtureRef {
        store: &fixture.store,
        worktree_id: &second_id,
        workflow_id: &fixture.workflow_id,
    };
    let correct = success_json(&mut correct_fixture.guard_check("src/lib.rs", "task-001"));
    assert_eq!(correct["allowed"], true);
}

#[test]
fn unapproved_config_is_not_misclassified_as_a_protected_path_scope() {
    let fixture = GuardFixture::new();
    fixture.write_guard_config_without_approval();

    let blocked = blocked_json(&mut fixture.guard_check("src/lib.rs", Some("task-001")));
    assert_eq!(blocked["allowed"], false);
    assert!(blocked["decisions"].as_array().is_some_and(|decisions| {
        decisions.iter().any(|decision| {
            decision["reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("manifest hash is not approved"))
        })
    }));

    assert!(
        blocked["required_task_spec"].is_null(),
        "an unapproved config hash should not be delegated as a protected path task: {blocked}"
    );
    let next_command = blocked["next_command"]
        .as_array()
        .expect("unapproved config should expose an approval command");
    let expected_prefix = [
        Value::String("foundry".to_string()),
        Value::String("--store".to_string()),
        Value::String(fixture.store.display().to_string()),
        Value::String("worktree".to_string()),
        Value::String("approve-config".to_string()),
    ];
    assert!(next_command.starts_with(&expected_prefix));
}

struct GuardFixtureRef<'a> {
    store: &'a Path,
    worktree_id: &'a str,
    workflow_id: &'a str,
}

impl GuardFixtureRef<'_> {
    fn guard_check(&self, path: &str, task: &str) -> Command {
        let mut command = foundry();
        command.arg("--store").arg(self.store).args([
            "worktree",
            "guard",
            "check",
            "--worktree",
            self.worktree_id,
            "--operation",
            "modify",
            "--path",
            path,
            "--reason",
            "verify the task-specific worktree binding",
            "--workflow",
            self.workflow_id,
            "--task",
            task,
            "--output",
            "json",
        ]);
        command
    }
}

#[test]
fn invalid_worktree_toml_is_an_explicit_failure_instead_of_falling_back_to_defaults() {
    let fixture = GuardFixture::new();
    let manifest = fixture.worktree_root.join(".foundry/worktree.toml");
    fs::write(
        &manifest,
        "schema_version = \"foundry.worktree.config.v1\"\n[guardrails\nmodifiable_paths = [\".\"]\n",
    )
    .unwrap();

    let output = command_output(&mut fixture.guard_check("src/lib.rs", Some("task-001")));
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid worktree config"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("worktree.toml"), "stderr: {stderr}");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("\"allowed\": true"));
}

#[test]
fn cli_integration_module_rename_keeps_the_legacy_harness_alias_without_the_old_file() {
    let new_type = std::any::type_name::<foundry_core::cli_integration::CliHarnessExecReceipt>();
    let legacy_type = std::any::type_name::<foundry_core::harness::CliHarnessExecReceipt>();
    assert_eq!(new_type, legacy_type);
    assert_eq!(
        foundry_core::cli_integration::CLI_HARNESS_EXEC_SCHEMA_VERSION,
        foundry_core::harness::CLI_HARNESS_EXEC_SCHEMA_VERSION
    );

    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert!(source_root.join("cli_integration.rs").is_file());
    assert!(!source_root.join("harness.rs").exists());

    let compatibility = success_json(foundry().args(["harness", "mode", "--output", "json"]));
    assert_eq!(compatibility["schema_version"], "foundry.harness.mode.v1");
}
