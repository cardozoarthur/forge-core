# Forge Core v0 Validation Report

Date: 2026-05-23

## Scope

This report covers the first CLI + Skill version of Forge Core.

## Validated Capabilities

- Atomic task graph generation from a human goal.
- Persistent workflow state in SQLite.
- Task-local context generation with byte budget.
- Validation gate that blocks incomplete workflows.
- Simulated execution runtime.
- Autonomous mixed workflow planning with `ai`, `command`, `wait` and `notification` executors.
- Cron representation for future continuation tasks.
- Simulated email notification payload with workflow cost report.
- Controlled improvement artifact generation.
- Artifact listing with SHA-256.
- Codex/OpenCode skill installation paths.

## Required Commands

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

## Publication Gate

The project may be published when all commands pass and a CLI smoke confirms:

```bash
forge --version
forge plan --goal "Create a delivery platform" --output json
forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke
```

## Safety Boundary

Forge Core v0 does not execute unrestricted self-modification. The improvement loop only generates experimental artifacts and requires validation/benchmark evidence before promotion.
