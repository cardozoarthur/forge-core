# Execution Plan — Forge Ecosystem Integration and Strategy (R1-R6)

This document outlines the step-by-step execution and verification plan for requirements R1 through R6.

## Decomposition & Milestones

### Milestone 1: Planning & Setup [In Progress]
- **Objective**: Initialize BRIEFING.md, progress.md, and compile plan.md. Update PROJECT.md at root.
- **Verification**: Files exist and are consistent.

### Milestone 2: Verification of R1–R4 [Pending]
- **Objective**: Spawn a worker subagent to verify:
  - R1: Review `forge_strategic_report.md` for accuracy and formatting.
  - R2: Verify bidirectional integration of `antigravity` (`agy`) in `src/executor.rs` and `src/milestone.rs` and the `forge` skill configuration.
  - R3: Verify Telegram notification delivery and mock delivery record.
  - R4: Verify the 5 domain skill folders exist and contain valid instructions under `.agents/skills/`.
- **Verification**: Worker reports success. Cargo tests and clippy for existing code pass.

### Milestone 3: Detached Workflow Execution (R5) [Pending]
- **Objective**: Implement `-d` / `--detached` on `forge plan` and `forge request start`.
  - Add `detached: bool` flag to CLI arguments for `Plan` and `RequestCommands::Start` in `src/main.rs`.
  - When `--detached` is set:
    - Spawn a background `forge request drive --run <run_id>` process using `std::process::Command` with stdio redirected to null.
    - Save workflow/run records in the SQLite database first.
    - Output JSON/Human response indicating background drive has started.
- **Verification**: Spawn worker to implement and run `cargo test`, `cargo clippy`, and manual check.

### Milestone 4: Forge Desktop Dashboard (R6) [Pending]
- **Objective**: Create `forge-desktop` at `/home/arthur/projects/forge-desktop`.
  - Implement a Node.js server (`server.js` using only built-in modules) that spawns `forge list --output json` (setting cwd to `/home/arthur/projects/forge-core`) and serves the results on `http://localhost:8080/api/workflows`.
  - Implement a stunning, dark-mode, glassmorphism HTML/CSS/JS frontend page (`index.html`) featuring:
    - Animations, modern typography, grid/flex layouts.
    - Live updates of active/running workflows, showing their goals, status, and progress.
- **Verification**: Spawn worker to create project, run server on port 8080, and verify query output.

### Milestone 5: Code Review & Quality Gates [Pending]
- **Objective**: Spawn reviewer agents to review implementation for detached execution (R5) and the Node.js desktop server/frontend (R6).
- **Verification**: Reviewer handoffs with clean approval.

### Milestone 6: Adversarial Testing & Verification [Pending]
- **Objective**: Spawn challenger to test:
  - Background processes spawn correctly and continue running after the CLI exits.
  - Web dashboard correctly serves active workflows.
- **Verification**: Challenger verifies detached execution process exists in OS process list and Node.js server responds with valid JSON on api endpoint.

### Milestone 7: Forensic Audit & Victory Report [Pending]
- **Objective**: Spawn a Forensic Auditor to ensure no cheating, faking, or bypasses were done.
- **Verification**: Forensic Auditor reports CLEAN.

### Milestone 8: Reporting & Handoff [Pending]
- **Objective**: Compile final handoff.md and notify Sentinel (parent) of completion.
- **Verification**: Sentinel confirms victory.
