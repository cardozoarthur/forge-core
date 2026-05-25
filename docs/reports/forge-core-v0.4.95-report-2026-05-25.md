# Forge Core v0.4.95 Report - Missed-Run Schedule Semantics

Prompt packet: `forge.self_evolution.prompt.v2`
Run id: `run_bfba8dcc4747450da9067f8cdc713b58`
Workflow id: `wf_047a8146d7fb42a7800cbfdad1b59f72`
Cycle: 7
Executor: `codex`

## Increment

Forge schedule nodes now execute their persisted `missed_run_policy` instead of only storing the field as metadata.

- `run_once_then_resume` still runs the due workflow, but if `next_run_at` is older than the missed-run grace window it appends run history with `missed=true`.
- `skip_missed` and `skip_and_resume` append a `skipped_missed` run-history entry, advance `next_run_at`, and avoid generating Goal artifacts for stale missed work.
- `schedule_summary.missed_run_nodes` now gives operators and agents a direct list/inspect signal that a scheduled node had missed-run history.

This keeps cron behavior Forge-owned and durable while avoiding ad hoc loops or external schedulers as the source of truth.

## TDD Evidence

- RED: `cargo test schedule_run_due --test forge_cli_contract` failed because `skip_missed` still executed the due workflow.
- RED: `cargo test schedule_update_next_run_at_and_run_due_generates_goal_artifacts --test forge_cli_contract` failed because an overdue run history entry still reported `missed=false`.
- GREEN: both focused tests passed after adding missed-run detection and skip-missed handling in `src/schedule.rs`.

## Validation

Required validation passed:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test` (`141` tests passed)
- `cargo build --release`

Required CLI smoke passed:

- `target/release/forge --store /tmp/forge-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json`
- `target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`

Daily Goal smoke passed in `/tmp/forge-cycle7-smoke.ia2tQj`:

- workflow: `wf_71d2adf804924abe8ee65117838bb49e`
- status: `due_workflow_executed`
- missed-run nodes: `1`
- daily Goal status: `smoke_artifacts_generated`
- artifacts: Markdown report, PDF report and redacted Telegram delivery record for `hackathon`

## Install And Publish Notes

- `cargo install --path . --force` was attempted after validation and was blocked by the sandbox because `/home/arthur/.cargo/.crates.toml` is read-only.
- A writable-root install smoke passed with `cargo install --path . --force --root /tmp/forge-install-v0.4.95 --offline`.
- `gh auth token` was checked with output discarded, and `git remote get-url origin` returned `https://github.com/cardozoarthur/forge-core.git`.
- `git add ...` was blocked because `.git/index.lock` could not be created on the read-only filesystem.
- `git push` was attempted and failed because `github.com` could not be resolved from the sandbox.

## Lean Overhead Ledger

- prompt bytes: approximately 11,500
- estimated prompt tokens: approximately 2,875
- validation/smoke command count: 15
- required validation command count: 4
- artifact count: 1 tracked report plus 3 temporary daily Goal smoke artifacts
- metadata bytes: approximately 1,600 smoke metadata bytes plus this report

## Safety

No Docker, Kubernetes, Knative or external user resources were mutated. The change only mutates Forge-owned SQLite workflow state and Forge-owned local artifact records.

## Next Cycle

Implement a deterministic, Forge-owned recurring scheduler worker that repeatedly calls `forge schedule run-due` for registered scheduled workflows, with lease protection and scale-to-zero idleness, instead of relying on tmux wrapper sleeps.
