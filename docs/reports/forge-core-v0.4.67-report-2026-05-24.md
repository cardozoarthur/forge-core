# Forge Core v0.4.67 Self-Evolution Report

Run id: `run_5b3530f0e8c14a3a9d192c14d09910df`  
Workflow id: `wf_670b388a9c0f4e71ae1775c4d62af4ae`  
Prompt packet: `forge.self_evolution.prompt.v1`

## Increment

Added a registry-level execution-policy summary to `forge list`.

Before this cycle, operators could see task status, context readiness, context
actions, context quality and reusable code-node refs from the workflow registry,
but they still had to inspect individual workflows to understand the AI versus
deterministic route mix. That made it harder to scan running/non-running workflow
inventory for model-call pressure and reusable local-code opportunities.

`forge list --output json` now includes `execution_policy` on both:

- `summary.execution_policy`;
- each `workflows[].execution_policy` row.

The schema is `forge.registry_execution_policy.v1` and counts:

- AI, command, wait, notification and mixed task executors;
- AI-allowed and no-AI tasks;
- deterministic tasks;
- model-call-required and model-call-avoided tasks;
- local-code nodes and reusable local-code nodes.

This moves `forge list` closer to an operational control-plane view for running
and non-running workflows without opening full context or inspection packets.

## Files Changed

- `src/registry.rs`
- `tests/forge_cli_contract.rs`
- `Cargo.toml`
- `Cargo.lock`
- `CHANGELOG.md`
- `README.md`
- `docs/reports/forge-core-v0.4.67-report-2026-05-24.md`

## Validation

- Red test: `cargo test list_aggregates_execution_policy_routes_for_registry_rows --test forge_cli_contract` failed first because `summary.execution_policy` was absent from `forge list`.
- Focused green test: `cargo test list_aggregates_execution_policy_routes_for_registry_rows --test forge_cli_contract` passed after implementation.
- Focused registry regression: `cargo test list_ --test forge_cli_contract` passed with 10 registry/list tests.
- Required validation:
  - `cargo fmt --check`: passed.
  - `cargo clippy --all-targets --all-features -- -D warnings`: passed.
  - `cargo test`: passed with 97 CLI contract tests.
  - `cargo build --release`: passed.
- CLI smoke:
  - `./target/release/forge --store /tmp/forge-plan-smoke-0.4.67.sqlite plan --goal "Create a delivery platform" --output json`: passed.
  - `./target/release/forge --store /tmp/forge-skill-smoke-0.4.67.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.67`: passed.

## Install Notes

- Required global install `cargo install --path . --force` was attempted after validation but was blocked by the sandbox: `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem.
- Workspace-local fallback install succeeded with `CARGO_NET_OFFLINE=true cargo install --path . --force --locked --offline --root /home/arthur/projects/forge-core/.forge/local-install`.
- The fallback binary reports `forge 0.4.67`.

## Publication Notes

- Publication was prepared through the required GitHub CLI contract.
- `gh auth token` succeeded with output redirected away from chat.
- `git remote get-url origin` reported `https://github.com/cardozoarthur/forge-core.git`.
- `git add ... && git commit ...` was attempted after validation but failed because `.git/index.lock` cannot be created on the current read-only git metadata mount.
- `git push` was attempted and failed because the sandbox cannot resolve `github.com`.
- The validated changes remain in the working tree and were not committed or pushed from this sandbox.

## Safety

- The change is read-only registry metadata over Forge-owned persisted workflows.
- It does not execute local Python/Node.js code, complete tasks, promote
  workflows, authorize CLIs, run installed CLIs as executors, install Knative or
  mutate Docker/Kubernetes/Knative resources.
- Reusable local-code routes remain proposed child-subflow candidates until
  later validation gates explicitly promote the binding.

## Next Recommended Cycle

Add `--execution-policy-action` or equivalent registry filters for high model-call
pressure, no-AI deterministic routes and reusable local-code nodes, so operators
can use `forge list` to select workflows by execution cost profile before opening
`forge inspect`.
