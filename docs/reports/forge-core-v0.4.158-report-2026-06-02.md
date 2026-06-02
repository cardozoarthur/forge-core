# Forge Core 0.4.158 Self-Evolution Report

## Summary

This cycle makes the self-evolution run shape auditable as ordinary Forge workflow state. A self-run now reports and persists its internal recurring loop, about-three-minute rest interval and next-goal product decision before executor work continues.

## Behavior Added

- `forge self run` returns `internal_loop` evidence with schema, loop count, loop-control task, loop-control kind, execution shape, sleep interval and rest policy.
- Self-evolution stores the selected next goal as a durable `ProductDecision` and records a `self_evolution_next_goal_decision` workflow revision.
- The next-goal decision includes product/business rationale, alternatives, trade-offs, success metrics and backlog mutation intent.
- Prompt packets now show the internal loop state and selected next goal so OpenCode/Codex/Gemini executors receive the same state that Forge reports.

## Behavior Changed

- `--sleep-seconds` for `forge self run` defaults to `180`, matching the required about-three-minute rest between iterations.
- Self-evolution loop normalization adds a dedicated `while_until` self-evolution loop node even when the planner already produced other loop nodes.

## Validation Evidence

- RED observed first with `cargo test self_run_persists_internal_recurring_loop_status_and_next_goal_decision --test forge_cli_contract`: the JSON report did not expose `internal_loop`.
- Targeted GREEN passed for `cargo test self_run_persists_internal_recurring_loop_status_and_next_goal_decision --test forge_cli_contract`.
- Self-run regression contracts passed for `cargo test self_run_ --test forge_cli_contract`.
- Required validation passed: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, and `cargo build --release`.
- CLI smokes passed with the installed workspace binary: `forge plan --goal "Create a delivery platform" --output json` and `forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.158`.
- Default `cargo install --path . --force` was blocked by read-only `/home/arthur/.cargo`; validated fallback install succeeded with `CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --locked --offline`, and `.forge/local-install/bin/forge --version` returned `forge 0.4.158`.

## Product Boundary

This version improves the recurrence requirement but is not 0.5 promotion-complete. The next useful cycle is to make PM/TUI decisions mutate backlog/tasks from the durable decision artifact, with validation evidence and human steering preserved.

## Safety

The change is scoped to Forge Core source, tests, changelog and report artifacts. It does not install Knative, mutate Docker/Kubernetes/Knative resources or modify external infrastructure.
