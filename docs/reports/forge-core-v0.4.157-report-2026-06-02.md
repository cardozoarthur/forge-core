# Forge Core 0.4.157 Self-Evolution Report

## Summary

This cycle moves Forge toward an AI-first product/workflow creation system by making PM decisions first-class Forge workflow state and by exposing PM-oriented commands through the interactive CLI surface.

## Behavior Added

- `forge workflow decision` records a durable product decision with author, rationale, affected goals/tasks/artifacts, revision and event history.
- Interactive slash commands now include `/pm` for product-management session kickoff and `/decision` for durable decision capture.
- Interactive home surfaces product decision count and PM/decision quick actions.
- Request start responses include `forge.flow_resolution.v1` evidence showing Forge searched existing flows and whether reusable subflows were attached before creating new work.
- Self-evolution default executor order is `opencode`, `gemini`, `codex`, with per-cycle fallback chains.
- Self-evolution refuses workflows with `loop_count == 0`, preserving the requirement that self-runs are represented as ordinary Forge workflows with internal loop-control tasks.

## Validation Evidence

- Targeted tests passed for flow-resolution evidence, product decision persistence, slash command discovery, quoted REPL argument parsing and self-run loop shape.
- Required validation passed: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, and `cargo build --release`.
- CLI smokes passed: `./target/release/forge plan --goal "Create a delivery platform" --output json` and `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.157`.
- Default `cargo install --path . --force` was blocked by read-only `/home/arthur/.cargo`; validated fallback install succeeded with `CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --locked --offline`, and `.forge/local-install/bin/forge --version` returned `forge 0.4.157`.

## Product Boundary

This version is not promotion-complete for the phase goal. It establishes durable PM decision state and interactive entry points, but the next cycles still need live TUI decision forms, converting approved decisions into executable backlog/tasks, agent/session switching with lineage, and visual workflow/product surfaces on top of Forge-owned state.

## Safety

The change is limited to Forge Core code, tests, changelog and report artifacts. It does not install Knative, mutate Docker/Kubernetes/Knative resources or modify external infrastructure.
