# Forge Core v0.4.47 Report - Context Routing Contract

## Summary

Forge Core v0.4.47 makes Context Routing Engine selector/profile behavior explicit in the executor-facing context packet.

`forge context` now emits schema `forge.context.v18` with routing policy
`task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_v18`.

The new `routing_contract` uses schema `forge.context.routing_contract.v1` and records:

- selector version: `forge.context.selector.v1`;
- executor profile version: `forge.context.executor_profile.v1`;
- selected profile id and selection strategy;
- requested budget, effective budget, minimum budget and max profile budget;
- compression allowance;
- allowed, required and optional section sets;
- stable profile hash.

## Runtime Impact

Executor adapters can now audit the Context Routing Engine contract directly instead of reconstructing selector intent from profile fields, shard decisions and budget summaries. The routing fingerprint includes a `routing_contract` component so cache keys change when selector/profile contracts change.

This is read-only routing metadata. It does not authorize executor use, complete tasks, promote workflows, execute local code nodes or mutate Docker/Kubernetes/Knative resources.

## Validation

Completed validation:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`
- `PATH=/home/arthur/projects/forge-core/target/release:$PATH forge --version`
- `PATH=/home/arthur/projects/forge-core/target/release:$PATH forge plan --goal "Create a delivery platform" --output json`
- `PATH=/home/arthur/projects/forge-core/target/release:$PATH forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v047`

## Local Install Note

The requested `cargo install --path . --force` was attempted after validation, but the sandbox blocked writes to `/home/arthur/.cargo/.crates.toml` with `Read-only file system (os error 30)`. The release binary at `target/release/forge` was validated as `forge 0.4.47`.
