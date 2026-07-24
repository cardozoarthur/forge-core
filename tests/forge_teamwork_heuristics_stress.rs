use assert_cmd::Command;
use rusqlite::{params, Connection};
use serde_json::Value;
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

fn forge() -> Command {
    Command::cargo_bin("forge").expect("forge binary should build")
}

/// 1. Blocked/cognitive tasks halt correctly and trigger `handoff_required`
#[test]
fn test_stress_cognitive_tasks_halting() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");

    // Initiate teamwork goal
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

    let json: Value = serde_json::from_slice(&output).unwrap();
    let workflow_id = json["workflow_id"].as_str().unwrap();
    let connection = Connection::open(&store_path).unwrap();
    let run_id: String = connection
        .query_row(
            "SELECT id FROM runs WHERE workflow_id = ?1",
            [workflow_id],
            |row| row.get(0),
        )
        .unwrap();

    // Step the run repeatedly. task-005 (Execute isolated task) is of Mixed executor type,
    // which is not auto-steppable, so it should halt and return handoff_required.
    let mut step_json: Value = Value::Null;
    for _ in 0..10 {
        let step_output = forge()
            .arg("--store")
            .arg(store_path.to_str().unwrap())
            .args([
                "request",
                "step",
                "--run",
                run_id.as_str(),
                "--output",
                "json",
            ])
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
    let reason = step_json["reason"].as_str().unwrap();
    assert!(
        reason.contains("requires an external executor")
            || reason.contains("explicit validation command"),
        "Unexpected handoff reason: {}",
        reason
    );
}

/// 2. Fallback logic resolves correctly when brains are disallowed via `executor_policy`
#[test]
fn test_stress_executor_policy_fallback() {
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

    // Helper to insert a policy
    let insert_policy = |brain: &str, allowed: bool| {
        let policy_json = serde_json::json!({
            "id": brain,
            "display_name": format!("{} Brain", brain),
            "command": brain,
            "installed": true,
            "configured": true,
            "allowed": allowed,
            "decision_source": "user_policy_denied",
            "synced_at": "2026-07-04T10:00:00Z"
        });
        let conn = Connection::open(&store_path).unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO executor_policy (id, data_json) VALUES (?, ?)",
            params![brain, serde_json::to_string(&policy_json).unwrap()],
        )
        .unwrap();
    };

    // Case A: Disallow codex, prefer Rust/coding goal.
    // Preferred list for coding is ["codex", "agy", "opencode"].
    // If codex is disallowed, it should fallback to Antigravity/agy before OpenCode.
    insert_policy("codex", false);
    insert_policy("opencode", true);
    insert_policy("gemini", true);

    let output_a = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Write rust coding helper for jwt auth",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json_a: Value = serde_json::from_slice(&output_a).unwrap();
    let roles_a = json_a["roster"]["roles"].as_array().unwrap();
    let worker_a = roles_a.iter().find(|r| r["role"] == "Worker").unwrap();
    assert_eq!(worker_a["brain"], "agy");

    // Case B: Disallow codex AND opencode, prefer Rust/coding goal.
    // Fallback should go to agy because Gemini is legacy-invalidated.
    insert_policy("codex", false);
    insert_policy("opencode", false);

    let output_b = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Write rust coding helper for jwt auth",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json_b: Value = serde_json::from_slice(&output_b).unwrap();
    let roles_b = json_b["roster"]["roles"].as_array().unwrap();
    let worker_b = roles_b.iter().find(|r| r["role"] == "Worker").unwrap();
    assert_eq!(worker_b["brain"], "agy");

    // Case C: Visual goal preferences: ["agy", "codex", "opencode"]
    // Legacy antigravity alias should not be required for agy selection.
    insert_policy("antigravity", false);
    insert_policy("agy", true);

    let output_c = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Create a visual web dashboard and CSS layout",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json_c: Value = serde_json::from_slice(&output_c).unwrap();
    let roles_c = json_c["roster"]["roles"].as_array().unwrap();
    let worker_c = roles_c.iter().find(|r| r["role"] == "Worker").unwrap();
    assert_eq!(worker_c["brain"], "agy");

    // Case D: Boundary condition: Deny ALL brains.
    // Disallow every modern brain; Gemini is already invalidated by default.
    for brain in &["codex", "opencode", "gemini", "antigravity", "agy"] {
        insert_policy(brain, false);
    }

    let assert_result = forge()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Write rust coding helper for jwt auth",
            "--output",
            "json",
        ])
        .assert()
        .failure();

    let stderr = String::from_utf8_lossy(&assert_result.get_output().stderr);
    assert!(stderr.contains("No allowed modern brain found in executor policy for role"));
}

/// 3. Benchmark URL fetches and SQLite caches operate correctly
#[test]
fn test_stress_benchmark_cache_and_url_fetch() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");

    let mock_response = r#"[
        {
            "brain": "custom_fetched_brain",
            "mismatch_penalty": 0.0,
            "evals": {
                "lmsys_chatbot_arena": 1750,
                "mmlu": 0.99,
                "human_eval": 0.998
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

    // Cache hit case: valid timestamp (within 24 hours / 86400s)
    let now_str = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT OR REPLACE INTO web_benchmark_cache (brain_id, lmsys_score, mmlu_score, human_eval_score, updated_at) VALUES (?, ?, ?, ?, ?)",
        params!["custom_cached_brain", 1600, 0.98, 0.99, &now_str]
    ).unwrap();

    // A. Verify cache is hit by default
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

    let json_hit: Value = serde_json::from_slice(&output_hit).unwrap();
    let roles_hit = json_hit["roster"]["roles"].as_array().unwrap();
    let worker_hit = roles_hit.iter().find(|r| r["role"] == "Worker").unwrap();
    assert_eq!(
        worker_hit["brain"], "custom_cached_brain",
        "Should have hit the cache"
    );

    // B. Verify cache bypass queries the mock server
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

    let json_bypass: Value = serde_json::from_slice(&output_bypass).unwrap();
    let roles_bypass = json_bypass["roster"]["roles"].as_array().unwrap();
    let worker_bypass = roles_bypass.iter().find(|r| r["role"] == "Worker").unwrap();
    assert_eq!(
        worker_bypass["brain"], "custom_fetched_brain",
        "Should have bypassed the cache"
    );

    // C. Verify cache expiration queries mock server (timestamp older than 24 hours)
    let expired_time = (chrono::Utc::now() - chrono::Duration::hours(25)).to_rfc3339();
    conn.execute(
        "UPDATE web_benchmark_cache SET updated_at = ? WHERE brain_id = 'custom_cached_brain'",
        params![&expired_time],
    )
    .unwrap();

    let output_expired = forge()
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

    let json_expired: Value = serde_json::from_slice(&output_expired).unwrap();
    let roles_expired = json_expired["roster"]["roles"].as_array().unwrap();
    let worker_expired = roles_expired
        .iter()
        .find(|r| r["role"] == "Worker")
        .unwrap();
    assert_eq!(
        worker_expired["brain"], "custom_fetched_brain",
        "Should have bypassed expired cache"
    );

    // D. Verify unreachable server is handled gracefully (defaults to static heuristics)
    let output_unreachable = forge()
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
        .env("FORGE_BENCHMARK_URL", "http://127.0.0.1:9999/benchmarks")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json_unreachable: Value = serde_json::from_slice(&output_unreachable).unwrap();
    let roles_unreachable = json_unreachable["roster"]["roles"].as_array().unwrap();
    let worker_unreachable = roles_unreachable
        .iter()
        .find(|r| r["role"] == "Worker")
        .unwrap();
    // Default preferred for coding goals is Codex -> Antigravity/agy -> OpenCode.
    // With the remote benchmark unreachable, Forge falls back to the first allowed modern brain.
    assert!(worker_unreachable["brain"].as_str().is_some());

    // E. Verify malformed server JSON is handled gracefully (defaults to static heuristics)
    let malformed_server = MockServer::start("{invalid_json".to_string());
    let output_malformed = forge()
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
        .env("FORGE_BENCHMARK_URL", &malformed_server.url)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json_malformed: Value = serde_json::from_slice(&output_malformed).unwrap();
    let roles_malformed = json_malformed["roster"]["roles"].as_array().unwrap();
    let worker_malformed = roles_malformed
        .iter()
        .find(|r| r["role"] == "Worker")
        .unwrap();
    assert!(worker_malformed["brain"].as_str().is_some());
}
