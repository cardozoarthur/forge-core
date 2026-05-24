# Forge Core v0.4.60 Self-Evolution Report

Run id: `run_71e9a3f5e65c4b56a58037a0f91d0c56`  
Workflow id: `wf_b30c9c39921143dda0dad7ca24bec163`  
Prompt packet: `forge.self_evolution.prompt.v1`

## Increment

Added focused workflow inspection with `forge inspect <workflow-id> --task <task-id>`.

Forge already rendered full terminal DAGs with context routing, persona, execution
policy, next-action, handoff and recursive child-subflow annotations. This increment
adds a bounded operator view for one selected task without changing the default full
inspection behavior.

Focused inspection returns a `focus` block, preserves `workflow_task_count`, routes
the same context package for the selected node and limits `nodes`, `handoff_summary`
and the terminal diagram to that task. This moves `forge inspect` closer to a
terminal-native DAG/subflow tool that can support long-running cognition without
forcing unrelated nodes into every operator or adapter view.

## Files Changed

- `src/inspection.rs`
- `src/main.rs`
- `tests/forge_cli_contract.rs`
- `Cargo.toml`
- `Cargo.lock`
- `README.md`
- `docs/technical-definition.md`
- `CHANGELOG.md`
- `docs/reports/forge-core-v0.4.60-report-2026-05-24.md`

## Validation

- Red test: `cargo test inspect_can_focus_terminal_view_on_one_task --test forge_cli_contract` failed first because `forge inspect` rejected `--task`.
- Focused green test: `cargo test inspect_can_focus_terminal_view_on_one_task --test forge_cli_contract` passed after implementation.
- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test`: passed, including 90 CLI contract tests.
- `cargo build --release`: passed.
- CLI smoke `./target/release/forge --store /tmp/forge-plan-smoke-0.4.60.sqlite plan --goal "Create a delivery platform" --output json`: passed.
- CLI smoke `./target/release/forge --store /tmp/forge-skill-smoke-0.4.60.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.60`: passed.

## Install Notes

- Required global install `cargo install --path . --force` was attempted after validation but was blocked by the sandbox: `/home/arthur/.cargo/.crates.toml` is read-only.
- Workspace-local fallback install succeeded with `CARGO_NET_OFFLINE=true cargo install --path . --force --locked --offline --root /home/arthur/projects/forge-core/.forge/local-install`.
- The fallback binary reports `forge 0.4.60`; the PATH binary at `/home/arthur/.cargo/bin/forge` still reports `forge 0.4.59`.

## Publication Notes

- `gh auth token`: passed.
- `git remote get-url origin`: `https://github.com/cardozoarthur/forge-core.git`.
- `git add ...`: blocked because `.git/index.lock` cannot be created on the read-only git metadata mount.
- `git push`: attempted and failed because the sandbox cannot resolve `github.com`.
- The validated changes remain in the working tree and were not committed or pushed from this sandbox.

## Safety

- Focused inspection is read-only over persisted Forge workflow state.
- It does not complete tasks, promote workflows, acquire leases, authorize CLIs,
  execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/
  Knative resources.
- Full workflow inspection remains the default unless a task focus is explicitly
  requested.

## Next Recommended Cycle

Add a focused subflow inspection selector that can follow a child-subflow path from
the parent task into the child workflow, then surface a bounded continuation packet
for resumable executor work on that path.
