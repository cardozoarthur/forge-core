## 2026-07-04T10:48:21Z
You are the Worker for the Forge Teamwork subcommand E2E testing track.
Your task is to implement the Tier 2 (Boundary & Error Handling) and Tier 3 (Cross-Feature Combinations/Pairwise) tests in `tests/forge_teamwork_e2e.rs`.
You must design these tests as opaque-box, requirement-driven, and independent of the internal implementation details of the subcommand.

Specifically:
1. Read the existing test suite in `tests/forge_teamwork_e2e.rs`.
2. Implement at least 5 boundary/corner/error test cases per feature (20 test cases in total) for Tier 2:
   - Feature 1: Goal input with maximum size (10,000 characters), goals with special control characters, and multiple flags together.
   - Feature 2: HTTP benchmark server returning 500 error, HTTP server connection timeout/unreachability, cache record expiration (by manually writing outdated timestamp in SQLite), and executor policy disallowing all brain options.
   - Feature 3: Attempting to step a workflow in a cancelled or failed state, acquiring a task lease that is already leased by another executor, and simulated dry-run wave execution with no tasks.
   - Feature 4: Corrupted data_json in SQLite tables, missing schema tables, and invalid column types.
3. Implement at least 4 cross-feature interaction/pairwise test cases for Tier 3:
   - Interaction 1: CLI planning output matches the SQLite stored workflow plan exactly.
   - Interaction 2: Execution runtime stepping updates the SQLite lineage metadata, which is then fetched via the CLI status command.
   - Interaction 3: Heuristics selects a brain, execution runs the task, and cost materialization updates the cost ledger in SQLite.
   - Interaction 4: Task lease expiration forces execution step to mark task as failed, and the improve candidate ranking reflects the failure.
4. Integrate these tests into `tests/forge_teamwork_e2e.rs` and verify that the test suite compiles successfully using `cargo test --test forge_teamwork_e2e --no-run`.
5. Write your handoff report at `/home/arthur/projects/forge-core/.agents/teamwork_preview_worker_t2/handoff.md` detailing the implemented tests, the verification commands used, and the compilation output.

MANDATORY INTEGRITY WARNING:
DO NOT CHEAT. All implementations must be genuine. DO NOT hardcode test results, create dummy/facade implementations, or circumvent the intended task. A Forensic Auditor will independently verify your work. Integrity violations WILL be detected and your work WILL be rejected.
