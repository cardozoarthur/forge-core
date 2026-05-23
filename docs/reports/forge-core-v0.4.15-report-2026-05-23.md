# Forge Core v0.4.15 Report - 2026-05-23

## Objective

Advance the Context Routing Engine so tight executor budgets preserve the most useful high-priority context instead of dropping oversized shards outright.

## Change

`forge context` now emits schema `forge.context.v4` with routing policy `task_local_revisioned_persona_compressed_budget_v4`.

When a shard does not fit in the remaining budget, Forge now builds a deterministic compressed payload from that shard's summary and includes it if the compressed form fits. The shard manifest records:

- `compressed`: whether the executor received a summary payload instead of the full shard;
- `bytes`: bytes selected for the emitted or audited payload;
- `original_bytes`: bytes in the original full shard;
- `content_sha256`: checksum of the selected payload.

This keeps context selection deterministic, auditable and bounded while reducing avoidable loss of high-priority workflow state.

## Safety

The change is local to context packet construction. It does not mutate workflow state, executor authorization, runtime substrates, Docker, Kubernetes, Knative or external resources. Persona routing and lineage remain node-scoped and auditable.

## Validation

Added a CLI contract test for compressed shard fallback:

- plans a workflow with an oversized workflow goal;
- requests a tight 420-byte context package;
- verifies `workflow_goal` is included as a compressed shard;
- verifies the package stays within budget and records `compressed` plus `original_bytes`.

Fresh validation run in this cycle:

- `cargo fmt --check`: passed;
- `cargo clippy --all-targets --all-features -- -D warnings`: passed;
- `cargo test`: passed, 39 CLI contract tests;
- `cargo build --release`: passed;
- CLI smoke `forge plan --goal "Create a delivery platform" --output json`: passed with the release binary and a temporary store;
- CLI smoke `forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke...`: passed with the release binary and a temporary store. Executor and runtime use stayed unauthorized pending human approval.

Post-validation local installation:

- `cargo install --path . --force`: blocked because `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem in this execution environment;
- `cargo install --path . --force --root .forge/local-install --offline`: passed;
- `.forge/local-install/bin/forge --version`: `forge 0.4.15`.

Publication attempt:

- `gh auth token`: passed locally, token value not logged;
- `git remote get-url origin`: `https://github.com/cardozoarthur/forge-core.git`;
- `git add ...`: blocked because Git could not create `.git/index.lock` on the read-only Git metadata path;
- `git push`: blocked because this environment could not resolve `github.com`.

## Next Recommended Cycle

Add executor-aware context profiles so deterministic command, Python and Node.js nodes receive a smaller no-AI context envelope while AI and human-facing nodes keep richer reasoning and persona shards.
