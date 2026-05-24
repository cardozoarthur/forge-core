# Forge Core v0.4.63 Self-Evolution Report

Run id: `run_a4fa9786c570445fa4d71d25836af459`  
Workflow id: `wf_45bf8345504d4f188debae7aa4318288`  
Prompt packet: `forge.self_evolution.prompt.v1`

## Increment

Added standalone deterministic local-code planning for repeated or frequent local
Python/Node.js work.

Before this cycle, Forge could mark repeated local Python/Node.js work as a
`local_code_node`, but only inside the autonomous cron/email extension path. A
goal such as `Run frequent local Node.js invoice normalization` stayed on the
generic base graph and left the repeated local work to a mixed executor path.

Forge now detects reusable local code policy before extension planning. If the
goal asks for repeated/frequent local Python or Node.js work and does not require
cron/email scaffolding, the base graph gets a standalone `Run deterministic
non-AI step` node. The node uses the existing `local_code_node` contract,
`reuse_compatible_code_node` hint, local no-network runtime, deterministic
validation gate and no-AI context routing profile. Existing cron/email behavior
keeps the established `task-011` path.

## Files Changed

- `src/graph.rs`
- `tests/forge_cli_contract.rs`
- `Cargo.toml`
- `Cargo.lock`
- `CHANGELOG.md`
- `docs/reports/forge-core-v0.4.63-report-2026-05-24.md`

## Validation

- Red test: `cargo test frequent_local_code_goals_select_deterministic_node_without_schedule_scaffolding --test forge_cli_contract` failed first because no deterministic local-code task existed for the standalone frequent Node.js goal.
- Focused green test: `cargo test frequent_local_code_goals_select_deterministic_node_without_schedule_scaffolding --test forge_cli_contract` passed after implementation.
- Focused regression set: `cargo test deterministic --test forge_cli_contract` passed with 6 tests.
- `cargo fmt --check`: passed after applying `cargo fmt` to the new code.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test`: passed with 93 CLI contract tests.
- `cargo build --release`: passed.
- CLI smoke `./target/release/forge --store /tmp/forge-plan-smoke-0.4.63.sqlite plan --goal "Create a delivery platform" --output json`: passed.
- CLI smoke `./target/release/forge --store /tmp/forge-skill-smoke-0.4.63.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.63`: passed.

## Install Notes

- Required global install `cargo install --path . --force` was attempted after validation but was blocked by the sandbox: `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem.
- Workspace-local fallback install succeeded with `CARGO_NET_OFFLINE=true cargo install --path . --force --locked --offline --root /home/arthur/projects/forge-core/.forge/local-install`.
- The fallback binary reports `forge 0.4.63`.

## Publication Notes

- Publication was attempted after validation through the required GitHub CLI contract.
- `gh auth token` was run with output redirected away from chat.
- `git remote get-url origin` reported `https://github.com/cardozoarthur/forge-core.git`.
- `git add ...` was attempted and blocked because `.git/index.lock` cannot be created on the current read-only git metadata mount.
- `git push` was attempted and failed because the sandbox cannot resolve `github.com`.
- The validated changes remain in the working tree and were not committed or pushed from this sandbox.

## Safety

- The change is graph-planning metadata plus context routing projection only.
- It does not execute local Python/Node.js code, complete tasks, promote workflows,
  authorize CLIs, acquire leases, install Knative or mutate Docker/Kubernetes/Knative
  resources.
- Scheduled continuation tasks are not added for standalone local code goals.

## Next Recommended Cycle

Add a persisted execution-policy decision record so `forge list` and `forge inspect`
can show why Forge chose model, mixed, deterministic executor or reusable local
code-node policy without rebuilding the full task context.
