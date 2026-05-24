# Forge Core v0.4.51 Report - 2026-05-24

## Summary

- Added a versioned Context Routing Engine budget plan to make minimum-correct context budgets explicit before executor handoff.
- `forge context` now emits schema `forge.context.v22` with routing policy `task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_persona_contract_next_action_v22`.
- `forge inspect` now projects each node's `budget_plan` and renders a compact terminal marker with minimum/recommended budget and plan status.

## Behavior

- `budget_plan` uses schema `forge.context.budget_plan.v1`.
- The plan records requested and effective budget, selected bytes, required original bytes, required minimum bytes, minimum-correct budget, optional bytes, profile-excluded bytes, omitted required/optional bytes, compression savings, recommended budget, missing required sections, budget-omitted sections and a status/reason.
- Context routing fingerprints now include a `budget_plan` component, so cache keys change when the deterministic budget contract changes.
- Missing required sections produce `status: repair_required` and a recommended budget above the current effective budget.
- Optional pressure without missing required context produces an advisory status instead of blocking handoff by itself.

## Validation

- Added CLI contract coverage for direct `forge context` budget-plan output.
- Added CLI contract coverage for `forge inspect --output json` budget-plan projection and terminal diagram marker.
- Ran `cargo test` during development after the implementation; full required validation is recorded in the cycle final report.

## Safety

- The feature is read-only metadata derived from Forge-owned workflow/task state and deterministic shard selection.
- It does not complete tasks, promote workflows, authorize executors, execute local code nodes, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor handoff remains gated by context readiness, dependency readiness, validation rules and task leases.
