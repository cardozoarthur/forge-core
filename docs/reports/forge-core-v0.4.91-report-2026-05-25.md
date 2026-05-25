# Forge Core v0.4.91 Self-Evolution Report

Run id: `run_bfba8dcc4747450da9067f8cdc713b58`  
Workflow id: `wf_047a8146d7fb42a7800cbfdad1b59f72`  
Executor: `codex`  
Date: `2026-05-25`

## Summary

Forge now persists durable schedule run evidence when the native daily Goal research smoke executes.

The previous native schedule/loop implementation generated Markdown, PDF and Telegram delivery artifacts, but the cron node's `run_history` stayed empty after the first smoke run. This cycle closes that state gap: `forge run --simulate` records a completed `run_...` entry on scheduled nodes, advances `next_run_at`, and keeps the workflow inspectable as `scaled_to_zero` with artifact lineage intact.

## Behavior Added

- Daily Goal research smoke execution appends a `forge.schedule.v1` run-history entry to each schedule node.
- Each run-history entry records `run_id`, `scheduled_at`, `started_at`, `finished_at`, `status=completed` and `missed=false`.
- The schedule node advances `next_run_at` after the simulated run.
- The CLI contract `run_daily_goal_research_smoke_generates_reports_and_telegram_record` now proves report artifacts, Telegram redaction and durable schedule run history together.
- The package version is now `0.4.91`.

## Canonical Hackathon Smoke

Smoke store: `/tmp/forge-daily-goal-v0491.sqlite`  
Smoke workflow: `wf_11e436b0e7b54ba6aef9276fe377eee9`

Generated artifacts:

- `artifacts/wf_11e436b0e7b54ba6aef9276fe377eee9/goal-hackathon-report.md`
- `artifacts/wf_11e436b0e7b54ba6aef9276fe377eee9/goal-hackathon-report.pdf`
- `artifacts/wf_11e436b0e7b54ba6aef9276fe377eee9/telegram-delivery-hackathon.json`

Focused inspection of `task-009` showed:

- `lifecycle_state=scaled_to_zero`
- `artifact_count=3`
- `schedule_summary.cron_nodes=1`
- `schedule_summary.scale_to_zero_when_idle_nodes=1`
- `schedule.run_history[0].run_id=run_ad6009ceee524c89b3b634fc17e8f157`
- `schedule.run_history[0].status=completed`
- `schedule.run_history[0].missed=false`
- `schedule.run_history[0].scheduled_at`, `started_at` and `finished_at` were persisted

The Telegram delivery artifact remained redacted. A direct search for `bot_token|chat_id` returned no matches.

## Validation

- `cargo fmt --check`: passed after applying `cargo fmt`
- `cargo clippy --all-targets --all-features -- -D warnings`: passed
- `cargo test`: passed, 132 tests
- `cargo build --release`: passed
- `target/release/forge --store /tmp/forge-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json`: passed
- `target/release/forge --store /tmp/forge-skill-smoke-v0491.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v0491`: passed
- `target/release/forge --store /tmp/forge-daily-goal-v0491.sqlite schedule create-daily-goal-research --goal hackathon --timezone America/Sao_Paulo --cron "0 8 * * *" --origin codex --output json`: passed
- `target/release/forge --store /tmp/forge-daily-goal-v0491.sqlite run --workflow wf_11e436b0e7b54ba6aef9276fe377eee9 --simulate --output json`: passed
- `target/release/forge --store /tmp/forge-daily-goal-v0491.sqlite inspect wf_11e436b0e7b54ba6aef9276fe377eee9 --task task-009 --verbose --output json`: passed
- `target/release/forge --store /tmp/forge-daily-goal-v0491.sqlite artifacts --workflow wf_11e436b0e7b54ba6aef9276fe377eee9 --output json`: passed

Install status:

- `target/release/forge --version`: `forge 0.4.91`
- `/tmp/forge-install-v0491/bin/forge --version`: `forge 0.4.91`
- installed `forge --version`: `forge 0.4.90`
- `cargo install --path . --force`: blocked by read-only `/home/arthur/.cargo/.crates.toml`
- `cargo install --path . --force --root /tmp/forge-install-v0491 --offline`: passed as an installability proof in a writable root

GitHub publication status:

- `gh auth token`: passed with output suppressed
- `git remote get-url origin`: `https://github.com/cardozoarthur/forge-core.git`
- `git add ...`: blocked by read-only `.git/index.lock`
- `git push`: blocked by DNS resolution for `github.com`

## Lean Overhead Ledger

- prompt bytes: approximately 10,900
- estimated prompt tokens: approximately 2,725
- validation command count: 15
- artifact count: 1 tracked cycle report plus 3 canonical smoke artifacts
- metadata bytes: 1,291 canonical smoke artifact bytes plus this report

## Safety

- No Docker, Kubernetes or Knative resources were mutated.
- No external Telegram secrets were read, written or printed.
- The canonical smoke wrote only Forge-owned local artifacts under `/tmp`.
- Schedule run-history mutation is local to Forge-owned workflow state.

## Next Recommended Cycle

Implement real due-work polling and missed-run reconciliation for scheduled workflows, including pause/resume and wake-from-scale-to-zero behavior, so Forge can run daily Goal research on schedule without relying on a wrapper loop.
