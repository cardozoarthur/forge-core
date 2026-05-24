# Forge Core v0.4.24 Report - 2026-05-24

## Objective

Improve the Context Routing Engine so executor adapters can distinguish a merely
budget-bounded context packet from one that is definitively ready for handoff.

## Change

`forge context` now emits schema `forge.context.v11` with routing policy
`task_local_revisioned_persona_compressed_executor_policy_subflow_checkpoint_budget_summary_required_v11`.

Each executor context profile now declares required sections. Context packets expose:

- `context_ready`;
- `required_sections`;
- `missing_required_sections`;
- `executor_profile.required_sections`;
- per-shard `required` and `missing_required`;
- `routing_summary.required_shards`;
- `routing_summary.required_omitted_shards`.

`forge context --strict` prints the same replayable JSON package, then exits non-zero
when required sections are missing. Non-strict mode remains useful for inspection,
debugging and cost analysis without changing the exit code.

## Safety

Strict readiness is validation metadata over Forge-owned context routing. It does not
complete tasks, promote workflows, authorize CLIs, execute local Python/Node.js code,
select a provider, mutate Docker/Kubernetes/Knative resources or alter external
infrastructure.

The strict path does not hide evidence: even blocked handoffs return the complete
context packet with missing section names and shard-level routing reasons.

## TDD Evidence

- RED: `cargo test strict_context_blocks_executor_when_required_sections_are_missing`
  failed because `forge context` did not yet accept `--strict` and returned no JSON
  readiness evidence.
- GREEN: the focused test passed after schema v11, required shard metadata and strict
  exit-code handling were implemented.

## Validation

- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test`: passed, 50 CLI contract tests.
- `cargo build --release`: passed.
- CLI smoke `./target/release/forge --store /tmp/forge-core-v0424-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json`: passed.
- CLI smoke `./target/release/forge --store /tmp/forge-core-v0424-skill-smoke-2.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v0424-2`: passed.

## Installation

- `cargo install --path . --force`: blocked because `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem in this execution environment.
- `cargo install --path . --force --root .forge/local-install --offline`: passed.
- `.forge/local-install/bin/forge --version`: `forge 0.4.24`.
- The default `forge` on PATH still resolves to `/home/arthur/.cargo/bin/forge` and reports `forge 0.4.23` until the global cargo directory is writable.

## Next Recommended Cycle

Expose strict context readiness in `forge request status` and `forge inspect` so
operators can see which running tasks are blocked by context budget before an executor
attempts a handoff.
