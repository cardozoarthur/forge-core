# Forge Core v0.4.74 Self-Evolution Report

Run id: `run_2ae9a5cff8b44d43839f5d0fc00775c7`
Workflow id: `wf_8e0efbd9bb664f238dae2c562c846ff9`
Prompt packet: `forge.self_evolution.prompt.v2`

## Goal

Advance the persisted clusterization goal with a small, safe runtime increment:
Forge should know registered cluster nodes and evaluate deterministic task
placement by capability, trust and sandbox policy before any distributed
execution exists.

## Implemented

- Added `forge cluster register` for explicit operator-provided node profiles.
- Added `forge cluster list` with schema `forge.cluster_registry.v1` and summary
  counts for online, reachable, OS-specific, Python, Node.js, Docker, GPU and
  MetaTrader 5-capable nodes.
- Added `forge cluster place --workflow <id> --task <task-id>` with schema
  `forge.cluster_placement.v1`.
- Persisted cluster nodes in SQLite table `cluster_nodes`.
- Placement derives task requirements from the Forge task executor and execution
  policy. A deterministic local Python code node requires a registered online,
  reachable and trusted node with `python` capability plus the declared sandbox
  permission.
- Updated README, technical definition and changelog for version `0.4.74`.

## Safety

This cycle is metadata and policy evaluation only. Forge does not:

- open SSH sessions;
- execute remote commands;
- run local Python or Node.js code;
- complete tasks or promote workflows;
- authorize external CLIs;
- install Knative;
- mutate Docker, Kubernetes, Knative or user resources.

Placement output is a dry-run scheduling precondition for later distributed
handoff and node-lease work.

## TDD Evidence

- RED: `cargo test cluster_registry_records_nodes_and_places_deterministic_code_task_by_capability --test forge_cli_contract` failed because `forge` did not recognize the `cluster` subcommand.
- GREEN: the same focused test passed after adding the cluster module, SQLite persistence and CLI commands.
- Regression: full `cargo test` passed with `103 passed`.

## Validation

- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed after boxing was avoided and the large Clap enum variant was explicitly allowed at the CLI boundary.
- `cargo test`: passed, `103 passed`.
- `cargo build --release`: passed.
- CLI smoke `./target/release/forge --store /tmp/forge-core-v0474-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json`: passed.
- CLI smoke `./target/release/forge --store /tmp/forge-core-v0474-skill-smoke-b.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v0474-b`: passed.

The first skill smoke attempt was rejected by sandbox policy because it chained a
cleanup command with the smoke command. The smoke was rerun without command
chaining in a new `/tmp` directory and passed.

## Install And Publication

- `cargo install --path . --force`: blocked by sandbox filesystem policy on
  `/home/arthur/.cargo/.crates.toml` with `Read-only file system (os error 30)`.
- `gh auth token >/dev/null`: passed, confirming GitHub CLI auth without
  printing the token.
- `git remote get-url origin`: returned
  `https://github.com/cardozoarthur/forge-core.git`.
- `git add ...`: blocked because the sandbox could not create
  `/home/arthur/projects/forge-core/.git/index.lock` on the read-only git
  metadata mount.
- `git push`: blocked by restricted network DNS resolution for `github.com`.

The validated source changes remain in the working tree and were not committed
or pushed from this sandbox.

## Next Recommended Cycle

Add cluster node leases and handoff binding:

- introduce a `cluster_node_leases` table keyed by node/task/workflow;
- make `forge cluster place` optionally reserve a node without executing work;
- extend `forge task handoff` to include selected cluster node metadata when a
  valid placement lease exists;
- keep remote AI execution disabled until executor policy explicitly allows it.
