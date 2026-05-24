# Forge Core v0.4.75 Self-Evolution Report

Run id: `run_96571c7e517e4bebbc99bef1950973f3`  
Workflow id: `wf_55ac304b163f423f8b49b316dc94413c`  
Prompt packet: `forge.self_evolution.prompt.v2`  
Executor: `codex`

## Goal

Turn the new n8n research directive into auditable Forge workflow state before
any n8n concept can become a native Forge primitive.

## Change

`forge plan` now recognizes goals that mention n8n and adds two explicit tasks:

- `Catalog n8n workflow primitives`
- `Evaluate Forge primitive candidates`

The graph construction task depends on the evaluation task. This makes the
research catalog and promotion recommendation part of the DAG instead of a
comment in a prompt. The intent also records dedicated deliverables, the risk of
blindly copying external concepts or licenses, and the unknown that current n8n
source/docs must be reviewed during execution.

## n8n Research Seed

Sources reviewed on 2026-05-24:

- n8n docs, Flow logic: https://docs.n8n.io/flow-logic/
- n8n docs, Core nodes library: https://docs.n8n.io/integrations/builtin/core-nodes/
- n8n docs, Looping: https://docs.n8n.io/flow-logic/looping/
- n8n docs, Loop Over Items: https://docs.n8n.io/integrations/builtin/core-nodes/n8n-nodes-base.splitinbatches/
- n8n docs, Wait: https://docs.n8n.io/integrations/builtin/core-nodes/n8n-nodes-base.wait/
- n8n docs, If: https://docs.n8n.io/integrations/builtin/core-nodes/n8n-nodes-base.if/
- n8n docs, Switch: https://docs.n8n.io/integrations/builtin/core-nodes/n8n-nodes-base.switch/
- n8n docs, Merge: https://docs.n8n.io/integrations/builtin/core-nodes/n8n-nodes-base.merge/
- n8n docs, Code: https://docs.n8n.io/integrations/builtin/core-nodes/n8n-nodes-base.code/
- n8n docs, Execute Sub-workflow: https://docs.n8n.io/integrations/builtin/core-nodes/n8n-nodes-base.executeworkflow/
- n8n docs, Human-in-the-loop for AI tool calls: https://docs.n8n.io/advanced-ai/human-in-the-loop-tools/
- n8n GitHub repository: https://github.com/n8n-io/n8n
- n8n `nodes-base` package metadata: https://github.com/n8n-io/n8n/blob/master/packages/nodes-base/package.json

Initial catalog:

- Splitting and routing: n8n's flow logic points operators to IF and Switch for
  conditional routing. Forge should not copy UI-oriented branching behavior, but
  the concept maps to explicit router nodes only if branch selection is
  deterministic, traceable and represented in the DAG.
- Looping and batches: n8n generally processes item lists automatically, while
  Loop Over Items is used for explicit batch loops, rate-limit handling and
  manual termination conditions. Forge should model this as bounded repeat or
  cursor/batch subflows with checkpointed termination evidence, not as an
  unconstrained infinite loop.
- Merge and joins: n8n Merge combines multiple data streams through append,
  matching, position, combinations and SQL-style modes. Forge should promote
  only typed join semantics that preserve dependency readiness, lineage and
  validation contracts.
- Wait and resume: n8n Wait pauses execution and resumes after time, specified
  date, webhook call or form submission. Forge already has wait/cron concepts;
  the next useful primitive is durable wait state with resume identity,
  checkpoint hash and timeout policy.
- Code/function behavior: n8n Code runs JavaScript or Python as a workflow step,
  with per-item or all-items modes. Forge's equivalent should remain a
  deterministic no-AI code-node policy with sandbox, runtime capability and
  validation gates before remote execution.
- Sub-workflows: n8n Execute Sub-workflow can call another workflow and decide
  whether to wait for completion. Forge should keep subflows as validated child
  DAG bindings with scale-to-zero lifecycle state, lineage and explicit
  promotion guards.
- Triggers and webhooks: n8n exposes manual, schedule, webhook, workflow and
  integration triggers. Forge should stage these as event sources that create or
  resume persisted workflows, not as direct external mutation.
- Error handling and retries: n8n's flow-logic docs identify Stop And Error and
  Error Trigger as error-handling concepts. Forge should map this to validation
  failure, retry policy, rework reason and observable failure artifacts.
- Transforms: Filter, Split Out, Aggregate/Summarize and Code belong in Forge
  only when expressed as deterministic transform nodes with typed input/output
  evidence and replayable checksums.
- Human approval: n8n HITL pauses AI tool use for approval or denial through a
  configured channel. Forge should support approval nodes as durable gates, but
  preserve mixed/autonomous workflows that do not require a human decision at
  every step.

## Validation Design

The new CLI contract test proves that an n8n research objective:

- carries an `n8n primitive research catalog` deliverable;
- creates a catalog task with n8n source/docs context requirements;
- creates a separate Forge primitive evaluation task;
- gates promotion with a validation rule tied to validated DAG execution,
  context routing, resumability, observability and operator clarity;
- makes `Build atomic task graph` depend on the evaluation task.

## Safety

No external code was copied into Forge. The research seed is a source-backed
catalog and the runtime behavior is limited to Forge-owned planning metadata.
No Docker, Kubernetes, Knative, remote host, CLI executor authorization or user
resource was mutated.

## Next Cycle

Promote the n8n catalog into a structured internal research artifact schema, for
example `forge.workflow_research_catalog.v1`, and expose it through `forge
inspect` or `forge artifacts` so future cycles can compare proposed Forge
primitives against the catalog without re-reading narrative docs.
