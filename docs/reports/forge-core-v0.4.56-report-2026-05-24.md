# Forge Core v0.4.56 Self-Evolution Report

Run id: `run_abb51e3b67e04617b10ac5f929b0fe2f`  
Workflow id: `wf_148960c522ed4239b614c84c3fde5887`  
Prompt packet: `forge.self_evolution.prompt.v1`

## Increment

Added validation-gated child-subflow reuse.

Forge already persisted compatible deterministic code-node reuse as proposed child-subflow bindings. This increment makes those bindings part of validation-before-promotion: a completed parent workflow cannot be promoted while a reused child subflow remains only `proposed`.

The new `forge workflow validate-subflow` command performs a Forge-owned revisioned mutation from `proposed` to `validated`. It checks that the child workflow/task exists, refuses child flows that are not `scaled_to_zero`, stamps the current child lifecycle and validation gate onto the parent binding, records an event and advances the workflow revision.

## Files Changed

- `src/main.rs`
- `src/validation.rs`
- `src/workflow.rs`
- `tests/forge_cli_contract.rs`
- `Cargo.toml`
- `Cargo.lock`
- `README.md`
- `docs/technical-definition.md`
- `CHANGELOG.md`
- `docs/reports/forge-core-v0.4.56-report-2026-05-24.md`

## Validation

- Red test: `cargo test validation_blocks_promotion_until_child_subflow_binding_is_validated` failed before implementation because `forge validate` still returned `promotable: true` for a completed parent workflow with a `proposed` child subflow.
- Focused green test: `cargo test validation_blocks_promotion_until_child_subflow_binding_is_validated` passed after implementation.
- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test`: passed, 84 CLI contract tests.
- `cargo build --release`: passed.
- CLI smoke `./target/release/forge plan --goal "Create a delivery platform" --output json`: passed.
- CLI smoke `./target/release/forge --store /tmp/forge-skill-smoke-0.4.56.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.56`: passed.

## Install Notes

- Required install `cargo install --path . --force` was attempted after validation but was blocked by the sandbox: `/home/arthur/.cargo/.crates.toml` is read-only.
- Workspace-local fallback install succeeded with `cargo install --path . --force --locked --offline --root /home/arthur/projects/forge-core/.forge/local-install`.
- The fallback binary reports `forge 0.4.56`.

## Publication Notes

- `gh auth token`: passed.
- `git remote get-url origin`: `https://github.com/cardozoarthur/forge-core.git`.
- `git add ...`: blocked because `.git/index.lock` cannot be created on the read-only git metadata mount.
- `git push`: attempted and failed because the sandbox cannot resolve `github.com`.

## Safety

- The increment only changes Forge-owned workflow metadata and validation behavior.
- It does not execute child subflows, acquire leases, complete tasks, authorize CLIs, run local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Parent promotion now requires completed task readiness, persona validation and child-subflow binding validation.

## Next Recommended Cycle

Add scheduled/active/reusable child-subflow lifecycle states with executor handoff integration, so validated subflows can move into controlled execution or scale-to-zero reuse without manual metadata patching.
