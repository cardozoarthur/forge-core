# Forge Core v0.4.78 Self-Evolution Report

Run id: `run_a5efab0974784634962da25846a5caec`  
Workflow id: `wf_355fcd16088e469bb68761f2f5ff8e3c`  
Prompt packet: `forge.self_evolution.prompt.v2`

## Increment

Forge cluster handoff sync manifests now include `manifest_sha256`, a
deterministic SHA-256 digest over the sync contract fields, excluding the hash
field itself.

The existing `forge.cluster_sync_manifest.v1` still carries the selected node,
lease id, context checksum, context routing cache key, lineage checksum,
checkpoint reference, replay shard refs, artifact refs, sync mode and explicit
no-remote-execution flags. The new digest gives operators and future distributed
adapters a single content-addressed handle for the whole staging contract.

## Why It Matters

The persisted Forge goal calls for distributed task handoff plus artifact,
checkpoint and context-shard sync by hash. Previous cluster handoff packets
exposed hashes for the individual inputs, but the manifest itself had no stable
checksum. A manifest-level digest lets schedulers, node supervisors and audit
tools compare the exact handoff contract without re-reading every nested field.

## Safety

This release only changes Forge-owned JSON metadata and documentation. It does
not open SSH sessions, execute remote commands, copy artifacts to external
machines, authorize AI execution, install Knative or mutate Docker, Kubernetes,
Knative or user resources.

Remote execution and external mutation remain explicitly disabled in every
cluster sync manifest.

## Validation

The release includes CLI contract coverage for the manifest checksum and the full
required validation set for this cycle:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`
