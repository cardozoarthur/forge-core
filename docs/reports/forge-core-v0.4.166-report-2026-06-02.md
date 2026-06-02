# Forge Core v0.4.166 Report - 2026-06-02

## What Changed

- Self-evolution executor policy now treats an allowed executor with a failed non-interactive probe as an active repair condition.
- The aggregate policy status becomes `repair_needed_non_interactive_probe_failed` when candidates are marked `eligible_interactive_hang_risk`.
- Policy reports now include concrete repair goals such as repairing OpenCode provider/model probing before executor handoff, with the current probe evidence attached.

## Product Decision

This cycle prioritized executor repair visibility before additional PM/TUI work because repeated OpenCode or Gemini handoff failures waste cycle time and can consume scarce non-local quota without improving the product. The increment improves business-quality decision making by making the execution blocker visible as a repair goal before Forge selects a fallback.

## Validation Results

- RED observed first: `cargo test self_evolve::tests::test_executor_policy_uses_persisted_executor_readiness_before_selection -- --nocapture` failed because `active_repair_status` remained `stable`.
- Targeted GREEN passed: `cargo test self_evolve::tests::test_executor_policy_uses_persisted_executor_readiness_before_selection -- --nocapture`.
- Required suite passed: `cargo fmt --check`.
- Required suite passed: `cargo clippy --all-targets --all-features -- -D warnings`.
- Required suite passed: `cargo test` with 79 unit tests, 240 CLI contract tests and doc-tests.
- Required suite passed: `cargo build --release`.
- CLI smoke passed: `./target/release/forge plan --goal "Create a delivery platform" --output json`.
- CLI smoke passed: `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v04166`.
- The skill smoke reported OpenCode as `skipped_interactive_hang_risk` after a model probe failure and Codex as the usable executor.
- Default `cargo install --path . --force` was blocked by read-only `/home/arthur/.cargo/.crates.toml`.
- Workspace fallback install passed with `CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --locked --offline`.
- The fallback binary reports `forge 0.4.166`.

## Safety

- No Docker, Kubernetes or Knative resources were installed or mutated.
- No Telegram token, chat id or secret is recorded in this report.
- The change only affects Forge-owned self-evolution executor policy/reporting behavior, tests, version metadata and report artifacts.

## Publication Status

- `gh auth token` succeeded without recording the token in this report.
- `git remote get-url origin` returned `https://github.com/cardozoarthur/forge-core.git`.
- `git add ... && git commit -m "fix: surface non-interactive executor repair policy"` was attempted after validation and blocked because `.git/index.lock` cannot be created on the read-only git filesystem.
- No commit or push was created from this environment.
- Workflow artifact evidence: this report was attached to `wf_dfa9a20f8ade43a69fb82cef22d0ba1a` from Codex; the attachment advanced the workflow to revision 16.

## v0.5 Movement

- Real-time agent runtime: prevents repeated interactive handoff stalls from being treated as normal executor state.
- Advanced TUI/status readiness: exposes a status string that terminal inspection can surface directly.
- Quota-aware executor policy: records why a candidate was skipped and what repair should happen before retry.
- Better business/product decisions: preserves high-value reasoning cycles by turning executor readiness failure into explicit product work.

## Next Cycle

Persist executor repair goals as ordinary Forge workflow tasks with lineage, priority and validation gates, so self-evolution can repair OpenCode/Gemini configuration instead of only reporting that repair is needed.
