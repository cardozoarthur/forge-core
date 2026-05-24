# Forge Core v0.4.71 Self-Evolution Report

Run id: `run_8d6d2c4258604dfd80352ca85573b941`  
Workflow id: `wf_a9495845b917468686ef820053e68168`  
Prompt packet: `forge.self_evolution.prompt.v1`

## Increment

Added per-shard context selection-cost audit metadata.

Before this cycle, `forge context` exposed whether a shard was included,
compressed, omitted by budget or filtered by profile, plus byte counts and a
budget ledger. Operators and executor adapters could infer cost behavior from
those fields, but replay manifests did not carry a direct per-shard audit of the
minimum routable size, selected-cost ratio or bytes saved by compression and
omission.

`forge context` now emits `forge.context.v30` packets with
`minimum_routable_bytes`, `selection_saved_bytes` and `selection_cost_bps` on
every shard. Replay manifest shard refs carry the same values, and routing
fingerprints include a `shard_selection_audit` component so route cache keys bind
the exact selection-cost ledger. This keeps context routing deterministic while
making compression and omission savings auditable without rehydrating full shard
content.

## Files Changed

- `src/context.rs`
- `tests/forge_cli_contract.rs`
- `Cargo.toml`
- `Cargo.lock`
- `CHANGELOG.md`
- `README.md`
- `docs/technical-definition.md`
- `docs/reports/forge-core-v0.4.71-report-2026-05-24.md`

## Validation

- Red test: `cargo test context_shards_expose_selection_cost_audit_for_compression_and_omission -- --nocapture` failed first because the context packet was still `forge.context.v29` and the shard audit fields were absent.
- Focused green test: `cargo test context_shards_expose_selection_cost_audit_for_compression_and_omission -- --nocapture` passed after implementation.
- Adjacent regression checks:
  - `cargo test context_shards_include_remaining_budget_ledger_for_replayable_selection -- --nocapture`: passed.
  - `cargo test context_package_exposes_replay_manifest_for_resumable_executor_context -- --nocapture`: passed.
- Required validation:
  - `cargo fmt --check`: passed.
  - `cargo clippy --all-targets --all-features -- -D warnings`: passed.
  - `cargo test`: passed with 101 CLI contract tests.
  - `cargo build --release`: passed.
- CLI smoke:
  - `./target/release/forge --store /tmp/forge-plan-smoke-0.4.71.sqlite plan --goal "Create a delivery platform" --output json`: passed.
  - `./target/release/forge --store /tmp/forge-skill-smoke-0.4.71.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.71`: passed.
  - `./target/release/forge --version`: reported `forge 0.4.71`.

## Install Notes

- Required global install `cargo install --path . --force` was attempted after validation and was blocked by the sandbox: `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem.
- Workspace-local fallback install succeeded with `cargo install --path . --force --locked --offline --root /home/arthur/projects/forge-core/.forge/local-install`.
- The fallback binary reports `forge 0.4.71`.

## Publication Notes

- `gh auth token` succeeded with output redirected away from chat and the temporary token check file was truncated.
- `git remote get-url origin` reported `https://github.com/cardozoarthur/forge-core.git`.
- `git add CHANGELOG.md Cargo.toml Cargo.lock README.md docs/technical-definition.md docs/reports/forge-core-v0.4.71-report-2026-05-24.md src/context.rs tests/forge_cli_contract.rs` was attempted after validation and failed because `.git/index.lock` cannot be created on the current read-only git metadata mount.
- `git push` was attempted and failed because the sandbox cannot resolve `github.com`.
- The validated changes remain in the working tree and were not committed or pushed from this sandbox.

## Safety

- The change is read-only metadata derived from deterministic context shard routing.
- It does not execute local Python/Node.js code, complete tasks, promote workflows,
  authorize CLIs, run installed CLIs as executors, install Knative or mutate
  Docker/Kubernetes/Knative resources.
- Executor handoff remains controlled by strict context readiness, dependency
  readiness, validation rules, task leases, persona gates, child-subflow validation
  gates and continuation plans.

## Next Recommended Cycle

Add per-task context action refs to `forge list --output json`, so an operator can
filter by handoff, dependency wait, context repair or partial retry and immediately
see the exact task ids, next actions and blocker refs without opening each workflow
with `forge inspect`.
