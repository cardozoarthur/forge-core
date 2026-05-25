# Forge Core v0.4.90 Self-Evolution Report

Run id: `run_bfba8dcc4747450da9067f8cdc713b58`  
Workflow id: `wf_047a8146d7fb42a7800cbfdad1b59f72`  
Executor: `codex`  
Date: `2026-05-25`

## Summary

Forge now models scheduled and looping workflow semantics directly in the graph instead of relying on ad hoc terminal loops as the architecture.

This cycle added native schedule, loop and native-subflow metadata, a canonical daily Goal research workflow for `hackathon`, MCP exposure for schedule/loop creation and inspection, and a deterministic smoke path that produces one Markdown report, one PDF report and one redacted Telegram delivery record for each configured Goal.

## Behavior Added

- `ScheduleSpec` now carries schema version, kind, timezone, `next_run_at`, missed-run policy, run history and scale-to-zero metadata.
- `LoopSpec` now records loop type, item list, bounded iteration metadata, retry/backoff and condition slots, subflow mode, stop policy and state.
- `NativeSubflowSpec` records finite/infinite subflow mode, trigger source and workflow/run/artifact lineage policy.
- `forge schedule create-daily-goal-research` creates a Forge-owned daily Goal research graph.
- `forge schedule list`, `forge schedule inspect` and `forge schedule update` expose schedule primitives through the CLI.
- MCP exposes `forge.schedule.create_daily_goal_research`, `forge.schedule.list`, `forge.schedule.update`, `forge.loop.inspect` and `forge.task.handoff`.
- `forge inspect` and `forge list` expose schedule and loop summaries.
- `forge run --simulate` generates daily Goal research smoke artifacts for configured Goals without exposing Telegram secrets.

## Canonical Hackathon Workflow

The initial `hackathon` Goal graph includes:

- daily cron node: `0 8 * * *`, `America/Sao_Paulo`, missed policy `run_once_then_resume`;
- loop-over-items node over `["hackathon"]`;
- finite per-Goal subflow lineage: `goal_research:hackathon`;
- deterministic nodes for DuckDuckGo discovery, Playwright inspection, Markdown generation, PDF generation and Telegram delivery record;
- one AI node for judgment-heavy fit evaluation.

## Smoke Evidence

Smoke store: `/tmp/forge-core-daily-goal-smoke.XQOxVf/forge.sqlite`  
Smoke workflow: `wf_42b4c6615cde46aa88d2a445f1505d0b`

Generated artifacts:

- `artifacts/wf_42b4c6615cde46aa88d2a445f1505d0b/goal-hackathon-report.md`
- `artifacts/wf_42b4c6615cde46aa88d2a445f1505d0b/goal-hackathon-report.pdf`
- `artifacts/wf_42b4c6615cde46aa88d2a445f1505d0b/telegram-delivery-hackathon.json`

The Telegram delivery record sets `secret_exposed=false`; a direct grep for `bot_token|chat_id` returned no matches.

## Validation

- `cargo fmt --check`: passed
- `cargo clippy --all-targets --all-features -- -D warnings`: passed
- `cargo test`: passed, 128 tests
- `cargo build --release`: passed
- `target/release/forge plan --goal "Create a delivery platform" --output json`: passed
- `target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke.fPHYIw`: passed

GitHub publication contract:

- `gh auth token`: passed with output suppressed.
- `git remote get-url origin`: `https://github.com/cardozoarthur/forge-core.git`
- `git add ...`: blocked by sandbox read-only access to `.git/index.lock`.
- `git push`: blocked by restricted network DNS resolution for `github.com`.

Install attempt:

- `cargo install --path . --force`: blocked by sandbox write restrictions on `/home/arthur/.cargo/.crates.toml`.
- `target/release/forge --version`: `forge 0.4.90`
- installed `forge --version`: `forge 0.4.89`

## Lean Overhead Ledger

- prompt bytes: approximately 10,900
- estimated prompt tokens: approximately 2,725
- validation command count: 12
- artifact count: 1 tracked cycle report plus 3 smoke artifacts
- metadata bytes: 1,291 smoke artifact bytes and this report

## Safety

- No Docker, Kubernetes or Knative resources were mutated.
- No external Telegram secrets were read, written or printed.
- The smoke writes only Forge-owned local artifacts under the selected SQLite store base directory.
- Schedule mutation is revisioned through Forge workflow state.

## Next Recommended Cycle

Implement durable scheduler execution state beyond simulation: due-work polling, missed-run reconciliation, schedule pause/resume, and recurring subflow run-history records that can wake finite workflows from scale-to-zero without needing a wrapper loop.
