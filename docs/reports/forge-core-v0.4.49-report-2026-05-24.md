# Forge Core v0.4.49 Report - 2026-05-24

## Increment

- Added context-level Personality/Soul Routing contract metadata for human-facing nodes.
- `forge context` now emits schema `forge.context.v20` with routing policy `task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_persona_contract_v20`.
- Context packets now include `persona_contract` when a task has node-scoped persona routing.

## Contract

- `persona_contract.schema_version = forge.context.persona_contract.v1`.
- The contract records mode, scope, instruction source, voice, tone, validation gate, source models and auditability.
- The contract also records the context lineage hash and persona-mode hash, so context reuse and validation can distinguish persona changes before executor handoff.
- Context routing fingerprints now include a `persona_contract` component.

## Validation Intent

- Added a CLI contract test proving a human-facing documentation task exposes the context persona contract and binds it to lineage.
- Existing context schema expectations were advanced from `forge.context.v19` to `forge.context.v20`.

## Safety Notes

- This is read-only metadata derived from Forge-owned task persona routing and context lineage.
- No external Docker, Kubernetes or Knative resources are installed or mutated.
- No local Python/Node.js code execution is introduced.
- Persona promotion remains gated by `persona_routing_required`; executor work remains gated by context readiness, dependency readiness, validation rules and task leases.
