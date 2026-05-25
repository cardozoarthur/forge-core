# Forge Core v0.4.98 Report - Human Interaction Node Groundwork

Prompt packet: `forge.self_evolution.prompt.v2`
Run id: `run_bfba8dcc4747450da9067f8cdc713b58`
Workflow id: `wf_047a8146d7fb42a7800cbfdad1b59f72`
Workflow revision: `13`
Executor: `codex`

## Increment

Forge now has a Forge-owned human interaction node contract as `0.5 groundwork`.

- Added `HumanInteractionSpec`, choice options, form schemas, form fields and durable decision records to workflow task state.
- Added `forge interaction create-choice`, `create-form`, `answer`, `expire` and `list`.
- Choice gates can represent single choice, multi-choice, ranked choice, approve/reject/refine/combine, yes/no and risk acknowledgement prompts.
- Form gates validate required fields before the workflow can resume.
- Human answers persist decision id, timestamp, origin, rationale, selected options, field values, affected task and affected goal metadata.
- Timeouts move a pending gate to `timed_out` and keep the workflow blocked.
- `forge run --simulate` now returns `blocked_on_human_interaction` instead of completing a graph with an unresolved required human gate.
- `forge status`, `forge list`, `forge inspect` and the interactive dashboard surface pending/timed-out human interaction counts.
- Added `docs/forge-0.5-milestone.md` so 0.4.x reports distinguish groundwork from 0.5 completion.

## TDD Evidence

- RED: `cargo test human_interaction_ --test forge_cli_contract` failed because `interaction` was an unknown subcommand.
- GREEN: the focused human interaction tests passed after adding the graph model, CLI commands, run gating and visibility projections.
- Regression: `cargo clippy --all-targets --all-features -- -D warnings` initially failed on `too_many_arguments`; the root cause was wide internal constructor APIs, fixed with a request struct.

## Validation

Required validation passed:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test` (`160` tests passed: 6 unit tests plus 154 CLI contract tests)
- `cargo build --release`

Required CLI smoke passed:

- `./target/release/forge --store /tmp/forge-core-v0498-plan-smoke-20260525.sqlite plan --goal "Create a delivery platform" --output json`
- `./target/release/forge --store /tmp/forge-core-v0498-skill-smoke-20260525-b.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v0498-20260525-b`
- `./target/release/forge --store /tmp/forge-core-v0498-interaction-smoke.sqlite interaction list --output json`

## Install Notes

- `cargo install --path . --force` was attempted after validation and was blocked by the sandbox because `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem.
- `cargo install --path . --force --root /tmp/forge-install-v0.4.98` was attempted and failed because network access to `index.crates.io` is unavailable.
- `cargo install --path . --force --root /tmp/forge-install-v0.4.98 --offline` passed.
- `/tmp/forge-install-v0.4.98/bin/forge --version` returned `forge 0.4.98`.

## Safety

- Human interaction state is persisted only in Forge-owned SQLite workflow JSON and event history.
- A pending or timed-out required human gate cannot be bypassed by `run --simulate`.
- Timeout handling keeps work blocked for explicit follow-up instead of guessing a default.
- This is `0.5 groundwork`; the web collaboration surface, MCP human approval bridge and full creative runtime are still planned.
- No Docker, Kubernetes, Knative or external user resources were mutated.

## Lean Overhead Ledger

- prompt bytes: approximately 45,000
- estimated prompt tokens: approximately 11,250
- validation/smoke/install command count: 12
- required validation command count: 4
- artifact count: 2 tracked documents (`docs/forge-0.5-milestone.md` and this report)
- metadata bytes: approximately 4,000 report/milestone bytes

## Next Cycle

Add the agent-facing bridge for human interactions: expose create/list/answer/expire through MCP, add repeated-answer default suggestions that require explicit approval, and produce a demo transcript of a workflow pausing for a form, receiving a decision, resuming and showing the decision in inspect output.
