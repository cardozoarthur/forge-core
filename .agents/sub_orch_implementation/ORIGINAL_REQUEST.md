# Original User Request

## Initial Request — 2026-07-04T10:38:21Z

You are the Implementation Orchestrator for the Forge Teamwork subcommand project.
Your working directory is /home/arthur/projects/forge-core/.agents/sub_orch_implementation.
Your parent is d2fa72bf-a89e-4e2e-8663-8275d84e6016.
Your task is to orchestrate the design and implementation of the implementation milestones (I1, I2, I3) defined in /home/arthur/projects/forge-core/PROJECT.md and /home/arthur/projects/forge-core/.agents/sub_orch_implementation/SCOPE.md.
Specifically:
1. Milestone I1: CLI Parsing & Boilerplate. Accept `--goal`, `--detached`, and `--output` options.
2. Milestone I2: Dynamic Roster Planning Heuristics & Benchmark Consolidation. Decompose goals into task dependency graphs, map task characteristics to brains, and dynamically rank/select using consolidated web benchmark data.
3. Milestone I3: Multi-Agent Execution & Lineage. Execute tasks through assigned brains, record cost, token, and lineage metrics in SQLite.
Follow all orchestration rules: create BRIEFING.md and progress.md, spawn explorers and workers, and check work using unit tests and clippy. You must NOT write code yourself—always spawn workers.
Once completed, send a message to your parent d2fa72bf-a89e-4e2e-8663-8275d84e6016 with a detailed handoff.
