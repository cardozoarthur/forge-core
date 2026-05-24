# Forge Core v0.4.59 Self-Evolution Report

Run id: `run_35532f01c7e94556b18b12c7287a2271`  
Workflow id: `wf_e6eb361719a24484ba809d4150f75699`  
Prompt packet: `forge.self_evolution.prompt.v1`

## Increment

Added a versioned executor response validation contract.

Forge already had bounded handoff packets, context checksums, leases, resumable
context and validation gates. This increment adds the next adapter-side boundary:
`forge task validate-response` checks a returned executor result before Forge treats
it as acceptable evidence.

The command expects `forge.executor_response.v1` and emits
`forge.executor_response_validation.v1`. Completed responses must match the task id,
include a replayable `trace_ref`, report finite non-negative cost/token values and
carry at least one passing validation evidence item. Rejected responses exit non-zero
with structured violation codes instead of silently promoting task output.

## Files Changed

- `src/adapter.rs`
- `src/lib.rs`
- `src/main.rs`
- `tests/forge_cli_contract.rs`
- `Cargo.toml`
- `Cargo.lock`
- `README.md`
- `docs/technical-definition.md`
- `CHANGELOG.md`
- `docs/reports/forge-core-v0.4.59-report-2026-05-24.md`

## Validation

- Red tests: `cargo test task_validate_response_ --test forge_cli_contract` failed first because `forge task validate-response` was not a recognized subcommand.
- Focused green tests: `cargo test task_validate_response_ --test forge_cli_contract` passed after implementation.
- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test`: passed, 89 CLI contract tests.
- `cargo build --release`: passed.
- CLI smoke `./target/release/forge --store /tmp/forge-plan-smoke-0.4.59.sqlite plan --goal "Create a delivery platform" --output json`: passed.
- CLI smoke `./target/release/forge --store /tmp/forge-skill-smoke-0.4.59-run35532.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.59-run35532`: passed.

## Install Notes

- Required global install `cargo install --path . --force` was attempted after validation but was blocked by the sandbox: `/home/arthur/.cargo/.crates.toml` is read-only.
- Workspace-local fallback install succeeded with `CARGO_NET_OFFLINE=true cargo install --path . --force --locked --offline --root /home/arthur/projects/forge-core/.forge/local-install-offline`.
- The fallback binary reports `forge 0.4.59`.

## Publication Notes

- `gh auth token`: passed.
- `git remote get-url origin`: `https://github.com/cardozoarthur/forge-core.git`.
- `git add ...`: blocked because `.git/index.lock` cannot be created on the read-only git metadata mount.
- `git push`: attempted and failed because the sandbox cannot resolve `github.com`.
- The validated changes remain in the working tree and were not committed or pushed from this sandbox.

## Safety

- Response validation is read-only with respect to workflow task state.
- The command records an audit event, but it does not complete tasks, promote workflows, acquire leases, authorize CLIs, execute local Python/Node.js code, install Knative or mutate Docker/Kubernetes/Knative resources.
- Executor output remains evidence to validate. Forge remains the authority for promotion.

## Next Recommended Cycle

Persist accepted executor response summaries as per-task execution evidence and wire them into `forge validate`, so promotion can require validated adapter output without trusting raw executor claims.
