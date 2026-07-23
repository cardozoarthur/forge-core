# Plan — Project Orchestrator (orchestrator_5)

This plan outlines the milestones and orchestration path for implementing `forge teamwork` subcommand, dynamic roster allocation, and execution orchestration.

## Objectives
1. **R1: Subcommand Implementation `forge teamwork`**: Add a top-level subcommand accepting `--goal`, `-d` / `--detached`, and `--output` formats.
2. **R2: Dynamic Roster & Brain Allocation Heuristics**: Decompose the goal, determine the roster (Orchestrator, Worker, Auditor), and automatically assign executor brains (`codex` for coding; `agy` or `gemini` for coordination/audit).
3. **R3: Multi-Agent Execution Orchestration**: Implement workflow execution, task handoffs, audits, validations, and record execution metrics (cost, token, lineage).

## Milestones & Decomposition

| Milestone | Name | Objective | Strategy / Worker |
|-----------|------|-----------|-------------------|
| M1 | Subcommand Definition & CLI Parsing | Add `teamwork` command with clap arguments, route command in `src/main.rs`. | Spawn Worker |
| M2 | Dynamic Roster & Heuristics | Analyze goal/tasks to allocate agent slots with `codex` or `agy`/`gemini` brains based on task type. | Spawn Worker |
| M3 | Execution Orchestration Engine | Implement multi-agent execution loop, role handoffs, mock/simulated brain execution, and metrics/lineage tracking. | Spawn Worker |
| M4 | Verification & Audit | Add integration tests, run `cargo test`, `cargo clippy`, and run Forensic Auditor for final clean audit. | Spawn Worker / Auditor |

## Verification Plan
1. Integration tests covering subcommand parsing, roster output, and executor brain mapping.
2. Build, fmt, clippy, and unit tests passing successfully.
3. Forensic Auditor check passing cleanly with no integrity violations.
