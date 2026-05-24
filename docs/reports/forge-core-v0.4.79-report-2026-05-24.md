# Forge Core v0.4.79 Self-Evolution Report

Run id: `run_712a91b69ab64da0bbacc27fb075d5d2`  
Workflow id: `wf_7e44b06dde0942d2bd18ff768c07d541`  
Prompt packet: `forge.self_evolution.prompt.v2`

## Increment

Forge cluster placement is now lease-aware. `forge cluster place` counts active
task leases per registered node, exposes the count as `active_lease_count` on
each placement candidate and penalizes busy eligible nodes in the placement
score.

When two nodes satisfy the same deterministic placement requirements, Forge now
prefers an eligible idle node over an otherwise stronger node that already holds
an active cluster handoff lease.

## Why It Matters

The persisted Forge goal calls for distributed task handoff, node leases and
placement by capabilities before remote execution exists. Previous placement
logic considered capability, trust, reachability, cost, latency and reliability,
but it did not account for node lease pressure. This increment makes the
scheduler's dry-run decision closer to an operational cluster runtime while
remaining fully local and auditable.

## Safety

This release only reads Forge-owned SQLite task leases and registered cluster
node metadata. It does not open SSH sessions, execute remote commands, copy
artifacts to external machines, authorize AI execution, install Knative or mutate
Docker, Kubernetes, Knative or user resources.

Remote execution and external mutation remain explicitly disabled in cluster
handoff contracts.

## TDD Evidence

- RED: `cargo test cluster_placement_prefers_idle_eligible_node_over_node_with_active_lease --test forge_cli_contract` failed because placement still selected the busy node.
- GREEN: the same focused test passed after adding active lease counts to placement scoring and candidate metadata.

## Validation

Required validation passed for this cycle:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`
- release CLI smoke: `forge plan --goal "Create a delivery platform" --output json`
- release CLI smoke: `forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`

## Installation Note

`cargo install --path . --force` was attempted after validation but the sandbox
blocked writes to `/home/arthur/.cargo/.crates.toml` with a read-only filesystem
error. A scoped install to `/tmp/forge-install-run_712a91b69ab64da0bbacc27fb075d5d2`
with `--root` and `--offline` succeeded and produced `forge 0.4.79`, while the
global `/home/arthur/.cargo/bin/forge` remains `0.4.78` until run outside this
sandbox.

## Next Recommended Cycle

Add a cluster handoff acknowledgment/receipt contract so a future node adapter can
prove it received and verified the content-addressed sync manifest before Forge
permits any remote execution policy.
