# Forge Core v0.4.58 Self-Evolution Report

Run id: `run_83db5370e0a447b5bc92b57823f6170e`  
Workflow id: `wf_a3269b9fa26342a9bf0694b0185487cb`  
Prompt packet: `forge.self_evolution.prompt.v1`

## Increment

Added a versioned prompt-packet contract to the Context Routing Engine.

Forge already emitted bounded context, shard routing, persona, quality, budget,
delta and economy contracts. This increment adds a compact `prompt_packet` object
to `forge context`, so executor adapters can validate the exact prompt/context
contract they are about to use without inferring it from scattered fields.

The new `forge.context.prompt_packet.v1` contract records packet version,
context schema, routing policy, workflow/task ids, workflow revision, executor
profile, executor kind, reasoning/deterministic flags, persona mode/profile,
instruction sources, validation gates, context checksum, lineage checksum,
budget status, routing-quality status, handoff status and a stable packet hash.

## Files Changed

- `src/context.rs`
- `src/inspection.rs`
- `tests/forge_cli_contract.rs`
- `Cargo.toml`
- `Cargo.lock`
- `README.md`
- `docs/technical-definition.md`
- `CHANGELOG.md`
- `docs/reports/forge-core-v0.4.58-report-2026-05-24.md`

## Validation

- Red test: `cargo test context_package_exposes_versioned_prompt_packet_for_executor_adapters --test forge_cli_contract` failed before implementation because `forge context` still emitted `forge.context.v25` and no v26 prompt-packet contract.
- Focused green test: `cargo test context_package_exposes_versioned_prompt_packet_for_executor_adapters --test forge_cli_contract` passed after implementation.
- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test`: passed, 87 CLI contract tests.
- `cargo build --release`: passed.
- CLI smoke `./target/release/forge plan --goal "Create a delivery platform" --output json`: passed.
- CLI smoke `./target/release/forge --store /tmp/forge-skill-smoke-0.4.58.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.58`: passed.

## Install Notes

- Required install `cargo install --path . --force` was attempted after validation but was blocked by the sandbox: `/home/arthur/.cargo/.crates.toml` is read-only.
- Workspace-local fallback install succeeded with `cargo install --path . --force --locked --offline --root /home/arthur/projects/forge-core/.forge/local-install`.
- The fallback binary reports `forge 0.4.58`.

## Publication Notes

- `gh auth token`: passed.
- `git remote get-url origin`: `https://github.com/cardozoarthur/forge-core.git`.
- `git add ...`: blocked because `.git/index.lock` cannot be created on the read-only git metadata mount.
- `git push`: attempted and failed because the sandbox cannot resolve `github.com`.
- The validated changes remain in the working tree and were not published from this sandbox.

## Safety

- Prompt packets are read-only metadata derived from Forge-owned workflow/task state and deterministic context routing.
- This change does not complete tasks, promote workflows, authorize CLIs, run local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency readiness, validation rules, task leases, persona gates and child-subflow validation gates.

## Next Recommended Cycle

Project the prompt-packet hash into `forge task handoff` and `forge list` registry aggregates so long-running executors and operators can correlate leases, resumed checkpoints and workflow inventory with the same adapter-facing packet identity.
