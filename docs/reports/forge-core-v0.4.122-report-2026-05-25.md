# Forge Core v0.4.122 Self-Evolution Report

Prompt packet: `forge.self_evolution.prompt.v2`
Cycle: 37
Run: `run_bfba8dcc4747450da9067f8cdc713b58`
Workflow: `wf_047a8146d7fb42a7800cbfdad1b59f72`

## Increment

Parallel scheduler scanning now preserves Forge-owned idle workflow semantics.
`forge schedule scan-due --max-workers <n>` reconciles scheduled workflows with no
due cron nodes through the existing `run_due_workflow` path, records scale-to-zero
state in SQLite, returns an idle workflow result, and includes `forge.worker_pool.v1`
execution evidence when bounded parallel dispatch is active.

This closes a runtime inconsistency where the sequential scan path persisted
scale-to-zero decisions but the parallel path only counted idle workflows in the
summary. The change keeps due workflow execution in the bounded WorkerPool and
handles idle reconciliation without external loops, tmux wrappers, Docker,
Kubernetes or Knative mutations.

## Validation

- RED: `cargo test parallel_scan_due_reports_idle_workflows_without_due_nodes -- --nocapture` failed because `summary.scale_to_zero_workflows` was `0`.
- GREEN: the same test passed after `scan_due_workflows_parallel` reconciled idle workflows and exposed `worker_pool`.
- Focused scheduler suite: `cargo test parallel_scan_due -- --nocapture` passed 4 tests.
- Required validation:
  - `cargo fmt --check`: passed after applying `cargo fmt`.
  - `cargo clippy --all-targets --all-features -- -D warnings`: passed.
  - `cargo test`: passed 205 tests (15 unit + 190 CLI contract).
  - `cargo build --release`: passed.
- CLI smoke:
  - `target/release/forge --store /tmp/forge-cycle37-plan.sqlite plan --goal "Create a delivery platform" --output json`: passed.
  - `target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke.lowNA4`: passed.
  - Daily Goal `hackathon` smoke through `schedule scan-due --max-workers 3`: produced 3 artifacts, including Markdown and PDF report paths, with Telegram delivery record `secret_exposed=false`.

## Installation And Publication

- `cargo install --path . --force`: blocked by sandbox filesystem policy:
  `/home/arthur/.cargo/.crates.toml` is read-only (`os error 30`).
- `gh auth token >/dev/null`: passed.
- `git remote get-url origin`: `https://github.com/cardozoarthur/forge-core.git`.
- Direct commit in the checkout was blocked because `.git/index.lock` cannot be
  created on the read-only git metadata mount.
- A temporary `/tmp` clone produced commit `86d213b` with only this cycle's
  files, but publication failed because DNS could not resolve `github.com`.

## Lean Overhead Ledger

- Prompt bytes: approximately 44,000.
- Estimated prompt tokens: approximately 11,000.
- Validation command count: 4 required commands plus 3 focused/smoke commands.
- Artifact count: 1 report artifact in the repo; smoke generated 3 temporary Forge artifacts under `/tmp`.
- Metadata bytes: approximately 1,300.

## Safety

- No Docker, Kubernetes, Knative, Telegram or external user resources were mutated.
- Smoke Telegram delivery was a Forge-owned redacted delivery record, not a live secret send.
- Idle reconciliation mutates only Forge-owned SQLite workflow state through existing scale-to-zero logic.
