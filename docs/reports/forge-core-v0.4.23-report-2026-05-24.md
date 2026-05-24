# Forge Core v0.4.23 Report - 2026-05-24

## Objective

Improve the Context Routing Engine with a compact, auditable cost and routing-pressure summary for each executor context package.

## Change

`forge context` now emits schema `forge.context.v10` with routing policy `task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_budget_summary_v10`.

Each context package now includes a top-level `routing_summary` derived from the emitted shard manifest:

- total, included and omitted shard counts;
- compressed, profile-omitted and budget-omitted shard counts;
- selected bytes and original candidate bytes;
- omitted bytes and compression-saved bytes;
- effective budget, remaining budget and budget utilization in basis points.

This keeps the full per-shard audit trail intact while giving executor adapters and operators a stable aggregate contract for cost dashboards, continuation decisions and context-quality validation.

## Safety

The routing summary is read-only metadata. It does not mutate workflow state, complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, or touch Docker/Kubernetes/Knative resources.

The summary is computed only after deterministic shard selection is complete, so it cannot alter profile omissions, budget omissions, checkpoint freshness, child-subflow routing or validation gates.

## TDD Evidence

- RED: `cargo test context_package_summarizes_routing_decisions_for_executor_cost_audit` failed because the existing context package still emitted `forge.context.v9`.
- GREEN: the focused test passed after schema v10 and `routing_summary` were implemented from the shard manifest.

## Validation

- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test`: passed, 49 CLI contract tests.
- `cargo build --release`: passed.
- CLI smoke `./target/release/forge --store /tmp/forge-core-v0423-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json`: passed.
- CLI smoke `./target/release/forge --store /tmp/forge-core-v0423-skill-smoke.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v0423`: passed.

## Installation

- `cargo install --path . --force`: blocked because `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem in this execution environment.
- `cargo install --path . --force --root .forge/local-install --offline`: passed.
- `.forge/local-install/bin/forge --version`: `forge 0.4.23`.
- The default `forge` on PATH still resolves to `/home/arthur/.cargo/bin/forge` and reports `forge 0.4.22`.

## Publication

- `gh auth token`: passed with output suppressed to avoid exposing the token.
- `git remote get-url origin`: `https://github.com/cardozoarthur/forge-core.git`.
- Local `.git` is mounted read-only, so a normal `git add`/`git commit` could not create `.git/index.lock`.
- Publish commits can be created through a temporary index and object database in `/tmp`, but the local read-only `.git` mount prevents recording them in this checkout.
- `git push` to `https://github.com/cardozoarthur/forge-core.git` was blocked because DNS could not resolve `github.com` in this execution environment.

## Next Recommended Cycle

Use `routing_summary` to introduce validation warnings for underfilled, over-compressed or profile-starved context packages, then expose those warnings through `forge inspect` and `forge request status` without changing executor selection.
