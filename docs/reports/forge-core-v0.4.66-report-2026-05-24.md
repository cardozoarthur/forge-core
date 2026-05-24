# Forge Core v0.4.66 Self-Evolution Report

Run id: `run_b62f61f0e8374cd5b817a8d7208b0745`  
Workflow id: `wf_2751a1acaa124f3ea0b5e90e701ed872`  
Prompt packet: `forge.self_evolution.prompt.v1`

## Increment

Added explicit recursive child-subflow cycle metadata to `forge inspect`.

Before this cycle, recursive subflow traversal already stopped when a repeated
workflow/task path was encountered, but the JSON contract only exposed a generic
terminal row. Operators and executor adapters could not distinguish a normal
terminal child task from a traversal stopped to avoid an infinite subflow loop.

`forge inspect --output json` now marks each expanded subflow with:

- `terminal_reason`, including `recursive_subflow_cycle`;
- `cycle_detected`;
- `cycle_ref`;
- `recursion_policy`, currently `stop_on_repeated_workflow_task_path`.

The terminal diagram also prints `cycle recursive_subflow_cycle` for recursive
edges. This keeps recursive and future infinite subflow composition inspectable
without executing children or mutating external runtime substrates.

## Files Changed

- `src/inspection.rs`
- `tests/forge_cli_contract.rs`
- `Cargo.toml`
- `Cargo.lock`
- `CHANGELOG.md`
- `docs/reports/forge-core-v0.4.66-report-2026-05-24.md`

## Validation

- Red test: `cargo test inspect_marks_recursive_child_subflow_cycles_as_terminal --test forge_cli_contract` failed first because `cycle_detected` was absent from `forge inspect` subflow rows.
- Focused green test: `cargo test inspect_marks_recursive_child_subflow_cycles_as_terminal --test forge_cli_contract` passed after implementation.
- Focused regression checks:
  - `cargo test inspect_expands_proposed_child_subflows_with_recursive_path_metadata --test forge_cli_contract`: passed.
  - `cargo test plan_persists_reuse_candidates_as_proposed_child_subflows_for_inspection --test forge_cli_contract`: passed.
  - `cargo test context_package_carries_proposed_child_subflow_routing_for_reused_nodes --test forge_cli_contract`: passed.
- Required validation:
  - `cargo fmt --check`: passed.
  - `cargo clippy --all-targets --all-features -- -D warnings`: passed.
  - `cargo test`: passed with 96 CLI contract tests.
  - `cargo build --release`: passed.
- CLI smoke:
  - `./target/release/forge --store /tmp/forge-plan-smoke-0.4.66.sqlite plan --goal "Create a delivery platform" --output json`: passed.
  - `./target/release/forge --store /tmp/forge-skill-smoke-0.4.66.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.66`: passed.

## Install Notes

- Required global install `cargo install --path . --force` was attempted after validation but was blocked by the sandbox: `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem.
- Workspace-local fallback install succeeded with `CARGO_NET_OFFLINE=true cargo install --path . --force --locked --offline --root /home/arthur/projects/forge-core/.forge/local-install`.
- The fallback binary reports `forge 0.4.66`.

## Publication Notes

- Publication was prepared through the required GitHub CLI contract.
- `gh auth token` succeeded with output redirected away from chat and the temporary token check file was deleted.
- `git remote get-url origin` reported `https://github.com/cardozoarthur/forge-core.git`.
- `git add ...` was attempted before commit but failed because `.git/index.lock` cannot be created on the current read-only git metadata mount.
- `git push` was attempted and failed because the sandbox cannot resolve `github.com`.
- The validated changes remain in the working tree and were not committed or pushed from this sandbox.

## Safety

- The change is read-only inspection metadata over Forge-owned persisted
  workflows.
- It does not execute child subflows, acquire task leases, complete tasks,
  promote workflows, authorize CLIs, run local Python/Node.js code, install
  Knative or mutate Docker/Kubernetes/Knative resources.
- Child-subflow execution remains future work behind validation, scheduling,
  executor handoff, lease and continuation gates.

## Next Recommended Cycle

Add a registry-level execution-policy summary to `forge list` so operators can
scan running and non-running workflows by AI, mixed, deterministic and reusable
local-code route counts without opening each workflow inspection.
