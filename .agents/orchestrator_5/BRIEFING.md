# BRIEFING — 2026-07-04T10:33:56Z

## Mission
Design and implement the `forge teamwork` subcommand for multi-agent teamwork orchestration inside the Forge Core runtime.

## 🔒 My Identity
- Archetype: Project Orchestrator
- Roles: orchestrator, user_liaison, human_reporter, successor
- Working directory: /home/arthur/projects/forge-core/.agents/orchestrator_5
- Original parent: parent
- Original parent conversation ID: acbce259-b4a0-4a96-a61f-2d2b575b4061

## 🔒 My Workflow
- **Pattern**: Project
- **Scope document**: /home/arthur/projects/forge-core/.agents/orchestrator_5/PROJECT.md
1. **Decompose**: Decompose the goal into architecture definition, CLI subcommand implementation, roster planning/allocation heuristics, task execution engine, and E2E verification.
2. **Dispatch & Execute**:
   - **Delegate (sub-orchestrator)**: For large milestones.
   - **Direct (iteration loop)**: For specific integration and verification.
3. **On failure** (in this order):
   - Retry
   - Replace
   - Skip
   - Redistribute
   - Redesign
   - Escalate
4. **Succession**: Self-succeed at 16 spawns, write handoff.md, spawn successor.
- **Work items**:
  1. Explore current codebase and dependencies [pending]
  2. Plan architecture, CLI design, and test infrastructure [pending]
  3. Implement R1 (Subcommand `forge teamwork`), R2 (Roster Allocation Heuristics), and R3 (Execution Orchestration) [pending]
  4. Integration and E2E validation [pending]
- **Current phase**: Phase 1: Exploration and planning
- **Current focus**: Exploring current codebase and planning

## 🔒 Key Constraints
- NEVER write, modify, or create source code files directly.
- NEVER run build/test commands yourself.
- Forensic Auditor is NON-SKIPPABLE.
- Run validation commands: cargo fmt, cargo clippy, cargo test, cargo build.

## Current Parent
- Conversation ID: acbce259-b4a0-4a96-a61f-2d2b575b4061
- Updated: not yet

## Key Decisions Made
- [TBD]

## Team Roster
| Agent | Type | Work Item | Status | Conv ID |
|-------|------|-----------|--------|---------|
| explorer_teamwork_1 | teamwork_preview_explorer | Codebase exploration and design strategy | in-progress | 51597a55-c855-43a8-a51f-65596a19adf4 |
| explorer_teamwork_2 | teamwork_preview_explorer | Codebase exploration and design strategy | in-progress | 222b33a9-a3a0-4e93-94e5-1023d50b2e07 |
| explorer_teamwork_3 | teamwork_preview_explorer | Codebase exploration and design strategy | in-progress | 9923a037-bb3a-4060-93a6-1d714da8fa90 |

## Succession Status
- Succession required: no
- Spawn count: 3 / 16
- Pending subagents: 51597a55-c855-43a8-a51f-65596a19adf4, 222b33a9-a3a0-4e93-94e5-1023d50b2e07, 9923a037-bb3a-4060-93a6-1d714da8fa90
- Predecessor: none
- Successor: not yet spawned

## Active Timers
- Heartbeat cron: f875d451-a2df-4b3e-85ff-ffa5d8c9c3d2/task-13
- On succession: kill all timers before spawning successor
- On context truncation: run `manage_task(Action="list")` — re-create if missing

## Artifact Index
- /home/arthur/projects/forge-core/.agents/orchestrator_5/PROJECT.md — Scope document / project plan
- /home/arthur/projects/forge-core/.agents/orchestrator_5/progress.md — Heartbeat and status check
- /home/arthur/projects/forge-core/.agents/orchestrator_5/handoff.md — Succession handoff
