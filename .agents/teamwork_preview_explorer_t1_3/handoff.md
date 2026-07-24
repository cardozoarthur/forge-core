# Handoff Report: E2E Test Plan for Feature 3 (Runtime & Lineage) and Feature 4 (SQLite Database Persistence)

## 1. Observation
Below are the direct observations from the codebase, including specific file paths, line numbers, and verbatim code quotes.

### A. SQLite Table Schema Definitions
In `src/storage.rs`, the SQLite schema is initialized in the `migrate` function. Below are the verbatim schema definitions for the key tables relevant to execution runs, task handoffs, audits, cost, and lineage:

*   **Workflows and Artifacts** (`src/storage.rs:617-631`):
    ```rust
    CREATE TABLE IF NOT EXISTS workflows (
        id TEXT PRIMARY KEY,
        goal TEXT NOT NULL,
        status TEXT NOT NULL,
        created_at TEXT NOT NULL,
        data_json TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS artifacts (
        id TEXT PRIMARY KEY,
        workflow_id TEXT NOT NULL,
        kind TEXT NOT NULL,
        path TEXT NOT NULL,
        sha256 TEXT NOT NULL,
        created_at TEXT NOT NULL
    );
    ```

*   **Runs** (`src/storage.rs:1009-1021`):
    ```rust
    CREATE TABLE IF NOT EXISTS runs (
        id TEXT PRIMARY KEY,
        workflow_id TEXT NOT NULL,
        organization_id TEXT NOT NULL DEFAULT '',
        brand_id TEXT NOT NULL DEFAULT '',
        product_id TEXT NOT NULL DEFAULT '',
        user_id TEXT NOT NULL DEFAULT '',
        channel_id TEXT NOT NULL DEFAULT '',
        status TEXT NOT NULL,
        data_json TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
    );
    ```

*   **Task Checkpoints** (`src/storage.rs:1037-1050`):
    ```rust
    CREATE TABLE IF NOT EXISTS task_checkpoints (
        id TEXT PRIMARY KEY,
        workflow_id TEXT NOT NULL,
        task_id TEXT NOT NULL,
        executor TEXT NOT NULL,
        organization_id TEXT NOT NULL DEFAULT '',
        brand_id TEXT NOT NULL DEFAULT '',
        product_id TEXT NOT NULL DEFAULT '',
        user_id TEXT NOT NULL DEFAULT '',
        channel_id TEXT NOT NULL DEFAULT '',
        state TEXT NOT NULL,
        created_at TEXT NOT NULL,
        data_json TEXT NOT NULL
    );
    ```

*   **Task Leases** (`src/storage.rs:1022-1036`):
    ```rust
    CREATE TABLE IF NOT EXISTS task_leases (
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
    );
    ```

*   **Cost Ledger Index** (`src/storage.rs:698-718`):
    ```rust
    CREATE TABLE IF NOT EXISTS cost_ledger_index (
        row_key TEXT PRIMARY KEY,
        source_kind TEXT NOT NULL,
        workflow_id TEXT NOT NULL,
        task_id TEXT,
        event_id INTEGER,
        organization_id TEXT NOT NULL,
        brand_id TEXT NOT NULL,
        product_id TEXT NOT NULL,
        addon_id TEXT,
        executor TEXT,
        model_call_required INTEGER NOT NULL,
        model_call_avoided INTEGER NOT NULL,
        estimated_task_cost_usd REAL NOT NULL,
        observed_event_cost_usd REAL NOT NULL,
        tokens_in INTEGER NOT NULL,
        tokens_out INTEGER NOT NULL,
        data_json TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
    );
    ```

*   **Event Observability Index** (`src/storage.rs:662-689`):
    ```rust
    CREATE TABLE IF NOT EXISTS event_observability_index (
        global_event_id INTEGER PRIMARY KEY,
        workflow_id TEXT NOT NULL,
        kind TEXT NOT NULL,
        category TEXT NOT NULL,
        severity TEXT NOT NULL,
        origin TEXT NOT NULL,
        source TEXT NOT NULL,
        organization_id TEXT NOT NULL,
        brand_id TEXT NOT NULL,
        product_id TEXT NOT NULL,
        node_ref TEXT,
        addon_id TEXT,
        duration_ms INTEGER,
        retry_count INTEGER,
        wait_state TEXT,
        wait_seconds INTEGER,
        context_budget_bytes INTEGER,
        selected_context_bytes INTEGER,
        context_remaining_bytes INTEGER,
        context_pressure_bps INTEGER,
        context_pressure_state TEXT,
        memory_level TEXT,
        memory_scope TEXT,
        data_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
    );
    ```

*   **Runtime Contract Dispatches** (`src/storage.rs:830-845`):
    ```rust
    CREATE TABLE IF NOT EXISTS runtime_contract_dispatches (
        id TEXT PRIMARY KEY,
        addon_id TEXT NOT NULL,
        contract_id TEXT NOT NULL,
        contract_type TEXT NOT NULL,
        capability_id TEXT NOT NULL,
        runtime TEXT NOT NULL,
        entrypoint TEXT NOT NULL,
        status TEXT NOT NULL,
        source TEXT NOT NULL,
        input_json TEXT NOT NULL,
        policy_json TEXT NOT NULL,
        data_json TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
    );
    ```

### B. Detached Execution Spawning and Loop Control
In `src/main.rs`, when `--detached` is supplied during planning or request startup, the main process spawns a background command executing `request drive-loop --run <run_id>` (`src/main.rs:4247-4261`):
```rust
if detached {
    if let Some(ref r_id) = run_id {
        let current_exe = std::env::current_exe()?;
        std::process::Command::new(current_exe)
            .arg("--store")
            .arg(&store_path)
            .arg("request")
            .arg("drive-loop")
            .arg("--run")
            .arg(r_id)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
    }
}
```

The background `drive-loop` handles execution driver steps as follows (`src/main.rs:8353-8370`):
```rust
let store = ForgeStore::open(cli.store.clone())?;
loop {
    let report = step_request(&store, &run_id, &executor, ttl_seconds, &origin)?;
    if report.status == "completed"
        || report.status == "failed"
        || report.status == "cancelled"
    {
        break;
    }
    if report.status == "skipped" && report.reason.contains("no ready handoff task")
    {
        break;
    }
    if report.status == "handoff_required" || report.status == "validation_failed" {
        break;
    }
    std::thread::sleep(std::time::Duration::from_millis(50));
}
```

### C. Simulated vs Real Execution
*   **Simulated Parallel Execution** (`src/execution.rs:129-165`): Spawns standard OS threads to simulate concurrent waves based on the parallel schedule plan:
    ```rust
    for wave in &parallel_plan.waves {
        ...
        for task_id in &wave_ids {
            let handle = thread::spawn(move || {
                ...
                comp.insert(task_id);
            });
            handles.push(handle);
        }
        for handle in handles {
            handle.join().ok();
        }
    }
    ```
    During simulation, actual task execution commands are not called; tasks are instantly marked as `Completed` in memory (`src/execution.rs:186-188`).

*   **Real Process Stepping** (`src/request.rs:1347-1366`): Real execution verifies if a task is "auto-steppable". If the task requires external interaction or cognitive brain validation, execution halts and returns a `handoff_required` status:
    ```rust
    if !is_auto_steppable_task(task) {
        return Ok(RequestStepReport {
            ...
            status: "handoff_required".to_string(),
            reason: "ready task requires an external executor or explicit validation command; Forge will not fake execution".to_string(),
            ...
        });
    }
    ```
    A task is auto-steppable ONLY if it has deterministic executor kinds and no validation commands (`src/request.rs:2107-2115`):
    ```rust
    fn is_auto_steppable_task(task: &AtomicTask) -> bool {
        matches!(
            task.executor,
            ExecutorKind::Command | ExecutorKind::Wait | ExecutorKind::Notification
        ) && task
            .validation_rules
            .iter()
            .all(|rule| rule.command.as_deref().unwrap_or("").trim().is_empty())
    }
    ```

### D. Task Handoff & Completion
When an external executor completes a task, it reports it using `complete_ready_task` in `src/request.rs:1446`. This function:
1. Validates the handoff task status.
2. Attaches outputs as artifacts (`src/request.rs:1537`).
3. Generates a replayable `execution_trace` payload including cost details (`src/request.rs:1548-1570`).
4. Performs validation using the imported `validate_executor_response_file` (`src/request.rs:1623`):
    ```rust
    let validation = validate_executor_response_file(store, &workflow.id, &task.id, response_path.as_path())?;
    ```
5. In `src/adapter.rs:123-144`, if the validation is accepted, the workflow task is promoted:
    ```rust
    if report.accepted {
        let promotion = promote_validated_task(&mut workflow, task_id, &response);
        store.save_workflow(&workflow)?;
        ...
    }
    ```

### E. Cost, Token Tracking & Candidate Ranking
*   **Cost Materialization** (`src/cost.rs:504-522`):
    ```rust
    pub fn materialize_cost_ledger_index(...) -> Result<CostLedgerIndexReport> {
        let ledger = build_cost_ledger(store, query.workflow_id, ...)?;
        let writes = cost_ledger_index_writes(&ledger);
        let materialized_row_count = store.replace_cost_ledger_index_records(&workflow_ids, &writes)?;
        ...
    }
    ```
    Cost index writes extract model calls, tokens in/out, and USD costs:
    - `model_call_required = ai_allowed && !deterministic`
    - `observed_event_cost_usd = sum of event.estimated_usd`
    - `tokens_in` and `tokens_out` are extracted from the `cost` or `executor_cost` payloads of events (`cost_event_from_store_event` at `src/cost.rs:1864`).

*   **Improvement Candidate Ranking** (`src/improve.rs:522-581`): Candidates are scored and sorted. Scored factors include `rework_event_count`, `stale_run_count`, `needs_attention_run_count`, and `failed_task_count`. Sorting orders by `score DESC` then `workflow_id ASC` (`src/improve.rs:456-461`).

---

## 2. Logic Chain
1. **Runtime Execution State Machine**:
   - Spawning a request begins by saving a `runs` record (state: `accepted`).
   - If detached (`--detached`), a separate thread of execution runs `drive-loop`. It progresses all deterministic steps (e.g. `Wait`, simple `Command` blocks) automatically.
   - When a cognitive block is met, the system transitions to `handoff_required` and halts. The driver process exits successfully, preserving resources (Scale-to-Zero semantics).
   - Task leases and checkpoints protect concurrent runs: a task checkpoint preserves `workflow_revision` and `context_routing_cache_key` to avoid race conditions.

2. **Cost Tracking & Metrics**:
   - The executor reports cost metrics (`estimated_usd`, `tokens_in`, `tokens_out`) via the task completion payload.
   - Upon successful validation, this writes an `executor_response_promoted` event.
   - The cost indexing loop (`materialize_cost_ledger_index`) aggregates this event list into actual entries in `cost_ledger_index` in SQLite.
   - Token metrics checking verifies that the sum of `tokens_in` and `tokens_out` matches the aggregation of the underlying events.

3. **Lineage Indexing**:
   - Observed context bounds (bytes used vs remaining) are recorded in the `event_observability_index` columns: `context_budget_bytes`, `selected_context_bytes`, `context_remaining_bytes`, `context_pressure_bps`.
   - By querying SQLite directly, we can assert that these indices exist and verify the complete lineage trace of the task tree.

---

## 3. Caveats
- Since this is a read-only investigation, no code has been modified.
- Spawning process verification in tests utilizes `Command::spawn()` and requires a compiled `forge` binary. The tests assume that `cargo build` was run prior to tests or use `assert_cmd::Command::cargo_bin("forge")` which automatically builds the binary.
- SQLite locks might block reads if write queries take too long, but SQLite's shared-cache and temporary directories mitigate this for tests.

---

## 4. Conclusion & Recommended Test Plan
To verify the E2E behaviors of runtime loop progression, detached executions, metrics tracking, and SQLite persistence, we design a recommended E2E test plan consisting of **six concrete test cases**.

### Test Case 1: Happy Path E2E Execution & Lineage
*   **Goal**: Verify a multi-agent workflow runs, executes deterministic tasks, requires handoff for cognitive tasks, completes tasks successfully with mock inputs, and generates a valid lineage trace.
*   **Input**: A goal requesting both deterministic command execution and a cognitive report write.
*   **Execution Flow**:
    1. Run `forge plan --goal "Generate layout and write analysis summary" --output json`
    2. Extract `workflow_id` and `run_id`.
    3. Step request: `forge request step --run <run_id> --executor codex --output json`. Verify it auto-steps deterministic task (e.g. "task-001") and moves to task-002.
    4. Step request again. Verify it returns status `handoff_required` for the cognitive task.
    5. Complete task: `forge request complete-task --run <run_id> --task task-002 --executor codex --summary "Analysis complete" --artifact /tmp/summary.md --estimated-usd 0.05 --tokens-in 1000 --tokens-out 500 --output json`.
    6. Verify run status is `completed`.
*   **Assertions**:
    - Verify `execution_trace` artifact exists at `artifacts/<wf_id>/execution-trace-task-002-*.json`.
    - Verify `replay` block in trace contains the valid status/drive replay commands.

### Test Case 2: Detached Execution (`--detached`) Background Driver Loop
*   **Goal**: Verify detached execution spawns the background drive-loop and automatically runs to a handoff or completion state without blockages.
*   **Input**: Detached run trigger for a deterministic workflow.
*   **Execution Flow**:
    1. Plan workflow with detached option: `forge plan --goal "Send automated wait and notify notifications" --detached --output json`.
    2. Inspect DB runs status. It should start as `accepted`.
    3. Sleep 200ms to allow the spawned background process to process tasks.
    4. Query request status: `forge request status --run <run_id> --output json`.
*   **Assertions**:
    - Verify the request automatically completes (status `completed`).
    - Verify all task statuses are updated to `completed` in the DB.

### Test Case 3: Simulated vs Real Execution Comparison
*   **Goal**: Validate the simulated engine executes thread-based concurrent waves and reports costs, while real execution performs state machine promotions.
*   **Input**: A parallel-ready task graph.
*   **Execution Flow**:
    1. Run simulated: `forge run --workflow <wf_id> --simulate --output json`.
    2. Run real stepping sequence.
*   **Assertions**:
    - Verify simulated output has `concurrent_wave_count` and `max_concurrent_tasks` matching the parallel waves plan.
    - Verify simulated output cost report totals match task cost predictions.
    - Verify real execution respects dependencies (blocks task-002 promotion until task-001 is completed).

### Test Case 4: Cost Tracking & Token Metrics Verification
*   **Goal**: Validate that token indices correctly accumulate metrics from execution events.
*   **Input**: Multiple completed tasks with custom tokens and USD costs.
*   **Execution Flow**:
    1. Complete Task 1 with `tokens_in = 200`, `tokens_out = 300`, `estimated-usd = 0.01`.
    2. Complete Task 2 with `tokens_in = 400`, `tokens_out = 600`, `estimated-usd = 0.02`.
    3. Materialize index: `forge cost materialize --workflow <wf_id> --output json`.
*   **Assertions**:
    - Select from SQLite `cost_ledger_index` table and verify:
      - `tokens_in` matches 600.
      - `tokens_out` matches 900.
      - `observed_event_cost_usd` matches 0.03.

### Test Case 5: Database State & Verification (SQLite Table Assertions)
*   **Goal**: Assert strict database column types, indices, and constraints are preserved after executing tasks.
*   **Input**: Executed workflow database.
*   **Execution Flow**: Open SQLite connection to test store.
*   **Assertions**:
    - Verifying Table Schema Assertions (see Section 5 below for details).

### Test Case 6: Cached Benchmark Rankings Scoring & Sorting
*   **Goal**: Assert that candidates are sorted correctly based on priority criteria.
*   **Input**: Multiple workflows with varying states (failed tasks, stale runs, completed runs).
*   **Execution Flow**: Run `forge improve candidates --limit 5 --output json`.
*   **Assertions**:
    - Assert that a workflow with a `failed` task has a higher score and appears before a workflow with only `completed` runs.

---

## 5. Verification Method

### Table Schema Verification Assertions (SQLite-specific)
To verify that Feature 3 and Feature 4 DB tables are correctly populated, E2E tests should connect directly to the SQLite file using `rusqlite` and run the following assertions:

```rust
// Open SQLite connection
let conn = Connection::open(&store_path).unwrap();

// 1. Assert Column existence on cost_ledger_index
let cost_columns = sqlite_table_columns(&conn, "cost_ledger_index");
assert!(cost_columns.contains(&"row_key".to_string()));
assert!(cost_columns.contains(&"source_kind".to_string()));
assert!(cost_columns.contains(&"estimated_task_cost_usd".to_string()));
assert!(cost_columns.contains(&"observed_event_cost_usd".to_string()));
assert!(cost_columns.contains(&"tokens_in".to_string()));
assert!(cost_columns.contains(&"tokens_out".to_string()));

// 2. Assert Indices on cost_ledger_index
let cost_indices = sqlite_index_names(&conn, "cost_ledger_index");
assert!(cost_indices.iter().any(|idx| idx.contains("idx_cost_ledger_workflow")));

// 3. Assert Column existence on event_observability_index
let obs_columns = sqlite_table_columns(&conn, "event_observability_index");
assert!(obs_columns.contains(&"context_budget_bytes".to_string()));
assert!(obs_columns.contains(&"selected_context_bytes".to_string()));
assert!(obs_columns.contains(&"context_pressure_bps".to_string()));
assert!(obs_columns.contains(&"memory_level".to_string()));

// 4. Assert task checkpoint state matching execution state
let mut stmt = conn.prepare("SELECT state, workflow_revision FROM task_checkpoints WHERE workflow_id = ?").unwrap();
let mut rows = stmt.query(params![workflow_id]).unwrap();
if let Some(row) = rows.next().unwrap() {
    let state_str: String = row.get(0).unwrap();
    let revision: u64 = row.get(1).unwrap();
    assert!(!state_str.is_empty());
    assert!(revision > 0);
}
```

### Commands to Run and Verify
To verify these E2E assertions directly via CLI tests:
```bash
# Run CLI Integration tests
cargo test --test forge_cli_contract

# Smoke test CLI executors listing and benchmark center route validation
cargo run --bin forge -- executors --output json
```
