# Forge Core v0.4.162 Report - 2026-06-02

## What Changed

- Self-evolution executor/model policy now reads persisted quota observations from the Forge store before selecting strategies.
- OpenCode non-local policy is split into two visible candidates: configured no-cost/free non-local first, then paid-or-unknown non-local after Gemini and Codex non-local quota-bound options.
- OpenCode local/Ollama remains visible as a local capacity fallback for cheap, repetitive, privacy-sensitive or quota-preserving work rather than a blanket second choice.
- Attempt reports keep quota, cost, latency, quality and fallback-risk fields from the selected policy candidate.
- Self-run cycle reports now include `selected_candidate` and `fallback_order`, and the Markdown report renders the selected provider/model plus the complete fallback path for human review and Telegram document publication.

## How It Was Worked On

- The cycle started from a partial dirty implementation in `src/executor.rs` and `src/self_evolve.rs`.
- RED was captured with `cargo test self_run_reports_quota_aware_executor_policy_for_cycle -- --nocapture`; the partial implementation failed to compile because candidate functions and store propagation were incomplete.
- RED was captured for the selected/fallback report increment with `cargo test self_run_reports_quota_aware_executor_policy_for_cycle`; the report returned `Null` for `selected_candidate`.
- RED was captured for Markdown visibility with `cargo test self_run_persists_markdown_executor_policy_report_for_human_review`; the Markdown report lacked the selected quota-aware candidate line.
- The fix propagated `ForgeStore` into self-evolution strategy selection, completed the missing OpenCode candidate split and updated contract expectations.
- The follow-up fix derived selected/fallback summaries from the sorted policy candidates and rendered them in the Markdown report before the detailed candidate table.

## Product Impact

This moves Forge toward v0.5 by making executor choice a quota-aware product decision instead of an implicit fallback list. High-value PM/business/creative reasoning can prefer non-local capability when the expected value justifies quota, while low-value deterministic or repetitive work can preserve quota and use local capacity.

The selected/fallback summary also improves operator trust: a Product/PM user can see exactly which executor/provider/model Forge would try first, which choices preserve or consume quota, and what fallback sequence will be used if the first capability fails.

## Validation

- `cargo test self_run_reports_quota_aware_executor_policy_for_cycle -- --nocapture`: passed.
- `cargo test self_run_`: passed.
- `cargo test executor_policy -- --nocapture`: passed.
- `cargo test test_executor_strategy_preserves_quota_cost_fields_for_attempt_reports -- --nocapture`: passed.
- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test`: passed.
- `cargo build --release`: passed.
- CLI smoke `forge plan --goal "Create a delivery platform" --output json`: passed.
- CLI smoke `forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v04162-2`: passed.

## Install Status

- `cargo install --path . --force`: blocked by sandbox filesystem policy on `/home/arthur/.cargo/.crates.toml` (`Read-only file system`).
- `CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --locked --offline`: passed and replaced local Forge 0.4.161 with 0.4.162.

## Publication Status

- `gh auth token`: available; token value was not exposed.
- `git remote get-url origin`: `https://github.com/cardozoarthur/forge-core.git`.
- `git commit -m "Expose self-evolution executor fallback decision"`: blocked because `.git/index.lock` could not be created on a read-only filesystem.
- `git push` was not attempted because there is no new local commit in this sandbox.

## Remaining Work

- Add live Gemini/OpenCode non-interactive probing so Forge records auth/model/approval failures before handoff.
- Add native scheduled GitHub/Telegram publication with persisted Markdown report delivery evidence.
