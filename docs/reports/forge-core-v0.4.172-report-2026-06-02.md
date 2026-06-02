# Forge Core v0.4.172 Report - 2026-06-02

Run id: `run_8c9d3e943a204d03a16a1d4b33f71e63`
Workflow id: `wf_bcbca0bc466649c2a1322ae30220f420`
Cycle: 2

## Product Decision

This cycle keeps quota-aware executor/model policy as the required self-evolution repair goal. The product decision is to make selection and fallback reasoning inspectable as a structured trace, not only as candidate tables or prose. The business value is reduced wasted quota and faster operator judgment: a PM, founder or runtime supervisor can see exactly why Forge selected, skipped or advanced past OpenCode, Gemini, Codex or local Ollama.

## Change

- Added `selection_trace` to `forge executors` quota policy reports.
- Added `selection_trace` to self-evolution executor policy JSON.
- Added an `Executor selection trace` table to persisted self-evolution Markdown reports.
- Added CLI contract coverage for executor sync JSON, self-run JSON and self-run Markdown visibility.
- Bumped Forge Core to `0.4.172` and updated the changelog.

## Validation Plan

- `cargo test sync_persists_human_allowed_executor_policy --test forge_cli_contract`
- `cargo test self_run_reports_quota_aware_executor_policy_for_cycle --test forge_cli_contract`
- `cargo test self_run_persists_markdown_executor_policy_report_for_human_review --test forge_cli_contract`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`
- `./target/release/forge plan --goal "Create a delivery platform" --output json`
- `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-8c9d3e`

## Validation Result

Passed:

- `cargo test sync_persists_human_allowed_executor_policy --test forge_cli_contract`
- `cargo test self_run_reports_quota_aware_executor_policy_for_cycle --test forge_cli_contract`
- `cargo test self_run_persists_markdown_executor_policy_report_for_human_review --test forge_cli_contract`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test` with 82 unit tests, 241 CLI contract tests and doctests.
- `cargo build --release`
- `./target/release/forge plan --goal "Create a delivery platform" --output json`
- `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-8c9d3e-v2`

Post-validation install:

- `cargo install --path . --force` was attempted and failed because this sandbox cannot write `/home/arthur/.cargo/.crates.toml` (`Read-only file system`, os error 30).

Publication:

- `gh auth token` succeeded without exposing the token.
- `git remote get-url origin` returned `https://github.com/cardozoarthur/forge-core.git`.
- `git add CHANGELOG.md Cargo.lock Cargo.toml docs/reports/forge-core-v0.4.172-report-2026-06-02.md src/executor.rs src/self_evolve.rs tests/forge_cli_contract.rs` was attempted after validation and failed because Git could not create `/home/arthur/projects/forge-core/.git/index.lock` (`Sistema de ficheiros só de leitura`).
- No commit or push was attempted after that blocker because there was no validated local commit to publish.

## v0.5 Movement

This advances quota-aware executor policy and better business/product decision support. It gives the future Product/PM CLI-TUI and live runtime status surfaces a compact trace for explaining model spend decisions, fallback choices and local-vs-non-local trade-offs without forcing the user to infer policy from raw candidate order.

## Remaining Work

- Attach concrete validation gates and remediation commands to OpenCode/Gemini non-interactive repair tasks.
- Promote quota observations from static estimates to measured provider/model signals where CLIs expose them.
- Continue improving the Product/PM CLI-TUI as the main guided workflow creation entry point.
