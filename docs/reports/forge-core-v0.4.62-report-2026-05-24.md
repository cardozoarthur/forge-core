# Forge Core v0.4.62 Self-Evolution Report

Run id: `run_2a7a201e667440119b248746b65e1097`  
Workflow id: `wf_27b3cd13f2f242759ffd2189c4b7f54c`  
Prompt packet: `forge.self_evolution.prompt.v1`

## Increment

Added a versioned continuation plan to the Context Routing Engine.

Forge already tracked checkpoints, context deltas, next actions and handoff resume
plans, but the adapter-facing decision was split across multiple fields and the
handoff packet owned a separate resume-plan projection. This increment makes the
continuation decision a first-class context contract.

`forge context` now emits `continuation_plan` with schema
`forge.context.continuation_plan.v1`. It records whether a checkpoint is reusable,
whether fresh context is required, whether partial retry is recommended, the
checkpoint/current checksums and route keys, the workflow revisions, a validation
gate and a concrete action. `forge inspect` projects the same contract and the
terminal diagram prints a compact `continue <action> <status>` marker.
`forge task handoff` now emits `forge.executor_handoff.v8` and reuses the same
context continuation plan as its `resume_plan`.

## Files Changed

- `src/context.rs`
- `src/inspection.rs`
- `src/handoff.rs`
- `tests/forge_cli_contract.rs`
- `Cargo.toml`
- `Cargo.lock`
- `README.md`
- `docs/technical-definition.md`
- `CHANGELOG.md`
- `docs/reports/forge-core-v0.4.62-report-2026-05-24.md`

## Validation

- Red test: `cargo test context_package_exposes_versioned_continuation_plan_for_executor_adapters -- --nocapture` failed first because the context packet was still `forge.context.v27`.
- Focused green test: `cargo test context_package_exposes_versioned_continuation_plan_for_executor_adapters -- --nocapture` passed after implementation.
- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test`: passed with 92 CLI contract tests.
- `cargo build --release`: passed.
- CLI smoke `./target/release/forge --store /tmp/forge-plan-smoke-0.4.62.sqlite plan --goal "Create a delivery platform" --output json`: passed.
- CLI smoke `./target/release/forge --store /tmp/forge-skill-smoke-0.4.62.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.62`: passed.
- `git diff --check`: passed.

## Install Notes

- Required global install `cargo install --path . --force` was attempted after validation but was blocked by the sandbox: `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem.
- Workspace-local fallback install succeeded with `CARGO_NET_OFFLINE=true cargo install --path . --force --locked --offline --root /home/arthur/projects/forge-core/.forge/local-install`.
- The fallback binary reports `forge 0.4.62`.

## Publication Notes

- `gh auth token`: passed.
- `git remote get-url origin`: `https://github.com/cardozoarthur/forge-core.git`.
- `git add ... && git commit -m "Add context continuation plans"` was attempted and blocked because `.git/index.lock` cannot be created on the current read-only git metadata mount.
- `git push` was attempted and failed because the sandbox cannot resolve `github.com`.
- The validated changes remain in the working tree and were not committed or pushed from this sandbox.

## Safety

- Continuation plans are read-only metadata derived from Forge-owned workflow
  checkpoints and deterministic context routing.
- This change does not complete tasks, promote workflows, acquire leases by itself,
  authorize CLIs, execute local Python/Node.js code, install Knative or mutate
  Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency
  readiness, validation rules, task leases, persona gates and child-subflow
  validation gates.

## Next Recommended Cycle

Persist compact continuation snapshots in the Forge store so async executors can
compare the last accepted route with the current route without rebuilding the full
context package on every resume attempt.
