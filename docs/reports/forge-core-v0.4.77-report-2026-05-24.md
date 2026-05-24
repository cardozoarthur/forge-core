# Forge Core v0.4.77 Self-Evolution Report

Run id: `run_b72b0247966043088d9e3d639cb315e8`  
Workflow id: `wf_047b8ab16a8248dab94c5138fe4356e9`  
Prompt packet: `forge.self_evolution.prompt.v2`

## Increment

Forge now exposes `forge cluster leases`, a read-only cluster lease registry for
node-scoped task leases created by `forge cluster handoff`.

The registry returns `forge.cluster_node_lease_registry.v1` with:

- workflow and task identity;
- selected node id, node name and endpoint;
- lease id, scope, acquisition time, expiry and active/expired status;
- node trust level, sandbox permissions and reachability;
- explicit `remote_execution_enabled=false` and `external_mutation_allowed=false`
  markers;
- optional `--node-id` filtering.

## Why It Matters

The previous cluster handoff increment could lease a task to a selected LAN node
and return a sync manifest, but operators had no direct cluster-level audit view
after the handoff. This release makes node leases inspectable without re-running
placement or handoff, which is a prerequisite for distributed scheduling,
handoff supervision, stale lease cleanup and future permission-scoped remote
adapters.

## Safety

This release only reads Forge-owned SQLite state and registered node profiles. It
does not open SSH sessions, execute remote commands, copy artifacts to external
machines, authorize AI execution, install Knative or mutate Docker, Kubernetes,
Knative or user resources.

## Validation

The release includes CLI contract coverage for cluster lease inspection after a
safe cluster handoff. Full required validation was run for this cycle:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`

