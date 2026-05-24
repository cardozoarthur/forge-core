# Forge Core v0.4.37 Report - Lifecycle-Filtered Registry Listing

## Increment

Forge Core now supports explicit lifecycle filtering in `forge list`:

- `forge list --lifecycle all --output json`
- `forge list --lifecycle running --output json`
- `forge list --lifecycle non-running --output json`

The registry response includes `filter.lifecycle`, and summary totals are recomputed from the filtered workflow rows.

## Runtime Impact

- Operators can inspect running workflows separately from idle, blocked, failed, completed and scaled-to-zero workflows.
- The default `forge list` behavior remains `all`, so existing internal registry callers keep using the complete source-of-truth view.
- Filtered registry views still reuse Forge-owned lifecycle derivation, task summaries, reusable subflow projection and context-handoff summaries.

## Safety

Lifecycle filtering is a read-only projection over persisted Forge workflow/task state. It does not complete tasks, promote workflows, authorize executors, execute local Python/Node.js code, install Knative, or mutate Docker/Kubernetes/Knative resources.

## Validation Evidence

- RED: `cargo test list_filters_workflow_registry_by_running_and_non_running_lifecycle -- --nocapture` first failed with `error: unexpected argument '--lifecycle' found`.
- GREEN: the same focused test passed after adding filtered registry plumbing.
- Required validation passed:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test` with 63 CLI contract tests passing
  - `cargo build --release`
- CLI smokes passed with `PATH="$PWD/target/release:$PATH"`:
  - `forge --version` reported `forge 0.4.37`
  - `forge plan --goal "Create a delivery platform" --output json`
  - `forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`

## Publication Note

Global `cargo install --path . --force` was attempted after validation, but the sandbox rejected writes to `/home/arthur/.cargo/.crates.toml` with `Read-only file system (os error 30)`.

The checkout-local installation succeeded with:

```bash
cargo install --path . --force --root "$PWD/.forge/local-install" --offline
.forge/local-install/bin/forge --version
```

The installed local binary reports `forge 0.4.37`.

The GitHub publication contract was started after validation:

- `gh auth token >/dev/null` succeeded.
- `git remote get-url origin` returned `https://github.com/cardozoarthur/forge-core.git`.
- `git diff --check` succeeded.
- `git add ... && git commit -m "Add lifecycle-filtered workflow listing"` was blocked because the sandbox rejected `.git/index.lock` creation with `Sistema de ficheiros só de leitura`.
- `git push` was attempted and failed with `Could not resolve host: github.com` under restricted network access.
