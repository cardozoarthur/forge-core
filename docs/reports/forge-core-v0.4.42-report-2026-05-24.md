# Forge Core v0.4.42 Report - Inspect Execution Policy Projection

## Increment

`forge inspect --output json` now emits a versioned `execution_policy`
projection for every DAG node.

The projection uses schema `forge.inspect_execution_policy.v1` and exposes:

- `mode`;
- `ai_allowed`;
- `deterministic`;
- `reuse_hint`;
- `selection_reason`;
- `validation_gate`;
- `code_runtime_language`;
- `code_runtime_entrypoint`;
- `code_runtime_sandbox`.

Human terminal diagrams now append compact policy metadata, for example:

```text
policy local_code_node no_ai deterministic python reuse_compatible_code_node
```

## Runtime Impact

Operators can inspect deterministic local runtime choices before requesting a
handoff packet. This moves no-AI local Python/Node.js routing decisions closer
to workflow inspection and fleet triage, while keeping `forge task handoff` as
the bounded executor adapter gate.

This is a read-only inspection increment. It does not execute local code and
does not change context readiness, dependency readiness, leases or validation
promotion semantics.

## Safety

The new projection is derived from Forge-owned workflow/task state. It does not
complete tasks, promote workflows, authorize CLIs, install Knative or mutate
Docker/Kubernetes/Knative resources.

Actual execution remains gated by strict context routing, dependency readiness,
task leases and validation rules.

## Validation Evidence

- RED: `cargo test inspect_projects_execution_policy_for_deterministic_code_nodes --test forge_cli_contract` failed first because the inspection diagram did not include the compact execution policy marker.
- GREEN: the same focused test passed after adding `nodes[].execution_policy` and the diagram policy suffix.
- Required validation passed:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test` with 68 CLI contract tests passing
  - `cargo build --release`
- CLI smokes passed with `target/release/forge`:
  - `target/release/forge --version` reported `forge 0.4.42`
  - `target/release/forge --store /tmp/forge-core-v042-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json`
  - `target/release/forge --store /tmp/forge-core-v042-skill-smoke.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v042`

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

The checkout-local binary reports `forge 0.4.42`.

## Publication Note

The GitHub CLI publication contract was attempted after validation:

- `gh auth token >/dev/null` succeeded.
- `git remote get-url origin` returned
  `https://github.com/cardozoarthur/forge-core.git`.
- `git diff --check` succeeded.
- `git add ... && git commit -m "Add inspect execution policy projection"`
  failed because Git could not create `.git/index.lock`:
  `Sistema de ficheiros só de leitura`.
- `git push` was attempted and failed with `Could not resolve host:
  github.com` under restricted network access.

The worktree changes and this report are present locally, but the sandbox did
not allow creating the commit, and restricted network access prevented
publication.

## Next Cycle

Add the same execution-policy projection to `forge list --output json` as an
aggregate count of deterministic local code nodes by runtime and reuse hint, so
operators can spot no-AI reusable work without opening every workflow.
