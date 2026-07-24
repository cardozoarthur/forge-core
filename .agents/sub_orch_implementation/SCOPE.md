# Scope: Implementation Track for Forge Teamwork

## Architecture
- Implementation of `forge teamwork` subcommand, brain allocation rules, web benchmark fetching/ranking/caching, and multi-agent execution loop with lineage logging.

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| I1 | CLI Parsing & Boilerplate | Implement CLI arguments, parsing, and subcommand wiring. | None | DONE |
| I2 | Roster Heuristics & Benchmark Consolidation | Implement task graph decomposition, mapping rules, and web benchmark retrieval/ranking. | I1 | DONE |
| I3 | Multi-Agent Execution & Lineage | Implement the orchestrator driver, task handoffs, audits, and metrics recording. | I2 | DONE |
| I4 | Final Integration & Test Pass | Pass 100% E2E tests and perform adversarial hardening. | I3 | PLANNED |

## Interface Contracts
- CLI `forge teamwork --goal "<goal>"`: Entry point.
- SQL table for caching web benchmark data.
- SQL table / metrics for task execution lineage.
