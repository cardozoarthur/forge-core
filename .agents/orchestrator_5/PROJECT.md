# Project: Forge Core Teamwork Orchestration

## Architecture
- `forge-core`: Rust workflow engine. We are adding the `teamwork` subcommand.
- `teamwork` subcommand:
  - CLI: `forge teamwork --goal "<goal>" [-d] [--output <format>]`
  - Heuristics: Goal decomposition into a task DAG. Analysis of tasks to dynamically assign agent roles (Orchestrator, Worker, Auditor) and their brains (`codex` / `opencode` for code, `agy` / `gemini` for coordination & verification).
  - Orchestration: Multi-agent execution loop driving task execution, handoffs, audits, logging cost/token metrics, and tracking lineage.

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| 1 | CLI & Parsing | Implement CLI parser and subcommand routing | None | PLANNED |
| 2 | Roster & Brain Allocation | Design and implement the heuristic rules to classify tasks and allocate specialized brains | M1 | PLANNED |
| 3 | Multi-Agent Execution | Implement the orchestration loop, handoffs, cost tracking, and lineage recording | M2 | PLANNED |
| 4 | Verification & Audit | Write integration tests and run cargo checks / forensic auditor | M3 | PLANNED |

## Interface Contracts
### CLI input/output formats
- Command: `forge teamwork --goal "<goal>" [--detached] [--output human|json]`
- Output contains planned roster, agent assignments, brain allocations, task graph, and execution metadata.
