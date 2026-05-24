# Forge Core v0.4.82 Self-Evolution Report

Run id: `run_b458a174caf64ed6ae34f41a2197cc3c`  
Workflow id: `wf_41a3b75e41f24f2db70cb0d3f01774ae`  
Prompt packet: `forge.self_evolution.prompt.v2`

## Increment

Forge Core now blocks remote cluster placement for reasoning-heavy AI/Mixed tasks
unless a future explicit authorization policy enables remote cognitive executors.

`forge cluster place --output json` requirements now include:

- `reasoning_required`;
- `remote_ai_execution_allowed`;
- placement requirement schema `forge.cluster_placement_requirements.v2`.

A node may still report `ai`, `gpu` or Python capability, but that is no longer
enough to receive an AI task. Deterministic and local-code cluster placement
continues to work through the existing manifest-only handoff path.

## Why It Matters

The persisted goal requires Forge to know the cluster before scheduling, support
heterogeneous nodes, and start safely with LAN/SSH deterministic/local-code tasks
before remote AI execution. Previous placement logic could select a remote node
for `task-002` when the node advertised `ai`, even though cluster handoff still
declares remote execution disabled.

This release closes that policy gap: capability discovery is now separate from
authorization to run remote cognition.

## TDD Evidence

- RED: `cargo test cluster_placement_blocks_remote_ai_tasks_without_explicit_authorization -- --exact`
  failed because `forge cluster place` selected `lan-ai-worker` for an AI task.
- GREEN: the same focused test passed after adding reasoning-aware placement
  requirements and candidate rejection.
- Regression: `cargo test cluster_ -- --nocapture` passed, proving deterministic
  cluster placement, handoff, leases and scheduling posture still work.

## Safety

This release only changes Forge-owned SQLite-backed scheduling metadata,
placement policy, tests and documentation. It does not open SSH connections,
run remote commands, copy artifacts to external machines, authorize remote AI
execution, install Knative or mutate Docker, Kubernetes, Knative or user
resources.

## Validation

Required validation passed for this cycle:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`
- release CLI smoke: `target/release/forge plan --goal "Create a delivery platform" --output json`
- release CLI smoke: `target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.82`

`cargo test` ran 110 integration tests plus unit/doc-test harnesses with zero
failures.

## Installation Note

`cargo install --path . --force` was attempted after validation, but the sandbox
blocked writes to `/home/arthur/.cargo/.crates.toml` with a read-only filesystem
error. A scoped offline install succeeded with:

```bash
cargo install --path . --force --root /tmp/forge-install-0.4.82 --offline
```

`/tmp/forge-install-0.4.82/bin/forge --version` returned `forge 0.4.82`.

## Next Recommended Cycle

Add an explicit cluster execution authorization policy object that can separately
permit deterministic remote commands, remote local-code nodes and remote AI
executors, with revisioned origin, operator approval and validation evidence.
