# Forge Core v0.4.20 Report - 2026-05-23

## Objective

Advance the Context Routing Engine so proposed child-subflow reuse decisions are carried into the task-local context package that executor adapters consume.

## Change

`forge context` now emits schema `forge.context.v7` with routing policy `task_local_revisioned_persona_compressed_executor_policy_subflow_budget_v7`.

When a task has proposed child subflows, the context packet now includes:

- `child_subflow_count`;
- structured `child_subflows` metadata copied from the persisted task;
- a compact `child_subflows` shard sourced from `subflow_registry`;
- deterministic no-AI profile priority for child-subflow bindings immediately after execution policy.

This closes the gap between the workflow registry and executor context. A reused deterministic code node no longer has to infer the proposed child-subflow relationship from broad workflow history or a separate inspect call.

## Safety

The new routing is metadata-only. It does not execute local Python or Node.js code, mark reused child subflows complete, promote a child workflow, authorize external CLIs or mutate Docker, Kubernetes or Knative resources.

The compact executor-facing shard only contains the child workflow/task id and binding status. The full audit surface remains in structured `child_subflows`, including lifecycle state, reuse key, lineage hash, validation gate and reason.

## Validation

Focused TDD evidence from this cycle:

- RED: `cargo test context_package_carries_proposed_child_subflow_routing_for_reused_nodes` failed because context still emitted `forge.context.v6`.
- GREEN: the same focused test passed after schema v7 and child-subflow routing were implemented.
- Regression: `cargo test` passed with 45 CLI contract tests.

Full release validation from this cycle:

- `cargo fmt --check`: passed;
- `cargo clippy --all-targets --all-features -- -D warnings`: passed;
- `cargo test`: passed, 45 CLI contract tests;
- `cargo build --release`: passed;
- CLI smoke `./target/release/forge --store /tmp/forge-core-v0420-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json`: passed;
- CLI smoke `./target/release/forge --store /tmp/forge-core-v0420-skill-smoke.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v0420`: passed.

## Installation

- `cargo install --path . --force`: blocked because `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem in this execution environment.
- `cargo install --path . --force --root .forge/local-install`: blocked by restricted network DNS while trying to read the crates.io index.
- `cargo install --path . --force --root .forge/local-install --offline`: passed.
- `.forge/local-install/bin/forge --version`: `forge 0.4.20`.

## Publication Attempt

- `gh auth token`: passed locally; token value was not recorded.
- `git remote get-url origin`: `https://github.com/cardozoarthur/forge-core.git`.
- `git add ...`: blocked because Git could not create `.git/index.lock` on a read-only filesystem.
- `git push`: blocked because this environment could not resolve `github.com`.

## Next Recommended Cycle

Add an activation gate for proposed child subflows: require validation before a proposed binding can become active, refresh child lifecycle state during context generation, and make stale child-subflow bindings visible as rework reasons instead of silently routing them.
