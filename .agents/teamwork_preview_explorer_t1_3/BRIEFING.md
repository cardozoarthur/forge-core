# BRIEFING — 2026-07-04T10:44:31Z

## Mission
Explore multi-agent execution runtime and SQLite schema to design E2E test cases for Feature 3 and 4.

## 🔒 My Identity
- Archetype: Teamwork explorer
- Roles: Read-only investigator
- Working directory: /home/arthur/projects/forge-core/.agents/teamwork_preview_explorer_t1_3
- Original parent: 6be33a06-3bee-4789-9527-65841a1d8b4a
- Milestone: Preview explorer t1_3

## 🔒 Key Constraints
- Read-only investigation — do NOT implement
- CODE_ONLY network mode - no external network requests or HTTP clients
- Write only to my folder (/home/arthur/projects/forge-core/.agents/teamwork_preview_explorer_t1_3)

## Current Parent
- Conversation ID: 6be33a06-3bee-4789-9527-65841a1d8b4a
- Updated: 2026-07-04T10:44:31Z

## Investigation State
- **Explored paths**:
  - `src/main.rs` (CLI commands: `plan`, `run`, `request`, `improve`)
  - `src/storage.rs` (SQLite DB schema definitions: `workflows`, `artifacts`, `runs`, `cost_ledger_index`, `event_observability_index`, `task_checkpoints`, `task_leases`, `runtime_contract_dispatches`)
  - `src/runtime.rs` (Substrate runtime syncing and policy controls: Docker, K8s, Knative)
  - `src/execution.rs` (Parallel simulation engine and cost estimation structures)
  - `src/request.rs` (Execution driver loop: `step_request`, `complete_ready_task`, auto-stepping logic, execution trace payload builder)
  - `src/adapter.rs` (Executor response validation and workflow promotion)
  - `src/improve.rs` (Scoring and candidate ranking algorithm)
  - `tests/forge_cli_contract.rs` (Integration test references and existing tests)
- **Key findings**:
  1. **Detached Execution (`--detached`)**: Spawns a background process running `request drive-loop --run <run_id>`. The drive-loop calls `step_request` in a sleep loop.
  2. **Simulated vs Real**: Simulated mode runs in memory waves using thread spawning. Real mode drives through the DB state machine using `step_request` (auto-steps deterministic nodes) and stops on cognitive tasks with `handoff_required`.
  3. **Handoff & Completion**: Resolved via `complete_ready_task` which receives the executor's evidence, generates an execution trace artifact, validates the response format/artifacts, and updates the DB state.
  4. **Cost & Token tracking**: Realized via `cost_ledger_index` table which compiles actual costs (`tokens_in`, `tokens_out`, `observed_event_cost_usd`) vs estimated cost (`estimated_task_cost_usd`).
  5. **Lineage tracking**: Tracked using `event_observability_index` for context budgets and `task_checkpoints` for state preservation.
- **Unexplored areas**: None.

## Key Decisions Made
- Compose a comprehensive E2E test plan detailing all required assertions and test scripts.
- Write findings to `/home/arthur/projects/forge-core/.agents/teamwork_preview_explorer_t1_3/handoff.md`.

## Artifact Index
- /home/arthur/projects/forge-core/.agents/teamwork_preview_explorer_t1_3/handoff.md — Detailed analysis and recommended test plan
