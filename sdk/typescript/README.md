# Forge TypeScript SDK

Status: scaffold

Purpose:
- compose Forge workflows from application code
- call subworkflows from browser or Node environments
- surface async flow composition with the same contract as other SDKs

Contract:
- import a Forge client
- create a workflow
- invoke nodes or subworkflows
- resume by workflow id
- fan out to Python/Go/Rust subflows and join in a TypeScript aggregator
