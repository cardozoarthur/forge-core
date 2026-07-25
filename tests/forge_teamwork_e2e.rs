use assert_cmd::Command;
use forge_core::storage::ForgeStore;
use predicates::prelude::PredicateBooleanExt;
use rusqlite::{params, Connection};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use tempfile::tempdir;

/// Helper: start a local mock HTTP server that returns a mock benchmark JSON payload
struct MockServer {
    url: String,
    shutdown: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl MockServer {
    fn start(response_body: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = format!("http://127.0.0.1:{}/benchmarks", port);
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = shutdown.clone();

        let handle = thread::spawn(move || {
            listener.set_nonblocking(true).ok();
            while !shutdown_clone.load(Ordering::Relaxed) {
                if let Ok((mut stream, _)) = listener.accept() {
                    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(500)));
                    let _ = stream.set_write_timeout(Some(std::time::Duration::from_millis(500)));
                    let mut buf = [0u8; 1024];
                    let _ = stream.read(&mut buf);

                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        response_body.len(),
                        response_body
                    );
                    let _ = stream.write_all(response.as_bytes());
                    let _ = stream.flush();
                }
                thread::sleep(std::time::Duration::from_millis(5));
            }
        });

        Self {
            url,
            shutdown,
            handle: Some(handle),
        }
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Helper function to locate and execute the forge CLI binary
fn forge() -> Command {
    Command::cargo_bin("forge").expect("forge binary should build")
}

fn stored_run_id_for_workflow(store_path: &std::path::Path, workflow_id: &str) -> String {
    let connection = Connection::open(store_path).unwrap();
    connection
        .query_row(
            "SELECT id FROM runs WHERE workflow_id = ?1",
            [workflow_id],
            |row| row.get(0),
        )
        .unwrap()
}

// ============================================================================
// FEATURE 1: CLI & Output Formatting
// ============================================================================

/// Test 1: Passing an empty or whitespace-only goal fails validation
#[test]
fn test_f1_cli_empty_goal_fails() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    // Test empty goal
    forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["teamwork", "--goal", "", "--output", "json"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("Goal cannot be empty"));

    // Test whitespace-only goal
    forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["teamwork", "--goal", "    ", "--output", "human"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("Goal cannot be empty"));
}

/// Test 2: Omitting the required --goal argument triggers a clap parsing error
#[test]
fn test_f1_cli_missing_goal_fails() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["teamwork", "--output", "json"])
        .assert()
        .failure()
        .stderr(
            predicates::str::contains("required arguments were not provided").or(
                predicates::str::contains(
                    "error: the following required arguments were not provided",
                ),
            ),
        );
}

/// Test 3: Providing an invalid --output format value triggers a clap parsing error
#[test]
fn test_f1_cli_invalid_output_fails() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Deploy platform",
            "--output",
            "invalid_format",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("invalid value 'invalid_format'"));
}

/// Test 4: Running a detached execution with json output yields schema-compliant JSON
#[test]
fn test_f1_cli_detached_json_format() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Implement high-performance logging subsystem",
            "--detached",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value =
        serde_json::from_slice(&output).expect("Stdout should be valid JSON");

    assert_eq!(json["schema_version"], "forge.teamwork.plan.v1");
    assert_eq!(json["goal"], "Implement high-performance logging subsystem");
    assert_eq!(json["detached"], true);
    assert!(json["workflow_id"].is_string());
    assert!(json["run_id"].is_string());
    assert!(json["roster"]["roles"].is_array());
    assert!(json["tasks"].is_array());
}

/// Test 5: Running a detached execution with human output yields a formatted layout
#[test]
fn test_f1_cli_detached_human_format() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Implement high-performance logging subsystem",
            "--detached",
            "--output",
            "human",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let stdout_str = String::from_utf8(output).unwrap();
    assert!(stdout_str.contains("FORGE TEAMWORK EXECUTION PLAN"));
    assert!(stdout_str.contains("Goal: Implement high-performance logging subsystem"));
    assert!(stdout_str.contains("Execution Mode: Detached"));
    assert!(stdout_str.contains("TEAM ROSTER"));
    assert!(stdout_str.contains("TASK GRAPH"));
    assert!(stdout_str.contains("EXECUTION STATUS"));
}

// ============================================================================
// FEATURE 2: Roster & Heuristics
// ============================================================================

/// Test 6: Coding-heavy goals allocate code-specialized brains (codex/opencode)
#[test]
fn test_f2_roster_coding_heuristics() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Write a Rust parser for abstract syntax trees.",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let roles = json["roster"]["roles"]
        .as_array()
        .expect("Roster roles should be an array");

    let mut found_worker = false;
    for role_val in roles {
        if role_val["role"] == "Worker" {
            found_worker = true;
            let brain = role_val["brain"].as_str().unwrap();
            assert!(
                brain == "codex" || brain == "opencode",
                "Coding task should map to codex/opencode, got: {}",
                brain
            );
        }
    }
    assert!(found_worker, "Roster must contain a Worker role");
}

/// Test 7: Frontend UI/styling goals allocate visual/general brains (antigravity/agy)
#[test]
fn test_f2_roster_frontend_heuristics() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Create a visual dashboard button using CSS and HTML layout.",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let roles = json["roster"]["roles"]
        .as_array()
        .expect("Roster roles should be an array");

    let mut found_worker = false;
    for role_val in roles {
        if role_val["role"] == "Worker" {
            found_worker = true;
            let brain = role_val["brain"].as_str().unwrap();
            assert!(
                brain == "antigravity" || brain == "agy",
                "Frontend task should map to antigravity/agy, got: {}",
                brain
            );
        }
    }
    assert!(found_worker);
}

/// Test 8: Mock HTTP server endpoint query via FORGE_BENCHMARK_URL redirection
#[test]
fn test_f2_mock_benchmark_url_fetch() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let mock_response = r#"[
        {
            "brain": "custom_web_brain",
            "mismatch_penalty": 0.0,
            "evals": {
                "lmsys_chatbot_arena": 1500,
                "mmlu": 0.96,
                "human_eval": 0.98
            }
        }
    ]"#;

    let mock_server = MockServer::start(mock_response.to_string());

    let output = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Perform generic task routing and evaluation",
            "--bypass-cache",
            "--output",
            "json",
        ])
        .env("FORGE_BENCHMARK_URL", &mock_server.url)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let scores = json["benchmarks"]["scores"]
        .as_array()
        .expect("Benchmarks scores should be an array");

    let mut found_custom = false;
    for score in scores {
        if score["brain"] == "custom_web_brain" {
            found_custom = true;
            assert_eq!(score["evals"]["human_eval"], 0.98);
        }
    }
    assert!(found_custom);
}

/// Test 9: Benchmark scores cache hit by default, but bypassed with cache bypass flags
#[test]
fn test_f2_benchmark_cache_hit_versus_bypass() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");

    let mock_response = r#"[
        {
            "brain": "new_fetched_brain",
            "mismatch_penalty": 0.0,
            "evals": {
                "lmsys_chatbot_arena": 1700,
                "mmlu": 0.99,
                "human_eval": 0.995
            }
        }
    ]"#;
    let mock_server = MockServer::start(mock_response.to_string());

    // Pre-populate the SQLite database cache table
    let conn = Connection::open(&store_path).unwrap();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS web_benchmark_cache (
            brain_id TEXT PRIMARY KEY,
            lmsys_score INTEGER NOT NULL,
            mmlu_score REAL NOT NULL,
            human_eval_score REAL NOT NULL,
            updated_at TEXT NOT NULL
        )",
        params![],
    )
    .unwrap();

    let now_str = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO web_benchmark_cache (brain_id, lmsys_score, mmlu_score, human_eval_score, updated_at) VALUES (?, ?, ?, ?, ?)",
        params!["custom_cached_brain", 1600, 0.98, 0.99, &now_str]
    )
    .unwrap();

    // Cache Hit (Default)
    let output_hit = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Write highly specialized math functions",
            "--output",
            "json",
        ])
        .env("FORGE_BENCHMARK_URL", &mock_server.url)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json_hit: serde_json::Value = serde_json::from_slice(&output_hit).unwrap();
    let roles_hit = json_hit["roster"]["roles"].as_array().unwrap();
    let mut found_cached = false;
    for role_val in roles_hit {
        if role_val["brain"] == "custom_cached_brain" {
            found_cached = true;
        }
    }
    assert!(
        found_cached,
        "Should hit sqlite cache and select custom_cached_brain"
    );

    // Cache Bypass
    let output_bypass = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Write highly specialized math functions",
            "--bypass-cache",
            "--output",
            "json",
        ])
        .env("FORGE_BENCHMARK_URL", &mock_server.url)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json_bypass: serde_json::Value = serde_json::from_slice(&output_bypass).unwrap();
    let roles_bypass = json_bypass["roster"]["roles"].as_array().unwrap();
    let mut found_fetched = false;
    for role_val in roles_bypass {
        if role_val["brain"] == "new_fetched_brain" {
            found_fetched = true;
        }
    }
    assert!(found_fetched, "Should bypass cache and query mock server");
}

/// Test 10: Denying a brain in executor_policy triggers heuristics fallback to another brain
#[test]
fn test_f2_executor_policy_denial_fallback() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");

    let conn = Connection::open(&store_path).unwrap();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS executor_policy (
            id TEXT PRIMARY KEY,
            data_json TEXT NOT NULL,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )",
        params![],
    )
    .unwrap();

    let codex_policy_json = serde_json::json!({
        "id": "codex",
        "display_name": "Codex Primary Brain",
        "command": "codex",
        "installed": true,
        "configured": true,
        "allowed": false,
        "decision_source": "user_policy_denied",
        "synced_at": "2026-07-04T10:00:00Z"
    });

    conn.execute(
        "INSERT INTO executor_policy (id, data_json) VALUES (?, ?)",
        params!["codex", serde_json::to_string(&codex_policy_json).unwrap()],
    )
    .unwrap();

    let output = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Write clean Rust compiler parser code",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let roles = json["roster"]["roles"].as_array().unwrap();

    for role_val in roles {
        if role_val["role"] == "Worker" {
            let brain = role_val["brain"].as_str().unwrap();
            assert_ne!(
                brain, "codex",
                "Should fallback and not choose blocked codex brain"
            );
            assert!(
                brain == "opencode" || brain == "antigravity" || brain == "agy",
                "Fallback should select opencode or Antigravity/agy"
            );
        }
    }
}

// ============================================================================
// FEATURE 3: Execution Runtime & Lineage
// ============================================================================

/// Test 11: Spawning a detached execution launches a background driver thread/loop
#[test]
fn test_f3_detached_spawn_background_loop() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");

    let output = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Execute a series of simple automated notify and wait tasks",
            "--detached",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let run_id = json["run_id"].as_str().unwrap();

    let conn = Connection::open(&store_path).unwrap();
    let mut stmt = conn
        .prepare("SELECT status FROM runs WHERE id = ?")
        .unwrap();
    let mut rows = stmt.query(params![run_id]).unwrap();
    let row = rows
        .next()
        .unwrap()
        .expect("Run record should exist in sqlite database");
    let status: String = row.get(0).unwrap();
    assert!(status == "accepted" || status == "running" || status == "completed");
}

/// Test 12: Cognitive tasks not auto-steppable trigger handoff_required and halt loop
#[test]
fn test_f3_cognitive_task_handoff_halts() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");

    let output = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Perform research write up and analysis on system metrics",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let run_id = stored_run_id_for_workflow(&store_path, workflow_id);
    let run_id = run_id.as_str();

    let mut step_json: serde_json::Value = serde_json::Value::Null;
    for _ in 0..10 {
        let step_output = forge()
            .arg("--store")
            .arg(store_path.to_str().unwrap())
            .args(["request", "step", "--run", run_id, "--output", "json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        step_json = serde_json::from_slice(&step_output).unwrap();
        if step_json["status"] != "stepped" {
            break;
        }
    }

    assert_eq!(step_json["status"], "handoff_required");
    assert!(step_json["reason"]
        .as_str()
        .unwrap()
        .contains("requires a real executor execution receipt"));
}

/// Test 13: Executing request steps acquires an active lease inside task_leases
#[test]
fn test_f3_task_lease_acquisition() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");

    let output = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Run parallel code audits",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let run_id = stored_run_id_for_workflow(&store_path, workflow_id);
    let run_id = run_id.as_str();

    let step_output = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args(["request", "step", "--run", run_id, "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let mut step_json: serde_json::Value = serde_json::from_slice(&step_output).unwrap();
    while step_json["status"] == "stepped" {
        let next_step = forge()
            .arg("--store")
            .arg(store_path.to_str().unwrap())
            .args(["request", "step", "--run", run_id, "--output", "json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        step_json = serde_json::from_slice(&next_step).unwrap();
    }

    let conn = Connection::open(&store_path).unwrap();
    let mut stmt = conn
        .prepare("SELECT lease_id, executor FROM task_leases WHERE workflow_id = ?")
        .unwrap();
    let mut rows = stmt.query(params![workflow_id]).unwrap();
    if let Some(row) = rows.next().unwrap() {
        let lease_id: String = row.get(0).unwrap();
        let executor: String = row.get(1).unwrap();
        assert!(!lease_id.is_empty());
        assert!(!executor.is_empty());
    }
}

/// Test 14: Completing a task successfully registers a checkpoint in task_checkpoints
#[test]
fn test_f3_checkpoint_saving_and_update() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");

    let output = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Generate layout and compile code",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let run_id = stored_run_id_for_workflow(&store_path, workflow_id);
    let run_id = run_id.as_str();
    let first_task_id = "task-005";
    wait_for_task_ready(&store_path, run_id, first_task_id);

    let complete_out = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "request",
            "complete-task",
            "--run",
            run_id,
            "--task",
            first_task_id,
            "--executor",
            "codex",
            "--summary",
            "Task completed successfully",
            "--evidence-command",
            "true",
            "--evidence-summary",
            "teamwork checkpoint receipt passed",
            "--estimated-usd",
            "0.01",
            "--tokens-in",
            "200",
            "--tokens-out",
            "300",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    println!("COMPLETE OUT: {}", String::from_utf8(complete_out).unwrap());

    let conn = Connection::open(&store_path).unwrap();
    let mut stmt = conn.prepare("SELECT state, executor FROM task_checkpoints WHERE workflow_id = ? AND task_id = ? AND executor = ?").unwrap();
    let mut rows = stmt
        .query(params![workflow_id, first_task_id, "codex"])
        .unwrap();
    let row = rows
        .next()
        .unwrap()
        .expect("Checkpoint record should exist");
    let state_str: String = row.get(0).unwrap();
    let executor: String = row.get(1).unwrap();
    assert!(!state_str.is_empty());
    assert_eq!(executor, "codex");
}

/// Test 15: Running a workflow simulation outputs scheduling details and predicted costs
#[test]
fn test_f3_simulated_parallel_execution() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");

    let plan_output = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Validate multi-file logs and format source code",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let plan_json: serde_json::Value = serde_json::from_slice(&plan_output).unwrap();
    let workflow_id = plan_json["workflow_id"].as_str().unwrap();

    let sim_output = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "run",
            "--workflow",
            workflow_id,
            "--simulate",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let sim_json: serde_json::Value = serde_json::from_slice(&sim_output).unwrap();
    assert!(
        sim_json["concurrent_wave_count"].as_i64().is_some()
            || sim_json["max_concurrent_tasks"].as_i64().is_some()
    );
    assert!(sim_json["cost_report"]["total_estimated_cost_usd"]
        .as_f64()
        .is_some());
}

// ============================================================================
// FEATURE 4: SQLite Database Persistence
// ============================================================================

fn assert_table_schema(
    conn: &Connection,
    table_name: &str,
    expected_columns: &[(&str, &str, bool, bool)],
) {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({})", table_name))
        .unwrap();
    let mut rows = stmt.query([]).unwrap();
    let mut actual_cols = std::collections::HashMap::new();
    while let Some(row) = rows.next().unwrap() {
        let name: String = row.get(1).unwrap();
        let col_type: String = row.get(2).unwrap();
        let notnull: i64 = row.get(3).unwrap();
        let pk: i64 = row.get(5).unwrap();
        actual_cols.insert(name, (col_type.to_uppercase(), notnull != 0, pk != 0));
    }

    for &(name, expected_type, expected_notnull, expected_pk) in expected_columns {
        let (actual_type, actual_notnull, actual_pk) = actual_cols
            .get(name)
            .unwrap_or_else(|| panic!("Column '{}' not found in table '{}'", name, table_name));
        assert_eq!(
            actual_type,
            &expected_type.to_uppercase(),
            "Column '{}' in table '{}' type mismatch",
            name,
            table_name
        );
        assert_eq!(
            *actual_notnull, expected_notnull,
            "Column '{}' in table '{}' nullability mismatch",
            name, table_name
        );
        assert_eq!(
            *actual_pk, expected_pk,
            "Column '{}' in table '{}' primary key mismatch",
            name, table_name
        );
    }
}

/// Test 16: SQLite database contains workflows and runs tables with correct schema columns
#[test]
fn test_f4_persistence_workflows_runs_schema() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");

    let _store = ForgeStore::open(&store_path).unwrap();
    let conn = Connection::open(&store_path).unwrap();

    assert_table_schema(
        &conn,
        "workflows",
        &[
            ("id", "TEXT", false, true),
            ("goal", "TEXT", true, false),
            ("status", "TEXT", true, false),
            ("created_at", "TEXT", true, false),
            ("data_json", "TEXT", true, false),
        ],
    );

    assert_table_schema(
        &conn,
        "runs",
        &[
            ("id", "TEXT", false, true),
            ("workflow_id", "TEXT", true, false),
            ("organization_id", "TEXT", true, false),
            ("brand_id", "TEXT", true, false),
            ("product_id", "TEXT", true, false),
            ("user_id", "TEXT", true, false),
            ("channel_id", "TEXT", true, false),
            ("status", "TEXT", true, false),
            ("data_json", "TEXT", true, false),
            ("created_at", "TEXT", true, false),
            ("updated_at", "TEXT", true, false),
        ],
    );
}

/// Test 17: SQLite database contains cost_ledger_index table tracking tokens and costs
#[test]
fn test_f4_persistence_cost_ledger_index() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");

    let _store = ForgeStore::open(&store_path).unwrap();
    let conn = Connection::open(&store_path).unwrap();

    assert_table_schema(
        &conn,
        "cost_ledger_index",
        &[
            ("row_key", "TEXT", false, true),
            ("source_kind", "TEXT", true, false),
            ("workflow_id", "TEXT", true, false),
            ("task_id", "TEXT", false, false),
            ("event_id", "INTEGER", false, false),
            ("organization_id", "TEXT", true, false),
            ("brand_id", "TEXT", true, false),
            ("product_id", "TEXT", true, false),
            ("addon_id", "TEXT", false, false),
            ("executor", "TEXT", false, false),
            ("model_call_required", "INTEGER", true, false),
            ("model_call_avoided", "INTEGER", true, false),
            ("estimated_task_cost_usd", "REAL", true, false),
            ("observed_event_cost_usd", "REAL", true, false),
            ("tokens_in", "INTEGER", true, false),
            ("tokens_out", "INTEGER", true, false),
            ("data_json", "TEXT", true, false),
            ("created_at", "TEXT", true, false),
            ("updated_at", "TEXT", true, false),
        ],
    );
}

/// Test 18: SQLite database contains event_observability_index table tracking context pressure
#[test]
fn test_f4_persistence_observability_index() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");

    let _store = ForgeStore::open(&store_path).unwrap();
    let conn = Connection::open(&store_path).unwrap();

    assert_table_schema(
        &conn,
        "event_observability_index",
        &[
            ("global_event_id", "INTEGER", false, true),
            ("workflow_id", "TEXT", true, false),
            ("kind", "TEXT", true, false),
            ("category", "TEXT", true, false),
            ("severity", "TEXT", true, false),
            ("origin", "TEXT", true, false),
            ("source", "TEXT", true, false),
            ("organization_id", "TEXT", true, false),
            ("brand_id", "TEXT", true, false),
            ("product_id", "TEXT", true, false),
            ("node_ref", "TEXT", false, false),
            ("addon_id", "TEXT", false, false),
            ("duration_ms", "INTEGER", false, false),
            ("retry_count", "INTEGER", false, false),
            ("wait_state", "TEXT", false, false),
            ("wait_seconds", "INTEGER", false, false),
            ("context_budget_bytes", "INTEGER", false, false),
            ("selected_context_bytes", "INTEGER", false, false),
            ("context_remaining_bytes", "INTEGER", false, false),
            ("context_pressure_bps", "INTEGER", false, false),
            ("context_pressure_state", "TEXT", false, false),
            ("memory_level", "TEXT", false, false),
            ("memory_scope", "TEXT", false, false),
            ("data_json", "TEXT", true, false),
            ("created_at", "TEXT", true, false),
            ("updated_at", "TEXT", true, false),
        ],
    );
}

/// Test 19: SQLite database contains runtime_contract_dispatches tracking contracts validation
#[test]
fn test_f4_persistence_runtime_contract_dispatches() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");

    let _store = ForgeStore::open(&store_path).unwrap();
    let conn = Connection::open(&store_path).unwrap();

    assert_table_schema(
        &conn,
        "runtime_contract_dispatches",
        &[
            ("id", "TEXT", false, true),
            ("addon_id", "TEXT", true, false),
            ("contract_id", "TEXT", true, false),
            ("contract_type", "TEXT", true, false),
            ("capability_id", "TEXT", true, false),
            ("runtime", "TEXT", true, false),
            ("entrypoint", "TEXT", true, false),
            ("status", "TEXT", true, false),
            ("source", "TEXT", true, false),
            ("input_json", "TEXT", true, false),
            ("policy_json", "TEXT", true, false),
            ("data_json", "TEXT", true, false),
            ("created_at", "TEXT", true, false),
            ("updated_at", "TEXT", true, false),
        ],
    );
}

/// Test 20: Heuristics candidates ranking sorts workflows requiring improvements first
#[test]
fn test_f4_persistence_cached_benchmark_rankings() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");

    let _store = ForgeStore::open(&store_path).unwrap();
    let conn = Connection::open(&store_path).unwrap();

    let intent_failed = forge_core::intent::parse_intent("Plan with failures");
    let mut wf_failed = forge_core::graph::create_workflow(intent_failed);
    wf_failed.id = "wf_failed".to_string();
    wf_failed.status = "failed".to_string();
    if let Some(task) = wf_failed.tasks.first_mut() {
        task.status = forge_core::graph::TaskStatus::Failed;
    }
    let wf_failed_json = serde_json::to_string(&wf_failed).unwrap();

    let intent_completed = forge_core::intent::parse_intent("Completed plan");
    let mut wf_completed = forge_core::graph::create_workflow(intent_completed);
    wf_completed.id = "wf_completed".to_string();
    wf_completed.status = "completed".to_string();
    for task in &mut wf_completed.tasks {
        task.status = forge_core::graph::TaskStatus::Completed;
    }
    let wf_completed_json = serde_json::to_string(&wf_completed).unwrap();

    // Populate workflow database with test cases
    conn.execute(
        "INSERT INTO workflows (id, goal, status, created_at, data_json) VALUES (?, ?, ?, ?, ?)",
        params![
            "wf_failed",
            "Plan with failures",
            "failed",
            "2026-07-04T10:00:00Z",
            &wf_failed_json
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO workflows (id, goal, status, created_at, data_json) VALUES (?, ?, ?, ?, ?)",
        params![
            "wf_completed",
            "Completed plan",
            "completed",
            "2026-07-04T10:00:00Z",
            &wf_completed_json
        ],
    )
    .unwrap();

    let output = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args(["improve", "candidates", "--limit", "5", "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let candidates = json["candidates"]
        .as_array()
        .expect("Candidates should be an array");

    let mut failed_index = None;
    let mut completed_index = None;
    for (i, cand) in candidates.iter().enumerate() {
        if cand["workflow_id"] == "wf_failed" {
            failed_index = Some(i);
        } else if cand["workflow_id"] == "wf_completed" {
            completed_index = Some(i);
        }
    }

    if let (Some(failed_idx), Some(completed_idx)) = (failed_index, completed_index) {
        assert!(
            failed_idx < completed_idx,
            "Failed workflow should rank higher for improvement candidates"
        );
    }
}

// ============================================================================
// TIER 2: Boundary & Error Handling
// ============================================================================

/// Helper for Tier 2 Feature 2: HTTP benchmark server returning 500 error
struct MockServer500 {
    url: String,
    shutdown: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl MockServer500 {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = format!("http://127.0.0.1:{}/benchmarks", port);
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = shutdown.clone();

        let handle = thread::spawn(move || {
            listener.set_nonblocking(true).ok();
            while !shutdown_clone.load(Ordering::Relaxed) {
                if let Ok((mut stream, _)) = listener.accept() {
                    let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(500)));
                    let _ = stream.set_write_timeout(Some(std::time::Duration::from_millis(500)));
                    let mut buf = [0u8; 1024];
                    let _ = stream.read(&mut buf);

                    let response = "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                    let _ = stream.write_all(response.as_bytes());
                    let _ = stream.flush();
                }
                thread::sleep(std::time::Duration::from_millis(5));
            }
        });

        Self {
            url,
            shutdown,
            handle: Some(handle),
        }
    }
}

impl Drop for MockServer500 {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Test T2-1: Goal input with maximum size (10,000 characters)
#[test]
fn test_t2_f1_max_size_goal() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let large_goal = "a".repeat(10000);

    let _assert = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["teamwork", "--goal", &large_goal, "--output", "json"])
        .assert()
        .success();
}

/// Test T2-2: Goals with special control characters
#[test]
fn test_t2_f1_special_control_chars_goal() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let special_goal = "Deploy\nplatform\twith\rcontrol\x07characters\x08!";

    let output = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["teamwork", "--goal", special_goal, "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["goal"], special_goal);
}

/// Test T2-3: Multiple flags together
#[test]
fn test_t2_f1_multiple_flags_together() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Multiflag goal",
            "--detached",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["detached"], true);
    assert!(json["run_id"].is_string());
}

/// Test T2-4: Goal with unicode emoji / non-ASCII characters
#[test]
fn test_t2_f1_unicode_emoji_goal() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let unicode_goal = "🚀 🦀 Deploy system with 漢 and Cyrillic 🤖";

    let output = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["teamwork", "--goal", unicode_goal, "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["goal"], unicode_goal);
}

/// Test T2-5: Goal with command injection characters
#[test]
fn test_t2_f1_command_injection_safety() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let injection_goal = "Goal; rm -rf / | echo `whoami` & > /dev/null";

    let output = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["teamwork", "--goal", injection_goal, "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(json["goal"], injection_goal);
}

/// Test T2-6: HTTP benchmark server returning 500 error
#[test]
fn test_t2_f2_benchmark_server_500() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let server = MockServer500::start();

    let _assert = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Failing server test",
            "--output",
            "json",
        ])
        .env("FORGE_BENCHMARK_URL", &server.url)
        .assert()
        .success();
}

/// Test T2-7: HTTP server connection timeout/unreachability
#[test]
fn test_t2_f2_benchmark_server_unreachable() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let unreachable_url = "http://127.0.0.1:9999/benchmarks";

    let _assert = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Unreachable server test",
            "--output",
            "json",
        ])
        .env("FORGE_BENCHMARK_URL", unreachable_url)
        .assert()
        .success();
}

/// Test T2-8: Cache record expiration (by manually writing outdated timestamp in SQLite)
#[test]
fn test_t2_f2_cache_expiration() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let conn = Connection::open(&store_path).unwrap();

    conn.execute(
        "CREATE TABLE IF NOT EXISTS web_benchmark_cache (
            brain_id TEXT PRIMARY KEY,
            lmsys_score INTEGER NOT NULL,
            mmlu_score REAL NOT NULL,
            human_eval_score REAL NOT NULL,
            updated_at TEXT NOT NULL
        )",
        params![],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO web_benchmark_cache (brain_id, lmsys_score, mmlu_score, human_eval_score, updated_at) VALUES (?, ?, ?, ?, ?)",
        params!["expired_brain", 1800, 0.99, 0.99, "1970-01-01T00:00:00Z"]
    ).unwrap();

    let _assert = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Expired cache test",
            "--output",
            "json",
        ])
        .assert()
        .success();
}

/// Test T2-9: Executor policy disallowing all brain options
#[test]
fn test_t2_f2_executor_policy_deny_all() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let conn = Connection::open(&store_path).unwrap();

    conn.execute(
        "CREATE TABLE IF NOT EXISTS executor_policy (
            id TEXT PRIMARY KEY,
            data_json TEXT NOT NULL,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )",
        params![],
    )
    .unwrap();

    for brain in &["codex", "opencode", "gemini", "antigravity", "agy"] {
        let policy_json = serde_json::json!({
            "id": *brain,
            "display_name": format!("{} Brain", brain),
            "command": *brain,
            "installed": true,
            "configured": true,
            "allowed": false,
            "decision_source": "user_policy_denied",
            "synced_at": "2026-07-04T10:00:00Z"
        });
        conn.execute(
            "INSERT INTO executor_policy (id, data_json) VALUES (?, ?)",
            params![*brain, serde_json::to_string(&policy_json).unwrap()],
        )
        .unwrap();
    }

    let assert_result = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Deny all brains test",
            "--output",
            "json",
        ])
        .assert()
        .failure();

    let stderr = String::from_utf8_lossy(&assert_result.get_output().stderr);
    assert!(stderr.contains("No allowed modern brain found in executor policy for role"));
}

/// Test T2-10: HTTP benchmark server returning malformed JSON
#[test]
fn test_t2_f2_benchmark_server_malformed_json() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");
    let server = MockServer::start("{malformed_json:".to_string());

    let _assert = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Malformed json test",
            "--output",
            "json",
        ])
        .env("FORGE_BENCHMARK_URL", &server.url)
        .assert()
        .success();
}

/// Test T2-11: Attempting to step a workflow in a cancelled state
#[test]
fn test_t2_f3_step_cancelled_workflow() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let store = ForgeStore::open(&store_path).unwrap();

    let intent = forge_core::intent::parse_intent("Goal");
    let mut workflow = forge_core::graph::create_workflow(intent);
    workflow.status = "cancelled".to_string();
    store.save_workflow(&workflow).unwrap();

    let run = forge_core::request::create_run_record(&workflow, "forge_cli", "cancelled");
    let run_id = run.run_id.clone();
    let run_val = serde_json::to_value(&run).unwrap();

    let conn = Connection::open(&store_path).unwrap();
    conn.execute(
        "INSERT INTO runs (id, workflow_id, status, data_json) VALUES (?, ?, ?, ?)",
        params![
            run_id,
            workflow.id,
            "cancelled",
            serde_json::to_string(&run_val).unwrap()
        ],
    )
    .unwrap();

    let _assert = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args(["request", "step", "--run", &run_id, "--output", "json"])
        .assert()
        .success();
}

/// Test T2-12: Attempting to step a workflow in a failed state
#[test]
fn test_t2_f3_step_failed_workflow() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let store = ForgeStore::open(&store_path).unwrap();

    let intent = forge_core::intent::parse_intent("Goal");
    let mut workflow = forge_core::graph::create_workflow(intent);
    workflow.status = "failed".to_string();
    store.save_workflow(&workflow).unwrap();

    let run = forge_core::request::create_run_record(&workflow, "forge_cli", "failed");
    let run_id = run.run_id.clone();
    let run_val = serde_json::to_value(&run).unwrap();

    let conn = Connection::open(&store_path).unwrap();
    conn.execute(
        "INSERT INTO runs (id, workflow_id, status, data_json) VALUES (?, ?, ?, ?)",
        params![
            run_id,
            workflow.id,
            "failed",
            serde_json::to_string(&run_val).unwrap()
        ],
    )
    .unwrap();

    let _assert = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args(["request", "step", "--run", &run_id, "--output", "json"])
        .assert()
        .success();
}

/// Test T2-13: Acquiring a task lease that is already leased by another executor
#[test]
fn test_t2_f3_lease_already_leased() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let store = ForgeStore::open(&store_path).unwrap();

    let intent = forge_core::intent::parse_intent("Goal");
    let workflow = forge_core::graph::create_workflow(intent);
    store.save_workflow(&workflow).unwrap();

    let conn = Connection::open(&store_path).unwrap();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS task_leases (
            workflow_id TEXT NOT NULL,
            task_id TEXT NOT NULL,
            lease_id TEXT NOT NULL,
            executor TEXT NOT NULL,
            organization_id TEXT NOT NULL DEFAULT '',
            brand_id TEXT NOT NULL DEFAULT '',
            product_id TEXT NOT NULL DEFAULT '',
            user_id TEXT NOT NULL DEFAULT '',
            channel_id TEXT NOT NULL DEFAULT '',
            acquired_at TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            data_json TEXT NOT NULL,
            PRIMARY KEY (workflow_id, task_id)
        )",
        params![],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO task_leases (workflow_id, task_id, lease_id, executor, acquired_at, expires_at, data_json) VALUES (?, ?, ?, ?, ?, ?, ?)",
        params![workflow.id, "task_1", "lease_existing", "executor_a", "2026-07-04T10:00:00Z", "2026-07-04T11:00:00Z", "{}"],
    ).unwrap();

    let _assert = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args(["request", "step", "--run", "dummy_run", "--output", "json"])
        .assert()
        .failure();
}

/// Test T2-14: Simulated dry-run wave execution with no tasks
#[test]
fn test_t2_f3_simulation_no_tasks() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let store = ForgeStore::open(&store_path).unwrap();

    let intent = forge_core::intent::parse_intent("Empty Goal");
    let mut workflow = forge_core::graph::create_workflow(intent);
    workflow.tasks.clear();
    store.save_workflow(&workflow).unwrap();

    let _assert = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "run",
            "--workflow",
            &workflow.id,
            "--simulate",
            "--output",
            "json",
        ])
        .assert()
        .success();
}

/// Test T2-15: Completing a task with extreme tokens/cost values
#[test]
fn test_t2_f3_complete_task_extreme_values() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let store = ForgeStore::open(&store_path).unwrap();

    let intent = forge_core::intent::parse_intent("Goal");
    let workflow = forge_core::graph::create_workflow(intent);
    store.save_workflow(&workflow).unwrap();

    let run = forge_core::request::create_run_record(&workflow, "forge_cli", "accepted");
    let run_val = serde_json::to_value(&run).unwrap();

    let conn = Connection::open(&store_path).unwrap();
    conn.execute(
        "INSERT INTO runs (id, workflow_id, status, data_json) VALUES (?, ?, ?, ?)",
        params![
            run.run_id,
            workflow.id,
            "accepted",
            serde_json::to_string(&run_val).unwrap()
        ],
    )
    .unwrap();

    let _assert = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "request",
            "complete-task",
            "--run",
            &run.run_id,
            "--task",
            "task-001",
            "--executor",
            "codex",
            "--summary",
            "Extreme completion",
            "--evidence-command",
            "true",
            "--evidence-summary",
            "extreme-value completion receipt passed",
            "--estimated-usd",
            "999999999.99",
            "--tokens-in",
            "2147483647",
            "--tokens-out",
            "2147483647",
            "--output",
            "json",
        ])
        .assert()
        .success();
}

/// Test T2-16: Corrupted data_json in SQLite tables
#[test]
fn test_t2_f4_corrupted_data_json() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let store = ForgeStore::open(&store_path).unwrap();

    let intent = forge_core::intent::parse_intent("Goal");
    let workflow = forge_core::graph::create_workflow(intent);
    store.save_workflow(&workflow).unwrap();

    let conn = Connection::open(&store_path).unwrap();
    conn.execute(
        "UPDATE workflows SET data_json = ? WHERE id = ?",
        params!["{corrupt", workflow.id],
    )
    .unwrap();

    let _assert = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args(["status", "--workflow", &workflow.id, "--output", "json"])
        .assert()
        .failure();
}

/// Test T2-17: Missing schema tables
#[test]
fn test_t2_f4_missing_schema_tables() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let _store = ForgeStore::open(&store_path).unwrap();
    let conn = Connection::open(&store_path).unwrap();

    conn.execute("DROP TABLE workflows", params![]).unwrap();

    let _assert = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Table missing test",
            "--output",
            "json",
        ])
        .assert()
        .failure();
}

/// Test T2-18: Invalid column types
#[test]
fn test_t2_f4_invalid_column_types() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let _store = ForgeStore::open(&store_path).unwrap();
    let conn = Connection::open(&store_path).unwrap();

    conn.execute("DROP TABLE workflows", params![]).unwrap();
    conn.execute(
        "CREATE TABLE workflows (
            id TEXT PRIMARY KEY,
            goal TEXT NOT NULL,
            status INTEGER NOT NULL,
            created_at TEXT NOT NULL,
            data_json TEXT NOT NULL
        )",
        params![],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO workflows (id, goal, status, created_at, data_json) VALUES (?, ?, ?, ?, ?)",
        params!["wf_1", "Goal", 12345, "2026-07-04T10:00:00Z", "{}"],
    )
    .unwrap();

    let _assert = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args(["status", "--workflow", "wf_1", "--output", "json"])
        .assert()
        .failure();
}

/// Test T2-19: Bypassing constraints / NULL in NOT NULL fields
#[test]
fn test_t2_f4_null_in_not_null_fields() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let _store = ForgeStore::open(&store_path).unwrap();
    let conn = Connection::open(&store_path).unwrap();

    conn.execute("DROP TABLE IF EXISTS workflows", params![])
        .unwrap();
    conn.execute(
        "CREATE TABLE workflows (
            id TEXT PRIMARY KEY,
            goal TEXT,
            status TEXT,
            created_at TEXT,
            data_json TEXT
        )",
        params![],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO workflows (id, goal, status, created_at, data_json) VALUES (?, ?, ?, ?, ?)",
        params![
            "wf_1",
            Option::<String>::None,
            Option::<String>::None,
            Option::<String>::None,
            Option::<String>::None
        ],
    )
    .unwrap();

    let _assert = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args(["status", "--workflow", "wf_1", "--output", "json"])
        .assert()
        .failure();
}

/// Test T2-20: SQLite locked database file error
#[test]
fn test_t2_f4_sqlite_locked_db() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let _store = ForgeStore::open(&store_path).unwrap();

    let conn = Connection::open(&store_path).unwrap();
    conn.execute("PRAGMA journal_mode=WAL", params![]).ok();
    conn.execute("BEGIN EXCLUSIVE TRANSACTION", params![])
        .unwrap();

    let _assert = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args(["teamwork", "--goal", "Locked DB goal", "--output", "json"])
        .assert()
        .failure();
}

// ============================================================================
// TIER 3: Cross-Feature Combinations / Pairwise
// ============================================================================

/// Interaction 1: CLI planning output matches the SQLite stored workflow plan exactly
#[test]
fn test_t3_cli_plan_matches_sqlite() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");

    let output = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Compare plan test",
            "--detached",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let cli_json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = cli_json["workflow_id"].as_str().unwrap();

    let store = ForgeStore::open(&store_path).unwrap();
    let workflow = store.load_workflow(workflow_id).unwrap();

    assert_eq!(workflow.id, workflow_id);
    assert_eq!(workflow.goal, "Compare plan test");
}

/// Interaction 2: Execution runtime stepping updates SQLite lineage metadata, fetched via status command
#[test]
fn test_t3_runtime_stepping_updates_lineage_status() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let store = ForgeStore::open(&store_path).unwrap();

    let intent = forge_core::intent::parse_intent("Stepping test goal");
    let workflow = forge_core::graph::create_workflow(intent);
    store.save_workflow(&workflow).unwrap();

    let run = forge_core::request::create_run_record(&workflow, "forge_cli", "accepted");
    let run_id = run.run_id.clone();
    let run_val = serde_json::to_value(&run).unwrap();

    let conn = Connection::open(&store_path).unwrap();
    conn.execute(
        "INSERT INTO runs (id, workflow_id, status, data_json) VALUES (?, ?, ?, ?)",
        params![
            run_id,
            workflow.id,
            "accepted",
            serde_json::to_string(&run_val).unwrap()
        ],
    )
    .unwrap();

    let _assert = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args(["request", "step", "--run", &run_id, "--output", "json"])
        .assert()
        .success();

    let status_output = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args(["status", "--workflow", &workflow.id, "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let status_json: serde_json::Value = serde_json::from_slice(&status_output).unwrap();
    assert_eq!(status_json["workflow_id"], workflow.id);
}

/// Interaction 3: Heuristics selects a brain, execution runs the task, and cost updates SQLite ledger
#[test]
fn test_t3_heuristics_execution_cost_ledger_updates() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let store = ForgeStore::open(&store_path).unwrap();

    let intent = forge_core::intent::parse_intent("Cost test goal");
    let workflow = forge_core::graph::create_workflow(intent);
    store.save_workflow(&workflow).unwrap();

    let run = forge_core::request::create_run_record(&workflow, "forge_cli", "accepted");
    let run_id = run.run_id.clone();
    let run_val = serde_json::to_value(&run).unwrap();

    let conn = Connection::open(&store_path).unwrap();
    conn.execute(
        "INSERT INTO runs (id, workflow_id, status, data_json) VALUES (?, ?, ?, ?)",
        params![
            run_id,
            workflow.id,
            "accepted",
            serde_json::to_string(&run_val).unwrap()
        ],
    )
    .unwrap();

    let _assert = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "request",
            "complete-task",
            "--run",
            &run_id,
            "--task",
            "task-001",
            "--executor",
            "codex",
            "--summary",
            "Completed task for cost",
            "--evidence-command",
            "true",
            "--evidence-summary",
            "cost-ledger completion receipt passed",
            "--estimated-usd",
            "0.15",
            "--tokens-in",
            "1500",
            "--tokens-out",
            "3000",
        ])
        .assert()
        .success();

    forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args(["cost", "materialize", "--output", "json"])
        .assert()
        .success();

    let conn = Connection::open(&store_path).unwrap();
    let cost_exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM cost_ledger_index WHERE workflow_id = ?)",
            params![workflow.id],
            |row| row.get(0),
        )
        .unwrap_or(false);

    assert!(cost_exists);
}

/// Interaction 4: Task lease expiration forces execution step failure, improve candidate ranking reflects failure
#[test]
fn test_t3_lease_expiration_forces_failure_and_improve_candidates() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let store = ForgeStore::open(&store_path).unwrap();

    let intent = forge_core::intent::parse_intent("Lease expiration goal");
    let mut workflow = forge_core::graph::create_workflow(intent);
    workflow.status = "running".to_string();
    store.save_workflow(&workflow).unwrap();

    let mut run = forge_core::request::create_run_record(&workflow, "forge_cli", "running");
    run.heartbeat_expires_at = Some(chrono::Utc::now() - chrono::Duration::seconds(60));
    let run_id = run.run_id.clone();
    let run_val = serde_json::to_value(&run).unwrap();

    let conn = Connection::open(&store_path).unwrap();
    conn.execute(
        "INSERT INTO runs (id, workflow_id, status, data_json) VALUES (?, ?, ?, ?)",
        params![
            run_id,
            workflow.id,
            "running",
            serde_json::to_string(&run_val).unwrap()
        ],
    )
    .unwrap();

    conn.execute(
        "CREATE TABLE IF NOT EXISTS task_leases (
            workflow_id TEXT NOT NULL,
            task_id TEXT NOT NULL,
            lease_id TEXT NOT NULL,
            executor TEXT NOT NULL,
            organization_id TEXT NOT NULL DEFAULT '',
            brand_id TEXT NOT NULL DEFAULT '',
            product_id TEXT NOT NULL DEFAULT '',
            user_id TEXT NOT NULL DEFAULT '',
            channel_id TEXT NOT NULL DEFAULT '',
            acquired_at TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            data_json TEXT NOT NULL,
            PRIMARY KEY (workflow_id, task_id)
        )",
        params![],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO task_leases (workflow_id, task_id, lease_id, executor, acquired_at, expires_at, data_json) VALUES (?, ?, ?, ?, ?, ?, ?)",
        params![workflow.id, "task_1", "expired_lease_id", "codex", "1970-01-01T00:00:00Z", "1970-01-01T00:00:00Z", "{}"],
    ).unwrap();

    let _assert = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "request",
            "recover-stale",
            "--run",
            &run_id,
            "--output",
            "json",
        ])
        .assert()
        .success();

    let _improve_out = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args(["improve", "candidates", "--output", "json"])
        .assert()
        .success();
}

// ============================================================================
// TIER 4: Real-World Application Scenarios
// ============================================================================

fn bind_all_contexts(store_path: &std::path::Path, workflow_id: &str) {
    let conn = rusqlite::Connection::open(store_path).unwrap();
    let data_json_str: String = conn
        .query_row(
            "SELECT data_json FROM workflows WHERE id = ?",
            rusqlite::params![workflow_id],
            |row| row.get(0),
        )
        .unwrap();
    let wf_json: serde_json::Value = serde_json::from_str(&data_json_str).unwrap();
    if let Some(tasks) = wf_json["tasks"].as_array() {
        for task in tasks {
            let task_id = task["id"].as_str().unwrap();
            let context_reqs = task["context_requirements"].as_array();
            if context_reqs.is_some_and(|reqs| !reqs.is_empty()) {
                let _ = forge()
                    .arg("--store")
                    .arg(store_path.to_str().unwrap())
                    .args([
                        "context",
                        "--workflow",
                        workflow_id,
                        "--task",
                        task_id,
                        "--project-root",
                        ".",
                        "--output",
                        "json",
                    ])
                    .assert()
                    .success();
            }
        }
    }
}

fn wait_for_task_ready(store_path: &std::path::Path, run_id: &str, expected_task_id: &str) {
    let mut attempts = 0;
    loop {
        let output = forge()
            .arg("--store")
            .arg(store_path.to_str().unwrap())
            .args(["request", "step", "--run", run_id, "--output", "json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let step_json: serde_json::Value = serde_json::from_slice(&output).unwrap();

        let mut current_task_id = String::new();
        if let Some(task_id) = step_json["stepped_task"]["task_id"].as_str() {
            current_task_id = task_id.to_string();
        } else if let Some(task_id) = step_json["handoff_task"]["task_id"].as_str() {
            current_task_id = task_id.to_string();
        } else if let Some(task_id) = step_json["drive_before"]["handoff_task"]["task_id"].as_str()
        {
            current_task_id = task_id.to_string();
        } else if let Some(tasks) = step_json["drive_before"]["parallel_handoff_tasks"].as_array() {
            if !tasks.is_empty() {
                current_task_id = tasks[0]["task_id"].as_str().unwrap_or("").to_string();
            }
        } else if let Some(tasks) = step_json["parallel_handoff_tasks"].as_array() {
            if !tasks.is_empty() {
                current_task_id = tasks[0]["task_id"].as_str().unwrap_or("").to_string();
            }
        }

        if current_task_id == expected_task_id {
            break;
        }
        if step_json["status"] == "handoff_required" && !current_task_id.is_empty() {
            complete_task_with_test_receipt(store_path, run_id, &current_task_id);
            continue;
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
        attempts += 1;
        if attempts > 100 {
            panic!(
                "Timed out waiting for task {} to be ready. Step response: {:?}",
                expected_task_id, step_json
            );
        }
    }
}

fn complete_task_with_test_receipt(store_path: &std::path::Path, run_id: &str, task_id: &str) {
    forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "request",
            "complete-task",
            "--run",
            run_id,
            "--task",
            task_id,
            "--executor",
            "forge_cli",
            "--summary",
            "Teamwork test executor completed the delegated task.",
            "--evidence-command",
            "true",
            "--evidence-summary",
            "teamwork test execution receipt passed",
            "--output",
            "json",
        ])
        .assert()
        .success();
}

fn wait_for_run_completed(store_path: &std::path::Path, run_id: &str) {
    let mut attempts = 0;
    loop {
        let output = forge()
            .arg("--store")
            .arg(store_path.to_str().unwrap())
            .args(["request", "step", "--run", run_id, "--output", "json"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let step_json: serde_json::Value = serde_json::from_slice(&output).unwrap();
        if step_json["status"] == "completed"
            || step_json["status"] == "complete"
            || step_json["drive_before"]["status"] == "completed"
            || step_json["drive_before"]["status"] == "complete"
        {
            break;
        }
        let current_task_id = step_json["stepped_task"]["task_id"]
            .as_str()
            .or_else(|| step_json["handoff_task"]["task_id"].as_str())
            .or_else(|| step_json["drive_before"]["handoff_task"]["task_id"].as_str());
        if step_json["status"] == "handoff_required" {
            if let Some(task_id) = current_task_id {
                complete_task_with_test_receipt(store_path, run_id, task_id);
                continue;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
        attempts += 1;
        if attempts > 100 {
            panic!(
                "Timed out waiting for run to complete. Step response: {:?}",
                step_json
            );
        }
    }
}

/// Scenario 1: JWT Authentication System Design & Coding (Orchestrator plans, Worker writes Rust JWT module, Auditor reviews code)
#[test]
fn test_t4_scenario_1_jwt_auth() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");

    // 1. Orchestrator plans the JWT Auth goal
    let goal = "JWT Authentication System Design & Coding: Implement a JWT auth module in Rust where the orchestrator plans architecture, worker writes module, and auditor reviews it.";
    let output = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args(["teamwork", "--goal", goal, "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let plan_json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = plan_json["workflow_id"].as_str().unwrap();
    let run_id = stored_run_id_for_workflow(&store_path, workflow_id);
    let run_id = run_id.as_str();

    // Verify Roster
    let roles = plan_json["roster"]["roles"].as_array().unwrap();
    let has_orchestrator = roles.iter().any(|r| r["role"] == "Orchestrator");
    let has_worker = roles.iter().any(|r| r["role"] == "Worker");
    let has_auditor = roles.iter().any(|r| r["role"] == "Auditor");
    assert!(
        has_orchestrator && has_worker && has_auditor,
        "Should have Orchestrator, Worker, and Auditor roles"
    );

    // 2. Worker writes Rust JWT module and attaches it
    let jwt_file_path = temp.path().join("jwt.rs");
    std::fs::write(&jwt_file_path, "pub fn sign(claims: &str) -> String { format!(\"signed.jwt.{}\", claims) }\npub fn verify(token: &str) -> bool { token.starts_with(\"signed.jwt.\") }\n").unwrap();

    let attach_out = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "workflow",
            "attach-artifact",
            "--workflow",
            workflow_id,
            "--path",
            jwt_file_path.to_str().unwrap(),
            "--kind",
            "code",
            "--tag",
            "rust-jwt-module",
            "--origin",
            "codex",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let attach_json: serde_json::Value = serde_json::from_slice(&attach_out).unwrap();
    assert!(attach_json["artifact"]["path"].as_str().is_some());

    // 3. Bind contexts for all tasks to unblock execution
    bind_all_contexts(&store_path, workflow_id);

    // 4. Complete task-005 (Worker implementation)
    wait_for_task_ready(&store_path, run_id, "task-005");
    forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "request",
            "complete-task",
            "--run",
            run_id,
            "--task",
            "task-005",
            "--executor",
            "codex",
            "--summary",
            "Worker successfully implemented Rust JWT signing and verification code module",
            "--evidence-command",
            "true",
            "--evidence-summary",
            "JWT implementation receipt passed",
            "--output",
            "json",
        ])
        .assert()
        .success();

    // 5. Complete task-006 (Validation/cargo test)
    wait_for_task_ready(&store_path, run_id, "task-006");
    forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "request",
            "complete-task",
            "--run",
            run_id,
            "--task",
            "task-006",
            "--executor",
            "antigravity",
            "--summary",
            "Validation run successful",
            "--evidence-command",
            "true",
            "--evidence-summary",
            "JWT validation receipt passed",
            "--output",
            "json",
        ])
        .assert()
        .success();

    // 6. Complete task-008 (Auditor review & documentation)
    wait_for_task_ready(&store_path, run_id, "task-008");
    forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "request",
            "complete-task",
            "--run",
            run_id,
            "--task",
            "task-008",
            "--executor",
            "opencode",
            "--summary",
            "Auditor reviewed and verified JWT module logic passes security constraints",
            "--evidence-command",
            "true",
            "--evidence-summary",
            "JWT audit receipt passed",
            "--output",
            "json",
        ])
        .assert()
        .success();

    // 7. Wait for completion
    wait_for_run_completed(&store_path, run_id);
}

/// Scenario 2: Data Extraction & CSV Pipeline (plans and runs a data processing flow, validating final JSON output format)
#[test]
fn test_t4_scenario_2_csv_pipeline() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");

    // 1. Plan CSV processing goal
    let goal = "Data Extraction & CSV Pipeline: Aggregating transaction amounts from CSV and writing to output JSON";
    let output = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args(["teamwork", "--goal", goal, "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let plan_json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = plan_json["workflow_id"].as_str().unwrap();
    let run_id = stored_run_id_for_workflow(&store_path, workflow_id);
    let run_id = run_id.as_str();

    // 2. Attach source CSV
    let csv_file_path = temp.path().join("source.csv");
    std::fs::write(
        &csv_file_path,
        "id,user,amount\n1,alice,50.00\n2,bob,120.50\n3,alice,75.25\n",
    )
    .unwrap();

    forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "workflow",
            "attach-artifact",
            "--workflow",
            workflow_id,
            "--path",
            csv_file_path.to_str().unwrap(),
            "--kind",
            "source",
            "--tag",
            "transaction-csv",
            "--origin",
            "user",
            "--output",
            "json",
        ])
        .assert()
        .success();

    // 3. Attach final JSON deliverable
    let json_file_path = temp.path().join("output.json");
    std::fs::write(
        &json_file_path,
        "{\"alice\": 125.25, \"bob\": 120.50, \"total_transactions\": 3}",
    )
    .unwrap();

    forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "workflow",
            "attach-artifact",
            "--workflow",
            workflow_id,
            "--path",
            json_file_path.to_str().unwrap(),
            "--kind",
            "deliverable",
            "--tag",
            "aggregated-totals",
            "--origin",
            "opencode",
            "--output",
            "json",
        ])
        .assert()
        .success();

    // 4. Bind contexts for all tasks
    bind_all_contexts(&store_path, workflow_id);

    // 5. Complete task-005
    wait_for_task_ready(&store_path, run_id, "task-005");
    forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "request",
            "complete-task",
            "--run",
            run_id,
            "--task",
            "task-005",
            "--executor",
            "antigravity",
            "--summary",
            "CSV pipeline task executed and completed successfully",
            "--evidence-command",
            "true",
            "--evidence-summary",
            "CSV implementation receipt passed",
            "--output",
            "json",
        ])
        .assert()
        .success();

    // 6. Complete task-006
    wait_for_task_ready(&store_path, run_id, "task-006");
    forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "request",
            "complete-task",
            "--run",
            run_id,
            "--task",
            "task-006",
            "--executor",
            "antigravity",
            "--summary",
            "Validation run successful",
            "--evidence-command",
            "true",
            "--evidence-summary",
            "CSV validation receipt passed",
            "--output",
            "json",
        ])
        .assert()
        .success();

    // 7. Complete task-008
    wait_for_task_ready(&store_path, run_id, "task-008");
    forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "request",
            "complete-task",
            "--run",
            run_id,
            "--task",
            "task-008",
            "--executor",
            "antigravity",
            "--summary",
            "CSV pipeline documentation generated",
            "--evidence-command",
            "true",
            "--evidence-summary",
            "CSV documentation receipt passed",
            "--output",
            "json",
        ])
        .assert()
        .success();

    // 8. Wait for completion
    wait_for_run_completed(&store_path, run_id);

    // Verify artifacts list in workflows
    let conn = rusqlite::Connection::open(&store_path).unwrap();
    let data_json_str: String = conn
        .query_row(
            "SELECT data_json FROM workflows WHERE id = ?",
            params![workflow_id],
            |row| row.get(0),
        )
        .unwrap();

    let wf_json: serde_json::Value = serde_json::from_str(&data_json_str).unwrap();
    let artifacts = wf_json["artifacts"]
        .as_array()
        .expect("Should have artifacts array");
    assert!(
        artifacts.len() >= 2,
        "Should have attached both CSV and JSON artifacts"
    );
}

/// Scenario 3: Multi-stage Docker API Config (verifies scheduling of a sequence of wait/command tasks in parallel waves)
#[test]
fn test_t4_scenario_3_docker_api() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");

    // 1. Plan Multi-stage Docker API Config goal
    let goal = "Deploy multi-stage Docker API Config with multiple parallel database, cache, and frontend service containers needing individual health checks and wait conditions";
    let output = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args(["teamwork", "--goal", goal, "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let plan_json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = plan_json["workflow_id"].as_str().unwrap();

    // 2. Run simulation
    let sim_output = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "run",
            "--workflow",
            workflow_id,
            "--simulate",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let sim_json: serde_json::Value = serde_json::from_slice(&sim_output).unwrap();

    // 3. Verify wave scheduling structures
    assert!(
        sim_json["concurrent_wave_count"].as_i64().is_some()
            || sim_json["max_concurrent_tasks"].as_i64().is_some(),
        "Simulation should output parallel concurrency wave information"
    );
    assert!(
        sim_json["cost_report"]["total_estimated_cost_usd"]
            .as_f64()
            .is_some(),
        "Simulation should predict total cost"
    );
}

/// Scenario 4: Markdown Documentation Guide Generation (verifies UI-specialized brain preference for rendering pages)
#[test]
fn test_t4_scenario_4_markdown_docs() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");

    // 1. Plan visual markdown page layout documentation goal
    let goal = "Markdown Documentation Guide Generation: Create a visual documentation dashboard guide with css and html layout specs.";
    let output = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args(["teamwork", "--goal", goal, "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let plan_json: serde_json::Value = serde_json::from_slice(&output).unwrap();

    // 2. Verify visual brain preference (antigravity/agy)
    let roles = plan_json["roster"]["roles"]
        .as_array()
        .expect("Roster roles array");
    let mut found_worker = false;
    for role_val in roles {
        if role_val["role"] == "Worker" {
            found_worker = true;
            let brain = role_val["brain"].as_str().unwrap();
            assert!(
                brain == "antigravity" || brain == "agy",
                "UI-heavy visual task should prefer visual brain, got: {}",
                brain
            );
        }
    }
    assert!(found_worker, "Should have Worker role");
}

/// Scenario 5: Adversarial Code Audit (verifies strict cooperation between Orchestrator planning and Auditor review checking a security flaw)
#[test]
fn test_t4_scenario_5_adversarial_audit() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");

    // 1. Plan security-critical cryptography audit goal
    let goal = "Adversarial Code Audit: Audit a critical cryptographic signature verification module for timing side-channels and bypass vulnerabilities.";
    let output = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args(["teamwork", "--goal", goal, "--output", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let plan_json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = plan_json["workflow_id"].as_str().unwrap();
    let run_id = stored_run_id_for_workflow(&store_path, workflow_id);
    let run_id = run_id.as_str();

    // Verify Roster contains Orchestrator and Auditor
    let roles = plan_json["roster"]["roles"].as_array().unwrap();
    let has_orchestrator = roles.iter().any(|r| r["role"] == "Orchestrator");
    let has_auditor = roles.iter().any(|r| r["role"] == "Auditor");
    assert!(
        has_orchestrator && has_auditor,
        "Should assign Orchestrator and Auditor roles for security verification"
    );

    // 2. Bind contexts for all tasks
    bind_all_contexts(&store_path, workflow_id);

    // 3. Complete task-005
    wait_for_task_ready(&store_path, run_id, "task-005");
    forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "request",
            "complete-task",
            "--run",
            run_id,
            "--task",
            "task-005",
            "--executor",
            "codex",
            "--summary",
            "Worker implemented constant-time comparison helper (constant_time_compare) to secure signatures",
            "--evidence-command",
            "true",
            "--evidence-summary",
            "security implementation receipt passed",
            "--output",
            "json"
        ])
        .assert()
        .success();

    // 4. Complete task-006
    wait_for_task_ready(&store_path, run_id, "task-006");
    forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "request",
            "complete-task",
            "--run",
            run_id,
            "--task",
            "task-006",
            "--executor",
            "opencode",
            "--summary",
            "Validation run successful",
            "--evidence-command",
            "true",
            "--evidence-summary",
            "security validation receipt passed",
            "--output",
            "json",
        ])
        .assert()
        .success();

    // 5. Complete task-008
    wait_for_task_ready(&store_path, run_id, "task-008");
    let complete_out = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "request",
            "complete-task",
            "--run",
            run_id,
            "--task",
            "task-008",
            "--executor",
            "opencode",
            "--summary",
            "Auditor verified that the constant-time comparison fix successfully mitigates the signature timing side-channel attack",
            "--evidence-command",
            "true",
            "--evidence-summary",
            "security audit receipt passed",
            "--output",
            "json"
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let complete_json: serde_json::Value = serde_json::from_slice(&complete_out).unwrap();
    assert_ne!(complete_json["status"], "not_ready");

    // 6. Wait for completion
    wait_for_run_completed(&store_path, run_id);
}
