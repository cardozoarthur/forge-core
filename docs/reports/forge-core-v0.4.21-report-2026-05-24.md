# Forge Core v0.4.21 Report - 2026-05-24

## Objective

Advance the Context Routing Engine with auditable per-shard routing decisions so executor adapters can understand why each context shard was included, compressed or omitted.

## Change

`forge context` now emits schema `forge.context.v8` with routing policy `task_local_revisioned_persona_compressed_executor_policy_subflow_budget_decisions_v8`.

Each context shard now includes:

- `routing_decision`: `included_full`, `included_compressed`, `omitted_profile` or `omitted_budget`;
- `decision_reason`: a concise human-readable explanation for that decision.

Budget-omitted shards now report `bytes = 0` and hash the empty selected payload, reflecting that no shard content was sent to the executor. This makes shard telemetry match executor-visible context instead of the candidate source text.

## Safety

The change is metadata-only. It does not execute local Python or Node.js code, authorize external CLIs, mutate Docker/Kubernetes/Knative resources, alter child-subflow state or bypass validation gates.

Profile exclusions remain deterministic and are driven by the executor profile selected from Forge-owned task metadata.

## Validation

Focused TDD evidence from this cycle:

- RED: `cargo test context_shards_explain_selection_decisions_for_budget_and_profile_routing` failed because `forge context` still emitted `forge.context.v7`.
- GREEN: the same focused test passed after schema v8 and shard routing decisions were implemented.

Full validation from this cycle:

- `cargo fmt --check`: passed;
- `cargo clippy --all-targets --all-features -- -D warnings`: passed;
- `cargo test`: passed, 46 CLI contract tests;
- `cargo build --release`: passed;
- CLI smoke `./target/release/forge --store /tmp/forge-core-v0421-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json`: passed;
- CLI smoke `./target/release/forge --store /tmp/forge-core-v0421-skill-smoke.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v0421`: passed.

## Installation

- `cargo install --path . --force`: blocked because `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem in this execution environment.
- `cargo install --path . --force --root .forge/local-install --offline`: passed.
- `.forge/local-install/bin/forge --version`: `forge 0.4.21`.
- The currently resolved user PATH binary remains `/home/arthur/.cargo/bin/forge` at `forge 0.4.20` until the global cargo directory is writable.

## Publication Attempt

- `gh auth token`: passed locally; token value was not recorded.
- `git remote get-url origin`: `https://github.com/cardozoarthur/forge-core.git`.
- `git commit -m "feat: audit context shard routing decisions"`: blocked because Git could not create `.git/index.lock` on a read-only filesystem.
- `git push`: blocked because this execution environment could not resolve `github.com`.

## Next Recommended Cycle

Refresh proposed child-subflow lifecycle state during `forge context`, then surface stale or no-longer-attachable child-subflow bindings as validation rework reasons before executor routing.
