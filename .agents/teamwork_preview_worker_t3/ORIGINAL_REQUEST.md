## 2026-07-04T07:52:28Z
Worker for the Forge Teamwork subcommand E2E testing track.
Your task is to implement the Tier 4 (Real-World Application Scenarios) E2E tests, and then write the `TEST_READY.md` and `TEST_INFRA.md` files to the project root.

Specifically:
1. Read the existing test suite in `tests/forge_teamwork_e2e.rs`.
2. Implement at least 5 realistic application-level scenarios (Tier 4) verifying complex end-to-end user workflows:
   - Scenario 1: JWT Authentication System Design & Coding (Orchestrator plans, Worker writes Rust JWT module, Auditor reviews code).
   - Scenario 2: Data Extraction & CSV Pipeline (plans and runs a data processing flow, validating final JSON output format).
   - Scenario 3: Multi-stage Docker API Config (verifies scheduling of a sequence of wait/command tasks in parallel waves).
   - Scenario 4: Markdown Documentation Guide Generation (verifies UI-specialized brain preference for rendering pages).
   - Scenario 5: Adversarial Code Audit (verifies strict cooperation between Orchestrator planning and Auditor review checking a security flaw).
3. Integrate these tests into `tests/forge_teamwork_e2e.rs` and verify that the test suite compiles successfully using `cargo test --test forge_teamwork_e2e --no-run`.
4. Create and write `TEST_INFRA.md` to the project root `/home/arthur/projects/forge-core/TEST_INFRA.md` following the template in the instructions. It must list the test philosophy, feature inventory (N=4 features), and real-world application scenarios.
5. Create and write `TEST_READY.md` to the project root `/home/arthur/projects/forge-core/TEST_READY.md` summarizing the test suite, including the runner command `cargo test --test forge_teamwork_e2e`, the coverage counts (T1: 20, T2: 20, T3: 4, T4: 5, Total: 49 tests), and the feature coverage checklist.
6. Write your handoff report at `/home/arthur/projects/forge-core/.agents/teamwork_preview_worker_t3/handoff.md` detailing the implemented tests, files created, and compilation check outputs.

MANDATORY INTEGRITY WARNING:
DO NOT CHEAT. All implementations must be genuine. DO NOT hardcode test results, create dummy/facade implementations, or circumvent the intended task. A Forensic Auditor will independently verify your work. Integrity violations WILL be detected and your work WILL be rejected.
