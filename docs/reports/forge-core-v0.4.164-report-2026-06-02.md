# Forge Core v0.4.164 Report - 2026-06-02

## What Changed

- Self-evolution executor policy now consumes persisted executor readiness from `forge sync executors` before selecting a candidate.
- Candidates with persisted evidence for missing install/config, missing human authorization, or failed non-interactive readiness are skipped before handoff.
- Self-run JSON reports now expose `forge.self_evolution.publication.v1` at cycle and run level.
- The publication report body includes the two-hour publication status, commit range, changed-file summary, validation/push status, current run/workflow state, remaining uncommitted work and v0.5 product movement.

## Product Decision

The cycle prioritized executor policy and publication evidence over more PM/TUI surface work because repeated executor timeouts and weak publication artifacts waste autonomous cycles. This improves business value by preserving quota for high-value reasoning, reducing failed runs, and making operator status/reporting more trustworthy.

## Validation Results

- Targeted policy contract passed: `cargo test self_run_reports_quota_aware_executor_policy_for_cycle --test forge_cli_contract`.
- Targeted persisted readiness guard passed: `cargo test test_executor_policy_uses_persisted_executor_readiness_before_selection --lib`.
- Targeted timeout repair guard passed: `cargo test test_executor_policy_detects_timeout_and_requires_repair --lib`.
- Required suite passed: `cargo fmt --check`.
- Required suite passed: `cargo clippy --all-targets --all-features -- -D warnings`.
- Required suite passed: `cargo test` with 78 unit tests, 240 CLI contract tests and doc-tests.
- Required suite passed: `cargo build --release`.
- CLI smoke passed: `./target/release/forge plan --goal "Create a delivery platform" --output json`.
- CLI smoke passed: `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`.

## Safety

- No Docker, Kubernetes or Knative resources were installed or mutated.
- No Telegram token, chat id or secret is recorded in this report.
- The publication struct can carry Telegram delivery evidence, but this cycle does not send Telegram.

## Install Status

- `cargo install --path . --force` was attempted and blocked by read-only `/home/arthur/.cargo/.crates.toml`.
- Workspace fallback install passed with `CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --locked --offline`.
- The fallback binary reports `forge 0.4.164`.

## Publication Status

- `gh auth token` succeeded without exposing the token.
- `git remote get-url origin` returned `https://github.com/cardozoarthur/forge-core.git`.
- `git add ... && git commit ... && git push origin HEAD:main` was attempted and blocked because `.git/index.lock` cannot be created on the read-only git filesystem.
- The local `.forge/github-telegram-publisher` bridge was attempted with a bounded timeout.
- The bridge wrote `.forge/github-telegram-publisher/reports/forge-core-periodic-2026-06-02T05-40-56-976Z.md`.
- GitHub push through the bridge failed with `Could not resolve host: github.com`.
- Telegram document delivery through the bridge failed because Kubernetes/Paperclip access to `127.0.0.1:6443` was blocked by `socket: operation not permitted`.

## Next Cycle

Replace the local publication bridge with a Forge-owned scheduled workflow/notification node that writes the Markdown report as an artifact, sends it as a Telegram document, persists delivery evidence, and records last-pushed commit state.
