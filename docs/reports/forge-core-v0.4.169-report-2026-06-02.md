# Forge Core v0.4.169 Report - 2026-06-02

Run id: `run_9ff8a6cdf43a4539a9d3245c5d4d403a`
Workflow id: `wf_dfa9a20f8ade43a69fb82cef22d0ba1a`
Cycle: 18

## Product Decision

This cycle improves executor policy observability before adding more PM/TUI work. The business value is lower wasted autonomous time: if Gemini or OpenCode is installed and authorized but still risks an interactive wait, Forge now exposes a concrete repair goal with the persisted probe evidence in `forge executors` quota-policy output.

## Change

- Added dynamic executor quota-policy repair goals for candidates skipped because of non-interactive hang risk.
- Included executor display name and current probe evidence in the repair goal so the operator can see why selection moved to the next candidate.
- Deduplicated dynamic repair goals by executor so OpenCode's non-local, configured-provider and local/Ollama candidates do not emit repeated repair text for the same probe failure.
- Added regression coverage for Gemini timeout evidence and OpenCode repair-goal deduplication in the executor sync/report path.

## Validation Plan

- `cargo test executor::tests::executor_report_excludes_interactive_hang_risk_from_usable`
- `cargo test executor::tests::executor_report_deduplicates_repair_goals_for_one_executor`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`
- `./target/release/forge plan --goal "Create a delivery platform" --output json`
- `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`

## Validation Result

Passed:

- RED: `cargo test executor::tests::executor_report_excludes_interactive_hang_risk_from_usable` failed before implementation because the Gemini repair goal was missing.
- GREEN: `cargo test executor::tests::executor_report_excludes_interactive_hang_risk_from_usable` passed after implementation.
- RED: `cargo test executor::tests::executor_report_deduplicates_repair_goals_for_one_executor` failed before deduplication with 3 OpenCode repair goals instead of 1.
- GREEN: `cargo test executor::tests::executor_report_deduplicates_repair_goals_for_one_executor` passed after deduplication.
- Focused executor suite: `cargo test executor::tests::` passed with 4 tests.
- Required validation passed: `cargo fmt --check`.
- Required validation passed: `cargo clippy --all-targets --all-features -- -D warnings`.
- Required validation passed: `cargo test` with 81 unit tests, 240 CLI contract tests and doctests.
- Required validation passed: `cargo build --release`.
- CLI smoke passed: `./target/release/forge plan --goal "Create a delivery platform" --output json`.
- CLI smoke passed: `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`.
- Smoke evidence confirmed OpenCode dynamic repair goals are deduplicated to one occurrence.

Post-validation install:

- `cargo install --path . --force` was attempted and failed because this sandbox cannot write `/home/arthur/.cargo/.crates.toml` (`Read-only file system`, os error 30).
- The validated binary is available at `./target/release/forge` and reports version `0.4.169`.

Publication:

- `gh auth token` succeeded without exposing the token.
- `git remote get-url origin` returned `https://github.com/cardozoarthur/forge-core.git`.
- `git commit -m "fix: surface executor repair goals in quota policy"` was attempted after validation and failed because Git could not create `/home/arthur/projects/forge-core/.git/index.lock` (`Sistema de ficheiros só de leitura`).
- No push was attempted because no commit was created.

## v0.5 Movement

This advances quota-aware executor policy and governed self-evolution by making executor repair work visible from runtime status artifacts rather than hidden in repeated timeouts. It supports better product/business decisions by preserving quota and human attention for high-value PM, creative and reasoning work.

## Remaining Work

- Persist executor-sync repair goals as ordinary workflow tasks or status/inspect evidence.
- Add concrete OpenCode/Gemini non-interactive probe artifacts with provider, model, locality, quota/cost assumptions and repair status.
- Continue improving Product/PM CLI-TUI and internal recurring self-evolution loop visibility.
