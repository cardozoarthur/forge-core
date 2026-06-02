# Forge Core v0.4.171 Report - 2026-06-02

Run id: `run_9ff8a6cdf43a4539a9d3245c5d4d403a`
Workflow id: `wf_dfa9a20f8ade43a69fb82cef22d0ba1a`
Cycle: 20

## Product Decision

This cycle keeps executor policy as the required self-evolution repair goal, but moves the evidence closer to human product review. The business value is that a PM/operator can open the cycle Markdown report and see why Forge spent or preserved non-local quota, which candidate had the best value, and what assumptions drove fallback selection without reverse-engineering the JSON artifact.

## Change

- Added business reasoning to the self-evolution cycle Markdown executor-policy section.
- Added selected candidate business-value justification to the Markdown report.
- Added a quota assumptions section so Gemini/Codex quota-bound status, OpenCode non-local quota risk, local compute semantics and deterministic zero-quota work are explicit.
- Added `business_value_score` to the executor-policy Markdown candidate table.
- Added CLI contract coverage for the persisted Markdown report.

## Validation Plan

- `cargo test self_run_persists_markdown_executor_policy_report_for_human_review --test forge_cli_contract -- --nocapture`
- `cargo test self_evolve::tests::test_executor_policy_quota_aware_selection -- --nocapture`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`
- `./target/release/forge plan --goal "Create a delivery platform" --output json`
- `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`

## Validation Result

Passed:

- RED: `cargo test self_run_persists_markdown_executor_policy_report_for_human_review --test forge_cli_contract -- --nocapture` failed before implementation because the Markdown report did not include selected candidate business-value justification.
- GREEN: `cargo test self_run_persists_markdown_executor_policy_report_for_human_review --test forge_cli_contract -- --nocapture` passed after implementation.
- GREEN: `cargo test self_evolve::tests::test_executor_policy_quota_aware_selection -- --nocapture` passed.
- Required validation passed: `cargo fmt --check`.
- Required validation passed: `cargo clippy --all-targets --all-features -- -D warnings`.
- Required validation passed: `cargo test` with 82 unit tests, 241 CLI contract tests and doctests.
- Required validation passed: `cargo build --release`.
- CLI smoke passed: `./target/release/forge plan --goal "Create a delivery platform" --output json`.
- CLI smoke passed: `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`.

Post-validation install:

- `cargo install --path . --force` was attempted and failed because this sandbox cannot write `/home/arthur/.cargo/.crates.toml` (`Read-only file system`, os error 30).
- The validated binary is available at `./target/release/forge` and reports version `0.4.171`.

Publication:

- `gh auth token` succeeded without exposing the token.
- `git remote get-url origin` returned `https://github.com/cardozoarthur/forge-core.git`.
- `git commit -m "feat: show executor policy business value in self-run reports"` was attempted after validation and failed because Git could not create `/home/arthur/projects/forge-core/.git/index.lock` (`Sistema de ficheiros só de leitura`).
- No push was attempted because no commit was created.

## v0.5 Movement

This advances quota-aware executor policy and better business/product decisions by making executor/model selection rationale visible in the durable human report artifact. It supports the Product/PM CLI-TUI direction because the report now carries fields suitable for terminal inspection and future status panels.

## Remaining Work

- Attach concrete validation gates and remediation commands to OpenCode/Gemini non-interactive repair tasks.
- Make executor repair tasks visible in `forge inspect` DAG/status output with validation evidence.
- Continue improving the Product/PM CLI-TUI as the main guided workflow creation entry point.
