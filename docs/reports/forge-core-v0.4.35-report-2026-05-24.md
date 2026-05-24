# Forge Core v0.4.35 Report - Content-Addressed Context Shards

## Increment

Forge Core now emits `forge.context.v15` context packets with content-addressed shard metadata. Each shard carries a stable `sequence`, `shard_id` and `source_sha256` derived from the Forge-owned workflow/task state.

## Runtime Impact

- Executor adapters can identify and cache individual source shards without comparing the full context packet.
- Omitted shards remain auditable because their original source hash is preserved separately from the selected payload hash.
- The routing fingerprint now includes a `source_shards` component, so cache keys change when source shard content changes even if the final section names look similar.

## Safety

This is read-only routing metadata. It does not complete tasks, promote workflows, authorize executors, execute local code, install Knative, or mutate Docker/Kubernetes/Knative resources.

## Validation Plan

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`
- `forge plan --goal "Create a delivery platform" --output json`
- `forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`

## Validation Evidence

- The new regression test first failed against `forge.context.v14`, then passed after the v15 implementation.
- `cargo test` passed with 61 CLI contract tests.
- The release binary reported `forge 0.4.35`.
- CLI smokes passed with `PATH="$PWD/target/release:$PATH"` so the newly built binary handled planning and skill installation.

## Install Note

`cargo install --path . --force` was attempted after the release build, but the current Codex sandbox rejected writes to `/home/arthur/.cargo/.crates.toml` with `Read-only file system (os error 30)`. The existing global `forge` remains `0.4.34`; `target/release/forge` is `0.4.35`.
