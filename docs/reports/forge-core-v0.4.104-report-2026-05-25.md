# Forge Core v0.4.104 Self-Evolution Report

Run id: `run_bfba8dcc4747450da9067f8cdc713b58`  
Workflow id: `wf_047a8146d7fb42a7800cbfdad1b59f72`  
Cycle: `19`  
Date: `2026-05-25`

## Summary

Forge now exposes human interaction nodes through MCP as agent-facing workflow primitives.

This is `0.5 groundwork`, not a completed Forge 0.5 creative runtime. The change closes the MCP approval-bridge gap for the existing human decision/form node model while preserving Forge-owned workflow state, revisions, validation gates and audit history.

## Added Behavior

- Added MCP tools:
  - `forge.interaction.create_choice`
  - `forge.interaction.create_form`
  - `forge.interaction.answer`
  - `forge.interaction.expire`
  - `forge.interaction.list`
- Reused the existing Forge interaction state machine instead of creating a parallel approval model.
- Agent-facing choice/form calls now persist the same `forge.human_interaction.v1` and `forge.human_decision.v1` state as the CLI.
- MCP answers resume blocked tasks by returning them to pending work through Forge state.
- MCP expiry keeps timed-out interactions blocked instead of letting workflow progression skip human judgment.

## TDD Evidence

- RED: `cargo test mcp_human_interaction --test forge_cli_contract` failed because `forge.interaction.create_choice` and `forge.interaction.create_form` were unknown MCP tools.
- GREEN: the same focused tests passed after wiring MCP manifest entries and dispatch handlers to `src/interaction.rs`.
- Additional focused pass: `cargo test mcp_exposes_human_interaction_bridge_tools --test forge_cli_contract`.

## Validation

- `cargo fmt --check`: passed
- `cargo clippy --all-targets --all-features -- -D warnings`: passed
- `cargo test`: passed, including 6 unit tests and 173 CLI contract tests
- `cargo build --release`: passed

## Smoke Evidence

- `./target/release/forge --store /tmp/forge-core-v04104-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json`: passed, produced workflow `wf_6ba19e3bb213423b8bfbbb60f5f2b1b9`.
- `./target/release/forge --store /tmp/forge-core-v04104-skill-smoke-2.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-04104-2`: passed, installed Codex and OpenCode skill files and preserved pending human approval for executors/runtimes.
- `./target/release/forge --store /tmp/forge-core-v04104-daily-smoke-2.sqlite schedule create-daily-goal-research --goal hackathon --timezone America/Sao_Paulo --cron "0 8 * * *" --origin codex --output json`: passed, produced workflow `wf_20c44cf02f4a4820a80ca78e8ada8e56`.
- `./target/release/forge --store /tmp/forge-core-v04104-daily-smoke-2.sqlite run --workflow wf_20c44cf02f4a4820a80ca78e8ada8e56 --simulate --output json`: passed, completed 16 tasks and generated the daily Goal smoke artifacts.
- Daily Goal smoke artifacts:
  - `artifacts/wf_20c44cf02f4a4820a80ca78e8ada8e56/goal-hackathon-report.md`
  - `artifacts/wf_20c44cf02f4a4820a80ca78e8ada8e56/goal-hackathon-report.pdf`
  - `artifacts/wf_20c44cf02f4a4820a80ca78e8ada8e56/telegram-delivery-hackathon.json`
- Telegram delivery record remained redacted with `secret_exposed=false`.

## Installation And Publication

- `cargo install --path . --force`: blocked by the current sandbox because `/home/arthur/.cargo/.crates.toml` is read-only (`os error 30`).
- `gh auth token >/dev/null`: passed without exposing the token.
- `git remote get-url origin`: `https://github.com/cardozoarthur/forge-core.git`.
- `git add ...`: blocked because `.git/index.lock` cannot be created on the current read-only Git metadata mount.

## Safety

- No Docker, Kubernetes, Knative, Telegram or external user resources were mutated.
- The only runtime mutations are Forge-owned SQLite/workflow-state writes in temporary smoke stores.
- MCP interaction tools do not auto-approve decisions, delete workflows, mutate external resources or bypass validation.

## Lean Overhead Ledger

- Schema: `forge.self_evolution.overhead_ledger.v1`
- Prompt bytes: approximately 43,000
- Estimated prompt tokens: approximately 10,800
- Validation command count: 4 required commands plus 5 focused/smoke commands
- Artifact count: 1 report artifact plus changelog, skill and milestone documentation updates
- Metadata bytes: approximately 8,500
- Orchestration cost score: 3
- Useful delivery: closes one agent-surface gap for human decision nodes and adds regression coverage for approval bridge behavior

## Next Recommended Cycle

Implement the Forge 0.5 design-token resolution engine: semantic/raw token resolution, inheritance and overrides, theme switching, impact preview metadata and a patch-by-intent path that changes tokens without rewriting unrelated creative artifacts.
