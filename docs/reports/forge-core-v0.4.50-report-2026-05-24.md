# Forge Core v0.4.50 Report - 2026-05-24

## Increment

- Added a top-level `next_action` decision to `forge context` packets.
- `forge context` now emits schema `forge.context.v21` with routing policy `task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_persona_contract_next_action_v21`.
- The decision reuses the existing `forge.inspect_context_action.v1` shape so executor adapters can read the same action contract that operators already see in `forge inspect`.

## Contract

- `next_action.action` reports fresh handoff, dependency wait, context budget repair, stale checkpoint refresh, checkpoint resume or partial retry with fresh context.
- The decision carries readiness, partial-retry recommendation, checkpoint ids and route keys, the current context routing cache key, blocking refs and a short reason.
- `forge inspect` now reads the next action from the context package instead of recomputing an inspection-only projection.

## Validation Intent

- Added a CLI contract test proving `forge context` exposes `next_action` for a ready task.
- The same test records a checkpoint and proves the next context packet changes to `partial_retry_with_fresh_context` when the route key changes.
- Existing context schema expectations were advanced from `forge.context.v20` to `forge.context.v21`.

## Safety Notes

- This is read-only metadata derived from Forge-owned workflow, task, dependency, checkpoint and context-routing state.
- No external Docker, Kubernetes or Knative resources are installed or mutated.
- No local Python/Node.js code execution is introduced.
- Executor work remains gated by context readiness, dependency readiness, validation rules and task leases.
