# Forge Core v0.4.97 Report - Interactive CLI Contract

Prompt packet: `forge.self_evolution.prompt.v2`
Run id: `run_bfba8dcc4747450da9067f8cdc713b58`
Workflow id: `wf_047a8146d7fb42a7800cbfdad1b59f72`
Workflow revision: `12`
Executor: `codex`

## Increment

Forge now has a first native interactive CLI contract.

- Added `src/interactive.rs` with the Forge-owned home dashboard, slash-command catalog, conversational router and retention decision model.
- Added `forge interactive home`, `forge interactive slash-commands` and `forge interactive route --input <text>`.
- In a TTY, running `forge` with no subcommand renders the interactive home with a lightweight anvil mark, `forge`, dashboard counts and quick slash actions.
- Non-TTY no-argument use remains script-safe and does not open the dashboard or create a store.
- Conversational routing now classifies input as direct answer, slash command or new async workflow/run.
- Complex requests return `workflow_id`, `run_id`, routing explanation and a retention decision immediately.
- Retention output requires human approval before deleting workflows that mention artifacts or external side effects.

## TDD Evidence

- RED: `cargo test interactive_ --test forge_cli_contract` failed because `interactive` was an unknown subcommand.
- GREEN: the focused interactive tests passed after adding `src/interactive.rs` and wiring it through `src/main.rs`.
- Additional no-argument contract tests cover non-TTY safety and pseudo-terminal home rendering.

## Validation

Required validation passed:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test` (`157` tests passed: 6 unit tests plus 151 CLI contract tests)
- `cargo build --release`

Required CLI smoke passed:

- `target/release/forge --store /tmp/forge-core-v0497-plan-smoke-20260525.sqlite plan --goal "Create a delivery platform" --output json`
- `target/release/forge --store /tmp/forge-core-v0497-skill-smoke-20260525.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v0497-20260525`

Interactive smoke passed:

- `target/release/forge --store /tmp/forge-core-v0497-interactive-smoke.sqlite interactive home`
- `target/release/forge --store /tmp/forge-core-v0497-interactive-smoke.sqlite interactive route --input "What is the current Forge status?" --origin codex --output json`

## Install Notes

- `cargo install --path . --force` was attempted after validation and was blocked by the sandbox because `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem.
- `cargo install --path . --force --root /tmp/forge-install-v0.4.97 --offline` passed.
- `/tmp/forge-install-v0.4.97/bin/forge --version` returned `forge 0.4.97`.

## Publish Notes

- `gh auth token` succeeded with output redirected away from chat.
- `git remote get-url origin` returned `https://github.com/cardozoarthur/forge-core.git`.
- `git add ... && git commit -m "feat: add forge interactive routing contract"` was attempted and failed because `.git/index.lock` could not be created on the current read-only git metadata mount.
- `git push` was attempted and failed because `github.com` could not be resolved from the sandbox.

## Lean Overhead Ledger

- prompt bytes: approximately 41,000
- estimated prompt tokens: approximately 10,250
- validation/smoke/install command count: 12
- required validation command count: 4
- artifact count: 1 tracked report
- metadata bytes: approximately 2,500 report metadata bytes

## Safety

- The router does not create workflow state for simple low-risk status/help questions.
- Workflow creation flows through existing Forge async request APIs, keeping Forge as the source of truth.
- Slash commands expose equivalent script commands, mutation flags and risk levels.
- Retention decisions are advisory state; no workflow deletion is executed automatically.
- No Docker, Kubernetes, Knative or external user resources were mutated.

## Next Cycle

Implement durable human interaction nodes for the TUI/web/agent bridge: choice prompts, form schemas, pending-decision visibility in status/list/inspect and pause/resume behavior that cannot be bypassed by workflow progression.
