# Plan - Forge Teamwork Subcommand

This plan outlines how Project Orchestrator `orchestrator_6` will manage the implementation and testing of the `forge teamwork` subcommand, dynamic roster brain allocation heuristics, web-sourced benchmark consolidation, and execution orchestration.

## Execution Tracks
We run two parallel tracks to ensure clean separation of concerns and robust test-driven verification:
1. **Implementation Track**: Handled by a dedicated sub-orchestrator (`implementation_orch`). Implements the production CLI, heuristics, web benchmark cache, and execution loop.
2. **E2E Testing Track**: Handled by a dedicated sub-orchestrator (`testing_orch`). Builds the E2E test harness and validates the requirements.

## Coordination & Sync
- The E2E Testing Track will publish `TEST_READY.md` when the test suite is complete.
- The Implementation Track will run the test suite against its work.
- The final milestone (I4) will be executed when both tracks have met their respective pre-requisites.

## Subagent Roster
- `implementation_orch`: A sub-orchestrator (spawns with `self`) to manage I1, I2, I3 milestones.
- `testing_orch`: A sub-orchestrator (spawns with `self`) to manage T1, T2, T3 milestones.
- `teamwork_preview_auditor`: Runs final forensics verification checks.

## Milestones and Status
- [ ] M0: Initialize Plan & PROJECT.md (Completed)
- [ ] M1: Spawn Implementation Track & E2E Testing Track Sub-orchestrators
- [ ] M2: Monitor Progress & Synchronize
- [ ] M3: Final Integration & Pass 100% E2E tests
- [ ] M4: Run Forensic Auditor Verification
