# Forge Core v0.4.85 Self-Evolution Report

Run id: `run_89df94ac0a8a4fe0a5c600406aaad4b8`  
Workflow id: `wf_2c871d9d086d45448bac5dd38e95936c`  
Prompt packet: `forge.self_evolution.prompt.v2`

## Increment

Forge now plans and places deterministic Windows software work for MetaTrader 5.

When a goal mentions MetaTrader 5 or MT5, `forge plan` adds a deterministic
command node named `Run MetaTrader 5 deterministic step`. The node carries a
`windows_software_node` execution policy with:

- `ai_allowed=false`;
- `deterministic=true`;
- runtime language `metatrader5`;
- entrypoint `metatrader5_terminal`;
- sandbox `windows_desktop_user_session`;
- validation gate `windows_software_node_validation_required`.

`forge cluster place` now emits `forge.cluster_placement_requirements.v3`.
Placement requirements include `required_os` and `required_software` in addition
to capabilities, sandbox permissions, trust and mutation policy. A MetaTrader 5
task requires:

- OS: `windows`;
- capability: `metatrader5`;
- software: `metatrader5`;
- sandbox permission: `windows_desktop_user_session`.

## Why It Matters

The persisted Forge goal requires heterogeneous LAN/SSH cluster support where
Windows-only software such as MetaTrader 5 can run on a real Windows machine
while Linux/GPU nodes handle AI, data and compute. Before this increment,
cluster placement could distinguish Python/Node local code and generic executor
kinds, but not OS-bound installed software requirements.

This cycle keeps the staged distributed-runtime boundary intact: Forge can know
that a task belongs on the Windows MT5 terminal before any remote execution path
exists.

## Safety

This change only updates Forge-owned planning and placement metadata. It does
not open SSH, start MetaTrader, copy files to Windows, mutate Docker,
Kubernetes, Knative or user resources, or authorize remote AI execution.

Placement remains policy-receipt driven with `remote_execution_enabled=false`,
`external_mutation_allowed=false` and explicit authorization required before any
future remote execution or external mutation.

## TDD Evidence

- RED: `cargo test cluster_placement_routes_metatrader5_work_to_windows_software_node --test forge_cli_contract` failed because the planner did not emit `Run MetaTrader 5 deterministic step`.
- GREEN: the same focused test passed after adding Windows software-node planning and OS/software-aware placement requirements.

## Validation

Validation passed for this cycle:

- RED: `cargo test cluster_placement_routes_metatrader5_work_to_windows_software_node --test forge_cli_contract`
- GREEN: `cargo test cluster_placement_routes_metatrader5_work_to_windows_software_node --test forge_cli_contract`
- Focused cluster regression: `cargo test cluster_ --test forge_cli_contract`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`
- CLI smoke: `target/release/forge --store /tmp/forge-core-v0.4.85-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json`
- CLI smoke: `target/release/forge --store /tmp/forge-core-v0.4.85-skill-smoke.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v0.4.85`

`cargo test` ran 111 integration tests plus unit/doc-test harnesses with zero
failures.

## Installation Note

`cargo install --path . --force` was attempted after validation, but the sandbox
blocked writes to `/home/arthur/.cargo/.crates.toml` with:

```text
Read-only file system (os error 30)
```

A scoped offline install succeeded with:

```bash
cargo install --path . --force --root /tmp/forge-install-0.4.85 --offline
```

`/tmp/forge-install-0.4.85/bin/forge --version` returned `forge 0.4.85`.

## Publication Check

`gh auth token >/dev/null` succeeded and `git remote get-url origin` returned
`https://github.com/cardozoarthur/forge-core.git`.

Creating the commit was blocked before publication:

```text
fatal: Unable to create '/home/arthur/projects/forge-core/.git/index.lock': Sistema de ficheiros só de leitura
```

No `git push` was run because there was no local commit to publish.
