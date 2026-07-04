use assert_cmd::Command;
use rusqlite::{params, Connection};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use tempfile::tempdir;

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

#[test]
fn test_challenger_cognitive_task_handoff_halts() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");

    let output = Command::cargo_bin("forge")
        .unwrap()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Perform research write up and analysis on system metrics",
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

    let mut step_json: serde_json::Value = serde_json::Value::Null;
    for _ in 0..15 {
        let step_output = Command::cargo_bin("forge")
            .unwrap()
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
        .contains("requires an external executor"));
}

#[test]
fn test_challenger_executor_policy_denial_fallback() {
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

    let output = Command::cargo_bin("forge")
        .unwrap()
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

    let mut found_worker = false;
    for role_val in roles {
        if role_val["role"] == "Worker" {
            found_worker = true;
            let brain = role_val["brain"].as_str().unwrap();
            assert_ne!(
                brain, "codex",
                "Should fallback and not choose blocked codex brain"
            );
            assert!(
                brain == "opencode" || brain == "antigravity" || brain == "agy",
                "Fallback should select opencode or Antigravity/agy, got {}",
                brain
            );
        }
    }
    assert!(found_worker);
}

#[test]
fn test_challenger_all_brains_denied_policy_bypass() {
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

    let assert_result = Command::cargo_bin("forge")
        .unwrap()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Implement high-performance logging subsystem",
            "--output",
            "json",
        ])
        .assert()
        .failure();

    let stderr = String::from_utf8_lossy(&assert_result.get_output().stderr);
    assert!(
        stderr.contains("No allowed modern brain found in executor policy for role"),
        "Expected error message not found in stderr: {}",
        stderr
    );
}

#[test]
fn test_challenger_missing_cache_table_creates_table_and_saves_benchmarks() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");

    let mock_response = r#"[
        {
            "brain": "custom_web_brain",
            "mismatch_penalty": 0.0,
            "evals": {
                "lmsys_chatbot_arena": 1800,
                "mmlu": 0.99,
                "human_eval": 0.99
            }
        }
    ]"#;

    let mock_server = MockServer::start(mock_response.to_string());

    // Run without creating web_benchmark_cache table. Since we fixed the logic inversion,
    // the code will detect it doesn't exist, create it, and save the fetched benchmarks.
    let output = Command::cargo_bin("forge")
        .unwrap()
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

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();

    // The benchmark scores should be returned in JSON since it fetched them
    let scores = json["benchmarks"]["scores"]
        .as_array()
        .expect("Benchmarks should be returned");
    assert!(!scores.is_empty());

    // The roster roles SHOULD have "custom_web_brain" as Worker, because the cache table
    // was created on the fly and the benchmark scores were saved/used.
    let roles = json["roster"]["roles"].as_array().unwrap();
    let mut found_custom = false;
    for role_val in roles {
        if role_val["brain"] == "custom_web_brain" {
            found_custom = true;
        }
    }
    assert!(
        found_custom,
        "Bug: Did not assign custom_web_brain even though missing cache table should have been created on the fly"
    );

    // Verify it was actually saved in the DB
    let conn = Connection::open(&store_path).unwrap();
    let has_cache_entry: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM web_benchmark_cache WHERE brain_id = 'custom_web_brain')",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);
    assert!(
        has_cache_entry,
        "The benchmark cache entry was not created and saved in the DB!"
    );
}

#[test]
fn test_challenger_expired_cache_without_url_drops_scores() {
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

    // Insert an expired cached brain (more than 24 hours ago, e.g., 25 hours ago)
    let expired_time = (chrono::Utc::now() - chrono::Duration::hours(25)).to_rfc3339();
    conn.execute(
        "INSERT INTO web_benchmark_cache (brain_id, lmsys_score, mmlu_score, human_eval_score, updated_at) VALUES (?, ?, ?, ?, ?)",
        params!["custom_cached_brain", 1600, 0.98, 0.99, &expired_time]
    )
    .unwrap();

    // Run without FORGE_BENCHMARK_URL set.
    let output = Command::cargo_bin("forge")
        .unwrap()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Write highly specialized math functions",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();

    // Benchmarks should be null/missing in the output because cache expired and no URL was set to update
    assert!(json["benchmarks"].is_null());

    // Roles should not contain custom_cached_brain
    let roles = json["roster"]["roles"].as_array().unwrap();
    let mut found_custom = false;
    for role_val in roles {
        if role_val["brain"] == "custom_cached_brain" {
            found_custom = true;
        }
    }
    assert!(
        !found_custom,
        "Should not select expired cached brain when no update URL is configured"
    );
}

#[test]
fn test_challenger_https_benchmark_url_ignored_silently() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");

    let output = Command::cargo_bin("forge")
        .unwrap()
        .arg("--store")
        .arg(store_path.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Write clean Rust compiler parser code",
            "--output",
            "json",
        ])
        .env("FORGE_BENCHMARK_URL", "https://127.0.0.1:0/benchmarks")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();

    // Benchmarks should be null because https is not supported and failed
    assert!(json["benchmarks"].is_null());
}
