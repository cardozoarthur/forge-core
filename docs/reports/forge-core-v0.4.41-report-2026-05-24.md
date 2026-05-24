# Forge Core v0.4.41 Report - Full Execution Policy Handoff

## Increment

`forge task handoff` now emits `forge.executor_handoff.v5`.

The v5 packet preserves the compact `execution_policy_mode` field and adds the
full `execution_policy` contract at the top level of the executor adapter
envelope. Deterministic local code nodes now expose:

- `mode`;
- `ai_allowed`;
- `deterministic`;
- `reuse_hint`;
- `selection_reason`;
- `validation_gate`;
- `code_runtime.language`;
- `code_runtime.entrypoint`;
- `code_runtime.sandbox`.

## Runtime Impact

Executor adapters can now decide whether a task is a no-AI deterministic node,
which local runtime should execute it, and which validation gate applies without
parsing the nested context package.

This is a contract-only increment. It does not execute local Python or Node.js
code, and it does not bypass context readiness, dependency readiness or task
lease checks.

## Safety

The new field is read-only metadata derived from Forge-owned workflow/task
state. It does not complete tasks, promote workflows, authorize CLIs, install
Knative or mutate Docker/Kubernetes/Knative resources.

Execution remains gated by `forge task handoff`, strict context routing,
dependency readiness, task leases and the task validation gate.

## Validation Evidence

- RED: `cargo test task_handoff_packet_carries_full_execution_policy_for_deterministic_code_nodes --test forge_cli_contract` failed first because the handoff packet still reported `forge.executor_handoff.v4`.
- GREEN: the same focused test passed after adding the full `execution_policy` to the packet and bumping the schema to `forge.executor_handoff.v5`.
- Focused regression passed: `cargo test task_handoff_ --test forge_cli_contract` with 5 handoff tests passing.
- Required validation passed:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test` with 67 CLI contract tests passing
  - `cargo build --release`
- CLI smokes passed with `target/release/forge`:
  - `target/release/forge --version` reported `forge 0.4.41`
  - `target/release/forge --store /tmp/forge-core-v041-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json`
  - `target/release/forge --store /tmp/forge-core-v041-skill-smoke.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v041`

## Installation Note

Global installation was attempted after validation with:

```bash
cargo install --path . --force
```

The sandbox rejected writes to `/home/arthur/.cargo/.crates.toml` with
`Read-only file system (os error 30)`.

Checkout-local installation succeeded offline with:

```bash
cargo install --path . --force --root .forge/local-install --offline
.forge/local-install/bin/forge --version
```

The checkout-local binary reports `forge 0.4.41`.

## Publication Note

The GitHub CLI publication contract was attempted after validation:

- `gh auth token >/dev/null` succeeded.
- `git remote get-url origin` returned
  `https://github.com/cardozoarthur/forge-core.git`.
- `git diff --check` succeeded.
- `git add ...` failed because Git could not create `.git/index.lock`:
  `Sistema de ficheiros só de leitura`.
- `git push` was attempted and failed with `Could not resolve host:
  github.com` under restricted network access.

The worktree changes and this report are present locally, but the sandbox did
not allow creating the commit or publishing it.

## Next Cycle

Add an executor-policy projection to `forge inspect --output json` and the
terminal DAG diagram so operators can see deterministic local runtime decisions
before requesting a task handoff.
