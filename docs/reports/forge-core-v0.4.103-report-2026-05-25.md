# Forge Core v0.4.103 Report - Missed-Run Reconciliation Visibility

Prompt packet: `forge.self_evolution.prompt.v2`
Run id: `run_bfba8dcc4747450da9067f8cdc713b58`
Workflow id: `wf_047a8146d7fb42a7800cbfdad1b59f72`
Workflow revision: `13`
Executor: `codex`

## Increment

Forge now emits auditable missed-run reconciliation receipts for native scheduled workflow execution.

- `ScheduleRunRecord` persists `missed_run_policy` and `reconciliation_action` with serde defaults for older workflow JSON.
- `forge schedule run-due --output json` returns `missed_run_reconciliation` for stale due cron nodes.
- `forge list --output json` and `forge inspect --output json` derive schedule-summary visibility for missed-run policies and reconciliation actions.
- The generated Forge skill tells agents to inspect reconciliation before interpreting stale cron work.
- The package version is now `0.4.103`.

## TDD Evidence

- RED: `cargo test schedule_run_due_reports_missed_run_reconciliation_for_cli_list_and_inspect --test forge_cli_contract` failed because `missed_run_reconciliation` was absent from `run-due` output.
- GREEN: the same focused test passed after adding persisted run-history metadata and schedule-summary projections.
- Adjacent regression: `cargo test schedule --test forge_cli_contract` passed with 17 schedule/MCP/loop tests.

## Validation

Required validation passed:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test` (`170` CLI contract tests plus 6 unit tests passed)
- `cargo build --release`

Required CLI smoke passed:

- `./target/release/forge --store /tmp/.../plan.sqlite plan --goal "Create a delivery platform" --output json`
- `./target/release/forge --store /tmp/.../skill.sqlite skill install --target codex --target opencode --output json --home /tmp/.../forge-skill-smoke`

Daily Goal research smoke passed:

- Created native scheduled Goal workflow for `hackathon`.
- Mutated the schedule `next_run_at` to `2000-01-01T00:00:00Z`.
- Ran `forge schedule run-due --workflow <workflow_id> --output json`.
- Produced one Markdown report, one PDF report and one Telegram delivery record.
- Telegram delivery record exposed no `bot_token` or raw `chat_id`.
- `missed_run_reconciliation[0].action` was `ran_once_then_resumed` and `artifacts_allowed` was `true`.

## Install Notes

- `cargo install --path . --force` was attempted and blocked by the sandbox read-only filesystem at `/home/arthur/.cargo/.crates.toml`.
- `cargo install --path . --force --root .forge/local-install` was attempted and blocked by network-restricted crates.io index refresh.
- `cargo install --path . --force --root .forge/local-install --offline` passed.
- `.forge/local-install/bin/forge --version` returned `forge 0.4.103`.
- `./target/release/forge --version` returned `forge 0.4.103`.

## GitHub Publication

- `gh auth token` passed with output redirected to `/dev/null`; the token value was not recorded.
- `git remote get-url origin` returned `https://github.com/cardozoarthur/forge-core.git`.
- `git add ... && git commit -m "chore: forge self evolution cycle 17"` was attempted after validation.
- Commit creation was blocked because the sandbox could not create `.git/index.lock`: `Sistema de ficheiros só de leitura`.
- `git push` was not run because no validated commit could be created from this environment.

## Safety

- No Docker, Kubernetes, Knative, Telegram or external user resources were mutated.
- Schedule mutations stayed inside Forge-owned SQLite workflow state.
- The Telegram smoke only wrote a redacted simulated delivery record.
- The change is additive for persisted workflow JSON; older run histories deserialize with `not_reconciled` until new native run evidence is recorded.

## Lean Overhead Ledger

- prompt bytes: approximately 51,000
- estimated prompt tokens: approximately 12,750
- required validation command count: 4
- validation/smoke/install/preflight command count: 12
- artifact count: 4 tracked artifacts (`CHANGELOG.md`, `skills/forge-core/SKILL.md`, this report, updated version metadata)
- metadata bytes: approximately 7,000 report/changelog/skill bytes

## Next Cycle

Add MCP stdio server mode over the existing `forge.mcp.tools.v1` and `forge.mcp.call.v1` contracts, with JSON-RPC tests for initialize, tools/list and tools/call. Keep it bounded to the same Forge APIs so MCP agents do not create a parallel workflow state path.
