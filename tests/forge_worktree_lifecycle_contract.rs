use assert_cmd::Command;
use forge_core::storage::ForgeStore;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Output};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::{tempdir, TempDir};

const LIFECYCLE_SCHEMA_VERSION: &str = "forge.worktree.sandbox_lifecycle.v1";
const SANDBOX_ID_FIELD: &str = "sandbox_id";
const SANDBOX_SELECTOR_FLAG: &str = "--sandbox";

fn forge() -> Command {
    Command::cargo_bin("forge").expect("forge binary should build")
}

fn command_output(command: &mut Command) -> Output {
    command.output().expect("forge command should run")
}

fn success_json(command: &mut Command) -> Value {
    let output = command_output(command);
    assert!(
        output.status.success(),
        "forge command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("successful forge output should be JSON")
}

fn failure_json(command: &mut Command) -> Value {
    let output = command_output(command);
    assert!(
        !output.status.success(),
        "forge command unexpectedly succeeded\nstdout:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "failed forge command should return a JSON contract: {error}\nstdout:\n{}\nstderr:\n{}",
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
        ("user.email", "forge-worktree-lifecycle@example.invalid"),
        ("user.name", "Forge Worktree Lifecycle Test"),
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
    commit_empty(path, "initial");
}

fn commit_empty(repository: &Path, message: &str) {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repository)
        .args([
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--allow-empty",
            "-m",
            message,
        ])
        .output()
        .expect("git commit should run");
    assert!(
        output.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_head(repository: &Path) -> String {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repository)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git rev-parse should run");
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

struct LifecycleFixture {
    _temp: TempDir,
    repository: PathBuf,
    store: PathBuf,
    worktree_id: String,
    workflow_id: String,
    task_id: String,
}

impl LifecycleFixture {
    fn new() -> Self {
        let temp = tempdir().unwrap();
        let repository = temp.path().join("repository");
        let store = temp.path().join("forge.sqlite");
        init_repository(&repository);

        let planned = success_json(
            forge()
                .current_dir(&repository)
                .arg("--store")
                .arg(&store)
                .args([
                    "plan",
                    "--goal",
                    "Validate persistent worktree preview lifecycle contracts",
                    "--output",
                    "json",
                ]),
        );
        let workflow_id = planned["workflow_id"].as_str().unwrap().to_string();
        let task_id = planned["tasks"][0]["id"].as_str().unwrap().to_string();

        let registered = success_json(
            forge()
                .arg("--store")
                .arg(&store)
                .args(["worktree", "register", "--path"])
                .arg(&repository)
                .args(["--output", "json"]),
        );
        let worktree_id = registered["worktree"]["id"].as_str().unwrap().to_string();
        success_json(
            forge()
                .arg("--store")
                .arg(&store)
                .args(["worktree", "init", "--worktree", &worktree_id])
                .args(["--allow-worktree-write", "--output", "json"]),
        );

        Self {
            _temp: temp,
            repository,
            store,
            worktree_id,
            workflow_id,
            task_id,
        }
    }

    fn config_path(&self) -> PathBuf {
        self.repository.join(".forge/worktree.toml")
    }

    fn rewrite_config(&self, rewrite: impl FnOnce(String) -> String) {
        let path = self.config_path();
        let before = fs::read_to_string(&path).unwrap();
        let after = rewrite(before.clone());
        assert_ne!(
            before, after,
            "test config rewrite should change the manifest"
        );
        fs::write(path, after).unwrap();
    }

    fn allow_command_and_timeout(&self, command: &str, timeout_seconds: u64) {
        self.rewrite_config(|config| {
            let config = config.replace(
                "    \"make\",\n]",
                &format!("    \"make\",\n    \"{command}\",\n]"),
            );
            config.replace(
                "max_command_seconds = 900",
                &format!("max_command_seconds = {timeout_seconds}"),
            )
        });
    }

    fn add_inherited_environment(&self, name: &str) {
        self.rewrite_config(|config| {
            config.replace(
                "inherit_environment = [\n",
                &format!("inherit_environment = [\n    \"{name}\",\n"),
            )
        });
    }

    fn approve_config_output(&self) -> Output {
        command_output(forge().arg("--store").arg(&self.store).args([
            "worktree",
            "approve-config",
            "--worktree",
            &self.worktree_id,
            "--allow-guardrail-update",
            "--approved-by",
            "contract-auditor",
            "--origin",
            "test",
            "--output",
            "json",
        ]))
    }

    fn approve_config(&self) {
        let output = self.approve_config_output();
        assert!(
            output.status.success(),
            "config approval failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn bind(&self) -> Value {
        success_json(
            forge()
                .arg("--store")
                .arg(&self.store)
                .args(["worktree", "bind", "--worktree", &self.worktree_id])
                .args([
                    "--workflow",
                    &self.workflow_id,
                    "--origin",
                    "test",
                    "--output",
                    "json",
                ]),
        )
    }

    fn context(&self) -> Value {
        success_json(
            forge()
                .current_dir(&self.repository)
                .arg("--store")
                .arg(&self.store)
                .args([
                    "context",
                    "--workflow",
                    &self.workflow_id,
                    "--task",
                    &self.task_id,
                    "--budget",
                    "4096",
                    "--output",
                    "json",
                ]),
        )
    }

    fn sandbox_run(&self, command: &[&str]) -> Command {
        let mut forge = forge();
        forge
            .arg("--store")
            .arg(&self.store)
            .args([
                "worktree",
                "sandbox",
                "run",
                "--worktree",
                &self.worktree_id,
                "--purpose",
                "preview",
                "--workflow",
                &self.workflow_id,
                "--task",
                &self.task_id,
                "--allow-exec",
                "--output",
                "json",
                "--",
            ])
            .args(command);
        forge
    }

    fn sandbox_plan(&self, command: &[&str]) -> Command {
        let mut forge = forge();
        forge
            .arg("--store")
            .arg(&self.store)
            .args([
                "worktree",
                "sandbox",
                "plan",
                "--worktree",
                &self.worktree_id,
                "--purpose",
                "preview",
                "--workflow",
                &self.workflow_id,
                "--task",
                &self.task_id,
                "--output",
                "json",
                "--",
            ])
            .args(command);
        forge
    }

    fn sandbox_start(&self, allow_exec: bool, command: &[&str]) -> Command {
        let mut forge = forge();
        forge.arg("--store").arg(&self.store).args([
            "worktree",
            "sandbox",
            "start",
            "--worktree",
            &self.worktree_id,
            "--purpose",
            "preview",
            "--workflow",
            &self.workflow_id,
            "--task",
            &self.task_id,
        ]);
        if allow_exec {
            forge.arg("--allow-exec");
        }
        forge.args(["--output", "json", "--"]).args(command);
        forge
    }

    fn mcp_command(&self, tool: &str, input: Value) -> Command {
        let mut forge = forge();
        forge
            .arg("--store")
            .arg(&self.store)
            .args(["mcp", "call"])
            .arg(tool)
            .arg("--input")
            .arg(input.to_string())
            .args(["--output", "json"]);
        forge
    }

    fn mcp_call(&self, tool: &str, input: Value) -> Value {
        success_json(&mut self.mcp_command(tool, input))
    }
}

struct PreviewCleanup {
    store: PathBuf,
    sandbox_id: String,
    armed: bool,
}

impl PreviewCleanup {
    fn new(store: &Path, sandbox_id: &str) -> Self {
        Self {
            store: store.to_path_buf(),
            sandbox_id: sandbox_id.to_string(),
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for PreviewCleanup {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let _ = forge()
            .arg("--store")
            .arg(&self.store)
            .args([
                "worktree",
                "sandbox",
                "stop",
                SANDBOX_SELECTOR_FLAG,
                &self.sandbox_id,
                "--allow-stop",
                "--output",
                "json",
            ])
            .output();
    }
}

fn process_exists(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

fn wait_for_file(path: &Path) -> String {
    for _ in 0..100 {
        if let Ok(value) = fs::read_to_string(path) {
            if !value.trim().is_empty() {
                return value.trim().to_string();
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!("timed out waiting for {}", path.display());
}

fn assert_process_stopped(pid: u32) {
    for _ in 0..100 {
        if !process_exists(pid) {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!("sandbox descendant process {pid} is still alive");
}

fn recorded_pids(path: &Path) -> Vec<u32> {
    wait_for_file(path)
        .split_whitespace()
        .map(|value| value.parse::<u32>().unwrap())
        .collect()
}

fn sandbox_status(fixture: &LifecycleFixture, sandbox_id: &str) -> Value {
    success_json(forge().arg("--store").arg(&fixture.store).args([
        "worktree",
        "sandbox",
        "status",
        SANDBOX_SELECTOR_FLAG,
        sandbox_id,
        "--output",
        "json",
    ]))
}

fn wait_for_sandbox_status(
    fixture: &LifecycleFixture,
    sandbox_id: &str,
    predicate: impl Fn(&Value) -> bool,
) -> Value {
    let mut latest = Value::Null;
    for _ in 0..100 {
        latest = sandbox_status(fixture, sandbox_id);
        if predicate(&latest) {
            return latest;
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!("sandbox {sandbox_id} did not reach the expected state: {latest}");
}

fn latest_sandbox_event(fixture: &LifecycleFixture) -> Value {
    let store = ForgeStore::open(&fixture.store).unwrap();
    store
        .load_workflow_events(&fixture.workflow_id)
        .unwrap()
        .into_iter()
        .rev()
        .find(|event| event.kind == "worktree_sandbox_execution")
        .expect("sandbox execution should persist a receipt event")
        .data
}

fn assert_workflow_events_do_not_contain(fixture: &LifecycleFixture, secret: &str) {
    let store = ForgeStore::open(&fixture.store).unwrap();
    for event in store.load_workflow_events(&fixture.workflow_id).unwrap() {
        assert!(
            !event.data.to_string().contains(secret),
            "event {} persisted the raw secret",
            event.kind
        );
    }
}

fn assert_store_files_do_not_contain(store: &Path, secret: &str) {
    assert!(!secret.is_empty());
    let file_prefix = store.file_name().unwrap().to_string_lossy();
    for entry in fs::read_dir(store.parent().unwrap()).unwrap() {
        let entry = entry.unwrap();
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with(&*file_prefix)
            || !entry.file_type().unwrap().is_file()
        {
            continue;
        }
        let bytes = fs::read(entry.path()).unwrap();
        assert!(
            !bytes
                .windows(secret.len())
                .any(|window| window == secret.as_bytes()),
            "{} persisted the raw secret",
            entry.path().display()
        );
    }
}

#[test]
fn rebind_after_a_new_commit_refreshes_the_head_fingerprint_and_clears_drift() {
    let fixture = LifecycleFixture::new();
    let initial = fixture.bind();
    let initial_head = initial["binding"]["head_at_binding"].as_str().unwrap();

    commit_empty(&fixture.repository, "advance-head");
    let current_head = git_head(&fixture.repository);
    assert_ne!(initial_head, current_head);

    let drifted = fixture.context();
    assert_eq!(drifted["worktree"]["binding_drifted"], true);
    assert!(drifted["worktree"]["binding_drift_reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reason| reason == "head_changed_since_binding"));

    let rebound = fixture.bind();
    assert_eq!(rebound["binding"]["head_at_binding"], current_head);
    let refreshed = fixture.context();
    assert_eq!(refreshed["worktree"]["binding_drifted"], false);
    assert_eq!(
        refreshed["worktree"]["binding_drift_reasons"],
        serde_json::json!([])
    );
}

#[test]
fn timeout_covers_pipe_holding_descendants_and_kills_the_process_group() {
    let fixture = LifecycleFixture::new();
    fixture.allow_command_and_timeout("sh", 1);
    fixture.approve_config();
    fixture.bind();

    let started = Instant::now();
    let receipt = failure_json(&mut fixture.sandbox_run(&["sh", "-c", "sleep 3 &"]));
    let elapsed = started.elapsed();
    assert_eq!(
        receipt["schema_version"],
        "forge.worktree.sandbox_receipt.v1"
    );
    assert_eq!(receipt["status"], "sandbox_timed_out");
    assert_eq!(receipt["timed_out"], true);
    assert!(elapsed >= Duration::from_millis(700), "elapsed={elapsed:?}");
    assert!(
        elapsed < Duration::from_millis(2_500),
        "elapsed={elapsed:?}"
    );

    let pid_path = fixture
        .repository
        .join(".forge/sandboxes/internal/artifacts/descendant.pid");
    let command = format!(
        "sleep 5 & child=$!; printf '%s' \"$child\" > '{}'",
        pid_path.display()
    );
    let second = failure_json(&mut fixture.sandbox_run(&["sh", "-c", &command]));
    assert_eq!(second["status"], "sandbox_timed_out");
    let pid = wait_for_file(&pid_path).parse::<u32>().unwrap();
    assert_process_stopped(pid);
}

#[cfg(target_os = "linux")]
#[test]
fn timeout_kills_a_setsid_daemon_that_keeps_captured_pipes_open() {
    let fixture = LifecycleFixture::new();
    fixture.allow_command_and_timeout("sh", 1);
    fixture.approve_config();
    fixture.bind();

    let pid_path = fixture
        .repository
        .join(".forge/sandboxes/internal/artifacts/setsid-pipes.pids");
    let command = format!(
        "setsid sh -c 'sleep 30 & child=$!; printf \"%s %s\" \"$$\" \"$child\" > \"{}\"; wait \"$child\"' &",
        pid_path.display()
    );
    let started = Instant::now();
    let receipt = failure_json(&mut fixture.sandbox_run(&["sh", "-c", &command]));
    let elapsed = started.elapsed();

    assert_eq!(receipt["status"], "sandbox_timed_out");
    assert_eq!(receipt["timed_out"], true);
    assert_eq!(receipt["exit_code"], 0);
    assert!(receipt["error"].is_null(), "receipt: {receipt}");
    assert!(elapsed >= Duration::from_millis(700), "elapsed={elapsed:?}");
    assert!(
        elapsed < Duration::from_millis(2_500),
        "elapsed={elapsed:?}"
    );

    let pids = recorded_pids(&pid_path);
    assert_eq!(pids.len(), 2, "recorded pids: {pids:?}");
    for pid in pids {
        assert_process_stopped(pid);
    }
}

#[test]
fn launch_failure_returns_and_persists_a_structured_sandbox_receipt() {
    let fixture = LifecycleFixture::new();
    let missing = "forge-contract-command-that-does-not-exist";
    fixture.allow_command_and_timeout(missing, 5);
    fixture.approve_config();
    fixture.bind();

    let receipt = failure_json(&mut fixture.sandbox_run(&[missing]));
    assert_eq!(
        receipt["schema_version"],
        "forge.worktree.sandbox_receipt.v1"
    );
    assert!(receipt["status"]
        .as_str()
        .is_some_and(|status| status.contains("failed")));
    assert_eq!(receipt["allowed"], true);
    assert_eq!(receipt["executed"], false);
    assert!(receipt["exit_code"].is_null());

    let store = ForgeStore::open(&fixture.store).unwrap();
    let event = store
        .load_workflow_events(&fixture.workflow_id)
        .unwrap()
        .into_iter()
        .rev()
        .find(|event| event.kind == "worktree_sandbox_execution")
        .expect("launch failure should persist a sandbox receipt event");
    assert_eq!(
        event.data["schema_version"],
        "forge.worktree.sandbox_receipt.v1"
    );
    assert_eq!(event.data["status"], receipt["status"]);
    assert_eq!(event.data["executed"], false);
}

#[test]
fn inherited_environment_rejects_sensitive_and_launcher_dangerous_names() {
    for name in ["AWS_SECRET_ACCESS_KEY", "LD_PRELOAD"] {
        let fixture = LifecycleFixture::new();
        fixture.add_inherited_environment(name);
        let output = fixture.approve_config_output();
        assert!(
            !output.status.success(),
            "inherited environment name {name} should be rejected"
        );
        let evidence = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(evidence.contains(name), "evidence: {evidence}");
        assert!(
            evidence.contains("not permitted")
                || evidence.contains("sensitive")
                || evidence.contains("credential vault"),
            "evidence: {evidence}"
        );
    }
}

#[test]
fn inherited_environment_blocks_a_secret_even_when_the_variable_name_is_neutral() {
    let fixture = LifecycleFixture::new();
    let environment_name = "FORGE_BUILD_LABEL";
    let secret = format!("sk-proj-{}", "n".repeat(48));
    fixture.allow_command_and_timeout("sh", 5);
    fixture.add_inherited_environment(environment_name);
    fixture.approve_config();
    fixture.bind();

    let receipt = failure_json(
        fixture
            .sandbox_run(&["sh", "-c", "printf should-not-run"])
            .env(environment_name, &secret),
    );
    assert_eq!(receipt["status"], "blocked_by_worktree_guardrails");
    assert_eq!(receipt["execution_attempted"], false);
    assert_eq!(receipt["executed"], false);
    assert!(receipt["plan"]["guardrail_decisions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|decision| {
            decision["id"] == "inherited_environment_secret_free"
                && decision["decision"] == "blocked"
        }));
    assert!(!receipt["plan"]["inherited_environment"]
        .as_array()
        .unwrap()
        .iter()
        .any(|name| name == environment_name));
    assert!(!receipt.to_string().contains(&secret));

    let event = latest_sandbox_event(&fixture);
    assert_eq!(event["execution_attempted"], false);
    assert!(!event.to_string().contains(&secret));
    assert_workflow_events_do_not_contain(&fixture, &secret);
    assert_store_files_do_not_contain(&fixture.store, &secret);
}

#[test]
fn preview_start_status_stop_is_persistent_authorized_and_kills_the_group() {
    let fixture = LifecycleFixture::new();
    fixture.allow_command_and_timeout("sh", 5);
    fixture.approve_config();
    fixture.bind();

    let leader_path = fixture
        .repository
        .join(".forge/sandboxes/internal/artifacts/preview-leader.pid");
    let child_path = fixture
        .repository
        .join(".forge/sandboxes/internal/artifacts/preview-child.pid");
    let command = format!(
        "printf '%s' \"$$\" > '{}'; sleep 30 & child=$!; printf '%s' \"$child\" > '{}'; wait \"$child\"",
        leader_path.display(),
        child_path.display()
    );

    let denied = command_output(&mut fixture.sandbox_start(false, &["sh", "-c", &command]));
    assert!(!denied.status.success());
    let denied_evidence = format!(
        "{}\n{}",
        String::from_utf8_lossy(&denied.stdout),
        String::from_utf8_lossy(&denied.stderr)
    );
    assert!(denied_evidence.contains("--allow-exec"));

    let started_at = Instant::now();
    let started = success_json(&mut fixture.sandbox_start(true, &["sh", "-c", &command]));
    assert!(started_at.elapsed() < Duration::from_secs(2));
    assert_eq!(started["schema_version"], LIFECYCLE_SCHEMA_VERSION);
    assert_eq!(started["status"], "sandbox_running");
    let sandbox_id = started[SANDBOX_ID_FIELD]
        .as_str()
        .expect("sandbox start should return a persistent id")
        .to_string();
    let mut cleanup = PreviewCleanup::new(&fixture.store, &sandbox_id);

    let leader_pid = wait_for_file(&leader_path).parse::<u32>().unwrap();
    let child_pid = wait_for_file(&child_path).parse::<u32>().unwrap();
    assert!(process_exists(leader_pid));
    assert!(process_exists(child_pid));

    let status = success_json(forge().arg("--store").arg(&fixture.store).args([
        "worktree",
        "sandbox",
        "status",
        SANDBOX_SELECTOR_FLAG,
        &sandbox_id,
        "--output",
        "json",
    ]));
    assert_eq!(status["schema_version"], LIFECYCLE_SCHEMA_VERSION);
    assert_eq!(status[SANDBOX_ID_FIELD], sandbox_id);
    assert_eq!(status["status"], "sandbox_running");

    let denied_stop = command_output(forge().arg("--store").arg(&fixture.store).args([
        "worktree",
        "sandbox",
        "stop",
        SANDBOX_SELECTOR_FLAG,
        &sandbox_id,
        "--output",
        "json",
    ]));
    assert!(!denied_stop.status.success());
    let denied_stop_evidence = format!(
        "{}\n{}",
        String::from_utf8_lossy(&denied_stop.stdout),
        String::from_utf8_lossy(&denied_stop.stderr)
    );
    assert!(denied_stop_evidence.contains("--allow-stop"));

    let still_running = success_json(forge().arg("--store").arg(&fixture.store).args([
        "worktree",
        "sandbox",
        "status",
        SANDBOX_SELECTOR_FLAG,
        &sandbox_id,
        "--output",
        "json",
    ]));
    assert_eq!(still_running["status"], "sandbox_running");

    let stopped = success_json(forge().arg("--store").arg(&fixture.store).args([
        "worktree",
        "sandbox",
        "stop",
        SANDBOX_SELECTOR_FLAG,
        &sandbox_id,
        "--allow-stop",
        "--output",
        "json",
    ]));
    assert_eq!(stopped["schema_version"], LIFECYCLE_SCHEMA_VERSION);
    assert_eq!(stopped[SANDBOX_ID_FIELD], sandbox_id);
    assert_eq!(stopped["status"], "sandbox_stopped");
    assert_eq!(stopped["receipt_status"], "sandbox_stopped");
    assert!(stopped["error"].is_null());
    cleanup.disarm();

    assert_process_stopped(leader_pid);
    assert_process_stopped(child_pid);
    let final_status = success_json(forge().arg("--store").arg(&fixture.store).args([
        "worktree",
        "sandbox",
        "status",
        SANDBOX_SELECTOR_FLAG,
        &sandbox_id,
        "--output",
        "json",
    ]));
    assert_eq!(final_status["status"], "sandbox_stopped");
}

#[cfg(target_os = "linux")]
#[test]
fn persistent_stop_kills_a_redirected_setsid_descendant() {
    let fixture = LifecycleFixture::new();
    fixture.allow_command_and_timeout("sh", 30);
    fixture.approve_config();
    fixture.bind();

    let pid_path = fixture
        .repository
        .join(".forge/sandboxes/internal/artifacts/setsid-redirected.pid");
    let command = format!(
        "setsid sh -c 'printf \"%s\" \"$$\" > \"{}\"; exec sleep 30' </dev/null >/dev/null 2>&1 &",
        pid_path.display()
    );
    let started = success_json(&mut fixture.sandbox_start(true, &["sh", "-c", &command]));
    assert_eq!(started["status"], "sandbox_running");
    let sandbox_id = started[SANDBOX_ID_FIELD].as_str().unwrap().to_string();
    let mut cleanup = PreviewCleanup::new(&fixture.store, &sandbox_id);
    let daemon_pid = wait_for_file(&pid_path).parse::<u32>().unwrap();
    assert!(process_exists(daemon_pid));

    let tracked = wait_for_sandbox_status(&fixture, &sandbox_id, |status| {
        status["payload_descendant_pids"]
            .as_array()
            .is_some_and(|pids| pids.iter().any(|pid| pid == daemon_pid))
    });
    assert_eq!(tracked["status"], "sandbox_running");

    let stopped = success_json(forge().arg("--store").arg(&fixture.store).args([
        "worktree",
        "sandbox",
        "stop",
        SANDBOX_SELECTOR_FLAG,
        &sandbox_id,
        "--allow-stop",
        "--output",
        "json",
    ]));
    assert_eq!(stopped["status"], "sandbox_stopped");
    assert_eq!(stopped["receipt_status"], "sandbox_stopped");
    assert!(stopped["error"].is_null());
    cleanup.disarm();

    assert_process_stopped(daemon_pid);
    assert_eq!(
        sandbox_status(&fixture, &sandbox_id)["status"],
        "sandbox_stopped"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn status_reconciles_a_sigkilled_supervisor_and_kills_payload_descendants() {
    let fixture = LifecycleFixture::new();
    fixture.allow_command_and_timeout("sh", 30);
    fixture.approve_config();
    fixture.bind();

    let child_path = fixture
        .repository
        .join(".forge/sandboxes/internal/artifacts/supervisor-loss-child.pid");
    let command = format!(
        "sleep 30 & child=$!; printf \"%s\" \"$child\" > \"{}\"; wait \"$child\"",
        child_path.display()
    );
    let started = success_json(&mut fixture.sandbox_start(true, &["sh", "-c", &command]));
    assert_eq!(started["status"], "sandbox_running");
    let sandbox_id = started[SANDBOX_ID_FIELD].as_str().unwrap().to_string();
    let mut cleanup = PreviewCleanup::new(&fixture.store, &sandbox_id);
    let child_pid = wait_for_file(&child_path).parse::<u32>().unwrap();

    let tracked = wait_for_sandbox_status(&fixture, &sandbox_id, |status| {
        status["payload_descendant_pids"]
            .as_array()
            .is_some_and(|pids| pids.iter().any(|pid| pid == child_pid))
    });
    let supervisor_pid = tracked["supervisor_pid"].as_u64().unwrap() as u32;
    let payload_pid = tracked["payload_pid"].as_u64().unwrap() as u32;
    assert!(process_exists(supervisor_pid));
    assert!(process_exists(payload_pid));
    assert!(process_exists(child_pid));

    let killed = ProcessCommand::new("kill")
        .args(["-KILL", &supervisor_pid.to_string()])
        .status()
        .expect("SIGKILL should be sent to the sandbox supervisor");
    assert!(killed.success());

    let reconciled = wait_for_sandbox_status(&fixture, &sandbox_id, |status| {
        status["status"] == "sandbox_execution_failed"
    });
    assert_eq!(reconciled["status"], "sandbox_execution_failed");
    assert!(reconciled["finished_at"].as_str().is_some());
    assert!(reconciled["error"]
        .as_str()
        .is_some_and(|error| error.contains("supervisor")));
    cleanup.disarm();

    assert_process_stopped(supervisor_pid);
    assert_process_stopped(payload_pid);
    assert_process_stopped(child_pid);
}

#[test]
fn mcp_sandbox_plan_run_start_status_and_stop_match_the_cli_contracts() {
    let fixture = LifecycleFixture::new();
    fixture.allow_command_and_timeout("sh", 5);
    fixture.approve_config();
    fixture.bind();

    let run_command = ["sh", "-c", "printf mcp-parity"];
    let common_input = serde_json::json!({
        "worktree": fixture.worktree_id,
        "purpose": "preview",
        "workflow_id": fixture.workflow_id,
        "task_id": fixture.task_id,
        "command": run_command,
    });

    let cli_plan = success_json(&mut fixture.sandbox_plan(&run_command));
    let mcp_plan_envelope = fixture.mcp_call("forge.worktree.sandbox.plan", common_input.clone());
    let mcp_plan = &mcp_plan_envelope["result"];
    for field in [
        "schema_version",
        "status",
        "allowed",
        "worktree_id",
        "purpose",
        "runtime",
        "command",
        "command_sha256",
        "config_sha256",
    ] {
        assert_eq!(mcp_plan[field], cli_plan[field], "plan field {field}");
    }

    let mut run_input = common_input.clone();
    run_input["allow_exec"] = serde_json::json!(true);
    let mcp_run_envelope = fixture.mcp_call("forge.worktree.sandbox.run", run_input);
    let mcp_run = &mcp_run_envelope["result"];
    let cli_run = success_json(&mut fixture.sandbox_run(&run_command));
    for field in [
        "schema_version",
        "status",
        "allowed",
        "execution_attempted",
        "executed",
        "worktree_id",
        "purpose",
        "runtime",
        "timed_out",
        "exit_code",
        "command_sha256",
        "config_sha256",
    ] {
        assert_eq!(mcp_run[field], cli_run[field], "run field {field}");
    }
    assert_eq!(mcp_run["stdout"]["content"], "mcp-parity");
    assert_eq!(mcp_run["stderr"]["content"], "");

    let started = fixture.mcp_call(
        "forge.worktree.sandbox.start",
        serde_json::json!({
            "worktree": fixture.worktree_id,
            "purpose": "preview",
            "workflow_id": fixture.workflow_id,
            "task_id": fixture.task_id,
            "command": ["sh", "-c", "sleep 30"],
            "allow_exec": true,
        }),
    );
    let started = &started["result"];
    assert_eq!(started["schema_version"], LIFECYCLE_SCHEMA_VERSION);
    assert_eq!(started["status"], "sandbox_running");
    let sandbox_id = started[SANDBOX_ID_FIELD]
        .as_str()
        .expect("MCP start should return the canonical sandbox_id")
        .to_string();
    let mut cleanup = PreviewCleanup::new(&fixture.store, &sandbox_id);

    let status = fixture.mcp_call(
        "forge.worktree.sandbox.status",
        serde_json::json!({"sandbox_id": sandbox_id}),
    );
    let status = &status["result"];
    assert_eq!(status["schema_version"], LIFECYCLE_SCHEMA_VERSION);
    assert_eq!(status[SANDBOX_ID_FIELD], sandbox_id);
    assert_eq!(status["status"], "sandbox_running");

    let cli_status = success_json(forge().arg("--store").arg(&fixture.store).args([
        "worktree",
        "sandbox",
        "status",
        SANDBOX_SELECTOR_FLAG,
        &sandbox_id,
        "--output",
        "json",
    ]));
    for field in [
        "schema_version",
        SANDBOX_ID_FIELD,
        "status",
        "worktree_id",
        "purpose",
        "workflow_id",
        "task_id",
        "command_sha256",
        "config_sha256",
    ] {
        assert_eq!(status[field], cli_status[field], "status field {field}");
    }

    let stopped = fixture.mcp_call(
        "forge.worktree.sandbox.stop",
        serde_json::json!({"sandbox_id": sandbox_id, "allow_stop": true}),
    );
    let stopped = &stopped["result"];
    assert_eq!(stopped["schema_version"], LIFECYCLE_SCHEMA_VERSION);
    assert_eq!(stopped[SANDBOX_ID_FIELD], sandbox_id);
    assert_eq!(stopped["status"], "sandbox_stopped");
    assert_eq!(stopped["receipt_status"], "sandbox_stopped");
    assert!(stopped["error"].is_null());
    cleanup.disarm();

    let final_status = fixture.mcp_call(
        "forge.worktree.sandbox.status",
        serde_json::json!({"sandbox_id": sandbox_id}),
    );
    assert_eq!(final_status["result"]["status"], "sandbox_stopped");
}

#[test]
fn mcp_rejects_task_scoped_sandboxes_without_a_workflow_id() {
    let fixture = LifecycleFixture::new();
    for tool in [
        "forge.worktree.sandbox.plan",
        "forge.worktree.sandbox.run",
        "forge.worktree.sandbox.start",
    ] {
        let mut input = serde_json::json!({
            "worktree": fixture.worktree_id,
            "purpose": "preview",
            "task_id": fixture.task_id,
            "command": ["sh", "-c", "exit 0"],
        });
        if tool != "forge.worktree.sandbox.plan" {
            input["allow_exec"] = serde_json::json!(true);
        }
        let output = command_output(&mut fixture.mcp_command(tool, input));
        assert!(
            !output.status.success(),
            "MCP tool {tool} accepted task_id without workflow_id: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("a task-scoped sandbox requires a workflow id"),
            "MCP tool {tool} returned unexpected stderr: {stderr}"
        );
    }

    let store = ForgeStore::open(&fixture.store).unwrap();
    for workflow_id in [&fixture.workflow_id, "_system"] {
        assert!(!store
            .load_workflow_events(workflow_id)
            .unwrap()
            .iter()
            .any(|event| event.kind == "worktree_sandbox_execution"));
    }
}

#[test]
fn inline_command_secrets_are_blocked_redacted_and_never_persisted_raw() {
    let fixture = LifecycleFixture::new();
    fixture.allow_command_and_timeout("sh", 5);
    fixture.approve_config();
    fixture.bind();

    let secret = format!("sk-proj-{}", "a".repeat(48));
    let command = format!("printf '%s' '{secret}'");
    let envelope = fixture.mcp_call(
        "forge.worktree.sandbox.run",
        serde_json::json!({
            "worktree": fixture.worktree_id,
            "purpose": "preview",
            "workflow_id": fixture.workflow_id,
            "task_id": fixture.task_id,
            "command": ["sh", "-c", command],
            "allow_exec": true,
        }),
    );
    let receipt = &envelope["result"];
    assert_eq!(receipt["status"], "blocked_by_worktree_guardrails");
    assert_eq!(receipt["allowed"], false);
    assert_eq!(receipt["execution_attempted"], false);
    assert_eq!(receipt["executed"], false);
    assert!(receipt["plan"]["guardrail_decisions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|decision| {
            decision["id"] == "command_secret_free" && decision["decision"] == "blocked"
        }));
    assert!(!envelope.to_string().contains(&secret));
    assert!(receipt["plan"]["command"]
        .to_string()
        .contains("{{vault:sandbox_command.openai.default}}"));

    let event = latest_sandbox_event(&fixture);
    assert_eq!(event["status"], "blocked_by_worktree_guardrails");
    assert_eq!(event["execution_attempted"], false);
    assert!(!event.to_string().contains(&secret));
    assert_workflow_events_do_not_contain(&fixture, &secret);
    assert_store_files_do_not_contain(&fixture.store, &secret);
}

#[test]
fn stdout_and_stderr_secrets_are_redacted_in_json_and_persisted_receipts() {
    let fixture = LifecycleFixture::new();
    fixture.allow_command_and_timeout("sh", 5);
    fixture.approve_config();
    fixture.bind();

    let stdout_secret = format!("sk-proj-{}", "0".repeat(48));
    let stderr_secret = format!("sk-proj-{}1", "0".repeat(47));
    let envelope = fixture.mcp_call(
        "forge.worktree.sandbox.run",
        serde_json::json!({
            "worktree": fixture.worktree_id,
            "purpose": "preview",
            "workflow_id": fixture.workflow_id,
            "task_id": fixture.task_id,
            "command": [
                "sh",
                "-c",
                "printf 'sk-proj-'; printf '%048d' 0; printf 'sk-proj-' >&2; printf '%048d' 1 >&2"
            ],
            "allow_exec": true,
        }),
    );
    let receipt = &envelope["result"];
    assert_eq!(receipt["status"], "sandbox_completed");
    assert!(receipt["stdout"]["redaction_count"].as_u64().unwrap() > 0);
    assert!(receipt["stderr"]["redaction_count"].as_u64().unwrap() > 0);
    assert!(!receipt["stdout"]["content"]
        .as_str()
        .unwrap()
        .contains(&stdout_secret));
    assert!(!receipt["stderr"]["content"]
        .as_str()
        .unwrap()
        .contains(&stderr_secret));
    assert!(!envelope.to_string().contains(&stdout_secret));
    assert!(!envelope.to_string().contains(&stderr_secret));

    let event = latest_sandbox_event(&fixture);
    assert_eq!(event["status"], "sandbox_completed");
    assert!(event["stdout"]["redaction_count"].as_u64().unwrap() > 0);
    assert!(event["stderr"]["redaction_count"].as_u64().unwrap() > 0);
    assert!(!event.to_string().contains(&stdout_secret));
    assert!(!event.to_string().contains(&stderr_secret));
    assert_workflow_events_do_not_contain(&fixture, &stdout_secret);
    assert_workflow_events_do_not_contain(&fixture, &stderr_secret);
    assert_store_files_do_not_contain(&fixture.store, &stdout_secret);
    assert_store_files_do_not_contain(&fixture.store, &stderr_secret);
}

#[test]
fn generic_high_entropy_secrets_are_blocked_in_argv_and_redacted_in_output() {
    let secret = ["N7vQ9xL4", "pR2sT8wY", "6zA3bC5d", "E1fG0hJ"].concat();

    let argv_fixture = LifecycleFixture::new();
    argv_fixture.allow_command_and_timeout("sh", 5);
    argv_fixture.approve_config();
    argv_fixture.bind();
    let argv_envelope = argv_fixture.mcp_call(
        "forge.worktree.sandbox.run",
        serde_json::json!({
            "worktree": argv_fixture.worktree_id,
            "purpose": "preview",
            "workflow_id": argv_fixture.workflow_id,
            "task_id": argv_fixture.task_id,
            "command": ["sh", "-c", format!("opaque={secret}")],
            "allow_exec": true,
        }),
    );
    let argv_receipt = &argv_envelope["result"];
    assert_eq!(argv_receipt["status"], "blocked_by_worktree_guardrails");
    assert_eq!(argv_receipt["execution_attempted"], false);
    assert_eq!(argv_receipt["executed"], false);
    assert!(argv_receipt["plan"]["guardrail_decisions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|decision| {
            decision["id"] == "command_secret_free" && decision["decision"] == "blocked"
        }));
    assert!(argv_receipt["plan"]["command"]
        .to_string()
        .contains("{{vault:sandbox_command.entropy.default}}"));
    assert!(!argv_envelope.to_string().contains(&secret));
    assert!(!latest_sandbox_event(&argv_fixture)
        .to_string()
        .contains(&secret));
    assert_workflow_events_do_not_contain(&argv_fixture, &secret);
    assert_store_files_do_not_contain(&argv_fixture.store, &secret);

    let output_fixture = LifecycleFixture::new();
    output_fixture.allow_command_and_timeout("sh", 5);
    output_fixture.approve_config();
    output_fixture.bind();
    let output_envelope = output_fixture.mcp_call(
        "forge.worktree.sandbox.run",
        serde_json::json!({
            "worktree": output_fixture.worktree_id,
            "purpose": "preview",
            "workflow_id": output_fixture.workflow_id,
            "task_id": output_fixture.task_id,
            "command": [
                "sh",
                "-c",
                "printf 'N7vQ9xL4'; printf 'pR2sT8wY'; printf '6zA3bC5d'; printf 'E1fG0hJ'; printf 'N7vQ9xL4' >&2; printf 'pR2sT8wY' >&2; printf '6zA3bC5d' >&2; printf 'E1fG0hJ' >&2"
            ],
            "allow_exec": true,
        }),
    );
    let output_receipt = &output_envelope["result"];
    assert_eq!(output_receipt["status"], "sandbox_completed");
    for stream in ["stdout", "stderr"] {
        assert!(
            output_receipt[stream]["redaction_count"].as_u64().unwrap() > 0,
            "stream {stream}: {output_receipt}"
        );
        assert_eq!(
            output_receipt[stream]["content"],
            "{{vault:sandbox_output.entropy.default}}"
        );
    }
    assert!(!output_envelope.to_string().contains(&secret));

    let output_event = latest_sandbox_event(&output_fixture);
    for stream in ["stdout", "stderr"] {
        assert!(output_event[stream]["redaction_count"].as_u64().unwrap() > 0);
        assert_eq!(
            output_event[stream]["content"],
            "{{vault:sandbox_output.entropy.default}}"
        );
    }
    assert!(!output_event.to_string().contains(&secret));
    assert_workflow_events_do_not_contain(&output_fixture, &secret);
    assert_store_files_do_not_contain(&output_fixture.store, &secret);
}
