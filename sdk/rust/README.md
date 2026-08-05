# Foundry Rust SDK

Status: scaffold

Purpose:
- native embedding
- low-level runtime control
- local server/desktop integration

Contract:
- expose workflow builders
- run async graphs
- preserve Foundry-owned context and resume ids
- act as a native aggregator or embedded worker in multi-language graphs

The canonical crate is `foundry-sdk-rust`. The old `forge-sdk-rust` package <!-- foundry-brand-allow: legacy-compat -->
name denotes the legacy Forge generation and requires a separately published <!-- foundry-brand-allow: legacy-compat -->
compatibility wrapper if existing registry consumers still depend on it.
