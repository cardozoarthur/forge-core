# Forge Core v0.4.22 Report - 2026-05-24

## Objective

Advance long-running cognition with durable task checkpoints that can be inspected by async callers and routed back into bounded executor context.

## Change

Forge now persists checkpoint metadata through `forge task checkpoint`.

Each checkpoint records:

- checkpoint id;
- workflow id and task id;
- executor;
- checkpoint state and summary;
- context packet SHA-256;
- workflow revision at the time the executor checkpointed.

`forge request status` now includes `checkpoint_count` and `latest_checkpoint`, so Codex/OpenCode callers can recover the latest continuation point from Forge state instead of keeping executor-local progress state.

`forge context` now emits schema `forge.context.v9` with routing policy `task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_budget_decisions_v9`. Context packets include `latest_checkpoint`, `resume_context_status`, `resume_context_reason`, and a checkpoint shard when the task has a checkpoint.

## Resume Semantics

- `checkpoint_current`: the checkpoint workflow revision matches the current workflow revision.
- `checkpoint_stale`: the workflow has moved on since the checkpoint, so an executor must refresh context before resuming.
- `no_checkpoint`: no task checkpoint exists yet.

## Safety

Checkpoints are metadata only. Recording a checkpoint does not complete a task, promote a workflow, execute local Python/Node.js code, authorize external CLIs, mutate Docker/Kubernetes/Knative resources, or bypass validation gates.

Stale checkpoints remain visible for audit and partial retry decisions. The context packet explicitly reports the stale reason instead of hiding old continuation state.

## TDD Evidence

- RED: `cargo test task_checkpoint_records_resumable_context_and_surfaces_request_status --test forge_cli_contract` failed because `forge task checkpoint` was not a recognized subcommand.
- GREEN: the same focused test passed after checkpoint persistence, CLI handling and request-status projection were implemented.
- GREEN: `cargo test context_package_includes_latest_checkpoint_and_marks_stale_after_goal_mutation --test forge_cli_contract` passed after context schema v9 checkpoint routing was implemented.

## Validation

- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test`: passed, 48 CLI contract tests.
- `cargo build --release`: passed.
- CLI smoke `./target/release/forge --store /tmp/forge-core-v0422-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json`: passed.
- CLI smoke `./target/release/forge --store /tmp/forge-core-v0422-skill-smoke.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v0422`: passed.

## Installation

- `cargo install --path . --force`: blocked because `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem in this execution environment.
- `cargo install --path . --force --root .forge/local-install --offline`: passed.
- `.forge/local-install/bin/forge --version`: `forge 0.4.22`.

## Next Recommended Cycle

Add partial retry planning from stale checkpoints: when a checkpoint is stale or a validation gate fails, derive the minimal retry node/subflow set and expose it through `forge inspect` and `forge request status` without promoting unfinished work.
