# Forge Core 0.4.120 - Self-Evolution Cycle 35 Report

**Date:** 2026-05-25  
**Cycle:** 35  
**Previous:** 0.4.119  
**Prompt packet:** `forge.self_evolution.prompt.v2`  
**Decision gate:** `forge.self_evolution.decision_gate.v1` -> `run_cycle`  
**Mode:** `balanced`

## Cycle Outcome

Forge scheduler worker status now exposes a deterministic bounded-worker assignment plan before any due workflow is leased or executed.

`forge schedule worker-status --output json` and MCP tool `forge.schedule.worker_status` now include `worker_pool.assignment_plan` with schema `forge.schedule.assignment_plan.v1`. The plan separates due scheduled workflows into:

- `assigned`: workflows that fit the current `max_workers` capacity;
- `queued`: due workflows left behind under backpressure;
- deterministic ordering metadata keyed by `next_run_at,workflow_id,schedule_task_id`.

This keeps Forge as the owner of cron/loop scheduling semantics and prevents agents from inventing ad hoc tmux loops when they need to reason about scheduler capacity.

This is `0.5 groundwork` for scheduler/runtime concurrency. It does not claim that the Forge 0.5 creative runtime is complete.

## Validation Results

| Command | Status |
|---|---|
| `cargo fmt --check` | Passed |
| `cargo clippy --all-targets --all-features -- -D warnings` | Passed |
| `cargo test` | Passed: 15 unit tests, 186 CLI contract tests, 0 doctests |
| `cargo build --release` | Passed |

## Smoke Results

| Smoke | Status |
|---|---|
| `./target/release/forge --store /tmp/forge-core-v04120-cycle35-plan.sqlite plan --goal "Create a delivery platform" --output json` | Passed |
| `./target/release/forge --store /tmp/forge-core-v04120-cycle35-skill.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v04120-cycle35` | Passed |
| Native daily `hackathon` Goal due-run | Passed: 1 Markdown report, 1 PDF report and 1 Telegram delivery record; `secret_exposed=false` |
| `forge schedule worker-status` on the due daily workflow | Passed: one due workflow assigned, no queued work with `max_workers=3`, deterministic assignment plan emitted |

Daily Goal smoke artifact paths:

- `/tmp/artifacts/wf_2f644690e4cd412396d77212d6197042/goal-hackathon-report.md`
- `/tmp/artifacts/wf_2f644690e4cd412396d77212d6197042/goal-hackathon-report.pdf`
- `/tmp/artifacts/wf_2f644690e4cd412396d77212d6197042/telegram-delivery-hackathon.json`

## Files Changed

- `Cargo.toml` / `Cargo.lock` - version bumped to `0.4.120`.
- `src/schedule.rs` - added assignment plan structs and deterministic worker-status assignment planning.
- `tests/forge_cli_contract.rs` - added CLI and MCP contract assertions for assignment-plan visibility.
- `skills/forge-core/SKILL.md` - documented `forge.schedule.worker_status` assignment-plan usage for agents.
- `README.md` and `docs/technical-definition.md` - documented the scheduler assignment-plan contract.
- `CHANGELOG.md` and this report.

## TDD Evidence

The new CLI contract assertion was written before implementation and failed for the expected reason:

- `schedule_worker_status_reports_sleep_backpressure_and_scale_to_zero_plan` failed because `worker_pool.assignment_plan.schema_version` was `null`.

After implementation, the focused CLI and MCP tests passed:

- `cargo test schedule_worker_status_reports_sleep_backpressure_and_scale_to_zero_plan --test forge_cli_contract`
- `cargo test mcp_schedule_worker_status_tool_exposes_native_scheduler_worker_surface --test forge_cli_contract`

## Lean Overhead Ledger

| Metric | Value |
|---|---|
| Prompt bytes | ~95,000 |
| Estimated prompt tokens | ~23,750 |
| Validation command count | 10 |
| Artifact count | 1 report plus daily smoke Markdown/PDF/Telegram artifacts in `/tmp` |
| Metadata bytes | ~4,900 |
| Orchestration cost score | 3 |

## Safety

- No Docker, Kubernetes or Knative resources were mutated.
- No external Telegram delivery was performed; the smoke wrote a Forge-owned delivery record with secrets redacted.
- `worker-status` is read-only and does not acquire leases, execute due workflows or mutate external resources.
- Daily smoke artifacts were written under `/tmp` using Forge-owned workflow semantics.

## Next Recommended Cycle

Move the assignment plan from read-only scheduler visibility toward execution: add a bounded `scan-due --max-workers <n>` dispatch path that uses Forge-owned leases and worker-pool waves while preserving deterministic result ordering, lineage, cancellation safe points and SQLite consistency.
