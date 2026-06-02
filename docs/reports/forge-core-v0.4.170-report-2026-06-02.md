# Forge Core v0.4.170 Report - 2026-06-02

Run id: `run_9ff8a6cdf43a4539a9d3245c5d4d403a`
Workflow id: `wf_dfa9a20f8ade43a69fb82cef22d0ba1a`
Cycle: 19

## Product Decision

This cycle keeps the focus on quota-aware executor policy before more PM/TUI work. The product value is operational: a human or agent inspecting a self-evolution run should see why Forge selected OpenCode, Gemini, Codex or local/Ollama, what fallback order remains, what quota-preservation rule applies and what repair goal is blocking better executor use without opening cycle artifacts by hand.

## Change

- Added `latest_executor_policy` to `forge request status` by projecting the latest self-evolution cycle report artifact.
- The summary includes schema, artifact path/checksum, cycle, requested executor, selected executor, selected candidate, fallback order, active repair status, quota preservation rules and repair goals.
- Updated the agent handoff polling contract so `latest_executor_policy` is part of the normal status surface.
- Added CLI contract coverage proving a dry-run self-evolution cycle is inspectable through `forge request status`.

## Validation Plan

- `cargo test request_status_surfaces_latest_self_evolution_executor_policy_summary --test forge_cli_contract`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`
- `./target/release/forge plan --goal "Create a delivery platform" --output json`
- `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`

## Validation Result

Passed:

- RED: `cargo test request_status_surfaces_latest_self_evolution_executor_policy_summary --test forge_cli_contract` failed before implementation because `latest_executor_policy` was absent.
- GREEN: `cargo test request_status_surfaces_latest_self_evolution_executor_policy_summary --test forge_cli_contract` passed after implementation.
- Required validation passed: `cargo fmt --check`.
- Required validation passed: `cargo clippy --all-targets --all-features -- -D warnings`.
- Required validation passed: `cargo test` with 81 unit tests, 241 CLI contract tests and doctests.
- Required validation passed: `cargo build --release`.
- CLI smoke passed: `./target/release/forge plan --goal "Create a delivery platform" --output json`.
- CLI smoke passed: `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`.

Post-validation install:

- `cargo install --path . --force` was attempted and failed because this sandbox cannot write `/home/arthur/.cargo/.crates.toml` (`Read-only file system`, os error 30).
- The validated binary is available at `./target/release/forge` and reports version `0.4.170`.

Executor/model evidence:

- The skill smoke classified Codex as usable.
- Gemini was installed but not configured because `GEMINI_API_KEY`/`GOOGLE_API_KEY` was not available in the environment, so it was not non-interactive ready.
- OpenCode was installed and configured but its `opencode models` probe failed with a SQLite WAL checkpoint error, so Forge marked OpenCode candidates as `skipped_interactive_hang_risk` and emitted a concrete repair goal.

Publication:

- `gh auth token` succeeded without exposing the token.
- `git remote get-url origin` returned `https://github.com/cardozoarthur/forge-core.git`.
- `git commit -m "feat: surface self-evolution executor policy in request status"` was attempted after validation and failed because Git could not create `/home/arthur/projects/forge-core/.git/index.lock` (`Sistema de ficheiros só de leitura`).
- No push was attempted because no commit was created.

## v0.5 Movement

This advances the real-time agent runtime and quota-aware executor policy by moving model/provider decisions into the ordinary run status path. It supports better business/product decisions by making quota spend, fallback risk and repair blockers visible at the operator surface where steering decisions happen.

## Remaining Work

- Attach concrete validation gates and remediation commands to OpenCode/Gemini non-interactive repair tasks.
- Add status/inspect visualization for persisted executor repair tasks in the DAG view.
- Continue improving Product/PM CLI-TUI and durable decision artifacts as the main entry point for guided workflow creation.
