# Forge Core v0.4.96 Report - Scheduled Artifact Lineage

Prompt packet: `forge.self_evolution.prompt.v2`
Run id: `run_bfba8dcc4747450da9067f8cdc713b58`
Workflow id: `wf_047a8146d7fb42a7800cbfdad1b59f72`
Cycle: 9
Executor: `codex`

## Increment

Forge daily Goal research artifacts now preserve explicit run lineage.

- Added optional `forge.artifact_lineage.v1` to workflow artifact records.
- Daily Goal Markdown, PDF and Telegram delivery artifacts now include parent `workflow_id`, inherited schedule `run_id`, schedule task, loop task, Goal, native subflow id and trigger.
- `forge schedule run-due` returns the same lineage in `daily_goal_research.goals[].lineage`.
- The Telegram delivery record remains redacted while proving which Forge-owned scheduled run produced the delivery artifact.

This closes a lineage gap in the scheduled/looping runtime: recurring subflows can now generate artifacts without losing the run identity that triggered them.

## TDD Evidence

- RED: `cargo test schedule_update_next_run_at_and_run_due_generates_goal_artifacts --test forge_cli_contract` failed because `daily_goal_research.goals[0].lineage.run_id` was missing.
- GREEN: the same focused test passed after adding artifact lineage propagation through `src/schedule.rs` and optional artifact metadata in `src/graph.rs`.
- Focused schedule/MCP regression checks passed: `cargo test schedule_ --test forge_cli_contract`, `cargo test mcp_call_schedule --test forge_cli_contract`, and `cargo test run_daily_goal_research_smoke_generates_reports_and_telegram_record --test forge_cli_contract`.

## Validation

Required validation passed:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test` (`149` tests passed: 6 unit tests plus 143 CLI contract tests)
- `cargo build --release`

Required CLI smoke passed:

- `target/release/forge --store /tmp/forge-core-v0496-plan-smoke-20260525a.sqlite plan --goal "Create a delivery platform" --output json`
- `target/release/forge --store /tmp/forge-core-v0496-skill-smoke-20260525a.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v0496-20260525a`

Daily Goal smoke passed:

- store: `/tmp/forge-cycle9-lineage-smoke-20260525a/forge.sqlite`
- workflow: `wf_109eb757b217474685d75043a0616c1b`
- status: `due_workflow_executed`
- artifacts: Markdown report, PDF report and redacted Telegram delivery record
- lineage: `workflow_id=wf_109eb757b217474685d75043a0616c1b`, `run_id=run_162c45e4b1cf43d1be5c2cc1c0d767cd`, `schedule_task_id=task-009`, `loop_task_id=task-010`, `subflow_id=goal_research:hackathon`
- Telegram secret exposure: `false`

## Install Notes

- `cargo install --path . --force` was attempted after validation and was blocked by the sandbox because `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem.
- `cargo install --path . --force --root /tmp/forge-install-v0.4.96` was attempted and failed because crates.io DNS resolution is blocked in the sandbox.
- `cargo install --path . --force --root /tmp/forge-install-v0.4.96 --offline` passed, proving the package installs from the validated local artifact set when using a writable root.

## Publish Notes

- `gh auth token` succeeded with output redirected away from chat.
- `git remote get-url origin` returned `https://github.com/cardozoarthur/forge-core.git`.
- `git add ...` was attempted and failed because `.git/index.lock` cannot be created on the current read-only git metadata mount.
- `git push` was attempted and failed because `github.com` could not be resolved from the sandbox.

## Lean Overhead Ledger

- prompt bytes: approximately 23,000
- estimated prompt tokens: approximately 5,750
- validation/smoke/install/publish command count: 20
- required validation command count: 4
- artifact count: 1 tracked report plus 3 temporary daily Goal smoke artifacts
- metadata bytes: approximately 2,200 report metadata bytes

## Safety

No Docker, Kubernetes, Knative or external user resources were mutated. The rejected initial smoke command attempted local `/tmp` cleanup, was blocked by policy, and was replaced with unique non-destructive smoke paths. The actual Forge changes only mutate Forge-owned SQLite workflow state and local artifact metadata.

## Next Cycle

Implement the next native scheduler step: a deterministic Forge-owned scheduled-work scanner that lists due workflows, acquires a local lease, runs `forge schedule run-due` for each due workflow, and reports scale-to-zero idleness without relying on tmux wrapper loops as the primary scheduling model.
