# Forge Python SDK

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
