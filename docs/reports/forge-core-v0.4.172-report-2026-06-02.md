# Forge Core v0.4.172 Report - 2026-06-02

Run id: `run_8c9d3e943a204d03a16a1d4b33f71e63`
Workflow id: `wf_bcbca0bc466649c2a1322ae30220f420`
Cycle: 1

## Product Decision

This cycle keeps quota-aware executor/model policy as the required self-evolution repair goal and makes the policy easier to inspect from durable status surfaces. The business value is faster operator judgment: a PM or founder can see whether Forge is spending non-local quota for a high-value self-evolution decision, preserving quota with local/deterministic execution, or blocking on executor repair.

## Change

- Added `quota_decision_summary` to self-evolution executor policy JSON.
- Added the same quota decision summary to persisted self-evolution Markdown reports.
- Added the quota decision summary to `forge request status` via `latest_executor_policy`.
- Added CLI contract coverage for JSON, Markdown and request-status visibility.

## Validation Plan

- `cargo test self_run_reports_quota_aware_executor_policy_for_cycle --test forge_cli_contract`
- `cargo test self_run_persists_markdown_executor_policy_report_for_human_review --test forge_cli_contract`
- `cargo test request_status_surfaces_latest_self_evolution_executor_policy_summary --test forge_cli_contract`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`
- `./target/release/forge plan --goal "Create a delivery platform" --output json`
- `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`

## Validation Result

Passed:

- RED: `cargo test self_run_reports_quota_aware_executor_policy_for_cycle --test forge_cli_contract` failed before implementation because `quota_decision_summary` was absent from the self-evolution executor policy JSON.
- RED: `cargo test self_run_persists_markdown_executor_policy_report_for_human_review --test forge_cli_contract` failed before implementation because the Markdown report did not include the quota decision line.
- RED: `cargo test request_status_surfaces_latest_self_evolution_executor_policy_summary --test forge_cli_contract` failed before implementation because `request status` did not expose the quota decision summary.
- GREEN: all three focused tests passed after implementation.
- Required validation passed: `cargo fmt --check`.
- Required validation passed: `cargo clippy --all-targets --all-features -- -D warnings`.
- Required validation passed: `cargo test` with 82 unit tests, 241 CLI contract tests and doctests.
- Required validation passed: `cargo build --release`.
- CLI smoke passed: `./target/release/forge plan --goal "Create a delivery platform" --output json`.
- CLI smoke passed: `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-8c9d3e`.

Post-validation install:

- `cargo install --path . --force` was attempted and failed because this sandbox cannot write `/home/arthur/.cargo/.crates.toml` (`Read-only file system`, os error 30).

Publication:

- `gh auth token` succeeded without exposing the token.
- `git remote get-url origin` returned `https://github.com/cardozoarthur/forge-core.git`.
- `git add ... && git commit -m "feat: surface self-evolution quota decisions"` was attempted after validation and failed because Git could not create `/home/arthur/projects/forge-core/.git/index.lock` (`Sistema de ficheiros só de leitura`).
- No push was attempted because no validated commit could be created in this sandbox.

## v0.5 Movement

This advances quota-aware executor policy and better business/product decision support. It gives the future Product/PM CLI-TUI a compact, stable status field for explaining model spend decisions without forcing the user to read full candidate tables. It also supports real-time agent runtime inspection because `request status` now carries the same decision summary as the cycle artifact.

## Remaining Work

- Attach concrete validation gates and remediation commands to OpenCode/Gemini non-interactive repair tasks.
- Promote quota observations from static estimates to measured provider/model signals where CLIs expose them.
- Continue improving the Product/PM CLI-TUI as the main guided workflow creation entry point.
