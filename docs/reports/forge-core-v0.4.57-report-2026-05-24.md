# Forge Core v0.4.57 Self-Evolution Report

Run id: `run_e41f285e902541cda50fc39ddb687492`  
Workflow id: `wf_f63f71e369d34868af2f22582a054043`  
Prompt packet: `forge.self_evolution.prompt.v1`

## Increment

Added a versioned context routing economy ledger.

Forge already emitted shard-level budget, compression, repair and quality data. This increment adds a compact `routing_economy` contract to `forge context` and `forge inspect`, so operators and executor adapters can audit the actual context-cost decision without reconstructing it from every shard.

The new `forge.context.routing_economy.v1` contract records the executor profile, reasoning/deterministic flags, baseline bytes, selected bytes, compression savings, budget omissions, profile-filtered omissions, total avoided bytes, reduction basis points and whether a deterministic no-AI route avoided a model call.

## Files Changed

- `src/context.rs`
- `src/inspection.rs`
- `tests/forge_cli_contract.rs`
- `Cargo.toml`
- `Cargo.lock`
- `README.md`
- `docs/technical-definition.md`
- `CHANGELOG.md`
- `docs/reports/forge-core-v0.4.57-report-2026-05-24.md`

## Validation

- Red test: `cargo test economy` failed before implementation because `forge context` still emitted `forge.context.v24` and no `routing_economy` object.
- Focused green test: `cargo test economy` passed after implementation.
- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test`: passed, 86 CLI contract tests.
- `cargo build --release`: passed.
- CLI smoke `./target/release/forge plan --goal "Create a delivery platform" --output json`: passed.
- CLI smoke `./target/release/forge --store /tmp/forge-skill-smoke-0.4.57.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.57`: passed.

## Install Notes

- Required install `cargo install --path . --force` was attempted after validation but was blocked by the sandbox: `/home/arthur/.cargo/.crates.toml` is read-only.
- Workspace-local fallback install succeeded with `cargo install --path . --force --locked --offline --root /home/arthur/projects/forge-core/.forge/local-install`.
- The fallback binary reports `forge 0.4.57`.

## Publication Notes

- `gh auth token`: passed.
- `git remote get-url origin`: `https://github.com/cardozoarthur/forge-core.git`.
- `git add ... && git commit -m "feat: add context routing economy ledger"`: blocked because `.git/index.lock` cannot be created on the read-only git metadata mount.
- `git push`: attempted and failed because the sandbox cannot resolve `github.com`.

## Safety

- The increment only adds read-only metadata derived from Forge-owned workflow/task state and deterministic context shard selection.
- It does not complete tasks, promote workflows, authorize CLIs, run local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency readiness, validation rules, task leases, persona gates and child-subflow validation gates.

## Next Recommended Cycle

Project `routing_economy` into `forge list` registry aggregates and `forge task handoff` packets, then add filters for high-savings no-AI routes and high-waste context routes.
