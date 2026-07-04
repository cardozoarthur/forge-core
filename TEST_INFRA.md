# Forge Teamwork Testing Infrastructure

This document outlines the design, architecture, and execution guidelines for the Forge Teamwork E2E and Integration testing suite.

## Test Philosophy

Forge Core treats teamwork orchestration as deterministic, auditable infrastructure rather than ad-hoc chatbot dialogs. Our testing infrastructure enforces:
1. **Validation-Before-Promotion**: A task cannot progress to completion unless its dependencies and validation rules are satisfied.
2. **Context Budgets & Isolation**: Tasks must not bleed context and must fit within designated token/byte limits.
3. **Asynchronous Execution Trace**: Detached background runs must step through tasks safely, logging revisions and events into the SQLite store.

## Test Suite Layout

- **Unit & Local Integration Tests**: Co-located within the respective source files in `src/` (e.g. `src/teamwork.rs`, `src/context.rs`).
- **Teamwork E2E Tests**: Located in `tests/forge_teamwork_e2e.rs`. This handles end-to-end command-line assertion runs using a temporary SQLite database store.

---

## The Four Tiers of Teamwork Testing

### Tier 1: CLI and Parameter Handling
Ensures that the CLI rejects malformed input and correctly parses command parameters.
- **Goals Verified**: Empty strings, missing arguments, invalid output formats.
- **Run Type**: Synchronous CLI assertions.

### Tier 2: Error Handling & Robustness
Verifies edge cases, database locks, and malicious inputs.
- **Goals Verified**: SQLite locked states, null in non-null DB columns, corrupt JSON payloads, command injection attempts.
- **Run Type**: Mock DB corruptions and argument escaping verification.

### Tier 3: System-Level Flows & Contracts
Verifies core runtime semantics like roster selection, heuristics cost ledgers, and stepping status tracking.
- **Goals Verified**: Heuristics cost logging in SQLite, lease expirations, roster assignments based on capability matching.
- **Run Type**: Stepping requests and SQLite transaction checks.

### Tier 4: Real-World Application Scenarios
Simulates realistic, multi-agent developer workflows from inception to final artifact generation.
- **Scenario 1 (JWT Auth)**: Orchestrator plans JWT module architecture, Worker writes `jwt.rs` Rust code, Auditor validates signature verify safety.
- **Scenario 2 (CSV Pipeline)**: Process transaction CSV pipeline, generate JSON totals, attach artifacts, and verify the integrated manifest.
- **Scenario 3 (Docker API Config)**: Multi-stage Docker API deployment. Verifies scheduler wave generation, parallel task count, and simulation estimates.
- **Scenario 4 (Markdown Docs)**: Visual dashboard guide generation. Confirms that UI-specialized brain heuristic selection prefers `agy` while legacy Gemini remains invalidated.
- **Scenario 5 (Cryptographic Security Audit)**: Code audit checking bypass timing attacks. Verifies strict cooperation between Orchestrator security constraints and Auditor review on constant-time compare fixes.

---

## Execution & Verification Commands

### Standard Test Execution
Runs Tier 1 to Tier 3 tests:
```bash
cargo test
```

### Tier 4 Scenario Execution
Runs the E2E application scenario tests:
```bash
cargo test --test forge_teamwork_e2e test_t4 -- --ignored
```

### Static Analysis & Formatting
Run these checks to maintain code quality:
```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```
