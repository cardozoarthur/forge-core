# Forge Core v0.4.93 Self-Evolution Report

Run id: `run_bfba8dcc4747450da9067f8cdc713b58`  
Workflow id: `wf_047a8146d7fb42a7800cbfdad1b59f72`  
Executor: `codex`  
Date: `2026-05-25`

## Summary

Forge can now drive due daily Goal research execution from native schedule state instead of treating `run-due` as only a run-history marker.

This cycle added explicit `next_run_at` mutation through CLI and MCP, then wired `forge schedule run-due` into the canonical daily Goal research artifact path. When the `hackathon` schedule is due, Forge records the scheduled timestamp, writes the Markdown report, writes the PDF report and records the redacted Telegram delivery artifact under the same workflow lineage. If the loop node is paused or stopped, due execution returns `loop_not_runnable` and does not create artifacts or run-history entries.

## Behavior Added

- `forge schedule update --next-run-at <RFC3339>` revisions a scheduled node's due timestamp.
- MCP `forge.schedule.update` accepts `next_run_at`.
- `forge schedule run-due` now emits `daily_goal_research` when due daily Goal work executes.
- Due run history uses the due timestamp as `scheduled_at`.
- Paused or stopped loop nodes block due execution without mutating artifacts.
- Generated Forge skills and README examples document the schedule due/pause/resume surface.

## Canonical Hackathon Due Smoke

Smoke store: `/tmp/forge-due-smoke-v0493.M7A7I5/forge.sqlite`  
Smoke workflow: `wf_3783d68a87e1449e8be35d58a952eac4`

Command path:

1. Created daily Goal research for `hackathon`.
2. Updated schedule task `task-009` with `next_run_at=2000-01-01T00:00:00Z`.
3. Ran `forge schedule run-due --workflow wf_3783d68a87e1449e8be35d58a952eac4 --output json`.
4. Inspected the workflow and listed artifacts.
5. Searched smoke artifacts for `bot_token|chat_id`; no matches were found.

Generated artifacts:

- `artifacts/wf_3783d68a87e1449e8be35d58a952eac4/goal-hackathon-report.md`
- `artifacts/wf_3783d68a87e1449e8be35d58a952eac4/goal-hackathon-report.pdf`
- `artifacts/wf_3783d68a87e1449e8be35d58a952eac4/telegram-delivery-hackathon.json`

`run-due` returned:

- `status=due_workflow_executed`
- `due_executed=true`
- `daily_goal_research.status=smoke_artifacts_generated`
- `daily_goal_research.artifact_count=3`
- `telegram_delivery.secret_exposed=false`
- `schedule_summary.due_nodes=0` after execution, with `next_run_at` advanced to the next day

## Validation

- RED: `cargo test schedule_update_next_run_at_and_run_due_generates_goal_artifacts --test forge_cli_contract` failed because `--next-run-at` did not exist.
- RED: `cargo test schedule_run_due_skips_paused_loop_nodes_without_artifacts --test forge_cli_contract` failed because `--next-run-at` did not exist.
- RED: `cargo test mcp_schedule_update_mutates_cron_and_timezone --test forge_cli_contract` failed because MCP ignored `next_run_at`.
- GREEN focused tests passed after implementation.
- `cargo test schedule --test forge_cli_contract`: passed, 14 tests.
- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed after replacing the expanded schedule update positional API with `ScheduleUpdateOptions`.
- `cargo test`: passed, 140 tests.
- `cargo build --release`: passed.
- `target/release/forge --version`: `forge 0.4.93`.
- `target/release/forge --store /tmp/forge-plan-smoke-v0493.sqlite plan --goal "Create a delivery platform" --output json`: passed.
- `target/release/forge --store /tmp/forge-skill-smoke-v0493b.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v0493b`: passed.

Install status:

- `cargo install --path . --force`: blocked by read-only `/home/arthur/.cargo/.crates.toml`.
- Installed shell `forge --version`: `forge 0.4.92`.
- `cargo install --path . --force --root /tmp/forge-install-v0493 --offline`: passed as a writable-root installability proof.
- `/tmp/forge-install-v0493/bin/forge --version`: `forge 0.4.93`.

GitHub publication status:

- `git add ...`: blocked by read-only `.git/index.lock` creation in this session.
- `gh auth token`: passed with output suppressed.
- `git remote get-url origin`: `https://github.com/cardozoarthur/forge-core.git`.
- `git push`: blocked by DNS resolution for `github.com`.

## Lean Overhead Ledger

- prompt bytes: approximately 12,600
- estimated prompt tokens: approximately 3,150
- validation command count: 21
- artifact count: 1 tracked cycle report, 3 canonical due-smoke artifacts and 3 skill-smoke install files
- metadata bytes: 2,853 canonical due-smoke artifact/report bytes plus this report

## Safety

- No Docker, Kubernetes or Knative resources were mutated.
- No external Telegram secrets were read, written or printed.
- The canonical due smoke wrote only Forge-owned local artifacts under `/tmp`.
- Schedule and loop mutations remained local to Forge-owned workflow state with revision/origin trace.

## Next Recommended Cycle

Add missed-run reconciliation policies to `run-due`: detect schedules whose `next_run_at` is far behind, mark skipped or caught-up runs according to `missed_run_policy`, and expose the reconciliation summary through CLI/MCP/list/inspect without relying on wrapper loops.
