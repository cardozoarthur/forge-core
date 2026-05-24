# Forge Core v0.4.64 Self-Evolution Report

Run id: `run_5ec36f1b288b46d38637f66023eb1afb`  
Workflow id: `wf_9ee620f1e42a4809b746bb879f8e2ec0`  
Prompt packet: `forge.self_evolution.prompt.v1`

## Increment

Added a versioned context selection receipt for executor-adapter audit and reuse.

Before this cycle, `forge context` exposed the selected shards, routing summary,
budget plan, replay manifest and prompt packet, but executor adapters still had
to reconstruct the route decision from several fields. Forge now emits a compact
`selection_receipt` with schema `forge.context.selection_receipt.v1`.

The receipt binds selector version, executor profile, reasoning/deterministic
mode, requested/effective budget, selected bytes, minimum-correct budget,
selected sections, required sections, missing required sections, compressed
sections, budget-omitted sections, profile-omitted sections, route status and
handoff status into a stable receipt hash. The routing fingerprint includes that
hash, and `forge inspect` projects the receipt hash/status in both JSON and the
terminal diagram.

## Files Changed

- `src/context.rs`
- `src/inspection.rs`
- `tests/forge_cli_contract.rs`
- `Cargo.toml`
- `Cargo.lock`
- `CHANGELOG.md`
- `docs/reports/forge-core-v0.4.64-report-2026-05-24.md`

## Validation

- Red test: `cargo test context_package_exposes_selection_receipt_for_auditable_context_routing --test forge_cli_contract` failed first because `selection_receipt` was absent from `forge context`.
- Focused green test: `cargo test context_package_exposes_selection_receipt_for_auditable_context_routing --test forge_cli_contract` passed after implementation.
- Focused regression checks:
  - `cargo test context_package_exposes_versioned_routing_contract_for_executor_adapters --test forge_cli_contract`
  - `cargo test context_package_exposes_replay_manifest_for_resumable_executor_context --test forge_cli_contract`
- Required validation:
  - `cargo fmt --check`: passed.
  - `cargo clippy --all-targets --all-features -- -D warnings`: passed.
  - `cargo test`: passed with 94 CLI contract tests.
  - `cargo build --release`: passed.
- CLI smoke:
  - `./target/release/forge --store /tmp/forge-plan-smoke-0.4.64.sqlite plan --goal "Create a delivery platform" --output json`: passed.
  - `./target/release/forge --store /tmp/forge-skill-smoke-0.4.64.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.64`: passed.

## Install Notes

- Required global install `cargo install --path . --force` was attempted after validation but was blocked by the sandbox: `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem.
- Workspace-local fallback install succeeded with `CARGO_NET_OFFLINE=true cargo install --path . --force --locked --offline --root /home/arthur/projects/forge-core/.forge/local-install`.
- The fallback binary reports `forge 0.4.64`.

## Publication Notes

- Publication was attempted after validation through the required GitHub CLI contract.
- `gh auth token` succeeded with output redirected away from chat.
- `git remote get-url origin` reported `https://github.com/cardozoarthur/forge-core.git`.
- `git add ...` was attempted and blocked because `.git/index.lock` cannot be created on the current read-only git metadata mount.
- `git push` was attempted and failed because the sandbox cannot resolve `github.com`.
- The validated changes remain in the working tree and were not committed or pushed from this sandbox.

## Safety

- Selection receipts are read-only metadata derived from Forge-owned workflow and
  task context routing state.
- This change does not execute local Python/Node.js code, complete tasks,
  promote workflows, authorize CLIs, acquire leases, install Knative or mutate
  Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency
  readiness, validation rules, task leases, persona gates, child-subflow
  validation gates and continuation plans.

## Next Recommended Cycle

Persist a lightweight execution-policy decision record so `forge list` and
`forge inspect` can show why Forge chose model, mixed, deterministic executor or
reusable local code-node policy without rebuilding full task context.
