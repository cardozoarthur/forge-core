# Forge Core v0.4.84 Self-Evolution Report

Run id: `run_d708430bebeb4159b40e961a000fb961`  
Workflow id: `wf_311f124a4f27442595e9740cb2919961`  
Prompt packet: `forge.self_evolution.prompt.v2`

## Increment

`forge cluster place` now returns a versioned `forge.cluster_placement_policy.v1`
receipt alongside placement requirements, candidates and selected node metadata.

The receipt records:

- authorized scope: `placement_metadata_only`;
- `remote_execution_enabled=false`;
- `remote_ai_execution_allowed=false` unless a future explicit authorization path
  changes the task requirements;
- `external_mutation_allowed=false`;
- required trust class and trust policy;
- explicit authorization requirement before remote execution or external mutation;
- deterministic `requirements_sha256` and `policy_sha256` audit hashes.

## Why It Matters

Forge already had cluster registry, placement, handoff, node leases and
content-addressed sync manifests. The missing operator surface was a compact
policy receipt on the dry-run placement itself, before a node lease is acquired.

This increment strengthens the "Forge must know the cluster before scheduling"
stage: operators and future adapters can inspect the exact trust and permission
boundary that governed placement without opening SSH, copying inputs or mutating
external resources.

## Safety

This change only adds Forge-owned metadata derived from SQLite workflow state,
task execution policy and registered cluster node profiles. It does not execute
remote code, open SSH sessions, copy artifacts, authorize remote AI, install
Knative or mutate Docker/Kubernetes/Knative/user resources.

## TDD Evidence

- RED: `cargo test cluster_registry_records_nodes_and_places_deterministic_code_task_by_capability -- --exact` failed because `placement["placement_policy"]["schema_version"]` was `Null`.
- GREEN: the same focused test passed after adding the placement policy receipt and hash fields.

## Validation

Validation passed for this cycle:

- RED: `cargo test cluster_registry_records_nodes_and_places_deterministic_code_task_by_capability -- --exact`
- GREEN: `cargo test cluster_registry_records_nodes_and_places_deterministic_code_task_by_capability -- --exact`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`
- CLI smoke: `target/release/forge plan --goal "Create a delivery platform" --output json`
- CLI smoke: `target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`

`cargo test` ran 110 integration tests plus unit/doc-test harnesses with zero
failures.

## Installation Note

`cargo install --path . --force` was attempted after validation, but the sandbox
blocked writes to `/home/arthur/.cargo/.crates.toml` with a read-only filesystem
error.

A scoped offline install succeeded with:

```bash
cargo install --path . --force --root /tmp/forge-install-0.4.84 --offline
```

`/tmp/forge-install-0.4.84/bin/forge --version` returned `forge 0.4.84`.

## Publication Check

`gh auth token >/dev/null` succeeded and `git remote get-url origin` returned
`https://github.com/cardozoarthur/forge-core.git`.

Creating the commit was blocked before publication:

```text
fatal: Unable to create '/home/arthur/projects/forge-core/.git/index.lock': Sistema de ficheiros só de leitura
```

No `git push` was run because there was no validated commit to publish.
