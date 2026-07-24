## 2026-07-04T10:45:32Z
You are the Worker for the Forge Teamwork subcommand E2E testing track.
Your task is to implement the E2E test harness and Tier 1 (Feature Coverage) tests in a new file `tests/forge_teamwork_e2e.rs`.
You must design these tests as opaque-box, requirement-driven, and independent of the internal implementation details of the subcommand (which will be implemented in parallel).

Specifically:
1. Read the research findings and plans from the three Explorers at:
   - `/home/arthur/projects/forge-core/.agents/teamwork_preview_explorer_t1_1/handoff.md`
   - `/home/arthur/projects/forge-core/.agents/teamwork_preview_explorer_t1_2/handoff.md`
   - `/home/arthur/projects/forge-core/.agents/teamwork_preview_explorer_t1_3/handoff.md`
2. Create `tests/forge_teamwork_e2e.rs` and implement Tier 1 (Feature Coverage) E2E tests for the following four features:
   - Feature 1: CLI & Output Formatting (parsing, `--goal`, `--detached`, `--output json/human`).
   - Feature 2: Roster & Heuristics (brain mapping, mock benchmark fetch using TcpListener local server, environment variable redirection).
   - Feature 3: Execution Runtime & Lineage (detached run stepping, checkpoints, task leases).
   - Feature 4: SQLite Database Persistence (lineage records, cost ledgers, observability indices).
3. Ensure you implement at least 5 distinct test cases per feature (20 test cases in total) for Tier 1.
4. Establish the mock HTTP server helper (using TcpListener) inside the test file to mock benchmark score queries redirected via the `FORGE_BENCHMARK_URL` environment variable.
5. Run `cargo test --test forge_teamwork_e2e --no-run` to verify that the test suite compiles successfully. Note: because the subcommand is not yet implemented, running the tests themselves will fail at runtime, which is expected. The compilation check is your primary verification.
6. Write a handoff report at `/home/arthur/projects/forge-core/.agents/teamwork_preview_worker_t1/handoff.md` detailing the implemented tests, the verification commands used, and the compilation output.

MANDATORY INTEGRITY WARNING:
DO NOT CHEAT. All implementations must be genuine. DO NOT hardcode test results, create dummy/facade implementations, or circumvent the intended task. A Forensic Auditor will independently verify your work. Integrity violations WILL be detected and your work WILL be rejected.
