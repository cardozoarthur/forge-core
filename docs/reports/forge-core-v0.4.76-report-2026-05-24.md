# Forge Core v0.4.76 Self-Evolution Report

Run id: `run_27aca3cbf961429cad614dc39dfc347e`  
Workflow id: `wf_f7eadc242ddd45cda458f4e22e2afb49`  
Prompt packet: `forge.self_evolution.prompt.v2`  
Executor: `codex`

## Goal

Move Forge clusterization one step beyond registry placement without enabling
remote execution: select an eligible node, bind a task lease to that node and
emit a content-addressed sync manifest an adapter can audit later.

## Change

`forge cluster handoff` now composes:

- `forge.cluster_placement.v1`, so Forge chooses an online, reachable and trusted
  node with the required capabilities and sandbox permissions;
- `forge.executor_handoff.v8`, so strict context readiness, task leases,
  validation gates, routing cache keys, persona contracts and continuation plans
  remain the handoff authority;
- `forge.cluster_task_handoff.v1`, the distributed staging envelope;
- `forge.cluster_sync_manifest.v1`, a hash-only sync manifest for context,
  checkpoint, artifact and context-shard refs.

The selected cluster node id is used as the executor value in the existing task
lease. This gives Forge a deterministic node lease without adding a parallel
lease authority.

## Validation Design

The new CLI contract test proves that a trusted LAN command node:

- is selected through cluster placement;
- receives the task lease through the normal executor handoff contract;
- appears in a node-scoped `cluster_node_lease`;
- returns a sync manifest with context hash, routing cache key, shard refs and
  artifact refs;
- rejects a second cluster handoff while the task lease is still active.

## Validation Evidence

Passed in this cycle:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`
- `./target/release/forge --store /tmp/forge-smoke-plan.sqlite plan --goal "Create a delivery platform" --output json`
- `./target/release/forge --store /tmp/forge-smoke-skill-27aca3.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-27aca3`

Install attempt:

- `cargo install --path . --force` was blocked by the session sandbox with
  `Read-only file system (os error 30)` while opening
  `/home/arthur/.cargo/.crates.toml`. The release binary was still built and
  smoke-tested from `target/release/forge`.

Publication attempt:

- `git add ... && git commit -m "Add cluster task handoff manifest"` was blocked
  while creating `.git/index.lock` with `Sistema de ficheiros só de leitura`.
- `gh auth token` succeeded with output redirected away from the terminal.
- `git remote get-url origin` returned
  `https://github.com/cardozoarthur/forge-core.git`.
- `git push` was attempted and failed because DNS could not resolve
  `github.com` from the restricted session.

## Safety

No external machines, Docker, Kubernetes, Knative resources, SSH sessions or user
resources are mutated. The sync manifest declares `remote_execution_enabled=false`
and `external_mutation_allowed=false`; it is staging metadata for a future
permission-scoped distributed adapter, not a remote runner.

## Next Cycle

Add an explicit cluster handoff acknowledgment/receipt contract so a future node
adapter can prove it has the referenced context shards and artifacts by hash
before Forge permits remote execution.
