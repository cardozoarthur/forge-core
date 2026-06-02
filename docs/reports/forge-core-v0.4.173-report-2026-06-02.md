# Forge Core v0.4.173 Report - 2026-06-02

Run id: `run_8c9d3e943a204d03a16a1d4b33f71e63`
Workflow id: `wf_bcbca0bc466649c2a1322ae30220f420`
Cycle: 3

## Product Decision

This cycle keeps the required self-evolution repair goal focused on quota-aware executor/model policy. The product decision is to make quota and cost evidence writable through Forge's CLI, not only inferred from environment variables or internal tests. The business value is practical governance: operators and future recurring workflows can record low quota, rate-limit risk, paid usage or high-value suitability before executor selection spends scarce non-local capacity.

## Change

- Added `forge executor-quota record` with explicit fields for executor, provider, model, locality, free/paid classification, remaining quota, rate-limit risk, cost, latency, expected quality, suitability, source and observed time.
- Persisted recorded observations in the existing `executor_quotas` store and recorded a `_system` event with schema `forge.executor_quota_record.v1`.
- Kept `forge executors` as the read surface; quota policy candidates now pick up the CLI-recorded observation for matching provider/model candidates.
- Added CLI contract coverage proving recorded Codex quota evidence appears in `observed_quota_evidence`, updates the Codex candidate model/quota fields and contributes audit evidence.
- Bumped Forge Core to `0.4.173` and updated the changelog.

## Validation Plan

- `cargo test executor_quota_record_persists_observation_for_policy_reporting`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`
- `./target/release/forge plan --goal "Create a delivery platform" --output json`
- `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-8c9d3e-cycle3`

## Validation Result

Passed:

- `cargo test executor_quota_record_persists_observation_for_policy_reporting`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test` with 82 unit tests, 242 CLI contract tests and doctests.
- `cargo build --release`
- `./target/release/forge plan --goal "Create a delivery platform" --output json`
- `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-8c9d3e-cycle3-v2`

Post-validation install:

- `cargo install --path . --force` was attempted and failed because this sandbox cannot write `/home/arthur/.cargo/.crates.toml` (`Read-only file system`, os error 30).

Executor/model evidence from the smoke:

- Codex was installed, configured, human-authorized and non-interactive ready.
- OpenCode was installed and human-authorized, but model listing failed with `PRAGMA wal_checkpoint(PASSIVE)`, so the quota policy marked OpenCode candidates as `skipped_interactive_hang_risk`.
- Gemini was installed, but `GEMINI_API_KEY` was not present in the smoke environment, so Gemini was not configured for non-interactive use.

Forge artifact lineage:

- Attached this report to workflow `wf_bcbca0bc466649c2a1322ae30220f420` as `artifact_7c0df5766410423cb70a9b26b1c4270f`; workflow revision advanced to 4.

Publication:

- `gh auth token` succeeded without exposing the token in the report.
- `git remote get-url origin` returned `https://github.com/cardozoarthur/forge-core.git`.
- `git add CHANGELOG.md Cargo.lock Cargo.toml docs/reports/forge-core-v0.4.173-report-2026-06-02.md src/main.rs tests/forge_cli_contract.rs` was attempted after validation and failed because Git could not create `/home/arthur/projects/forge-core/.git/index.lock` (`Sistema de ficheiros só de leitura`).
- No commit or push was attempted after that blocker because there was no validated local commit to publish.
- The allowed `.forge/github-telegram-publisher/watch.mjs` bridge was run once with `FORGE_PUBLISH_ONCE=1`. It generated a deterministic periodic Markdown report path under `.forge/github-telegram-publisher/reports/`, but push failed with `Could not resolve host: github.com`, AI report generation fell back because Codex could not initialize under the read-only filesystem, and Telegram delivery failed because the sandbox could not reach the Paperclip Kubernetes API at `127.0.0.1:6443`.

## v0.5 Movement

This advances quota-aware executor policy and better business/product decision support. It gives the Product/PM CLI-TUI and self-evolution loop a durable way to capture why Forge should spend or preserve OpenCode/Gemini/Codex/local capacity for product, business and creative work. It also reduces repeated timeout waste by making executor conditions explicit before fallback selection.

## Remaining Work

- Convert OpenCode/Gemini readiness repair goals into ordinary runnable validation tasks with concrete non-interactive probe commands and remediation status.
- Add native scheduled GitHub/Telegram publication with persisted Markdown report metadata and delivery evidence.
- Continue making the Product/PM CLI-TUI the main human-guided workflow creation entry point.
- Keep improving internal recurring self-evolution status so loop control, 180s rest, next-goal rationale and validation evidence are visible from normal inspect/status paths.
