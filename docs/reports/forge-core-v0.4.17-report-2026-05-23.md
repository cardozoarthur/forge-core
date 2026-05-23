# Forge Core v0.4.17 Report - 2026-05-23

## Objective

Advance the Context Routing Engine and deterministic execution policy with a small structural increment: make Forge explicitly record when repeated or frequent no-AI work should run as a local Python/Node.js code node instead of being routed through a model call.

## Change

`forge context` now emits schema `forge.context.v6` with routing policy `task_local_revisioned_persona_compressed_executor_policy_budget_v6`.

Each atomic task now includes `execution_policy` metadata:

- `mode`: model executor, bounded mixed executor, deterministic executor or local code node;
- `ai_allowed`: whether the node may spend a model call;
- `deterministic`: whether the node should be handled as no-AI deterministic work;
- `code_runtime`: optional local runtime contract for Python or Node.js code nodes;
- `reuse_hint`: whether compatible code-node work should be reused for repeated/frequent tasks;
- `validation_gate`: the gate that must pass before promotion.

When a goal explicitly asks for repeated local Python or Node.js work without AI, Forge marks the deterministic non-AI step as `local_code_node`, records the local runtime entrypoint and routes that policy into the bounded context packet through both a top-level `execution_policy` field and an `execution_policy` shard.

## Safety

This is metadata and routing policy only. Planning does not execute local Python or Node.js code, does not authorize external CLIs, does not mutate Docker/Kubernetes/Knative resources and does not bypass validation gates. Deterministic code-node work remains Forge-owned and validation-gated through `deterministic_code_node_validation_required`.

The deterministic context envelope was adjusted so the current workflow goal remains visible after runtime mutations while also preserving execution policy and validation context.

## Validation

Fresh validation run in this cycle:

- `cargo fmt --check`: passed;
- `cargo clippy --all-targets --all-features -- -D warnings`: passed;
- `cargo test`: passed, 41 CLI contract tests;
- `cargo build --release`: passed;
- CLI smoke `forge plan --goal "Create a delivery platform" --output json`: passed with `.forge/local-install/bin` first on `PATH`;
- CLI smoke `forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`: passed with `.forge/local-install/bin` first on `PATH`.

Post-validation local installation:

- `cargo install --path . --force`: blocked because `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem in this execution environment;
- `cargo install --path . --force --root .forge/local-install --offline`: passed;
- `.forge/local-install/bin/forge --version`: `forge 0.4.17`.

Publication attempt:

- `git add ...`: blocked because Git could not create `.git/index.lock` on a read-only Git metadata path;
- `gh auth token`: passed locally, token value not logged;
- `git remote get-url origin`: `https://github.com/cardozoarthur/forge-core.git`;
- `git push`: blocked because this environment could not resolve `github.com`.

## Next Recommended Cycle

Persist reusable code-node registry entries keyed by language, validation gate and context lineage so Forge can reuse compatible deterministic subflows before creating new Python/Node.js work.
