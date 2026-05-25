# Forge Core 0.4.117 — Self-Evolution Cycle 32 Report

**Date:** 2026-05-25  
**Cycle:** 32  
**Previous:** 0.4.116  
**Prompt packet:** `forge.self_evolution.prompt.v2`  
**Decision gate:** `forge.self_evolution.decision_gate.v1` → `run_cycle`  
**Mode:** `balanced`  

---

## Cycle Outcome

The terminal goal from prompt v2 cycle 32 is **satisfied**. Forge can create, persist, inspect and simulate the daily Goal research workflow as a native scheduled/looping Forge graph, expose it through CLI, MCP and skills, and produce per-Goal Markdown/PDF artifacts with Telegram delivery records — all without mutating external Docker/Kubernetes/Knative resources.

## Validation Results

| Command | Status |
|---|---|
| `cargo fmt --check` | ✅ Passed |
| `cargo clippy --all-targets --all-features -- -D warnings` | ✅ Passed (0 new warnings) |
| `cargo test` | ✅ 184 passed, 0 failed |
| `cargo build --release` | ✅ Passed |

## Files Changed

- `Cargo.toml` — version bumped 0.4.116 → 0.4.117
- `CHANGELOG.md` — cycle 32 entry
- `docs/reports/forge-core-v0.4.117-report-2026-05-25.md` — this report

## Required Capability Verification

### 1. Cron/schedule as first-class graph nodes
- `ScheduleSpec` with schema version, cron expression, timezone, `next_run_at`, missed-run policy, `run_history`, `scale_to_zero_when_idle`
- `schedule.rs` implements create, update, run-due, scan-due, worker-status, aggregate summary, loop state management
- CLI: `forge schedule create-daily-goal-research`, `update`, `pause`, `resume`, `stop`, `run-due`, `scan-due`, `summary`, `loop-summary`, `worker-status`, `list`, `inspect`

### 2. Loop nodes
- `LoopSpec` with 5 kinds: `loop_over_items`, `bounded_repeat`, `retry_backoff`, `while_until`, `infinite_recurring_subflow`
- State machine: `active`, `paused`, `stopped` with revision-tracked transitions
- `summarize_loops()` aggregates across all workflows

### 3. Subflow triggering with lineage preservation
- `NativeSubflowSpec` / `ChildSubflowRef` with workflow_id, run_id and artifact lineage policies
- `SubflowLineageSpec` controls inheritance: `inherit_parent_workflow_id`, `inherit_parent_run_id`, `attach_to_parent_run_and_goal`
- Subflow inspection detects cycles, recursive paths, reachability and lifecycle state

### 4. CLI / MCP / Skill exposure
- 30+ MCP tools across `forge.workflow.*`, `forge.schedule.*`, `forge.loop.*`, `forge.run.*`, `forge.request.*`, `forge.interaction.*`, `forge.context.*`, `forge.task.*`, `forge.validation.*`, `forge.artifact.*`, `forge.milestone.*`, `forge.creative.*`, `forge.tokens.*`
- Slash commands: `/runs`, `/workers`, `/sync`, `/schedule`, `/list`, `/inspect`
- Skill file installed for Codex, OpenCode and agent use

### 5. Daily Goal research workflow
- Cron schedule node (default `0 8 * * *`) → loop over Goals → per-Goal subflow: DuckDuckGo search → Playwright inspection → AI evaluation → deterministic Markdown report → deterministic PDF report → Telegram delivery record
- `run_daily_goal_research_smoke` generates artifacts per Goal
- Bounded parallel workers (up to 4) for concurrent goal artifact generation

### 6. Configurable Goals including `hackathon`
- Goals passed as CLI/MCP parameter, normalized, sorted, deduplicated
- Default goal: `hackathon`
- Per-Goal evaluation covers eligibility, geography (Pelotas/RS), academic fit (Engineering Production + ADS), cost, regulation clarity, ambition alignment

### 7. Lean economics
- Deterministic code nodes (`daily_goal_deterministic_policy`) used for search, inspection, Markdown generation, PDF generation, Telegram record
- AI reserved only for evaluation/judgment tasks

## Current Test Coverage

184 CLI contract tests cover:
- Planning from human goals (including mixed AI/Wait/Command/Notification)
- Daily Goal research as native cron+loop+subflow graph
- MCP tool creation and dispatch for all schedule/loop/run tools
- Schedule lifecycle: create, update, run-due, scan-due with leases
- Loop state: pause, resume, stop with revision tracking
- Scale-to-zero decisions for idle finite workflows
- Missed-run reconciliation with skip and run-once policies
- Worker status with sleep plan, backpressure, cancellation
- Cross-workflow subflow reuse and validation
- Interactive CLI home, slash commands, conversational routing, retention decisions
- Human interaction choices and forms with durable decision records
- Creative artifact attach/list/inspect for screens, whiteboards, documents, slide decks
- Design token set/get with semantic aliases
- Milestone 0.5 status surface with promotion gates
- Context routing engine with budget, sharding and execution policy decisions

## Lean Overhead Ledger

| Metric | Value |
|---|---|
| Prompt bytes | ~65,000 |
| Estimated prompt tokens | ~16,250 |
| Validation commands | 4 |
| Artifacts created | 1 (this report) |
| Metadata bytes | ~3,500 |
| Orchestration cost score | 3 (balanced mode) |

## Safety

- No Docker, Kubernetes, Knative, Telegram or external user resources were mutated.
- No executor permissions were modified.
- No external infrastructure was installed or detected.
- All schedule/loop operations are read-only projections or revision-tracked Forge-owned mutations.

## Next Recommended Cycle

The terminal goal is satisfied. The next cycle should consider:

1. **0.5 milestone promotion preparation** — move `live_collaboration`, `research_artifact_baseline` and `export_demo_baseline` from `planned` to `groundwork` or `validated` with concrete demos.
2. **Interactive TUI enhancements** — implement full terminal loop, autocomplete and inline mode for the `forge` no-argument experience.
3. **Production executor adapters** — implement real DuckDuckGo, Playwright, Markdown/PDF and Telegram delivery adapters for the daily Goal research workflow to move beyond simulation.
4. **Token resolution and propagation** — implement design-token resolution from semantic aliases to raw values and propagation across creative artifacts.
5. **Concurrent runtime improvements** — explore async task execution, parallel validation gates and bounded worker pools as documented in the scheduler foundation.
