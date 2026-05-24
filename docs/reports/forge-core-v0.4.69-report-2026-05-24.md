# Forge Core v0.4.69 Self-Evolution Report

Run id: `run_33eb66702df046cd9df3c6b8c7142e10`  
Workflow id: `wf_271d140870c4431d9572099fde4723d5`  
Prompt packet: `forge.self_evolution.prompt.v1`

## Increment

Added a versioned minimum-correct context section receipt to the Context Routing
Engine.

Before this cycle, `forge context` exposed aggregate `budget_plan` and shard-level
routing decisions, but executor adapters had to join those structures themselves
to answer a narrower question: which required context sections define the
minimum-correct handoff floor, which of those sections are missing, and what
repair action is needed.

`forge context` now emits `minimum_correct_set` with schema
`forge.context.minimum_correct_set.v1`. The set records each required section's
included/compressed/missing state, routing decision, repair action,
selected/original byte counts and source/content hashes. The set has its own
checksum and is included in the routing fingerprint, so resumable executor
handoffs and cache keys account for the exact required-section floor.

## Files Changed

- `src/context.rs`
- `src/inspection.rs`
- `tests/forge_cli_contract.rs`
- `Cargo.toml`
- `Cargo.lock`
- `CHANGELOG.md`
- `README.md`
- `docs/technical-definition.md`
- `docs/reports/forge-core-v0.4.69-report-2026-05-24.md`

## Validation

- Red test: `cargo test context_package_exposes_minimum_correct_set_for_required_sections -- --nocapture` failed first because `minimum_correct_set` was absent.
- Focused green test: `cargo test context_package_exposes_minimum_correct_set_for_required_sections -- --nocapture` passed after implementation.
- Focused inspection regression: `cargo test inspect_projects_budget_plan_for_terminal_context_routes -- --nocapture` passed.
- Required validation:
  - `cargo fmt --check`: passed.
  - `cargo clippy --all-targets --all-features -- -D warnings`: passed.
  - `cargo test`: passed with 99 CLI contract tests.
  - `cargo build --release`: passed.
- CLI smoke:
  - `./target/release/forge --store /tmp/forge-plan-smoke-0.4.69.sqlite plan --goal "Create a delivery platform" --output json`: passed.
  - `./target/release/forge --store /tmp/forge-skill-smoke-0.4.69.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.69`: passed.

## Install Notes

- Required global install `cargo install --path . --force` was attempted after validation and was blocked by the sandbox: `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem.
- Workspace-local fallback install succeeded with `CARGO_NET_OFFLINE=true cargo install --path . --force --locked --offline --root /home/arthur/projects/forge-core/.forge/local-install`.
- The fallback binary reports `forge 0.4.69`.

## Publication Notes

- Publication preparation followed the requested GitHub CLI contract.
- `gh auth token` succeeded with output redirected away from chat.
- `git remote get-url origin` reported `https://github.com/cardozoarthur/forge-core.git`.
- `git add CHANGELOG.md Cargo.toml Cargo.lock README.md docs/technical-definition.md docs/reports/forge-core-v0.4.69-report-2026-05-24.md src/context.rs src/inspection.rs tests/forge_cli_contract.rs` was attempted and failed because `.git/index.lock` cannot be created on the current read-only git metadata mount.
- `git push` was attempted and failed because the sandbox cannot resolve `github.com`.
- The validated changes remain in the working tree and were not committed or pushed from this sandbox.

## Safety

- The change is read-only metadata derived from Forge-owned context routing state.
- It does not execute local Python/Node.js code, complete tasks, promote workflows,
  authorize CLIs, run installed CLIs as executors, install Knative or mutate
  Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency
  readiness, validation rules, task leases, persona gates, child-subflow
  validation gates and continuation plans.

## Next Recommended Cycle

Add a `forge list --context-actions` discovery catalog mirroring
`forge list --quality-actions`, so operators can enumerate valid context-action
filters without reading source or memorizing JSON field names.
