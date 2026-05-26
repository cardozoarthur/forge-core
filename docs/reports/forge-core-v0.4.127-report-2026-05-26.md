# Forge Core v0.4.127 Self-Evolution Report

## Scope

This cycle is Forge 0.5 groundwork. It adds the smallest Forge-owned live collaboration baseline for creative artifacts without claiming the full Forge 0.5 creative runtime.

## Changes

- Added durable collaboration state to `CreativeArtifact`: presence, cursors/selections, comments, patch stream events, conflict records, rollback records and audit history.
- Added `forge workflow collaboration-event` for presence/comment/patch/conflict/rollback events with workflow revisions and audit events.
- Added `forge workflow collaboration-status` for inspecting collaboration state on a creative artifact.
- Added MCP tools `forge.creative.collaboration_event` and `forge.creative.collaboration_status`.
- Added workflow status summaries for creative artifact collaboration state.
- Updated the Forge 0.5 milestone boundary so `live_collaboration` is validated as a baseline while richer browser transport and source-grounded research remain before 0.5 promotion.

## Lean Overhead Ledger

- Prompt packet: `forge.self_evolution.prompt.v2`.
- Estimated prompt bytes: about 54,000.
- Estimated prompt tokens: about 13,500.
- Validation commands recorded during implementation: 11.
- New report artifacts: 1.
- Metadata bytes added in this report: about 2,200.

## Validation Notes

- RED: `cargo test collaboration --test forge_cli_contract` failed because `workflow collaboration-event` and MCP collaboration tools were missing.
- GREEN: `cargo test collaboration --test forge_cli_contract` passed after implementation.
- GREEN: `cargo test milestone --test forge_cli_contract` passed after milestone evidence updates.
- Clippy initially rejected the wide collaboration mutation function; the fix was a typed `CreativeCollaborationEventRequest` so CLI and MCP callers pass one structured request into the workflow API.

Full required validation passed for this cycle:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test` with 27 unit tests and 196 CLI contract tests passing.
- `cargo build --release`

CLI smoke passed with:

- `./target/release/forge --store /tmp/forge-smoke-plan-0.4.127.sqlite plan --goal "Create a delivery platform" --output json`
- `./target/release/forge --store /tmp/forge-smoke-skill-0.4.127.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.127`

## Safety

- No Docker, Kubernetes, Knative, Telegram or external user resources were mutated.
- Collaboration events mutate only Forge-owned workflow state and record revisioned audit evidence.
- The 0.5 promotion gate still fails while source-grounded research and richer creative demos are incomplete.

## Next Recommended Cycle

Produce the source-grounded Forge 0.5 research artifact baseline: Penpot, Stitch, v0, Impeccable/AGUI-style protocols, Superpowers, Remotion/Figma capabilities and OBS/media composition lessons, converted into Forge-owned validation gates and workflow templates.
