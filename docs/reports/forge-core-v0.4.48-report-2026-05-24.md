# Forge Core v0.4.48 Report - 2026-05-24

## Summary

Forge Core now emits a versioned Context Routing Engine repair plan for budget-related context failures. When required context is omitted, `forge context` returns an auditable `routing_repair` object that tells operators and executor adapters the repair action and the recommended effective budget for retry.

## Behavior

- `forge context` now emits schema `forge.context.v19` with routing policy `task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_v19`.
- `routing_repair` uses schema `forge.context.routing_repair.v1`.
- The repair contract records status, action, current effective budget, recommended budget, required budget deficit, missing required sections, omitted-by-budget sections, compressed sections and reason.
- Context routing fingerprints include a `routing_repair` component, making repair-plan changes part of executor cache-key lineage.
- `forge inspect --output json` projects the repair contract on terminal nodes through `context_route.routing_repair`.

## Validation Scope

- Added CLI contract coverage for a missing-required-context route that recommends `increase_context_budget`.
- Required validation commands for the cycle remain `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test` and `cargo build --release`.

## Safety

- The repair plan is read-only metadata derived from Forge-owned workflow/task state and deterministic context shard selection.
- It does not complete tasks, promote workflows, authorize executors, execute local code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains blocked until context readiness, dependency readiness, validation rules and task leases allow execution.
