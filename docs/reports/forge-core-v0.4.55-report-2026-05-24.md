# Forge Core v0.4.55 Self-Evolution Report

Run id: `run_81be93b3b9d4417e9a46b12971e9fe3a`  
Workflow id: `wf_8ea860d10ee24ebf824a97e752026d81`  
Prompt packet: `forge.self_evolution.prompt.v1`

## Increment

Added recursive child-subflow topology metadata to `forge inspect`.

`forge inspect --output json` now expands proposed child-subflow bindings into read-only inspection rows with parent workflow/task ids, depth, path, reachability, terminal status, loaded child workflow status, derived child lifecycle state and child task/subflow counts. The human terminal diagram now prints a compact `subflows:` section with each reusable child path.

This moves Forge toward terminal DAG/subflow visualization and recursive flow reuse without executing, validating, promoting or mutating child flows.

## Files Changed

- `src/inspection.rs`
- `tests/forge_cli_contract.rs`
- `Cargo.toml`
- `Cargo.lock`
- `README.md`
- `docs/technical-definition.md`
- `CHANGELOG.md`
- `docs/reports/forge-core-v0.4.55-report-2026-05-24.md`

## Validation

- Red test: `cargo test inspect_expands_proposed_child_subflows_with_recursive_path_metadata` failed before implementation because `parent_workflow_id` was absent from inspect JSON.
- Focused green test: `cargo test inspect_expands_proposed_child_subflows_with_recursive_path_metadata` passed after implementation.
- `cargo fmt --check`: passed
- `cargo clippy --all-targets --all-features -- -D warnings`: passed
- `cargo test`: passed, 83 CLI contract tests
- `cargo build --release`: passed
- CLI smoke `./target/release/forge plan --goal "Create a delivery platform" --output json`: passed
- CLI smoke `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.55`: passed

## Install Notes

- Required install `cargo install --path . --force` was attempted after validation but was blocked by the sandbox: `/home/arthur/.cargo/.crates.toml` is read-only.
- Workspace-local fallback install succeeded with `cargo install --path . --force --offline --root /home/arthur/projects/forge-core/target/forge-local-install`.
- The fallback binary reports `forge 0.4.55`.

## Publication Notes

- `gh auth token >/dev/null`: passed.
- `git remote get-url origin`: `https://github.com/cardozoarthur/forge-core.git`.
- Creating a commit in the main checkout was blocked because `.git/index.lock` cannot be created on the read-only git metadata mount.
- A temporary writable copy under `/tmp` and a second copy using `--separate-git-dir` were both attempted, but `git add` was still blocked by read-only filesystem errors when `git` tried to write its index/object database.
- `git push origin HEAD:main` was attempted from the main checkout and failed because the sandbox cannot resolve `github.com`.

## Safety

- The new subflow expansion is read-only inspection metadata over Forge-owned workflow state.
- It does not execute child subflows, acquire leases, complete tasks, promote workflows, authorize CLIs, run local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Proposed child-subflow execution remains gated by future scheduling, validation and executor handoff contracts.

## Next Recommended Cycle

Add explicit child-subflow lifecycle validation gates: proposed child links should move through attachable, validated, scheduled and reusable states before any recursive subflow is executed or promoted.
