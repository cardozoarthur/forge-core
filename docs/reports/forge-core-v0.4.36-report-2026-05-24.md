# Forge Core v0.4.36 Report - Context Budget Ledger

## Increment

Forge Core now emits `forge.context.v16` context packets with a replayable per-shard budget ledger. Each shard carries `remaining_budget_before` and `remaining_budget_after`, and the routing fingerprint includes a `budget_ledger` component.

## Runtime Impact

- Executor adapters can audit why each shard was included, compressed, profile-omitted or budget-omitted without reconstructing selector state from final content bytes.
- Context cache keys now account for the per-shard budget cursor, so changes in routing budget decisions invalidate bounded context reuse.
- The change strengthens the Context Routing Engine without changing Forge's authority model or executing any external code.

## Safety

This is read-only routing metadata derived from Forge-owned workflow/task state. It does not complete tasks, promote workflows, authorize executors, execute local Python/Node.js code, install Knative, or mutate Docker/Kubernetes/Knative resources.

## Validation Evidence

- RED: `cargo test context_shards_include_remaining_budget_ledger_for_replayable_selection` first failed because `forge context` still emitted `forge.context.v15`.
- GREEN: the same focused test passed after adding the v16 budget ledger fields.
- Required validation passed:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test` with 62 CLI contract tests passing
  - `cargo build --release`
- CLI smokes passed with `PATH="$PWD/target/release:$PATH"`:
  - `forge --version` reported `forge 0.4.36`
  - `forge plan --goal "Create a delivery platform" --output json`
  - `forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`

## Install Note

`cargo install --path . --force` was attempted after validation, but the current Codex sandbox rejected writes to `/home/arthur/.cargo/.crates.toml` with `Read-only file system (os error 30)`. The existing global `forge` remains `0.4.35`; the validated release binary is `target/release/forge 0.4.36`.

## Publication Note

The GitHub publication contract was attempted after validation:

- `gh auth token >/dev/null` succeeded.
- `git remote get-url origin` returned `https://github.com/cardozoarthur/forge-core.git`.
- `git add ... && git commit -m "Add context budget ledger"` was blocked because the sandbox rejected `.git/index.lock` creation with `Sistema de ficheiros só de leitura`.
- `git push` was attempted and failed with `Could not resolve host: github.com` under restricted network access.

The validated workspace changes remain present locally, but no commit was created and no remote push completed in this sandbox.
