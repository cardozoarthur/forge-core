# Forge Core v0.4.45 Report - Registry Quality-Action Filters

## Objective

Improve the Context Routing Engine operator surface with a small structural
increment that lets `forge list` slice workflow inventory by the recommended
context-quality intervention.

## Change

`forge list` now accepts:

```bash
forge list --quality-action increase_context_budget --output json
forge list --lifecycle running --quality-action increase_context_budget --output json
```

The JSON registry filter report now includes `filter.quality_action`, so operators
can audit both lifecycle and Context Routing Engine quality-action filters from the
same output.

Internally, registry listing now uses a composable `WorkflowRegistryFilters`
contract. The existing lifecycle-only `list_workflows_filtered` API remains
available and delegates to the new filter contract.

## Safety

This is a read-only registry projection. It does not acquire leases, complete
tasks, promote workflows, authorize CLIs, execute local Python/Node.js code,
install Knative or mutate Docker/Kubernetes/Knative resources.

Executor work remains gated by `forge task handoff`, strict context readiness,
dependency readiness and task leases.

## TDD Evidence

- RED: `cargo test list_filters_workflow_registry_by_quality_action --test forge_cli_contract`
  failed because Clap rejected `--quality-action`.
- GREEN: `cargo test list_filters_workflow_registry_by_quality_action_and_lifecycle --test forge_cli_contract`
  passed after adding the CLI flag and registry filter contract.
- Focused regression checks also passed for lifecycle filtering and registry
  context-quality aggregation.

## Validation

Required validation passed:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test` with 72 CLI contract tests passing
- `cargo build --release`

CLI smoke passed with `PATH="$PWD/target/release:$PATH"`:

- `forge --store /tmp/forge-core-v045-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json`
- `forge --store /tmp/forge-core-v045-skill-smoke.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v045`

The checkout-local release binary reports `forge 0.4.45`.

## Post-Validation Install

`cargo install --path . --force` was attempted after validation, but this sandbox
blocked writes to `/home/arthur/.cargo/.crates.toml` with `Read-only file system
(os error 30)`.

The workspace-local fallback install passed:

- `CARGO_NET_OFFLINE=true CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --offline`
- `/home/arthur/projects/forge-core/.forge/local-install/bin/forge --version`: `forge 0.4.45`

## Publication Attempt

The GitHub CLI publication contract was attempted after validation:

- `gh auth token` exited successfully with output redirected away from chat.
- `git remote get-url origin` resolved to `https://github.com/cardozoarthur/forge-core.git`.
- `git add ...` was blocked because the sandbox could not create
  `/home/arthur/projects/forge-core/.git/index.lock`: `Sistema de ficheiros só de leitura`.
- `git push` was attempted and failed because DNS/network access to `github.com`
  was unavailable: `Could not resolve host: github.com`.

No commit or push was completed from inside this sandbox.

## Next Recommended Cycle

Add explicit quality-action discovery to the CLI, for example
`forge list --quality-actions --output json`, so operators can enumerate supported
registry intervention keys before filtering large workflow fleets.
