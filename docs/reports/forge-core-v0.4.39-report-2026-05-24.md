# Forge Core v0.4.39 Report - Inspect Context Next Actions

## Increment

Forge Core now projects an explicit next operational action for every node in
`forge inspect`.

Each `context_route` in `forge inspect --output json` includes
`next_action` with schema `forge.inspect_context_action.v1`. The projection is
derived from the same Context Routing Engine package already used by inspect:

- `start_executor_handoff` when context and dependencies are ready and no
  checkpoint exists;
- `wait_for_dependencies` when dependency tasks still block handoff;
- `increase_context_budget` or `repair_context_and_wait_for_dependencies` when
  required context was omitted;
- `refresh_context_before_resume` for stale or route-unknown checkpoints;
- `resume_from_checkpoint` when the checkpoint route still matches;
- `partial_retry_with_fresh_context` when a checkpoint exists but the current
  inspect route has changed.

Human terminal diagrams also append `next <action>` to each node's context route,
so operators can inspect a DAG and immediately see whether the next move is
handoff, dependency wait, context repair or resumable retry.

## Runtime Impact

- `forge inspect` is now closer to a terminal operations surface: raw handoff and
  resume statuses are translated into deterministic operator actions.
- The projection includes blocking refs, checkpoint context SHA-256, checkpoint
  routing cache key, current routing cache key and a partial-retry flag when
  applicable.
- `forge.context.v16` remains unchanged. This is a read-only inspection
  projection derived from existing Forge-owned context, dependency and checkpoint
  state.

## Safety

The new action contract does not acquire leases, execute tasks, promote
workflows, authorize executors, run local Python/Node.js code, install Knative or
mutate Docker/Kubernetes/Knative resources.

Executor execution is still gated by `forge task handoff`, strict context
readiness and task leases.

## Validation Evidence

- RED: `cargo test inspect_projects_next_context_action_for_handoff_and_resume --test forge_cli_contract` failed first because `forge inspect` diagrams did not include `next start_executor_handoff`.
- GREEN: `cargo test inspect_projects_next_context_action_for_handoff_and_resume --test forge_cli_contract` passed after adding the inspection projection.
- Focused inspect regression suite passed:
  - `cargo test inspect_ --test forge_cli_contract` with 4 inspect tests passing.
- Required validation passed:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test` with 65 CLI contract tests passing
  - `cargo build --release`
- CLI smokes passed with `target/release/forge`:
  - `target/release/forge --version` reported `forge 0.4.39`
  - `target/release/forge --store /tmp/forge-core-v039-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json`
  - `target/release/forge --store /tmp/forge-core-v039-skill-smoke-a.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v039-a`

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

The checkout-local binary reports `forge 0.4.39`.

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

## Next Cycle

Add a registry-level next-action summary to `forge list`, aggregating how many
tasks are ready for handoff, waiting on dependencies, blocked by context budget,
or eligible for checkpoint resume/partial retry across all workflows.
