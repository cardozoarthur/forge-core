# Forge Core v0.4.72 Self-Evolution Report

Run id: `run_0898c5f0045b44a089bf6745f242cc94`  
Workflow id: `wf_cf450c494ee44e5c962c179e172e40ff`  
Prompt packet: `forge.self_evolution.prompt.v1`

## Increment

Added per-task context-action refs to `forge list --output json`.

Before this cycle, `forge list` exposed aggregate `context_actions` counts for each
workflow row. Operators could filter workflows by action, but still had to open
`forge inspect` to find the exact task ids, blockers, checkpoint refs and routing
cache key behind an aggregate such as `wait_for_dependencies` or
`partial_retry_with_fresh_context`.

Workflow registry rows now include `context_action_refs` using schema
`forge.registry_context_action_ref.v1`. Each ref records task id, title, executor,
next action, context/dependency readiness, handoff status, routing quality status,
blocker refs, checkpoint refs, current routing cache key, context checksum and the
action reason. This keeps registry filtering actionable without changing workflow
state or promoting tasks.

## Files Changed

- `src/registry.rs`
- `tests/forge_cli_contract.rs`
- `Cargo.toml`
- `Cargo.lock`
- `CHANGELOG.md`
- `README.md`
- `docs/technical-definition.md`
- `docs/reports/forge-core-v0.4.72-report-2026-05-24.md`

## Validation

- Red test: `cargo test list_aggregates_context_next_actions_for_registry_rows -- --nocapture` failed first because `context_action_refs` was absent from registry rows.
- Focused green test: `cargo test list_aggregates_context_next_actions_for_registry_rows -- --nocapture` passed after implementation.
- Adjacent registry checks: `cargo test list_ -- --nocapture` passed with 12 filtered `forge list` tests.
- Required validation:
  - `cargo fmt --check`: passed.
  - `cargo clippy --all-targets --all-features -- -D warnings`: passed.
  - `cargo test`: passed with 101 CLI contract tests.
  - `cargo build --release`: passed.
- CLI smoke:
  - `./target/release/forge --store /tmp/forge-plan-smoke-0.4.72.sqlite plan --goal "Create a delivery platform" --output json`: passed.
  - `./target/release/forge --store /tmp/forge-skill-smoke-0.4.72-run0898.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.72-run0898`: passed.

## Install Notes

- Required global install `cargo install --path . --force` was attempted after validation and was blocked by the sandbox: `/home/arthur/.cargo/.crates.toml` is read-only.
- Workspace-local fallback install succeeded with `cargo install --path . --force --locked --offline --root /home/arthur/projects/forge-core/.forge/local-install`.
- The fallback binary reports `forge 0.4.72`.

## Publication Notes

- `gh auth token` succeeded with output redirected away from chat.
- `git remote get-url origin` reported `https://github.com/cardozoarthur/forge-core.git`.
- `git add CHANGELOG.md Cargo.lock Cargo.toml README.md docs/technical-definition.md src/registry.rs tests/forge_cli_contract.rs docs/reports/forge-core-v0.4.72-report-2026-05-24.md` was attempted after validation and failed because `.git/index.lock` cannot be created on the current read-only git metadata mount.
- `git push` was attempted and failed because the sandbox cannot resolve `github.com`.
- The validated changes remain in the working tree and were not committed or pushed from this sandbox.

## Safety

- Context action refs are read-only metadata derived from deterministic context routing packages.
- This change does not execute local Python/Node.js code, complete tasks, promote workflows, authorize CLIs, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency readiness, validation rules, task leases, persona gates, child-subflow validation gates and continuation plans.

## Next Recommended Cycle

Add `forge list --task-action <action>` or `forge list --task <task-id>` style focused registry output so operators can request only affected task refs for a given context-action filter instead of receiving every task ref in matching workflows.
