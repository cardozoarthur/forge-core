# Forge Core v0.4.68 Self-Evolution Report

Run id: `run_d3408e69d6564fc5bc5ec549dd9b543f`  
Workflow id: `wf_9d5f4a4fe2d24d72a2b9c875a5aff03d`  
Prompt packet: `forge.self_evolution.prompt.v1`

## Increment

Added context-action filtering to `forge list`.

Before this cycle, the registry already aggregated next context-routing actions in
`context_actions`, but operators could only read the counts after listing a
slice. They could not directly ask the registry for workflows that currently
need a specific context action such as dependency waiting, budget repair or
partial retry.

`forge list` now accepts:

```bash
forge list --context-action wait_for_dependencies --output json
forge list --lifecycle running --context-action wait_for_dependencies --output json
```

The registry response now includes `filter.context_action`, and all registry
summaries are recomputed after applying lifecycle, context-action and
quality-action filters together. The filter is read-only and uses the existing
versioned `forge.registry_context_action.v1` aggregate, so no task state is
advanced by listing.

## Files Changed

- `src/main.rs`
- `src/registry.rs`
- `tests/forge_cli_contract.rs`
- `Cargo.toml`
- `Cargo.lock`
- `CHANGELOG.md`
- `README.md`
- `docs/reports/forge-core-v0.4.68-report-2026-05-24.md`

## Validation

- Red test: `cargo test list_filters_workflow_registry_by_context_action_and_lifecycle --test forge_cli_contract` failed first because Clap rejected `--context-action`.
- Focused green test: `cargo test list_filters_workflow_registry_by_context_action_and_lifecycle --test forge_cli_contract` passed after implementation.
- Focused registry regression: `cargo test list_ --test forge_cli_contract` passed with 11 list/registry tests.
- Required validation:
  - `cargo fmt --check`: passed.
  - `cargo clippy --all-targets --all-features -- -D warnings`: passed.
  - `cargo test`: passed with 98 CLI contract tests.
  - `cargo build --release`: passed.
- CLI smoke:
  - `./target/release/forge --store /tmp/forge-plan-smoke-0.4.68.sqlite plan --goal "Create a delivery platform" --output json`: passed.
  - `./target/release/forge --store /tmp/forge-skill-smoke-0.4.68.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.68`: passed.

## Install Notes

- Required global install `cargo install --path . --force` was attempted after validation and was blocked by the sandbox: `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem.
- Workspace-local fallback install succeeded with `CARGO_NET_OFFLINE=true cargo install --path . --force --locked --offline --root /home/arthur/projects/forge-core/.forge/local-install`.
- The fallback binary reports `forge 0.4.68`.

## Publication Notes

- Publication preparation followed the requested GitHub CLI contract.
- `gh auth token` succeeded with output redirected away from chat.
- `git remote get-url origin` reported `https://github.com/cardozoarthur/forge-core.git`.
- `git add CHANGELOG.md Cargo.toml Cargo.lock README.md src/main.rs src/registry.rs tests/forge_cli_contract.rs docs/reports/forge-core-v0.4.68-report-2026-05-24.md` was attempted and failed because `.git/index.lock` cannot be created on the current read-only git metadata mount.
- `git push` was attempted and failed because the sandbox cannot resolve `github.com`.
- The validated changes remain in the working tree and were not committed or pushed from this sandbox.

## Safety

- The change is read-only registry metadata over Forge-owned persisted workflows.
- It does not execute local Python/Node.js code, complete tasks, promote
  workflows, authorize CLIs, run installed CLIs as executors, install Knative or
  mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency
  readiness, validation rules, task leases, persona gates, child-subflow
  validation gates and continuation plans.

## Next Recommended Cycle

Add discovery for supported `--context-action` filter values, mirroring
`forge list --quality-actions`, so operators can enumerate context-action slices
without reading source or remembering field names from the JSON schema.
