# Forge Core v0.4.165 Report - 2026-06-02

## What Changed

- Self-evolution executor policy now converts persisted low-quota or high-rate-limit observations into `skipped_quota_preservation`.
- The selected self-evolution candidate now falls through past scarce non-local capacity instead of spending it just because the executor is early in the fallback chain.
- Policy reports keep the quota observation evidence and add an explicit preservation reason so operators can see why a provider/model was skipped.

## Product Decision

This cycle prioritized quota-aware executor selection over new PM/TUI features because wasted non-local quota directly reduces Forge's ability to make strong product and business decisions later. The change improves v0.5 progress by preserving scarce remote reasoning capacity for high-value PM, business, creative and strategy work while still allowing deterministic work or lower-value tasks to continue through the next eligible executor.

## Validation Results

- RED observed first: `cargo test self_evolve::tests::test_executor_policy_skips_non_local_candidate_when_quota_is_low` failed because the low-quota OpenCode non-local candidate remained `eligible`.
- Targeted GREEN passed: `cargo test self_evolve::tests::test_executor_policy_skips_non_local_candidate_when_quota_is_low`.
- Focused self-evolution coverage passed: `cargo test self_evolve::tests::`.
- Focused CLI contract coverage passed: `cargo test self_run_reports_quota_aware_executor_policy_for_cycle`.
- Required suite passed: `cargo fmt --check`.
- Required suite passed: `cargo clippy --all-targets --all-features -- -D warnings`.
- Required suite passed: `cargo test` with 79 unit tests, 240 CLI contract tests and doc-tests.
- Required suite passed: `cargo build --release`.
- CLI smoke passed: `./target/release/forge plan --goal "Create a delivery platform" --output json`.
- CLI smoke passed: `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v04165-2`.
- Default `cargo install --path . --force` was blocked by read-only `/home/arthur/.cargo/.crates.toml`.
- Workspace fallback install passed with `CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --locked --offline`.
- The fallback binary reports `forge 0.4.165`.

## Safety

- No Docker, Kubernetes or Knative resources were installed or mutated.
- No Telegram token, chat id or secret is recorded in this report.
- The change only affects Forge-owned executor policy/reporting behavior and tests.

## Publication Status

- `gh auth token` succeeded without recording the token in this report.
- `git remote get-url origin` returned `https://github.com/cardozoarthur/forge-core.git`.
- `git add ...` was attempted after validation and blocked because `.git/index.lock` cannot be created on the read-only git filesystem.
- No commit or push was created from this environment.

## Next Cycle

Expose quota-preservation decisions in active workflow status or inspect output so a human can see, during a running self-evolution loop, which model/executor was skipped, which quota/cost assumption caused the skip and which fallback was selected.
