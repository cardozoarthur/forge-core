# Project: Forge Ecosystem Integration and Strategy

## Architecture
- `forge-core`: The universal, domain-agnostic Rust workflow runtime. Controls workflow authority: decomposition, graph state, context, scheduling, validation, artifacts, memory, etc.
- `forge-flow`: Workflow automation/adapter layers.
- `forge-crm`: CRM/identity routing and integration adapters.
- `forge-desktop`: Lightweight desktop dashboard querying active workflows and providing a stunning web TUI operational console.
- `antigravity` (CLI: `agy`): Bounded task executor. Bidirectional integration allows:
  - Forge to call `agy` as an executor.
  - `agy` agents to call `forge` CLI via the loaded `forge` skill.

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| 1 | Strategic Analysis | Verify existing `forge_strategic_report.md` and update with `forge-desktop` details | None | IN_PROGRESS |
| 2 | Bidirectional Integration | Verify `antigravity` integration in `src/executor.rs`/`src/milestone.rs` and skill file | None | PLANNED |
| 3 | Telegram Delivery | Deliver report via simulated Telegram egress mode and verify delivery record | M1, M2 | PLANNED |
| 4 | Skills Expansion | Verify and improve domain skills under `.agents/skills/` | M1, M2, M3 | PLANNED |
| 5 | Detached Workflow Execution | Implement `-d`/`--detached` flag on `forge plan` and `forge request start` | None | PLANNED |
| 6 | Forge Desktop Dashboard | Create `forge-desktop` Node.js server and HTML dashboard on port 8080 | M5 | PLANNED |
| 7 | Final Verification & Audit | Run formatting, clippy, tests, and Forensic Audit | M6 | PLANNED |

## Interface Contracts
### Forge ↔ Antigravity (CLI)
- Forge calls `agy` via process execution with `--version` or task packets.
- `agy` agents call `forge` commands via the skill definition in `/home/arthur/.gemini/config/skills/forge/SKILL.md`.

### Forge ↔ Forge Desktop
- Node.js server executes `forge list --output json` with `cwd: /home/arthur/projects/forge-core` to retrieve active workflows.
- Web dashboard presents the JSON data reactively.
