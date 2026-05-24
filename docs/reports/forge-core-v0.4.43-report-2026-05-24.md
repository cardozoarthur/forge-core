# Forge Core v0.4.43 Report - Context Routing Quality Contract

## Objective

Improve the Context Routing Engine with a small structural increment that lets
executors and operators audit context quality without reopening full context
packets or recomputing shard decisions.

## Change

`forge context` now emits schema `forge.context.v17` with routing policy
`task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_v17`.

Each context package now includes `routing_quality` using schema
`forge.context_routing_quality.v1`. The contract reports:

- `status`: `pass`, `warning` or `blocked`;
- `score_bps`: deterministic quality score from 0 to 10000;
- `warnings`: code, severity, message, recommendation and affected context section refs.

Current warning sources are missing required context, budget pressure, compressed
context and profile-filtered optional context.

The context routing fingerprint now includes a `routing_quality` component so replay
and cache keys account for quality-relevant routing state.

`forge inspect` and `forge request status` now expose routing quality through
handoff summaries using schema `forge.context_routing_quality_summary.v1`, and each
handoff task carries its own quality contract. `forge task handoff` now emits
`forge.executor_handoff.v6` and includes the selected `context_routing_quality`
contract at the top level of the adapter packet.

## Safety

Routing quality is read-only metadata derived from Forge-owned workflow/task state
and deterministic context shard selection. It does not acquire leases, complete
tasks, promote workflows, authorize CLIs, execute local Python/Node.js code, install
Knative or mutate Docker/Kubernetes/Knative resources.

Executor handoff is still gated by strict context readiness, dependency readiness,
validation rules and task leases.

## TDD Evidence

- RED: `cargo test context_package_scores_routing_quality_for_budget_pressure --test forge_cli_contract` failed because `routing_quality` was absent.
- GREEN: the same test passed after adding the context quality contract and routing fingerprint component.
- Added `inspect_and_request_status_surface_context_routing_quality` to prove inspect and async request status project the quality signal.
- Full CLI contract suite passed with 70 tests.

## Validation

Required validation passed:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`

CLI smoke passed with `PATH="$PWD/target/release:$PATH"`:

- `forge --store /tmp/forge-core-v043-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json`
- `forge --store /tmp/forge-core-v043-skill-smoke.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v043`

The checkout-local release binary reports `forge 0.4.43`.

## Post-Validation Install

`cargo install --path . --force` was attempted after validation, but this sandbox
blocked writes to `/home/arthur/.cargo/.crates.toml` with `Read-only file system
(os error 30)`. The validated release binary remains available at
`target/release/forge`.

## Publication Attempt

The GitHub CLI publication contract was started after validation:

- `gh auth token` exited successfully.
- `git remote get-url origin` resolved to `https://github.com/cardozoarthur/forge-core.git`.
- `git add ...` was blocked because the sandbox could not create
  `/home/arthur/projects/forge-core/.git/index.lock`: `Sistema de ficheiros só de leitura`.
- `git push` was attempted and failed because DNS/network access to `github.com` was
  unavailable: `Could not resolve host: github.com`.

No commit or push was completed from inside this sandbox.

## Next Recommended Cycle

Use `routing_quality` to drive operator triage in `forge list`: aggregate quality
pressure by lifecycle slice, then add a read-only `quality_action` recommendation
for workflows that are blocked by context budget/profile pressure rather than by
dependencies.
