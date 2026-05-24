# Forge Core v0.4.46 Report - Quality-Action Catalog Discovery

## Objective

Improve the Context Routing Engine operator surface with a small structural
increment that lets operators discover supported `forge list --quality-action`
filter values before querying workflow fleets.

## Change

`forge list` now accepts:

```bash
forge list --quality-actions --output json
```

The command emits schema `forge.registry_quality_action_catalog.v1` with the
registry quality-action taxonomy:

- `repair_context_and_wait_for_dependencies`
- `increase_context_budget`
- `wait_for_dependencies`
- `verify_executor_profile`
- `review_context_summary_before_reuse`
- `start_executor_handoff`

Each catalog row includes the action, filter value, possible priorities,
description and trigger text. This makes `--quality-action <action>` filters
discoverable without requiring operators to infer valid values from current
workflow rows.

## Safety

This is a static, read-only CLI projection over Forge's registry recommendation
contract. It does not open or mutate workflow stores, acquire leases, complete
tasks, promote workflows, authorize CLIs, execute local Python/Node.js code,
install Knative or mutate Docker/Kubernetes/Knative resources.

Executor work remains gated by `forge task handoff`, strict context readiness,
dependency readiness and task leases.

## TDD Evidence

- RED: `cargo test list_surfaces_quality_action_catalog_for_filter_discovery --test forge_cli_contract`
  failed because Clap rejected `--quality-actions` and suggested the existing
  `--quality-action` filter.
- GREEN: the same focused test passed after adding the versioned catalog
  contract and CLI flag.

## Validation

Required validation passed:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test` with 73 CLI contract tests passing
- `cargo build --release`

CLI smoke passed with `PATH="$PWD/target/release:$PATH"`:

- `forge --store /tmp/forge-core-v046-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json`
- `forge --store /tmp/forge-core-v046-skill-smoke.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v046`
- `forge --store /tmp/forge-core-v046-catalog-smoke.sqlite list --quality-actions --output json`

The checkout-local release binary reports `forge 0.4.46`.

## Post-Validation Install

`cargo install --path . --force` was attempted after validation, but this sandbox
blocked writes to `/home/arthur/.cargo/.crates.toml` with `Read-only file system
(os error 30)`.

The workspace-local fallback install passed:

- `CARGO_NET_OFFLINE=true CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --offline`
- `/home/arthur/projects/forge-core/.forge/local-install/bin/forge --version`: `forge 0.4.46`

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

Add strict validation for unknown `--quality-action` filter values, using the
catalog as the source of truth, so typos fail fast instead of returning an empty
workflow slice.
