# Foundry Python SDK

Status: scaffold

Purpose:
- async orchestration
- data workflow composition
- parallel fan-out/fan-in flows
- workflow resume and artifact attachment

Contract:
- create a workflow object
- run or resume it
- call external subflows as children
- fan out to Rust/Go workers and aggregate results in Python when needed

Canonical imports come from `foundry_sdk` and use `Foundry*` classes. The
`forge_sdk` package in this source tree is a deprecated re-export shim for the <!-- foundry-brand-allow: legacy-compat -->
legacy Forge API during `0.6.x`; it emits `DeprecationWarning` and must not be <!-- foundry-brand-allow: legacy-compat -->
used by new integrations.
