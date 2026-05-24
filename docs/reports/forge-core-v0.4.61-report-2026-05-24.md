# Forge Core v0.4.61 Self-Evolution Report

Run id: `run_29eb341a91574f399eca1beabb479a5f`  
Workflow id: `wf_94f78ded07ec42b3984ce5050b10bdfa`  
Prompt packet: `forge.self_evolution.prompt.v1`

## Increment

Added replay manifests to the Context Routing Engine.

Forge already emitted versioned context packets with shard manifests, budget plans,
routing quality, economy ledgers, prompt packets and context deltas. This increment
adds a bounded `replay_manifest` to `forge context` so long-running executor adapters
can pause, compare and resume against the exact context route without reparsing the
full packet or relying on implicit packet structure.

The manifest records the context schema, routing policy, selector version,
workflow/task ids, workflow revision, executor profile, requested and effective
budget, context checksum, selected byte count, included/missing-required sections,
the minimal replay command and content-addressed shard refs. Prompt packets now bind
the replay manifest checksum, routing fingerprints include it as a component, and
`forge inspect` projects the same checksum with a compact terminal marker.

## Files Changed

- `src/context.rs`
- `src/inspection.rs`
- `tests/forge_cli_contract.rs`
- `Cargo.toml`
- `Cargo.lock`
- `README.md`
- `docs/technical-definition.md`
- `CHANGELOG.md`
- `docs/reports/forge-core-v0.4.61-report-2026-05-24.md`

## Validation

- Red test: `cargo test context_package_exposes_replay_manifest_for_resumable_executor_context --test forge_cli_contract` failed first because the context packet was still `forge.context.v26`.
- Focused green test: `cargo test context_package_exposes_replay_manifest_for_resumable_executor_context --test forge_cli_contract` passed after implementation.
- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: initially caught `build_replay_manifest` argument sprawl; passed after refactoring to an input struct.
- `cargo test`: passed, including 91 CLI contract tests.
- `cargo build --release`: passed.
- CLI smoke `./target/release/forge --store /tmp/forge-plan-smoke-0.4.61.sqlite plan --goal "Create a delivery platform" --output json`: passed.
- CLI smoke `./target/release/forge --store /tmp/forge-skill-smoke-0.4.61.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.61`: passed.
- `git diff --check`: passed.

## Install Notes

- Required global install `cargo install --path . --force` was attempted after validation but was blocked by the sandbox: `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem.
- Workspace-local fallback install succeeded with `CARGO_NET_OFFLINE=true cargo install --path . --force --locked --offline --root /home/arthur/projects/forge-core/.forge/local-install`.
- The fallback binary reports `forge 0.4.61`.

## Publication Notes

- `gh auth token`: passed.
- `git remote get-url origin`: `https://github.com/cardozoarthur/forge-core.git`.
- `git add ... && git commit -m "Add context replay manifests"` was attempted and blocked because `.git/index.lock` cannot be created on the read-only git metadata mount.
- `git push` was attempted and failed because the sandbox cannot resolve `github.com`.
- The validated changes remain in the working tree and were not committed or pushed from this sandbox.

## Safety

- Replay manifests are read-only metadata derived from Forge-owned workflow/task
  state and deterministic context shard selection.
- This change does not complete tasks, promote workflows, acquire leases, authorize
  CLIs, execute local Python/Node.js code, install Knative or mutate Docker/
  Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency
  readiness, validation rules, task leases, persona gates and child-subflow
  validation gates.

## Next Recommended Cycle

Persist compact context-route snapshots in the Forge store so async executors can ask
for the last accepted replay manifest by workflow/task id, compare it with the current
route and decide between direct resume, context refresh or partial retry without
rerunning full inspection.
