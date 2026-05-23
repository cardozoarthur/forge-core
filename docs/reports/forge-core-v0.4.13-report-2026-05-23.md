# Forge Core v0.4.13 Report - 2026-05-23

## Objective

Turn the Personality/Soul Routing goal into a small executable contract in the Forge graph and context router.

## Change

Human-facing tasks now carry a `PersonaRoutingSpec` with mode, scope, instruction source, voice, tone, validation gate, source-model references and audit flag. The default graph assigns:

- `operator_report` to `Generate documentation`;
- `stakeholder_notice` to workflow cost email notifications.

`forge context` now emits schema `forge.context.v3` and routing policy `task_local_revisioned_persona_budget_v3`. Persona-aware task packets include:

- top-level `persona` metadata;
- a `persona_routing` shard in the bounded context manifest;
- `persona_mode_sha256` and `persona_scope` in lineage.

This keeps persona switching explicit, node-scoped and replayable without making a model provider or CLI the source of truth.

## Validation

Added CLI contract tests for:

- planned human-facing tasks carrying node-scoped persona routing metadata;
- context packets exposing persona metadata, shard selection and lineage hash.

Fresh validation run in this cycle:

- `cargo fmt --check`: passed;
- `cargo clippy --all-targets --all-features -- -D warnings`: passed;
- `cargo test`: passed, 37 CLI contract tests;
- `cargo build --release`: passed;
- CLI smoke `forge plan --goal "Create a delivery platform" --output json`: passed with the release binary and a temporary store;
- CLI smoke `forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`: passed with the release binary and a temporary store.

Post-validation operational blockers:

- `cargo install --path . --force` could not update the user-local install because `/home/arthur/.cargo` is mounted read-only in this execution sandbox.
- `git add` could not create `.git/index.lock` because `.git` is mounted read-only.
- A GitHub API publication fallback could not reach `api.github.com` from this sandbox.

## Next Recommended Cycle

Add `forge inspect` to render terminal DAGs with task dependencies, lifecycle state and persona/subflow annotations so operators can inspect running and scaled-to-zero workflows without reading raw JSON.
