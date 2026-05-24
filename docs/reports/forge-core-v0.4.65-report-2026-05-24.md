# Forge Core v0.4.65 Self-Evolution Report

Run id: `run_d10b7a17c91e4982800d039132ed3d9b`  
Workflow id: `wf_61e3fc34ceb747739012030c0ccab1ac`  
Prompt packet: `forge.self_evolution.prompt.v1`

## Increment

Added a versioned execution-policy decision record for executor adapter routing.

Before this cycle, Forge exposed the raw task `execution_policy` plus context
routing economy, selection receipts and inspection policy summaries. Executor
adapters and operators still had to infer the concrete route decision from those
separate fields. Forge now emits `execution_policy_decision` with schema
`forge.context.execution_policy_decision.v1`.

The decision record binds workflow/task identity, workflow revision, executor
profile, task executor, policy mode, route class, AI/deterministic flags,
model-call requirement/avoidance, reusable child-subflow eligibility, reuse key,
local code runtime metadata, selection reason and validation gate into a stable
decision checksum. Context routing fingerprints now include that checksum, and
`forge inspect` projects the decision record in JSON plus a compact terminal
marker.

## Files Changed

- `src/context.rs`
- `src/inspection.rs`
- `tests/forge_cli_contract.rs`
- `Cargo.toml`
- `Cargo.lock`
- `CHANGELOG.md`
- `docs/reports/forge-core-v0.4.65-report-2026-05-24.md`

## Validation

- Red test: `cargo test context_package_exposes_execution_policy_decision_for_adapter_routing --test forge_cli_contract` failed first because `execution_policy_decision` was absent from `forge context`.
- Focused green test: `cargo test context_package_exposes_execution_policy_decision_for_adapter_routing --test forge_cli_contract` passed after implementation.
- Focused regression checks:
  - `cargo test inspect_projects_execution_policy_for_deterministic_code_nodes --test forge_cli_contract`
  - `cargo test context_package_exposes_selection_receipt_for_auditable_context_routing --test forge_cli_contract`
- Required validation:
  - `cargo fmt --check`: passed.
  - `cargo clippy --all-targets --all-features -- -D warnings`: passed.
  - `cargo test`: passed with 95 CLI contract tests.
  - `cargo build --release`: passed.
- CLI smoke:
  - `./target/release/forge --store /tmp/forge-plan-smoke-0.4.65.sqlite plan --goal "Create a delivery platform" --output json`: passed.
  - `./target/release/forge --store /tmp/forge-skill-smoke-0.4.65-unique.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.65-unique`: passed.

## Install Notes

- Required global install `cargo install --path . --force` was attempted after validation but was blocked by the sandbox: `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem.
- Workspace-local fallback install succeeded with `CARGO_NET_OFFLINE=true cargo install --path . --force --locked --offline --root /home/arthur/projects/forge-core/.forge/local-install`.
- The fallback binary reports `forge 0.4.65`.

## Publication Notes

- Publication was attempted after validation through the required GitHub CLI contract.
- `gh auth token` succeeded with output redirected away from chat.
- `git remote get-url origin` reported `https://github.com/cardozoarthur/forge-core.git`.
- `git add ... && git commit -m "feat: add execution policy decision records"` was blocked because `.git/index.lock` cannot be created on the current read-only git metadata mount.
- `git push` was attempted and failed because the sandbox cannot resolve `github.com`.
- The validated changes remain in the working tree and were not committed or pushed from this sandbox.

## Safety

- Execution-policy decisions are read-only metadata derived from Forge-owned workflow/task execution policy and context profile state.
- This change does not execute local Python/Node.js code, complete tasks, promote workflows, authorize CLIs, acquire leases, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency readiness, validation rules, task leases, persona gates, child-subflow validation gates and continuation plans.

## Next Recommended Cycle

Add a registry-level execution-policy summary to `forge list` so operators can
scan running and non-running workflows by model, mixed, deterministic and
reusable local-code route counts without opening each workflow inspection.
