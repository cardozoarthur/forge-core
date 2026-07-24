# Teamwork Feature 2 - Roster Heuristics & Benchmark Consolidation Test Plan

## 1. Observation

Based on direct exploration of the `forge-core` repository:

### A. E2E Test Suite Layout & Execution (`tests/forge_cli_contract.rs`)
- Integration tests execute the compiled `forge` binary via the `assert_cmd` crate.
- File path: `/home/arthur/projects/forge-core/tests/forge_cli_contract.rs`.
- A temporary database store is generated for isolation (lines 169-170):
  ```rust
  let temp = tempdir().unwrap();
  let store = temp.path().join("forge.sqlite");
  ```
- Subprocess dry-runs are validated using custom CLI flags, environment variables, and pre-populated SQLite state.

### B. Task Graph & Context Decomposition (`src/graph.rs`)
- Goal decomposition is handled by `create_workflow(intent: IntentSpec)` which invokes `build_tasks(&intent)` (lines 616-633).
- The default tasks sequence is built using helper functions like `task(...)` which set a static `ExecutorKind` and generic `node_brain_routing` settings:
  ```rust
  let node_brain_routing = node_brain_routing_for_executor(&executor);
  ```
- The tasks inside the workflow contain metadata structures (e.g. `AtomicTask` and `WorkItemSpec`) where role assignment details and brain settings are tracked.

### C. Database Architecture (`src/storage.rs`)
- Database initialization happens in `ForgeStore::open` which executes table creation batches (lines 617-1065).
- Currently, there are NO tables defined for benchmark data or roster brain selection cache. The only related tables are `executor_policy` and `executor_quotas` (lines 999-1063).

### D. Executor Candidates & Configs (`src/executor.rs`)
- Available executors are queried using functions like `build_brain_router` which return a `BrainRouterReport` containing available brains such as `codex_primary_brain` (lines 877-1352).

---

## 2. Logic Chain

To enable opaque-box, requirement-driven E2E testing of the Feature 2 roster heuristics and benchmark ranking logic, we reason as follows:

1. **Role & Brain Mapping Assertion**:
   - Because the CLI decomposes the goal into tasks and assigns them to roles (Orchestrator, Worker, Auditor) with specific brains based on benchmarks, we can verify this by passing different goals to `forge teamwork --goal "<goal>" --output json` and asserting on the returned JSON object.
   - For coding-heavy goals, the Worker task brain should map to `codex` or `opencode`. For frontend goals, it should map to `antigravity`. For coordination/audit, it should map to `antigravity` or `gemini`.

2. **Benchmark Retrieval Mocking**:
   - Since E2E tests run in a CODE_ONLY restricted network environment, the CLI cannot fetch real online benchmark data.
   - Therefore, the E2E test suite must spawn a local mock HTTP server (using `TcpListener`) returning mock JSON scores.
   - The CLI must accept an environment variable (e.g. `FORGE_BENCHMARK_URL`) to redirect its web queries to this local server.

3. **Cache Testing (Hit vs. Bypass vs. Fallback)**:
   - **Cache Hit**: We can pre-populate the `web_benchmark_cache` SQLite table with high scores for a specific brain, then run `forge teamwork` and verify that the pre-populated brain is chosen.
   - **Cache Bypass**: We can run the command with a bypass flag (e.g. `--bypass-cache` or `FORGE_BYPASS_BENCHMARK_CACHE=true`) and verify (via mock server logs or sqlite changes) that the CLI ignored the cache and performed a new HTTP fetch.
   - **Offline Fallback**: If the mock server returns a 500 status or is unreachable, the CLI should fall back to the SQLite cache (even if expired) and log a warning to `stderr`. If no cache exists, it must fall back to safe hardcoded baseline defaults.

4. **Executor Policy Integration**:
   - Product rules state that we must not use an installed brain unless the executor policy allows it.
   - We can pre-populate the SQLite database to mark the highest-ranked brain (e.g. `codex`) as blocked/denied in `executor_policy`, then run `forge teamwork` and verify that the heuristics fallback to the next highest-ranked allowed brain (e.g. `opencode`).

---

## 3. Caveats

- **Mock Execution Engine**: In E2E tests, the backend brains (`codex`, `agy`) might not be installed or authenticated. The E2E test plan assumes that `forge teamwork` plans the roster and logs the plan, but execution itself runs as a simulated dry-run (e.g. via `--detached` or when executing simulated steps).
- **Time/Expiration Mocking**: If caching uses an expiration window (e.g. 24 hours), test cases verifying cache expiration might need to manually update the `updated_at` column in SQLite to a timestamp in the past to trigger a fresh fetch.

---

## 4. Conclusion & Recommended Test Plan

We recommend introducing the `web_benchmark_cache` table to SQLite, building a local HTTP mock server helper in `tests/forge_cli_contract.rs`, and implementing the following 7 E2E test cases.

### SQLite Schema Design
```sql
CREATE TABLE IF NOT EXISTS web_benchmark_cache (
    brain_id TEXT PRIMARY KEY,
    lmsys_score INTEGER NOT NULL,
    mmlu_score REAL NOT NULL,
    human_eval_score REAL NOT NULL,
    updated_at TEXT NOT NULL
);
```

### Proposed E2E Test Cases

#### Case 1: Coding Goal Roster Decomposition (Happy Path)
- **Goal**: `"Write a Rust parser for abstract syntax trees."`
- **Method**: Invoke `forge teamwork --goal "..." --output json`.
- **Expected**: The output contains a Worker task mapped to `codex` or `opencode` (due to high HumanEval prioritization for coding tasks).

#### Case 2: Frontend Goal Roster Decomposition (Happy Path)
- **Goal**: `"Create a visual dashboard button using CSS."`
- **Method**: Invoke `forge teamwork --goal "..." --output json`.
- **Expected**: The output contains a Worker task mapped to `antigravity` or a UI-specialized executor.

#### Case 3: Cache Hit Verification
- **Method**:
  1. Pre-populate SQLite `web_benchmark_cache` with a mock brain `"custom_brain"` having the highest HumanEval score.
  2. Run `forge teamwork --goal "Write code" --output json`.
  3. Verify that the selected brain is `"custom_brain"`.
  4. Verify that no mock HTTP server requests were received.

#### Case 4: Cache Bypass & Web Fetch Verification
- **Method**:
  1. Pre-populate the cache with a mock brain.
  2. Spawn a local mock HTTP server returning updated scores where `"alternative_brain"` has the highest scores.
  3. Run `forge teamwork --goal "Write code" --bypass-cache --output json` with `FORGE_BENCHMARK_URL` pointing to the mock server.
  4. Verify that `"alternative_brain"` is selected.
  5. Verify that `web_benchmark_cache` in SQLite was updated with the mock server's values.

#### Case 5: Offline Fallback to Cache
- **Method**:
  1. Pre-populate cache with mock scores.
  2. Point `FORGE_BENCHMARK_URL` to an invalid port (simulating offline state).
  3. Run `forge teamwork --goal "Write code" --bypass-cache --output json`.
  4. Verify that the CLI prints a warning to `stderr` indicating connection failure, falls back to SQLite cache, and successfully outputs the plan using cached scores.

#### Case 6: Offline & Empty Cache Fallback (Hardcoded Baseline)
- **Method**:
  1. Ensure cache table is empty.
  2. Run the command with an unreachable `FORGE_BENCHMARK_URL`.
  3. Verify that the command succeeds by falling back to hardcoded defaults (e.g. `codex` / `opencode` for coding) and prints a warning to `stderr`.

#### Case 7: Executor Policy Exclusion Boundary
- **Method**:
  1. Pre-populate cache indicating `codex` has the highest coding score.
  2. Write an `executor_policy` record in SQLite marking `codex` as disallowed (`"allowed": false`).
  3. Run `forge teamwork --goal "Write Rust code" --output json`.
  4. Verify that the CLI selects the next best allowed brain (e.g. `opencode` or `gemini`) and documents the policy bypass reason in the metadata.

---

## 5. Verification Method

### How to Run the Tests
Once Feature 2 and the E2E tests are implemented:
```bash
cargo test --test forge_cli_contract test_teamwork_
```

### Mock HTTP Server Helper Implementation Sketch
The following helper can be added to `tests/forge_cli_contract.rs` to mock the web benchmark endpoint:
```rust
use std::net::TcpListener;
use std::io::{Read, Write};
use std::thread;

fn start_benchmark_mock_server(response_body: &'static str) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let url = format!("http://127.0.0.1:{}/benchmarks", port);

    let handle = thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });

    (url, handle)
}
```
