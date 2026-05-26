# Forge Core v0.4.146 Self-Evolution Report

Run id: `run_35ef1be524e640b98b117d39b2d37998`  
Workflow id: `wf_19f0b1b0a91a4e8698a234932d9fa9e3`  
Executor: `codex`  
Cycle: `23`  
Version line: `0.4.x` groundwork for Forge 0.5 runtime readiness

## Increment

Forge now exposes registry-level run health summaries in `forge list` and `forge inspect`.

Before this cycle, a workflow could show `run_statuses: ["running"]` while `active_run_count` was `0`, which made stale or missing-heartbeat self-evolution runs harder to diagnose from Forge-owned observability surfaces. `active_run_count` still means fresh active heartbeat count, but the adjacent `run_activity` summary now makes the difference explicit.

New `run_activity` fields:

- `total`
- `active`
- `accepted`
- `resumed`
- `running`
- `needs_attention`
- `stale`
- `missing_heartbeat`
- `inactive`
- `not_running`

The inspect diagram also renders running, missing-heartbeat, stale and needs-attention counts beside the run id/status list.

This is `0.5 groundwork`: it improves replacement-grade CLI observability and self-run trust, but does not claim Forge 0.5 completion.

## Validation Evidence

RED:

- `cargo test list_and_inspect_surface_running_run_health_even_without_fresh_heartbeat --test forge_cli_contract`
  - failed because `summary.run_activity` and row/inspect `run_activity` did not exist.

Targeted GREEN:

- `cargo test list_and_inspect_surface_running_run_health_even_without_fresh_heartbeat --test forge_cli_contract` passed.
- `cargo test request_heartbeat_marks_async_run_active_and_surfaces_it_in_status_list_and_inspect --test forge_cli_contract` passed.

Required validation:

- `cargo fmt --check` passed after `cargo fmt`.
- `cargo clippy --all-targets --all-features -- -D warnings` passed.
- `cargo test` passed: 57 unit tests, 220 CLI contract tests and 0 doc tests.
- `cargo build --release` passed.

Release smokes:

- `./target/release/forge plan --goal "Create a delivery platform" --output json` passed.
- `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.146-a` passed.
- `./target/release/forge --version` returned `forge 0.4.146`.

Install:

- `cargo install --path . --force` was attempted and blocked by read-only `/home/arthur/.cargo/.crates.toml`.
- Direct replacement of `/home/arthur/.cargo/bin/forge` was attempted and blocked by the same read-only filesystem boundary.
- `CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --locked --offline` passed.
- `/home/arthur/projects/forge-core/.forge/local-install/bin/forge --version` returned `forge 0.4.146`.
- The user-visible `forge` on PATH still returns `forge 0.4.145` until `/home/arthur/.cargo` is writable outside this sandbox.

Publication:

- `gh auth token >/dev/null` passed.
- `git remote get-url origin` returned `https://github.com/cardozoarthur/forge-core.git`.
- `git add ...` failed because `.git/index.lock` could not be created on a read-only filesystem.
- `git push` was attempted and failed because the sandbox could not resolve `github.com`.
- No commit was created and no remote publication claim is made for this cycle.

## Lean Overhead Ledger

- Prompt packet bytes: approximately `65000`
- Estimated prompt tokens: approximately `16250`
- Validation command count before publication: `15`
- Artifact count changed: `2` primary artifacts (`CHANGELOG.md`, this report) plus source/test/version metadata updates
- Metadata/report bytes added: `3682` bytes for the attached report copy
- Useful value: turns an ambiguous self-run observability gap into explicit, scriptable run health state without changing heartbeat semantics or adding orchestration bloat.
- Cost control evidence: all changes are deterministic Rust and contract tests; no model calls, external sends, infrastructure mutation, device access or runtime installation were needed.

## Files Changed

- `Cargo.toml`
- `Cargo.lock`
- `README.md`
- `src/inspection.rs`
- `src/milestone.rs`
- `src/registry.rs`
- `tests/forge_cli_contract.rs`
- `CHANGELOG.md`
- `docs/forge-0.5-milestone.md`
- `docs/reports/forge-core-v0.4.146-report-2026-05-26.md`

## Safety

No Docker, Kubernetes, Knative, Telegram send, camera, microphone, screen, mouse, keyboard, peripheral, model download, Blender execution or external user resource was mutated. The `skill install` smoke only wrote to `/tmp/forge-skill-smoke-0.4.146-a`; the fallback install wrote only inside `.forge/local-install`.

## Next Cycle

Move from observability metadata to real replacement-grade CLI work: add a native Forge file-editing and diff-review contract with permission gates, rollback metadata, validation hooks and inspectable session state.
