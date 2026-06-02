# Forge Core v0.4.167 Report - 2026-06-02

## What Changed

- Self-evolution executor repair guidance is now persisted into the workflow as ordinary repair tasks when the executor policy is not stable.
- The persisted repair tasks capture executor policy status, timeout evidence and probe failure report requirements, then depend on the workflow's latest task so repair work remains part of the DAG.
- Cycle Markdown reports now expose executor expected quality, product/business suitability and attempt rationale directly in the human-readable artifact.

## Product Decision

This cycle prioritized executor repair persistence over more PM/TUI surface area because repeated OpenCode or Gemini interactive failures block autonomous improvement and waste high-value reasoning cycles. Turning repair guidance into workflow tasks improves business outcome by making the next action executable and inspectable instead of burying it in a report.

## Validation Results

- Targeted test passed: `cargo test self_evolve::tests::test_ensure_executor_repair_goals_persists_tasks`.
- Required validation passed: `cargo fmt --check`.
- Required validation passed: `cargo clippy --all-targets --all-features -- -D warnings`.
- Required validation passed: `cargo test` with 80 unit tests, 240 CLI contract tests and doc-tests.
- Required validation passed: `cargo build --release`.
- CLI smoke passed: `./target/release/forge plan --goal "Create a delivery platform" --output json`.
- CLI smoke passed: `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v04167b`.
- The skill smoke classified OpenCode as `skipped_interactive_hang_risk` after provider/model probe failure, classified Gemini as not configured, and left Codex as the usable non-local fallback.
- Default `cargo install --path . --force` was blocked by read-only `/home/arthur/.cargo/.crates.toml`.
- Workspace fallback install passed with `CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --locked --offline`.
- The fallback binary reports `forge 0.4.167`.

## Safety

- No Docker, Kubernetes or Knative resources were installed or mutated.
- No Telegram token, chat id or secret is recorded in this report.
- The change only affects Forge-owned self-evolution executor policy persistence, report rendering, tests, version metadata and report artifacts.

## Publication Status

- GitHub publication should run after this report is committed.
- The expected publication contract is `gh auth token`, `git remote get-url origin`, then `git push`.
- Telegram publication should send this Markdown report as a document through the existing allowed local publisher or a future Forge-owned scheduled notification node.

## v0.5 Movement

- Real-time agent runtime: executor repair needs become durable workflow work instead of transient report text.
- Advanced TUI/status readiness: repair tasks can be listed and inspected as ordinary workflow state.
- Quota-aware executor policy: reports now show quality, suitability, quota and rationale together.
- Better business/product decisions: Forge preserves scarce non-local reasoning capacity by making timeout repair a product priority before more feature work.

## Next Cycle

Surface persisted executor repair tasks in `forge status` and `forge inspect`, then add concrete validation gates for OpenCode provider/model probes and Gemini non-interactive auth/model/approval detection.
