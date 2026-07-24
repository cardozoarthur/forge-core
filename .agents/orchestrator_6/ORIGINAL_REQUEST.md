# Original User Request

## 2026-07-04T10:37:11Z

<USER_REQUEST>
The goal is to design and implement a multi-agent teamwork orchestration subcommand (`forge teamwork`) inside the Forge Core runtime (`forge-core`), modeled after the `teamwork-preview` system of the `agy` CLI, enabling goal planning, dynamic agent role slot allocation, and automated execution using different backend brains (like `codex` or `agy`).

Working directory: /home/arthur/projects/forge-core
Integrity mode: benchmark

## Requirements

### R1. Subcommand Implementation: `forge teamwork`
Add a new top-level subcommand `forge teamwork` to the Forge CLI:
- Accept a `--goal "<goal>"` argument (required).
- Accept an optional `-d` / `--detached` flag to run the execution in the background.
- Support standard `--output` formats (`human`, `json`).

### R2. Dynamic Roster & Brain Allocation Heuristics (with Web-Sourced Benchmark Consolidation)
When planning the goal:
- Decompose the goal into task dependency graphs.
- Analyze the tasks to dynamically determine a Team Roster (e.g. Orchestrator, Worker agents, Auditors).
- Automatically allocate brain executors based on task characteristics:
  - Coding/Implementation tasks -> Assign to `codex` / `opencode` brain.
  - Frontend/Interface tasks -> Allow allocating to `antigravity` (`agy`) or other UI-specialized executors.
  - Planning, Coordination, Context Routing, and Audit/Verification tasks -> Assign to `antigravity` (`agy`) or `gemini` brain.
- **Dynamic Selection via Web Benchmarks**: Integrate a benchmarking comparison utility that retrieves/references consolidated public LLM benchmarks from the web (e.g., LMSYS Chatbot Arena, HumanEval, MMLU rankings). Support fetching or caching these benchmark comparisons to rank available provider candidates dynamically and select the highest-performing brain for each task type.
- Expose the roster, retrieved web benchmark scores, and task allocations in the planned output metadata.

### R3. Multi-Agent Execution Orchestration
Execute the workflow by orchestrating the multi-agent team:
- Spin up/drive the tasks through the assigned executor brains (e.g., executing Codex or `agy` as subprocesses or API endpoints).
- Model task handoffs, audits, and validations between the roles.
- Record the role assignments, cost/token metrics, and lineage for each task execution.

## Acceptance Criteria

### Verification & Delivery
- [ ] `cargo test` and `cargo clippy --all-targets --all-features -- -D warnings` run and pass.
- [ ] Subcommand `forge teamwork --goal "..."` plans, shows the roster (specifying agent roles and their allocated brains), and starts execution.
- [ ] Integration tests verify that task type analysis correctly maps coding tasks to code-specialized brains (`codex`) and audit/routing tasks to agentic/coordination brains (`agy`).
- [ ] Integration tests verify that the system consolidated and processed web benchmark data (e.g., LMSYS, MMLU, HumanEval) to dynamically rank and select the best brain candidate.
- [ ] Sane defaults and smoke tests verify that the multi-agent execution pipeline runs to completion or runs simulated step-by-step correctly.
</USER_REQUEST>

## 2026-07-04T10:37:37Z

You are Project Orchestrator (orchestrator_6). Your working directory is /home/arthur/projects/forge-core/.agents/orchestrator_6. Your task is to coordinate the design and implementation of the multi-agent teamwork orchestration subcommand (`forge teamwork`), dynamic roster brain allocation heuristics with web-sourced benchmark consolidation, and execution orchestration. Read /home/arthur/projects/forge-core/.agents/orchestrator_6/ORIGINAL_REQUEST.md for the full requirements. Follow all orchestration rules, including creating plan.md, progress.md, and PROJECT.md, spawning explorers and workers, running cargo clippy, fmt, and tests, and running the victory auditor.
