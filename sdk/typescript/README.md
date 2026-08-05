# Foundry TypeScript SDK

Status: scaffold

Purpose:
- compose Foundry workflows from application code
- call subworkflows from browser or Node environments
- surface async flow composition with the same contract as other SDKs

Contract:
- import a Foundry client
- create a workflow
- invoke nodes or subworkflows
- resume by workflow id
- fan out to Python/Go/Rust subflows and join in a TypeScript aggregator

Use `@foundry/core-sdk` and the `Foundry*` exports. Deprecated `Forge*` export <!-- foundry-brand-allow: legacy-compat -->
aliases exist only for source compatibility during `0.6.x`; the legacy
`@forge/core-sdk` package name is not the canonical distribution. <!-- foundry-brand-allow: legacy-compat -->
