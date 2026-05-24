# Forge Core v0.4.52 Report - 2026-05-24

## Summary

- Added a versioned Context Routing Engine delta contract for resumable executor work.
- `forge context` now emits schema `forge.context.v23` with routing policy `task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_persona_contract_next_action_delta_v23`.
- `forge inspect` and `forge task handoff` now surface the same context delta so operators and adapters can decide whether to reuse checkpointed context, refresh context or perform a partial retry.

## Behavior

- `context_delta` uses schema `forge.context.delta.v1`.
- The contract compares the latest checkpoint against the current context payload hash, current routing cache key and current workflow revision.
- Delta statuses include `no_checkpoint`, `checkpoint_stale`, `checkpoint_route_unknown`, `unchanged`, `route_changed`, `content_changed` and `changed`.
- The object exposes checkpoint/current hashes and routing keys, changed components, `can_reuse_checkpoint_context`, `partial_retry_recommended` and a short reason.
- Terminal inspection diagrams now include a compact `delta <status>` marker beside the next action and budget plan marker.

## Validation

- Added CLI contract coverage for `forge context` delta output before and after recording a task checkpoint.
- Required validation commands are recorded in the cycle final report.

## Safety

- Context delta is read-only metadata derived from Forge-owned workflow/task/checkpoint state.
- It does not complete tasks, promote workflows, authorize executors, execute local code nodes, install Knative or mutate Docker/Kubernetes/Knative resources.
- Execution remains gated by context readiness, dependency readiness, validation rules and task leases.
