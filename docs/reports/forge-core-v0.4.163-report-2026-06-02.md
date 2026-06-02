# Forge Core v0.4.163 Report - 2026-06-02

## What Changed

- Executor sync state now records `non_interactive_ready` and `probe_evidence` for installed CLIs.
- Non-interactive probes are bounded by short timeouts so `forge sync` does not hang when a CLI waits for auth, approval or model selection.
- `forge executors` and sync reports now exclude allowed-but-interactive executors from `usable`.
- Quota policy candidates surface interactive hang risk as `skipped_interactive_hang_risk` with probe evidence.
- Legacy executor records without probe fields still load through serde defaults, so existing Forge stores do not break after upgrade.

## How It Was Worked On

- The cycle prioritized the live human correction that executor selection must become quota-aware, non-interactive and visible before further PM/TUI work.
- The implementation kept Forge as the source of truth for executor policy and made sync/status evidence deterministic.
- A focused regression test was added for interactive hang-risk exclusion from usable executors.
- A compatibility test was added for older persisted executor records that do not yet include probe fields.

## Product Impact

This moves Forge toward v0.5 by reducing wasted self-evolution cycles on executor timeouts. Forge can now show an operator whether OpenCode, Gemini, Codex or local capacity is actually usable for non-interactive work before handoff, and it can turn interactive readiness failures into repair goals instead of repeating failed attempts.

The decision quality improves because executor choice is no longer only a technical fallback list. The visible status connects provider/model readiness, quota/cost assumptions, product/business suitability and fallback risk to the next execution decision.

## Validation

- `cargo test executor::tests`: passed.
- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test`: passed.
- `cargo build --release`: passed.
- CLI smoke `./target/release/forge plan --goal "Create a delivery platform" --output json`: passed.
- CLI smoke `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`: passed.

## Install Status

- `cargo install --path . --force`: blocked by sandbox filesystem policy on `/home/arthur/.cargo/.crates.toml` (`Read-only file system`).
- The release binary at `./target/release/forge` was built and used for smoke validation.

## Publication Status

- `gh auth token`: available; token value was not exposed.
- `git remote get-url origin`: `https://github.com/cardozoarthur/forge-core.git`.
- `git commit -m "Harden executor non-interactive readiness"`: blocked because `.git/index.lock` could not be created on a read-only filesystem.
- `git push` was not attempted because there is no new local commit in this sandbox.

## Remaining Work

- Persist measured quota/rate-limit observations from actual executor attempts, not only estimated policy.
- Add explicit Gemini probe repair goals for auth/model/approval prompts.
- Replace the operational publication bridge with a native Forge scheduled workflow that persists the last pushed commit and Telegram document evidence.
