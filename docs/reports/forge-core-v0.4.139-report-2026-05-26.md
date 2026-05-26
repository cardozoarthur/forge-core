# Forge Core v0.4.139 Self-Evolution Report

## Cycle

- Prompt packet: `forge.self_evolution.prompt.v2`
- Run: `run_35ef1be524e640b98b117d39b2d37998`
- Workflow: `wf_19f0b1b0a91a4e8698a234932d9fa9e3`
- Executor: `codex`
- Cycle: `15`
- Version line: `0.4.x` groundwork for Forge 0.5 runtime readiness

## Increment

Forge now surfaces executor handoffs that need attention directly in the interactive home dashboard. Recovered `needs_attention` runs and stale heartbeat-backed runs are counted as first-class dashboard state, with safe follow-up commands for listing, inspecting, resuming, cancelling or recovering the affected runs.

This closes the immediate gap after v0.4.137: operators no longer need to remember a separate `request list --status needs_attention` query after stale recovery. The default interactive surface now points them to the next Forge-owned action.

## User-Visible Surfaces

- `forge interactive home --output json` includes:
  - `dashboard.runs_needing_attention`
  - `dashboard.attention_actions`
- Human `forge interactive home` renders:
  - `Runs needing attention: <n>`
  - `Attention actions: <commands>`
- Suggested commands are read-only or explicit operator actions:
  - `forge request list --status needs_attention`
  - `forge request list --status stale`
  - `forge request status --run <run-id>`
  - `forge request resume --run <run-id>`
  - `forge request cancel --run <run-id>`
  - `forge request recover-stale --run <run-id>`

## Lean Economics

- Prompt bytes estimate: `73000`.
- Estimated prompt tokens: `18250`.
- Validation command count: `7` successful checks plus `1` intentional red test.
- Artifact count: `3` human-facing artifacts changed or created (`CHANGELOG.md`, `README.md`, this report).
- Metadata bytes estimate: `6100` for changelog, report and version metadata.
- Useful value: reduces operator search cost and stale-run ambiguity in long-running Forge-owned executor handoffs.
- Accepted complexity: two dashboard fields, one rendering line group and one focused contract test.
- Rejected complexity: no process polling, no tmux integration, no automatic recovery, no automatic resume/cancel and no external resource mutation.

## Validation Evidence

- RED was observed first:
  - `interactive_home_surfaces_needs_attention_runs_with_recovery_actions` failed because `dashboard.runs_needing_attention` was absent.
- GREEN targeted validation:
  - `cargo test interactive_home_surfaces_needs_attention_runs_with_recovery_actions --test forge_cli_contract -- --nocapture` passed.
  - `cargo test interactive_home_renders_anvil_forge_and_operational_dashboard_sections --test forge_cli_contract -- --nocapture` passed.
- Required validation:
  - `cargo fmt --check` passed after mechanical `cargo fmt`.
  - `cargo clippy --all-targets --all-features -- -D warnings` passed.
  - `cargo test` passed: `27` unit tests and `213` CLI/MCP contract tests.
  - `cargo build --release` passed.
- CLI smoke:
  - `./target/release/forge plan --goal "Create a delivery platform" --output json` passed.
  - `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.139-<timestamp>` passed.
- Local install:
  - Default `cargo install --path . --force` was blocked by the sandbox because `/home/arthur/.cargo/.crates.toml` is read-only.
  - Repo-local offline install passed with `CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --offline`.
  - `.forge/local-install/bin/forge --version` returned `forge 0.4.139`.
  - User-visible `forge --version` remains `forge 0.4.137` until the default install path can be updated outside this sandbox.
- GitHub contract preflight:
  - `gh auth token` succeeded with token output suppressed and discarded.
  - `git remote get-url origin` returned `https://github.com/cardozoarthur/forge-core.git`.
  - `git add ...` was blocked because `/home/arthur/projects/forge-core/.git/index.lock` cannot be created on the read-only `.git` filesystem.
  - `git push` was attempted and failed because `github.com` DNS resolution is unavailable in this environment.

## Safety

- No Docker, Kubernetes, Knative, Telegram, camera, microphone, screen, mouse, keyboard, peripheral, model download or external user resource was mutated.
- Dashboard attention actions are suggestions only. Forge does not resume, cancel, recover or execute runs without an explicit command.
- The skill smoke wrote only to a fresh `/tmp` home and did not authorize unavailable CLIs or mutate external runtimes.

## Next Cycle Recommendation

Promote the same attention projection into `/status` and `forge request status` summary text: include a compact next-action router for `needs_attention`, stale, accepted and active runs so interactive chat, slash commands and scripted JSON all converge on the same recovery model.
