# Forge Core v0.4.73 Self-Evolution Priority Report

Run source: Codex supervisor intervention  
Priority: make self-evolution honor persisted Forge goals

## Increment

Forge self-evolution now carries runtime goal mutations into future cycles.

Before this change, `forge self run` created each self-evolution workflow from a
fixed base goal and rendered prompt packet `forge.self_evolution.prompt.v1` with a
hardcoded strategic backlog. Human updates made through `forge workflow update-goal`
were persisted in Forge state, but a later self-evolution cycle could still start
from the generic prompt and miss new priorities such as clusterization or n8n node
research.

`forge self run` now:

- finds the most specific persisted Forge self-evolution goal in the SQLite store;
- creates the next self-evolution workflow from that persisted goal;
- reloads the current workflow before each cycle;
- emits prompt packet `forge.self_evolution.prompt.v2`;
- places the current persisted goal, initial goal and workflow revision before
  generic strategic guidance.

This makes the persisted Forge workflow goal authoritative for future cycles.

## Files Changed

- `src/self_evolve.rs`
- `tests/forge_cli_contract.rs`
- `Cargo.toml`
- `Cargo.lock`
- `CHANGELOG.md`
- `README.md`
- `docs/technical-definition.md`
- `docs/reports/forge-core-v0.4.73-report-2026-05-24.md`

## Validation

- Focused test:
  - `cargo test self_run_prompt_uses_persisted_self_evolution_goal_updates -- --nocapture`: passed.
- Required validation:
  - `cargo fmt --check`: passed.
  - `cargo clippy --all-targets --all-features -- -D warnings`: passed.
  - `cargo test`: passed with 102 CLI contract tests.
  - `cargo build --release`: passed.

## Safety

- This change only affects self-evolution planning and prompt generation.
- It does not execute local Python/Node.js code, complete tasks, promote workflows,
  authorize CLIs, install Knative or mutate Docker/Kubernetes/Knative resources.
- Generic strategic guidance remains present, but persisted runtime goal state now
  takes priority.

## Next Recommended Cycle

Run the next self-evolution cycle from v0.4.73 so the prompt includes the currently
persisted goals for clusterization and n8n node research before the generic backlog.
