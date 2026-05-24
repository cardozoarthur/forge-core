# Forge Core v0.4.19 Report - 2026-05-23

## Objective

Advance workflow composition with a small structural increment: persist compatible reusable deterministic code-node candidates as proposed child subflows and expose those links through `forge inspect`.

## Change

`forge plan` now keeps reuse candidates as more than transient response metadata. When the registry finds an attachable compatible reusable local code-node subflow, Forge stores one best candidate per requested task in the task's `child_subflows` field. The saved reference includes:

- candidate workflow id and task id;
- child task title;
- `binding_status = proposed`;
- candidate lifecycle state;
- reuse key;
- context lineage SHA-256;
- validation gate;
- registry reason.

`forge inspect --verbose` now reads those persisted references and reports them in:

- top-level `subflow_count`;
- structured `subflows`;
- each task node's `subflow_refs`;
- terminal DAG text as `subflows <workflow>/<task>:proposed`.

## Safety

This remains a planning and inspection contract. Forge does not execute local Python or Node.js code during planning, does not mark reused child subflows complete, does not auto-promote a reused flow, does not authorize external CLIs and does not mutate Docker, Kubernetes or Knative resources.

Only registry candidates already marked attachable through lifecycle policy (`idle`, `completed` or `scaled_to_zero`) are persisted as proposed child subflows. Later cycles still need explicit validation and execution semantics before a proposed child subflow can advance workflow state.

## Validation

Fresh validation run in this cycle:

- `cargo fmt --check`: passed;
- `cargo clippy --all-targets --all-features -- -D warnings`: passed;
- `cargo test`: passed, 44 CLI contract tests;
- `cargo build --release`: passed;
- CLI smoke `./target/release/forge --store /tmp/forge-core-v0419-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json`: passed;
- CLI smoke `./target/release/forge --store /tmp/forge-core-v0419-skill-smoke.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v0419`: passed;
- CLI smoke `./target/release/forge --store /tmp/forge-core-v0419-plan-smoke.sqlite inspect <workflow-id> --verbose --output json`: passed.

New focused test added before implementation:

- `plan_persists_reuse_candidates_as_proposed_child_subflows_for_inspection`

## Installation

- `cargo install --path . --force`: blocked because `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem in this execution environment.
- `cargo install --path . --force --root .forge/local-install --offline`: passed.
- `.forge/local-install/bin/forge --version`: `forge 0.4.19`.

## Publication Attempt

- `git add ... && git commit -m "feat: persist proposed child subflows"`: blocked because Git could not create `.git/index.lock` on a read-only filesystem.
- `gh auth token`: passed locally; token value was not recorded.
- `git remote get-url origin`: `https://github.com/cardozoarthur/forge-core.git`.
- `git push`: blocked because this environment could not resolve `github.com`.

## Next Recommended Cycle

Add validation and execution semantics for proposed child subflows: require an explicit gate before a proposed link becomes active, refresh child lifecycle during inspection, and route child-subflow context without executing or promoting reused work automatically.
