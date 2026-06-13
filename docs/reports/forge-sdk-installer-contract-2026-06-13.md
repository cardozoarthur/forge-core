# Forge SDK And Installer Contract

Data: 2026-06-13

This document turns the SDK and installer work into a shared contract rather than language-specific experiments.

## Shared workflow model

All SDKs should speak the same core units:

- `workflow`
- `subworkflow`
- `node`
- `resume`
- `approval`
- `artifact`
- `event`
- `context`

The key rule is that language bindings do not invent new workflow semantics. They expose the same Forge model through native APIs.

The workflow model must also support:

- async nodes;
- parallel branches;
- external subworkflow calls;
- a final aggregator node that joins results;
- file and artifact workflows as first-class graphs;
- resume by stable workflow identity.

## Language targets

### TypeScript

Best for browser/client composition and app-level orchestration.

Expected surface:

```ts
const flow = forge.workflow("demo");
await flow.node("fetch-data").run();
await flow.subworkflow("rust-merge").run(payload);
await flow.parallel([
  forge.workflow("py-parse").run(input),
  forge.workflow("go-sync").run(input),
]).join("ts-aggregate");
```

### Python

Best for async orchestration, data workflows and parallel fan-out/fan-in.

Expected surface:

```py
flow = forge.workflow("demo")
await flow.run(input)
await flow.subworkflow("go-worker").run(payload)
results = await flow.parallel([
    forge.workflow("rust-normalize").run(input),
    forge.workflow("go-enrich").run(input),
]).join("py-merge")
```

### Go

Best for service-side orchestration, background workers and infra-facing flows.

Expected surface:

```go
flow := forge.Workflow("demo")
result, err := flow.Run(ctx, input)
joined, err := flow.Parallel(
    forge.Workflow("python-parse"),
    forge.Workflow("rust-validate"),
).Join(ctx, "go-aggregate")
```

### Rust

Best for native embedding and direct runtime control.

Expected surface:

```rust
let flow = forge.workflow("demo");
flow.run(input).await?;
let joined = flow.parallel(vec![
    forge.workflow("python-parse"),
    forge.workflow("go-enrich"),
]).join("rust-aggregate").await?;
```

## Cross-language behavior

- any language can start a workflow
- workflows can call subworkflows in other languages
- async functions should be first-class
- parallel branches should converge into a final aggregator
- resume must preserve the same workflow identity

## Installer contract

The installer must produce the same shell-visible `forge` entrypoint on:

- Linux
- macOS
- Windows

The packaging layer may differ per platform, but the product contract may not.

## Current scaffold in the repo

- `sdk/README.md`
- `installer/README.md`

Those are the first anchor points for the later concrete SDK and packaging implementations.
