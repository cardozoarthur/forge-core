## 2026-07-04T11:18:12Z

You are the Worker for the Forge Teamwork subcommand E2E testing track.
Your task is to fix the E2E test suite in `tests/forge_teamwork_e2e.rs` and verify that all tests compile and execute cleanly under clippy and format checks.

Specifically, implement the following fixes in `tests/forge_teamwork_e2e.rs`:
1. MockServer Robustness:
   Add read/write timeouts of 500ms on all accepted connections in `MockServer::start` and `MockServer500::start` (using `stream.set_read_timeout` and `stream.set_write_timeout`) to resolve Slowloris blocking and thread join deadlocks on drop.
2. Command Exit Code Assertions:
   Ensure all 21 unvalidated `.assert();` calls are updated to `.assert().success();` or `.assert().failure();` as appropriate so that failures are not silently ignored.
3. Fix test_f4_persistence_cached_benchmark_rankings:
   Instead of inserting raw `"{}"` for `data_json` in the `workflows` table, generate a valid workflow using `create_workflow(parse_intent("..."))` and serialize it via `serde_json::to_string(&workflow).unwrap()`, then insert that JSON string.
4. Fix test_f3_task_lease_acquisition, test_f3_cognitive_task_handoff_halts, and test_f3_checkpoint_saving_and_update:
   Add the `--detached` argument to the `forge teamwork` commands in these tests so that `run_id` is populated in the JSON output, resolving the unwrap panics.
5. Fix test_f2_benchmark_cache_hit_versus_bypass:
   - Add `.env("FORGE_BENCHMARK_URL", &mock_server.url)` to the command builder so the env var is set.
   - Replace the hardcoded `updated_at` cache timestamp with `chrono::Utc::now().to_rfc3339()` to prevent cache expiration due to time drift.
6. Fix test_f3_simulated_parallel_execution:
   Change the cost assertion from `sim_json["estimated_total_cost_usd"]` to `sim_json["cost_report"]["total_estimated_cost_usd"]`.
7. Upgrade Schema Assertions:
   Implement the robust `assert_table_schema` helper function in the test file that validates column names, uppercase type, nullability (`notnull`), and primary key constraints (`pk`). Use it to verify all columns in the 5 schema test cases.

Verify your changes:
1. Run `cargo test --test forge_teamwork_e2e --no-run` and verify that compilation is successful.
2. Run clippy: `cargo clippy --all-targets --all-features -- -D warnings` (must be completely warning-free).
3. Run format: `cargo fmt --check` (must be formatted correctly).
4. Write your handoff report to `/home/arthur/projects/forge-core/.agents/teamwork_preview_worker_t3_fix/handoff.md`.

MANDATORY INTEGRITY WARNING:
DO NOT CHEAT. All implementations must be genuine. DO NOT hardcode test results, create dummy/facade implementations, or circumvent the intended task. A Forensic Auditor will independently verify your work. Integrity violations WILL be detected and your work WILL be rejected.
