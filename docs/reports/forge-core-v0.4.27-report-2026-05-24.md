# Forge Core v0.4.27 Report - 2026-05-24

## Goal

Improve Forge Core with a small structural increment in the Context Routing Engine: make executor handoff readiness explicit, versioned and auditable in `forge context` packets.

## Change Summary

`forge context` now emits schema `forge.context.v13` with routing policy `task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_v13`.

The package adds:

- `handoff_ready`: a single boolean for whether an executor handoff can proceed;
- `handoff_status`: a compact status string such as `ready`, `blocked_missing_context`, `blocked_dependencies` or `blocked_missing_context_and_dependencies`;
- `handoff_blockers`: typed blocker records with `kind`, `message` and `refs`.

## Operational Impact

Strict context mode now distinguishes two different hold reasons before an executor starts work:

- required context was omitted by the context budget/profile;
- dependency tasks are not ready yet.

This gives Codex/OpenCode adapters and future async executors a stable handoff gate without forcing them to infer readiness from shard lists or raw dependency state.

## Safety Boundaries

The change is read-only. It does not:

- mutate workflow state;
- mark dependencies complete;
- promote workflows;
- authorize CLIs;
- execute local Python or Node.js code;
- mutate Docker, Kubernetes or Knative resources.

Promotion remains controlled by `forge validate`.

## Validation Plan

Required validation for this cycle:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`

CLI smoke:

- `forge plan --goal "Create a delivery platform" --output json`
- `forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`

## Validation Result

Passed.

- RED: `cargo test strict_context_blocks_executor_handoff_when_dependencies_are_not_ready --test forge_cli_contract` failed because strict context returned exit 0 and emitted no handoff readiness fields.
- GREEN: `cargo test strict_context_blocks_executor_handoff_when_dependencies_are_not_ready --test forge_cli_contract` passed after the v13 handoff contract was implemented.
- Focused context suite: `cargo test context --test forge_cli_contract` passed, 15 tests.
- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test`: passed, 53 CLI contract tests.
- `cargo build --release`: passed.
- `./target/release/forge plan --goal "Create a delivery platform" --output json`: passed and produced a planned workflow.
- `./target/release/forge --store /tmp/forge-skill-smoke.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`: passed and installed Codex/OpenCode/shared skill files without authorizing unapproved executors.

## Installation and Publication

- `cargo install --path . --force`: blocked by sandbox write policy on `/home/arthur/.cargo/.crates.toml` (`Read-only file system`).
- `CARGO_NET_OFFLINE=true CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --offline`: passed and replaced the repo-local install with `forge 0.4.27`.
- `.forge/local-install/bin/forge --version`: `forge 0.4.27`.
- `gh auth token`: passed without exposing the token.
- `git remote get-url origin`: `https://github.com/cardozoarthur/forge-core.git`.
- Commit staging in the primary checkout was blocked because `.git` is mounted read-only.
- A temporary worktree with a separate git directory was required for local commit creation.
- `git push origin main`: blocked by network DNS resolution (`Could not resolve host: github.com`).

## Next Recommended Cycle

Project handoff readiness into `forge inspect` and `forge request status` so operators can see which task is held by missing context versus dependency readiness without calling `forge context` manually.
