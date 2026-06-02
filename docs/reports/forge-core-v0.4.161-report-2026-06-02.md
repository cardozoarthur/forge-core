# Forge Core 0.4.161 Self-Evolution Report

## Summary

This cycle makes quota-aware executor selection visible at the actual attempt level. Forge already planned executor candidates with provider, locality, quota, cost, quality and fallback-risk fields; now the selected execution strategy carries those same fields into `executor_attempts` so self-evolution reports explain why each attempted provider/model was selected or skipped during fallback.

## Behavior Changed

- `forge.self_evolution.executor_policy.v1` now exposes an explicit `decision_factors` list for quota-aware selection.
- `executor_attempts` now records:
  - remaining quota assumption;
  - rate-limit risk;
  - monetary/token cost;
  - expected quality;
  - fallback risk.
- The executable strategy path preserves candidate quota/cost fields instead of reducing attempts to only executor, provider, model, local flag and tier.

## Business/Product Decision

The product decision is to make executor choice accountable in the report artifact users inspect after a cycle. For Forge v0.5, using Gemini, Codex, OpenCode or a local model is not just a technical fallback; it is a business choice about quota, cost, speed, expected reasoning quality and whether the task is valuable enough to spend non-local capacity. This change makes that decision legible when a cycle succeeds, fails or falls back.

## Validation Evidence

- Targeted tests passed:
  - `cargo test executor_policy -- --nocapture`
  - `cargo test test_executor_strategy_preserves_quota_cost_fields_for_attempt_reports -- --nocapture`
- Required validation passed:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test`
  - `cargo build --release`
- CLI smoke passed:
  - `./target/release/forge plan --goal "Create a delivery platform" --output json`
  - `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-cycle6`
- Local install:
  - `cargo install --path . --force` was blocked by read-only `/home/arthur/.cargo/.crates.toml`.
  - `CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --locked` was interrupted after repeated restricted-network crates.io index retries.
  - `CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --locked --offline` succeeded.
  - `.forge/local-install/bin/forge --version` returned `forge 0.4.161`.

## Product Boundary

This moves Forge closer to v0.5 by strengthening quota-aware executor policy and reliable agent switching evidence. It does not yet probe live provider quotas, repair Gemini interactive mode automatically, or replace the operational GitHub/Telegram bridge with a native scheduled workflow notification node.

## Publication Status

- `gh auth token` succeeded without exposing the token.
- `git remote get-url origin` returned `https://github.com/cardozoarthur/forge-core.git`.
- `git commit -m "chore: forge self evolution cycle 6"` was blocked because `.git/index.lock` could not be created on a read-only filesystem.
- No push was attempted because no commit could be created in this sandbox.

## Safety

No external Docker, Kubernetes, Knative, model runtime or Telegram resources were modified. The code change is limited to Forge Core self-evolution report structures, strategy propagation and tests.
