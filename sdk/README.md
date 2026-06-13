# Forge SDKs

This directory defines the shared contract for language-specific Forge SDKs.

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
They should serialize the same Forge contract into the language-native API shape.

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
- `python/forge_sdk/workflow.py`
- `go/forge.go`
- `rust/src/lib.rs`

These files are intentionally small but executable as a contract baseline. The next step is to swap the stubbed `_invoke`/`invoke` bodies for real transport once the Forge API boundary is finalized.
