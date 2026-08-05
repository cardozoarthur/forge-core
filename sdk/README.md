# Foundry SDKs

This directory defines the shared contract for language-specific Foundry SDKs.

## Shared model

Every SDK should speak the same workflow language:

- `workflow`: create or inspect a workflow
- `subworkflow`: attach or invoke reusable subflows
- `node`: represent one step in the execution graph
- `resume`: continue a paused graph from a stable identifier
- `artifact`: attach outputs and evidence
- `approval`: capture a human decision gate
- `event`: publish runtime state changes

## Target bindings

- `typescript/`: browser and application SDK for workflow composition
- `python/`: async orchestration and data-heavy workflow integration
- `go/`: backend/service orchestration and fan-out/fan-in execution
- `rust/`: native embedding and local runtime control

## Design rule

The SDKs should not invent separate workflow semantics per language.
They should serialize the same Foundry contract into the language-native API shape.

## Composition Rules

The contract should support:

- async workflow execution;
- fan-out / fan-in parallel branches;
- subworkflow calls across language boundaries;
- file and artifact workflows;
- stable resume ids and replay-safe state.

One useful mental model is:

1. start a workflow in any language;
2. fan out to other language runtimes as subworkflows;
3. join the results in an aggregator node;
4. attach the final artifact back to the parent workflow.

## Initial File Layout

The repository already carries the first language surfaces:

- `typescript/index.js` and `typescript/index.d.ts`
- `python/foundry_sdk/workflow.py`
- `go/foundry.go`
- `rust/src/lib.rs`

These files are intentionally small but executable as a contract baseline. The next step is to swap the stubbed `_invoke`/`invoke` bodies for real transport once the Foundry API boundary is finalized.

## Compatibilidade da era Forge <!-- foundry-brand-allow: migration -->

Foundry names are canonical. During the `0.6.x` migration window, the Python
and TypeScript scaffolds expose explicitly deprecated `Forge*` aliases so an <!-- foundry-brand-allow: legacy-compat -->
application can change package/import paths before renaming every type. New
code must use `Foundry*`; aliases are not emitted in examples and are eligible
for removal after this single compatibility cycle. Go callers should use an
explicit import alias while moving from the legacy repository path, because a
Go directory cannot declare both `package forge` and `package foundry`. See <!-- foundry-brand-allow: legacy-compat -->
[`docs/migration-to-foundry.md`](../docs/migration-to-foundry.md) for the full
contract.
