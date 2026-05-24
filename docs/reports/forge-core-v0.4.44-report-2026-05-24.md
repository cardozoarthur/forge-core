# Forge Core v0.4.44 Report - Registry Context Quality Triage

## Objective

Improve the Context Routing Engine with a small structural increment that lets
operators triage context quality pressure directly from `forge list`, scoped to the
same lifecycle filter used for workflow inventory.

## Change

`forge list --output json` now includes `context_quality` on each workflow row and on
the filtered registry summary.

The projection uses schema `forge.registry_context_quality.v1` and aggregates:

- routing quality status counts: pass, warning and blocked;
- warning severity counts: blocking, warning and advisory;
- warning-code pressure counts for required context missing, budget pressure,
  compressed context and profile-filtered optional context;
- minimum and average routing quality score.

Each workflow row also includes `quality_action` using schema
`forge.registry_quality_action.v1`. The recommendation is read-only and currently
selects actions such as `increase_context_budget`, `wait_for_dependencies`,
`verify_executor_profile`, `review_context_summary_before_reuse` and
`start_executor_handoff`.

The registry reuses the existing Context Routing Engine quality contract already
computed for handoff summaries. It does not recompute shard selection in the registry
layer.

## Safety

This is a read-only registry projection. It does not acquire leases, complete tasks,
promote workflows, authorize CLIs, execute local Python/Node.js code, install Knative
or mutate Docker/Kubernetes/Knative resources.

Executor work remains gated by `forge task handoff`, strict context readiness,
dependency readiness and task leases.

## TDD Evidence

- RED: `cargo test list_aggregates_context_quality_and_recommends_quality_actions_by_lifecycle --test forge_cli_contract` failed because `summary.context_quality` was absent from `forge list`.
- GREEN: the same focused test passed after adding registry quality aggregation and workflow-level `quality_action`.
- The full CLI contract suite passed with 71 tests.

## Validation

Required validation passed:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`

CLI smoke passed with `PATH="$PWD/target/release:$PATH"`:

- `forge --store /tmp/forge-core-v044-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json`
- `forge --store /tmp/forge-core-v044-skill-smoke.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v044`

The checkout-local release binary reports `forge 0.4.44`.

## Post-Validation Install

`cargo install --path . --force` was attempted after validation, but this sandbox
blocked writes to `/home/arthur/.cargo/.crates.toml` with `Read-only file system
(os error 30)`.

The workspace-local fallback install passed:

- `CARGO_NET_OFFLINE=true CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --offline`
- `/home/arthur/projects/forge-core/.forge/local-install/bin/forge --version`: `forge 0.4.44`

## Publication Attempt

The GitHub CLI publication contract was attempted after validation:

- `gh auth token` exited successfully.
- `git remote get-url origin` resolved to `https://github.com/cardozoarthur/forge-core.git`.
- `git add ... && git commit -m "Add registry context quality triage"` was blocked
  because the sandbox could not create
  `/home/arthur/projects/forge-core/.git/index.lock`: `Sistema de ficheiros só de leitura`.
- `git push` was attempted and failed because DNS/network access to `github.com` was
  unavailable: `Could not resolve host: github.com`.

No commit or push was completed from inside this sandbox.

## Next Recommended Cycle

Add registry quality filtering or sorting, for example
`forge list --quality-action increase_context_budget`, so long-running operators can
slice large workflow fleets by the next context-quality intervention without opening
each workflow inspection report.
