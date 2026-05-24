# Forge Core v0.4.40 Report - Registry Context Actions

## Increment

Forge Core now aggregates Context Routing Engine next actions in `forge list`.

Each workflow row and the filtered registry summary include `context_actions`
with schema `forge.registry_context_action.v1`. The summary counts:

- `start_executor_handoff`;
- `wait_for_dependencies`;
- `increase_context_budget`;
- `repair_context_and_wait_for_dependencies`;
- `refresh_context_before_resume`;
- `resume_from_checkpoint`;
- `partial_retry_with_fresh_context`;
- `ready_for_handoff`;
- `blocked_tasks`;
- `partial_retry_recommended`.

The next-action decision is now shared from `src/context.rs` as
`ContextNextAction`. `forge inspect` still emits the same
`forge.inspect_context_action.v1` node payload, while `forge list` emits only
the aggregate action counts needed for fleet-level triage.

## Runtime Impact

- Operators can use `forge list --output json` to see whether workflows are
  mainly ready for executor handoff, waiting on dependencies, blocked by context
  budget, or eligible for checkpoint resume/partial retry.
- Lifecycle filters keep the aggregate local to the selected view, so
  `--lifecycle running` and `--lifecycle non-running` report action pressure
  for only those workflows.
- The registry does not acquire leases or execute adapters. It derives action
  counts from Forge-owned workflow, task, checkpoint and context routing state.

## Safety

This change is read-only registry metadata. It does not complete tasks, promote
workflows, authorize CLIs, execute local Python/Node.js code, install Knative or
mutate Docker/Kubernetes/Knative resources.

Executor execution remains gated by `forge task handoff`, strict context
readiness and task leases.

## Validation Evidence

- RED: `cargo test list_aggregates_context_next_actions_for_registry_rows --test forge_cli_contract` failed first because `summary.context_actions.schema_version` was absent.
- GREEN: `cargo test list_aggregates_context_next_actions_for_registry_rows --test forge_cli_contract` passed after adding the shared next-action projection and registry aggregation.
- Focused regression suites passed:
  - `cargo test list_ --test forge_cli_contract` with 6 list tests passing.
  - `cargo test inspect_ --test forge_cli_contract` with 4 inspect tests passing.
- Required validation passed:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test` with 66 CLI contract tests passing
  - `cargo build --release`
- CLI smokes passed with `target/release/forge`:
  - `target/release/forge --version` reported `forge 0.4.40`
  - `target/release/forge --store /tmp/forge-core-v040-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json`
  - `target/release/forge --store /tmp/forge-core-v040-skill-smoke.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v040`

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

The checkout-local binary reports `forge 0.4.40`.

## Publication Note

The GitHub CLI publication contract was attempted after validation:

- `gh auth token >/dev/null` succeeded.
- `git remote get-url origin` returned
  `https://github.com/cardozoarthur/forge-core.git`.
- `git diff --check` succeeded.
- `git add ...` failed because Git could not create `.git/index.lock`:
  `Sistema de ficheiros só de leitura`.
- `.git` is mounted read-only in this sandbox, while the worktree is writable.
- `git push` was attempted and failed with `Could not resolve host:
  github.com` under restricted network access.

## Next Cycle

Add a compact terminal projection for registry action pressure in human
`forge list` output, so operators can see the dominant next action without
opening the JSON payload.
