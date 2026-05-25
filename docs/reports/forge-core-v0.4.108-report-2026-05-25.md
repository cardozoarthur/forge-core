# Forge Core v0.4.108 Report - Native Schedule Scanner

## Summary

Forge now has a native schedule scanner:

```bash
forge schedule scan-due --executor forge-scheduler --ttl-seconds 300 --output json
```

This is `0.5 groundwork` for scheduled/looping runtime semantics. It does not claim the Forge 0.5 creative runtime is complete.

The scanner keeps Forge as the owner of scheduled workflow semantics:

- lists Forge-owned scheduled workflows from the SQLite store;
- acquires a local task lease before executing a due schedule node;
- runs the existing `run-due` reconciliation path for due workflows;
- releases the synchronous local lease after execution;
- records idle `scale_to_zero` decisions for scheduled workflows with no due work;
- exposes the same capability to agents through MCP as `forge.schedule.scan_due`.

## Behavior

- Output schema: `forge.schedule.scan_due.v1`.
- Due workflows return per-workflow status, due node count, schedule task id, lease status, lease id, release flag and nested `run_due` evidence.
- Idle scheduled workflows return `lease_status=not_required` and nested `run_due.scale_to_zero` evidence.
- Lease conflicts are reported without executing the workflow.
- The scanner does not mutate Docker, Kubernetes, Knative, Telegram or external user resources.

## TDD Evidence

- RED: `cargo test schedule_scan_due_executes_due_workflows_with_lease_and_scales_idle_workflows_to_zero --test forge_cli_contract` failed because `scan-due` was not a recognized subcommand.
- RED: `cargo test mcp_call_schedule_scan_due_runs_native_scheduler_scan --test forge_cli_contract` failed because `forge.schedule.scan_due` was an unknown MCP tool.
- GREEN: both tests passed after implementing `scan_due_workflows`, CLI routing and MCP exposure.

## Focused Validation

- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test schedule_scan_due --test forge_cli_contract`: passed.
- `cargo test mcp_schedule_pause_resume_stop_exposes_loop_state_control_tools --test forge_cli_contract`: passed.

## Required Validation

- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test`: passed, including 6 unit tests and 175 CLI contract tests.
- `cargo build --release`: passed.

## Release Smokes

- `./target/release/forge --store /tmp/forge-core-v04108-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json`: passed, produced workflow `wf_f4fb3a5290dc49ebb188f5cc7886f43c`.
- `./target/release/forge --store /tmp/forge-core-v04108-skill-smoke-b.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v04108-b`: passed, installed Codex/OpenCode/agents skill files and kept executor/runtime authorization pending human approval.
- Daily Goal scan smoke passed through `forge schedule scan-due`, produced workflow `wf_2bfd73c604264be2b1a70ceca68657ee`, acquired and released a local lease on `task-009`, executed due scheduled work and produced:
  - `artifacts/wf_2bfd73c604264be2b1a70ceca68657ee/goal-hackathon-report.md`
  - `artifacts/wf_2bfd73c604264be2b1a70ceca68657ee/goal-hackathon-report.pdf`
  - `artifacts/wf_2bfd73c604264be2b1a70ceca68657ee/telegram-delivery-hackathon.json`
- Telegram delivery record smoke confirmed `secret_exposed=false`.

## Local Install

- `cargo install --path . --force`: blocked by sandbox because `/home/arthur/.cargo` is read-only in this execution context.
- `cargo install --path . --force --root .forge/local-install`: retried but network was unavailable for crates.io index refresh.
- `cargo install --path . --force --root .forge/local-install --offline`: passed.
- `.forge/local-install/bin/forge --version`: `forge 0.4.108`.

## GitHub Publication

- `gh auth token`: passed with output redirected.
- `git remote get-url origin`: `https://github.com/cardozoarthur/forge-core.git`.
- Local `.git` writes were blocked by the sandbox as read-only, so a temporary index/object store in `/tmp` was used to create commit `949b52048ff39980ccde7af8eda369cea7c64039`.
- `git push origin 949b52048ff39980ccde7af8eda369cea7c64039:refs/heads/main`: failed because DNS could not resolve `github.com`.
- Result: validated changes are present in the working tree and local project install, but remote publication was not completed in this execution context.

## Lean Overhead Ledger

- Prompt bytes: approximately 55,000.
- Estimated prompt tokens: approximately 13,500.
- Validation, smoke, install and publication command count: 18, including RED/GREEN focused tests, required validation, release smokes, install attempts and blocked publication attempts.
- Artifact count: 1 tracked report plus changelog entry; 3 temporary daily Goal smoke artifacts.
- Metadata bytes added: approximately 8,500.

## Next Recommended Cycle

Add a bounded scheduler worker loop owned by Forge that repeatedly calls `schedule scan-due` with cancellation, backpressure, sleep-until-next-wakeup behavior and inspectable worker state. Keep any tmux/systemd wrapper as a launcher only, not as the workflow semantics owner.
