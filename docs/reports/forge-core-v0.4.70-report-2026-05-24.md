# Forge Core v0.4.70 Self-Evolution Report

Run id: `run_5ab6bee52d14443685699c37e07a8147`  
Workflow id: `wf_6363a587e503427cab991131d131ae08`  
Prompt packet: `forge.self_evolution.prompt.v1`

## Increment

Added context-action catalog discovery to the workflow registry CLI.

Before this cycle, `forge list --context-action <action>` could filter workflows by
context routing action, but operators had to know the accepted filter values by
reading source, tests or prior reports. That made the registry less useful as an
operational surface for running/non-running workflow triage.

`forge list --context-actions --output json` now returns a static, versioned catalog
with schema `forge.registry_context_action_catalog.v1`. The catalog lists every
accepted `--context-action` filter value, its readiness class and the trigger that
causes that action to appear in registry summaries. This mirrors the existing
quality-action catalog and gives terminal operators a deterministic discovery path
for handoff, dependency wait, context repair, checkpoint resume and partial retry
filters.

## Files Changed

- `src/registry.rs`
- `src/main.rs`
- `tests/forge_cli_contract.rs`
- `Cargo.toml`
- `Cargo.lock`
- `CHANGELOG.md`
- `README.md`
- `docs/technical-definition.md`
- `docs/reports/forge-core-v0.4.70-report-2026-05-24.md`

## Validation

- Red test: `cargo test list_surfaces_context_action_catalog_for_filter_discovery -- --nocapture` failed first because Clap rejected `--context-actions`.
- Focused green test: `cargo test list_surfaces_context_action_catalog_for_filter_discovery -- --nocapture` passed after implementation.
- Adjacent catalog regression: `cargo test list_surfaces_quality_action_catalog_for_filter_discovery -- --nocapture` passed.
- Required validation:
  - `cargo fmt --check`: passed.
  - `cargo clippy --all-targets --all-features -- -D warnings`: passed.
  - `cargo test`: passed with 100 CLI contract tests.
  - `cargo build --release`: passed.
- CLI smoke:
  - `./target/release/forge --store /tmp/forge-plan-smoke-0.4.70.sqlite plan --goal "Create a delivery platform" --output json`: passed.
  - `./target/release/forge --store /tmp/forge-skill-smoke-0.4.70.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.70`: passed.
  - `./target/release/forge --store /tmp/forge-context-actions-smoke-0.4.70.sqlite list --context-actions --output json`: passed.

## Install Notes

- Required global install `cargo install --path . --force` was attempted after validation and was blocked by the sandbox: `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem.
- Workspace-local fallback install succeeded with `cargo install --path . --force --locked --offline --root /home/arthur/projects/forge-core/.forge/local-install`.
- The fallback binary reports `forge 0.4.70`.

## Publication Notes

- `gh auth token` succeeded with output redirected away from chat.
- `git remote get-url origin` reported `https://github.com/cardozoarthur/forge-core.git`.
- `git add CHANGELOG.md Cargo.toml Cargo.lock README.md docs/technical-definition.md docs/reports/forge-core-v0.4.70-report-2026-05-24.md src/main.rs src/registry.rs tests/forge_cli_contract.rs` was attempted after validation and failed because `.git/index.lock` cannot be created on the current read-only git metadata mount.
- `git push` was attempted and failed because the sandbox cannot resolve `github.com`.
- The validated changes remain in the working tree and were not committed or pushed from this sandbox.

## Safety

- The change is static read-only metadata over already-existing registry context actions.
- It does not execute local Python/Node.js code, complete tasks, promote workflows,
  authorize CLIs, run installed CLIs as executors, install Knative or mutate
  Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency
  readiness, validation rules, task leases, persona gates, child-subflow validation
  gates and continuation plans.

## Next Recommended Cycle

Add per-workflow context-action task refs to `forge list --output json`, so an
operator filtering by `wait_for_dependencies`, `start_executor_handoff` or
`partial_retry_with_fresh_context` can see the exact task ids and reasons to inspect
or hand off without opening every workflow one by one.
