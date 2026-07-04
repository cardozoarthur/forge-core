# Project: Forge Teamwork Orchestration

## Architecture
- `forge teamwork` CLI subcommand: accepts `--goal`, `--detached`, and `--output` options.
- Dynamic Roster & Brain Heuristics: decomposes a goal into a task dependency graph, maps task characteristics to roles, and dynamically ranks/selects the best brain using consolidated web benchmark data (e.g. LMSYS, HumanEval, MMLU).
- Antigravity parity: mirrors the observed `agy` `/teamwork-preview` pattern as prompt/goal review, delegated execution wave, and auditor promotion gates while keeping Forge workflow state authoritative.
- Executor policy: Gemini is legacy-invalidated by default for teamwork planning; Codex and `agy` are the primary modern agent paths, with OpenCode as an additional allowed fallback.
- Multi-Agent execution runtime: orchestrates role-based task execution, simulates or runs sub-processes/APIs, handles handoffs/audits, and records cost, token, and lineage metadata.
- Persistence: stores cached web benchmark data and execution lineage in SQLite.

## Code Layout
- `src/cli_factory.rs` / `src/main.rs`: subcommand parsing and CLI entry points.
- `src/execution.rs` / `src/runtime.rs`: execution orchestration, dynamic roster planning, and benchmark consolidation.
- `tests/`: integration and E2E tests for the subcommand and heuristics.

## Milestones
### Implementation Track
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| I1 | CLI Parsing & Boilerplate | Implement CLI arguments, parsing, and subcommand wiring. | None | DONE |
| I2 | Roster Heuristics & Benchmark Consolidation | Implement task graph decomposition, mapping rules, and web benchmark retrieval/ranking. | I1 | DONE |
| I3 | Multi-Agent Execution & Lineage | Implement the orchestrator driver, task handoffs, audits, and metrics recording. | I2 | DONE |
| I4 | Final Integration & Test Pass | Pass 100% E2E tests and perform adversarial hardening. | I3, T3 | DONE |

### E2E Testing Track
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| T1 | Test Infra & Tier 1 Coverage | Establish E2E test harness and basic subcommand feature tests. | None | DONE |
| T2 | Tier 2 & 3 Boundary & Interaction | Implement boundary, error handling, and cross-feature interaction tests. | T1 | DONE |
| T3 | Tier 4 Real-World Application | Implement complex goal execution scenarios. Publish TEST_READY.md. | T2 | DONE |

## Interface Contracts
- `forge teamwork --goal "<goal>"`: Entry point. Outputs JSON or human-readable format.
- Benchmark Cache DB Schema: SQLite table for cached rankings/metrics.
- Executed Task Lineage: records roles, brain, execution time, token counts, and cost metrics.
