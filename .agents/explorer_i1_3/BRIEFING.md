# BRIEFING — 2026-07-04T07:41:20-03:00

## Mission
Investigate how `forge` CLI parsing is implemented and design the `teamwork` subcommand.

## 🔒 My Identity
- Archetype: Explorer
- Roles: Read-only investigator
- Working directory: /home/arthur/projects/forge-core/.agents/explorer_i1_3
- Original parent: 73b36158-af0a-4ca8-bd02-524e45daa89a
- Milestone: CLI parsing investigation

## 🔒 Key Constraints
- Read-only investigation — do NOT implement
- Analyze main.rs and other CLI parsing files
- Recommend design/fix strategy for `teamwork` subcommand

## Current Parent
- Conversation ID: 73b36158-af0a-4ca8-bd02-524e45daa89a
- Updated: 2026-07-04T07:39:00-03:00

## Investigation State
- **Explored paths**:
  - `src/main.rs` (analyzed CLI parsing with `Clap` derive, `Commands` enum, and command dispatch in `run()`)
  - `src/cli_factory.rs` (analyzed command-level CLI planning/creation spec)
  - `src/graph.rs` (analyzed `NodeBrainAgentSlotSpec` and `NodeBrainRoutingSpec` related to agent slot rosters)
  - `src/request.rs` (analyzed request/run driver orchestration, step execution, and background drive-loop spawning)
- **Key findings**:
  - `forge` parses arguments using Clap derive macros. Adding a subcommand involves adding a variant to `Commands` enum in `src/main.rs`.
  - Stubs/interfaces can be registered in `src/runtime.rs` or `src/execution.rs` for teamwork planning and heuristics.
  - Background driving mechanism is already in place via `RequestCommands::DriveLoop` spawning `std::process::Command` pointing to self.
- **Unexplored areas**:
  - Milestone I2's heuristics/LMSYS benchmark retrieval (which will build on top of our subcommand structure).

## Key Decisions Made
- Outlined a design utilizing stubs in `src/runtime.rs` for Milestone I1.
- Proposed extending `Commands::Teamwork` subcommand with standard arguments matching existing subcommands.

## Artifact Index
- /home/arthur/projects/forge-core/.agents/explorer_i1_3/analysis.md — Report containing CLI parsing structure and design for teamwork subcommand.
