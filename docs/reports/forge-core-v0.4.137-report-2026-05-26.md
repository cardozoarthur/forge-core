# Forge Core v0.4.137 Self-Evolution Report

## Cycle

- Prompt packet: `forge.self_evolution.prompt.v2`
- Run: `run_35ef1be524e640b98b117d39b2d37998`
- Workflow: `wf_19f0b1b0a91a4e8698a234932d9fa9e3`
- Executor: `codex`
- Cycle: 13
- Version line: `0.4.x` groundwork for Forge 0.5 runtime readiness

## Increment

Forge now has an explicit stale async-run recovery contract. Heartbeat-backed external executor handoffs can move from stale `running` to `needs_attention` through Forge-owned state instead of relying on tmux, process inspection or ad hoc operator inference.

## User-Visible Surfaces

- `forge request status --run <run-id> --output json` includes `activity.recovery`.
- `forge request list --status stale --output json` lists stale running handoffs.
- `forge request recover-stale --run <run-id> --origin codex --output json` transitions stale `running` requests and workflows to `needs_attention`.
- `forge request list --status needs_attention --output json` lists recovered handoffs requiring resume, cancel or inspect.
- MCP tool `forge.run.recover_stale` exposes the same contract to agents.
- The packaged Forge skill and public skill copy document stale recovery commands.

## Lean Economics

- Prompt bytes estimate: `72000`.
- Estimated prompt tokens: `18000`.
- Validation command count: `10` successful verification commands plus `3` intentional/red or rejected checks.
- Artifact count: `2` human-facing artifacts changed or created (`CHANGELOG.md`, this report).
- Metadata bytes estimate: `5200` for this report plus changelog entry.
- Useful value: removes a manual observability gap for long-running Codex/OpenCode handoffs, preserving workflow/run lineage and avoiding stale runs being mistaken for active work.
- Accepted complexity: one status transition, one recovery recommendation object, one CLI command, one MCP tool and tests.
- Rejected complexity: no process-manager integration, no tmux polling, no automatic executor restart and no external resource mutation.

## Validation Evidence

- RED was observed first:
  - `stale_request_heartbeat_surfaces_recovery_and_transitions_to_needs_attention` failed because `activity.recovery` was absent.
  - `mcp_exposes_stale_request_recovery_tool_for_agent_observability` failed because `forge.run.recover_stale` was absent.
- GREEN targeted validation:
  - `cargo test stale_request_heartbeat_surfaces_recovery_and_transitions_to_needs_attention -- --nocapture` passed.
  - `cargo test mcp_exposes_stale_request_recovery_tool_for_agent_observability -- --nocapture` passed.
- Required validation:
  - `cargo fmt --check` passed.
  - `cargo clippy --all-targets --all-features -- -D warnings` passed.
  - `cargo test` passed: 27 unit tests and 211 CLI/MCP contract tests.
  - `cargo build --release` passed.
- CLI smoke:
  - `./target/release/forge plan --goal "Create a delivery platform" --output json` passed.
  - `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.137-<timestamp>` passed.
- Local install:
  - Default `cargo install --path . --force` was blocked by the sandbox because `/home/arthur/.cargo/.crates.toml` is read-only.
  - Repo-local offline install passed with `CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --offline`.
  - `.forge/local-install/bin/forge --version` returned `forge 0.4.137`; user-visible `forge --version` remains `forge 0.4.136` until the default install path can be updated outside this sandbox.
- GitHub publication:
  - `gh auth token` succeeded with the token value suppressed from output.
  - `git remote get-url origin` returned `https://github.com/cardozoarthur/forge-core.git`.
  - `git commit -m "feat: recover stale async handoffs"` was blocked because `.git/index.lock` cannot be created on the read-only `.git` filesystem.
  - `git push` was attempted and failed because `github.com` DNS resolution is unavailable in this environment.

## Safety

- No Docker, Kubernetes, Knative, Telegram, camera, microphone, screen, mouse, keyboard, peripheral, model download or external user resource was mutated.
- The skill smoke only wrote to a fresh `/tmp` home and inspected installed local tools.
- Stale recovery only mutates Forge-owned SQLite run/workflow state and records an audit event.

## Next Cycle Recommendation

Add a `/status` and interactive dashboard projection for `needs_attention` runs, including suggested next actions: resume, cancel, inspect logs, refresh heartbeat or attach validation evidence.
