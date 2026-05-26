# Forge Core v0.4.135 Self-Evolution Report

## Cycle

- Prompt packet: `forge.self_evolution.prompt.v2`
- Run: `run_35ef1be524e640b98b117d39b2d37998`
- Workflow: `wf_19f0b1b0a91a4e8698a234932d9fa9e3`
- Executor: `codex`
- Version line: `0.4.x` groundwork for Forge 0.5 runtime readiness

## Increment

Forge now has an explicit async-run heartbeat contract for external executors. A Codex/OpenCode-style executor can call `forge request heartbeat` or MCP tool `forge.run.heartbeat` while it is alive. Forge persists the run as `running`, stores heartbeat TTL metadata, marks the workflow as running for registry/inspect visibility, and records a Forge-owned event.

## Why This Matters

The previous cycle made workflow-level `running` visible when `forge self run` owned execution internally. This cycle closes the executor-handoff gap: when an external executor receives a prompt packet and works outside the `forge self run` process, it can still keep Forge request/list/inspect surfaces truthful without relying on tmux or `ps` as the primary observability model.

## User-Visible Surfaces

- `forge request heartbeat --run <run-id> --executor codex --summary "<progress>" --ttl-seconds 300 --origin codex --output json`
- `forge request status --run <run-id> --output json` now includes `activity`.
- `forge request list --status running --output json` filters running requests and includes `activity`.
- `forge inspect <workflow-id> --output json` now includes `run_ids`, `run_statuses`, `active_run_count` and a `runs:` line in the diagram.
- MCP tool `forge.run.heartbeat` exposes the same contract to agents.
- The packaged Forge skill documents the heartbeat step.

## Lean Economics

- Added state fields are optional and only appear when a heartbeat exists.
- No new runtime dependency was added.
- The feature avoids manual process inspection and reduces operator retry/confusion cost during long self-evolution handoffs.
- Complexity stays bounded to request lifecycle projection, MCP exposure and inspection/list/status visibility.

## Validation Evidence

- RED was observed first:
  - `request heartbeat` initially failed with an unrecognized subcommand.
  - MCP manifest initially lacked `forge.run.heartbeat`.
- GREEN targeted validation:
  - `cargo test heartbeat -- --nocapture`
  - Result: 2 heartbeat contract tests passed.
- Required validation:
  - `cargo fmt --check` passed.
  - `cargo clippy --all-targets --all-features -- -D warnings` passed.
  - `cargo test` passed: 27 unit tests and 209 CLI/MCP contract tests.
  - `cargo build --release` passed.
- CLI smoke:
  - `./target/release/forge plan --goal "Create a delivery platform" --output json` passed.
  - `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.135` passed.
- Local install:
  - Default `cargo install --path . --force` was blocked by the sandbox because `/home/arthur/.cargo/.crates.toml` was read-only.
  - Project-local offline install passed with `CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --offline`.
  - `.forge/local-install/bin/forge --version` returned `forge 0.4.135`.
- GitHub publication:
  - `gh auth token` succeeded.
  - `git remote get-url origin` returned `https://github.com/cardozoarthur/forge-core.git`.
  - `git add` was blocked because `.git/index.lock` could not be created on the read-only `.git` filesystem.
  - `git push` was attempted and failed because `github.com` DNS resolution is unavailable in this environment.

## Safety

- No Docker, Kubernetes, Knative or external user infrastructure was mutated.
- No Telegram delivery was performed by this code path.
- Heartbeat summaries should stay operational and secret-free because they are persisted as workflow events.

## Next Cycle Recommendation

Use the heartbeat metadata to add stale-run recovery policy: list stale running requests separately, expose a recommended action in `/status`, and support controlled transition from stale `running` to `needs_attention` without losing run/workflow lineage.
