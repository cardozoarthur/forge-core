# Forge Core v0.4.106 Self-Evolution Report

Run id: `run_bfba8dcc4747450da9067f8cdc713b58`  
Workflow id: `wf_047a8146d7fb42a7800cbfdad1b59f72`  
Cycle: `21`  
Date: `2026-05-25`

## Summary

Forge now records native scale-to-zero decisions when a scheduled workflow has no due cron work.

This is `0.5 groundwork` for scheduled/looping runtime semantics, not a completed Forge 0.5 creative runtime. The change keeps Forge as the owner of cron lifecycle state instead of requiring a tmux wrapper or external loop to infer idle behavior.

## Added Behavior

- `forge schedule run-due --output json` now includes a `forge.scale_to_zero_decision.v1` receipt.
- The receipt records whether scale-to-zero was applied, the reason, the next wakeup timestamp, scheduled-node count and due-node count.
- When every scheduled node opts into `scale_to_zero_when_idle` and no node is due, Forge persists the workflow status as `scaled_to_zero`.
- `forge list` and `forge inspect` project that persisted `scaled_to_zero` lifecycle state for agents and humans.

## TDD Evidence

- RED: `cargo test schedule_run_due_reports_no_due_when_next_run_is_in_future --test forge_cli_contract` failed because `scale_to_zero.schema_version` was missing from `run-due` output.
- GREEN: the same focused test passed after adding `ScaleToZeroDecision`, no-due persistence and lifecycle projection support.

## Validation

- `cargo fmt --check`: passed
- `cargo clippy --all-targets --all-features -- -D warnings`: passed
- `cargo test`: passed, including 6 unit tests and 173 CLI contract tests
- `cargo build --release`: passed

## Smoke Evidence

- `./target/release/forge --store /tmp/forge-core-v04106-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json`: passed, produced workflow `wf_a058cfe566404534b126176340bcb4c2`.
- `./target/release/forge --store /tmp/forge-core-v04106-skill-smoke.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v04106`: passed, installed Codex and OpenCode skill files and preserved pending human approval for executors/runtimes.
- `./target/release/forge --store /tmp/forge-core-v04106-daily-smoke.sqlite schedule create-daily-goal-research --goal hackathon --timezone America/Sao_Paulo --cron "0 8 * * *" --origin codex --output json`: passed, produced workflow `wf_b2c8b3729cad4c3bb9634ed0fb30550f`.
- `./target/release/forge --store /tmp/forge-core-v04106-daily-smoke.sqlite run --workflow wf_b2c8b3729cad4c3bb9634ed0fb30550f --simulate --output json`: passed, completed 16 tasks and generated the daily Goal smoke artifacts.
- Daily Goal smoke artifacts:
  - `artifacts/wf_b2c8b3729cad4c3bb9634ed0fb30550f/goal-hackathon-report.md`
  - `artifacts/wf_b2c8b3729cad4c3bb9634ed0fb30550f/goal-hackathon-report.pdf`
  - `artifacts/wf_b2c8b3729cad4c3bb9634ed0fb30550f/telegram-delivery-hackathon.json`
- Telegram delivery record remained redacted with `secret_exposed=false`.
- `./target/release/forge --store /tmp/forge-core-v04106-daily-smoke.sqlite schedule run-due --workflow wf_b2c8b3729cad4c3bb9634ed0fb30550f --output json`: passed with `status=no_due_cron_nodes`, `scale_to_zero.applied=true` and `reason=finite_workflow_has_no_due_scheduled_work`.
- `./target/release/forge --store /tmp/forge-core-v04106-daily-smoke.sqlite list --output json`: passed and showed the workflow lifecycle as `scaled_to_zero`.

## Installation Status

- `cargo install --path . --force`: blocked by the current sandbox because `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem (`os error 30`).

## Forge Workflow State

- Attached this report to workflow `wf_047a8146d7fb42a7800cbfdad1b59f72` through Forge CLI.
- Attachment revision advanced to at least `17`.
- Attached artifact: `artifacts/wf_047a8146d7fb42a7800cbfdad1b59f72/attached-report-forge-core-v0.4.106-report-2026-05-25.md`

## Safety

- No Docker, Kubernetes, Knative, Telegram or external user resources were mutated.
- The only runtime mutations are Forge-owned SQLite/workflow-state writes in temporary smoke stores and the configured Forge workflow store.
- Scale-to-zero is additive workflow metadata and does not skip validation, delete workflows or authorize external executors.

## Lean Overhead Ledger

- Schema: `forge.self_evolution.overhead_ledger.v1`
- Prompt bytes: approximately 54,000
- Estimated prompt tokens: approximately 13,500
- Validation command count: 4 required commands plus 8 focused/smoke/artifact commands
- Artifact count: 1 report artifact plus changelog and milestone updates
- Metadata bytes: approximately 7,900
- Orchestration cost score: 3
- Useful delivery: closes one lifecycle-state gap for native scheduled workflows and prevents agents from inferring idle schedule state from external wrappers.

## Next Recommended Cycle

Implement the design-token resolution engine for Forge 0.5 groundwork: raw/semantic token resolution, inheritance and override precedence, theme switching, impact preview metadata and patch-by-intent updates that preserve human edits.
