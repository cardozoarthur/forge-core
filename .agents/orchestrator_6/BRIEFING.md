# BRIEFING — 2026-07-04T10:37:37Z

## Mission
Design and implement the `forge teamwork` subcommand, dynamic roster brain allocation, and multi-agent execution orchestration.

## 🔒 My Identity
- Archetype: teamwork_preview_orchestrator
- Roles: orchestrator, user_liaison, human_reporter, successor
- Working directory: /home/arthur/projects/forge-core/.agents/orchestrator_6
- Original parent: parent
- Original parent conversation ID: 879d4aeb-3cd6-4607-a1dc-e64b0e3994ea

## 🔒 My Workflow
- **Pattern**: Project
- **Scope document**: /home/arthur/projects/forge-core/PROJECT.md
1. **Decompose**: Decompose the requirements into independent milestones.
2. **Dispatch & Execute**:
   - **Delegate (sub-orchestrator)**: For large milestones.
   - **Direct (iteration loop)**: Explorer -> Worker -> Reviewer -> Challenger -> Auditor -> Gate.
3. **On failure** (in this order):
   - Retry: nudge stuck agent or re-send task
   - Replace: spawn fresh agent with partial progress
   - Skip: proceed without (only if non-critical)
   - Redistribute: split stuck agent's remaining work
   - Redesign: re-partition decomposition
   - Escalate: report to parent (sub-orchestrators only, last resort)
4. **Succession**: Self-succeed at 16 spawns.
- **Work items**:
  1. Decompose requirements and design PROJECT.md [pending]
  2. Subcommand CLI boilerplate (`forge teamwork`) [pending]
  3. Dynamic roster & brain allocation heuristics [pending]
  4. Web-sourced benchmark consolidation [pending]
  5. Multi-agent execution orchestration [pending]
  6. E2E test track & verification [pending]
  7. Final integration, hardening, & verification [pending]
- **Current phase**: 1
- **Current focus**: Decompose requirements and design PROJECT.md

## 🔒 Key Constraints
- NEVER write, modify, or create source code files directly.
- NEVER run build/test commands yourself — require workers to do so.
- Integrity mode: benchmark.
- All implementations must be genuine. No hardcoding or dummy logic.

## Current Parent
- Conversation ID: 879d4aeb-3cd6-4607-a1dc-e64b0e3994ea
- Updated: not yet

## Key Decisions Made
- Use Project pattern with parallel Implementation and E2E Testing tracks.

## Team Roster
| Agent | Type | Work Item | Status | Conv ID |
|-------|------|-----------|--------|---------|
| implementation_orch | teamwork_preview_orchestrator | Implementation Track (I1-I3) | completed | 73b36158-af0a-4ca8-bd02-524e45daa89a |
| testing_orch | teamwork_preview_orchestrator | E2E Testing Track (T1-T3) | completed | 6be33a06-3bee-4789-9527-65841a1d8b4a |
| final_worker | teamwork_preview_worker | Final verification and builds | completed | 70cb82a1-1a56-45de-8c77-a0a27745214a |
| final_auditor | teamwork_preview_auditor | Forensic integrity verification | completed | a9b5b341-e623-4163-b2e5-21843f4a9117 |

## Succession Status
- Succession required: no
- Spawn count: 4 / 16
- Pending subagents: none
- Predecessor: none
- Successor: not yet spawned

## Active Timers
- Heartbeat cron: d2fa72bf-a89e-4e2e-8663-8275d84e6016/task-13
- Safety timer: none
- On succession: kill all timers before spawning successor
- On context truncation: run `manage_task(Action="list")` — re-create if missing

## Artifact Index
- `/home/arthur/projects/forge-core/.agents/orchestrator_6/ORIGINAL_REQUEST.md` — Original request requirements
- `/home/arthur/projects/forge-core/.agents/orchestrator_6/BRIEFING.md` — Orchestrator memory and status
