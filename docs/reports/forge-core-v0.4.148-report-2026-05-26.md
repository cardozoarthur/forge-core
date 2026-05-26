# Forge Core v0.4.148 Self-Evolution Report

Run id: `run_35ef1be524e640b98b117d39b2d37998`  
Workflow id: `wf_19f0b1b0a91a4e8698a234932d9fa9e3`  
Executor: `codex`  
Cycle: `25`  
Version line: `0.4.x` groundwork for Forge 0.5 runtime readiness

## Increment

Forge run observability now uses the recorded executor PID as supporting liveness evidence. If a running request has an expired heartbeat but its `executor_pid` still resolves to a live process, `forge request status` reports:

- `activity.active: true`
- `activity.heartbeat_status: "process_alive"`
- `activity.process_status: "alive"`
- `activity.process_alive: true`
- `activity.recovery.action: "none"`

This prevents a long self-evolution or agent handoff from being incorrectly treated as stale only because the heartbeat TTL expired while the executor process was still alive. It directly supports the observability goal that Forge-owned list/status/inspect surfaces should remain trustworthy without falling back to tmux or `ps` as the primary operator model.

This is `0.5 groundwork`: it improves replacement-grade CLI and async agent handoff observability, but it does not claim Forge 0.5 completion.

## Runtime Behavior

- `forge.request.status` and `forge request status` include additive `process_status` and `process_alive` fields in `forge.run_activity.v1`.
- `forge list` registry summaries include `run_activity.process_alive`.
- `forge inspect` includes `run_activity.process_alive` and renders `process_alive: N` in the compact DAG run line.
- `forge request list --status stale` excludes running requests whose heartbeat is expired but whose recorded executor process is still alive.
- `forge request recover-stale` remains reserved for genuinely stale running requests without live process evidence.

Process liveness is read-only. Forge checks the stored PID and does not signal, kill, attach to or mutate any process.

## Validation Evidence

TDD:

- RED: `cargo test --test forge_cli_contract live_executor_pid_keeps_expired_heartbeat_active_without_stale_recovery -- --exact` failed because the expired heartbeat still reported `activity.active = false`.
- GREEN: the same test passed after adding process-liveness-aware activity.
- Regression: `cargo test --test forge_cli_contract heartbeat` passed all 5 heartbeat/observability tests.

Required validation:

- `cargo fmt --check` passed.
- `cargo clippy --all-targets --all-features -- -D warnings` passed.
- `cargo test` passed: 58 unit tests, 221 CLI contract tests, 0 doc tests.
- `cargo build --release` passed.

Release smoke:

- `./target/release/forge --version` returned `forge 0.4.148`.
- `./target/release/forge plan --goal "Create a delivery platform" --output json` passed.
- `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.148-1779774250` passed.

Install:

- Default `cargo install --path . --force` was blocked by read-only `/home/arthur/.cargo`.
- Fallback local install passed with `CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --locked --offline`.
- `.forge/local-install/bin/forge --version` returned `forge 0.4.148`.
- The current shell `forge --version` still returns `forge 0.4.147` because `/home/arthur/.cargo/bin/forge` is outside the writable sandbox for this run.

Publication:

- `gh auth token >/dev/null` passed without exposing the token.
- `git remote get-url origin` returned `https://github.com/cardozoarthur/forge-core.git`.
- Commit/push was blocked because `.git` is read-only in this sandbox: `git add ...` failed with `Unable to create '.git/index.lock': Sistema de ficheiros só de leitura`.

## Lean Overhead Ledger

- Prompt packet bytes: approximately `56000`
- Estimated prompt tokens: approximately `14000`
- Validation command count before publication: `9`
- Artifact count changed: `1` report artifact plus source/docs/version metadata
- Metadata/report bytes added: approximately `3600`
- Useful value: prevents false stale recovery for live long-running executor handoffs, reducing operator confusion and avoiding unnecessary resume/recover cycles.
- Cost control evidence: the implementation reuses existing run records, heartbeat metadata, registry summaries and inspect output. No new orchestration service, external daemon or dependency was added.

## Files Changed

- `Cargo.toml` and `Cargo.lock` - version `0.4.147` to `0.4.148`
- `README.md` - version and run-activity capability note
- `src/request.rs` - process-liveness-aware `RunActivity`
- `src/registry.rs` - `process_alive` registry summary count
- `src/inspection.rs` - compact inspect diagram now renders process liveness
- `src/milestone.rs` and `docs/forge-0.5-milestone.md` - replacement-grade CLI groundwork evidence
- `src/skill.rs` and `skills/forge-core/SKILL.md` - heartbeat examples now include `--pid`
- `tests/forge_cli_contract.rs` - process-liveness CLI contract
- `CHANGELOG.md` - v0.4.148 entry
- `docs/reports/forge-core-v0.4.148-report-2026-05-26.md` - this report

## Safety

No Docker, Kubernetes, Knative, Telegram send, camera, microphone, screen, mouse, keyboard, peripheral, model download, Blender execution or external user resource was mutated. The skill smoke only wrote to `/tmp/forge-skill-smoke-0.4.148-1779774250`; the fallback install wrote only inside `.forge/local-install`.

## Next Cycle

Move replacement-grade CLI beyond observability by adding a Forge-owned interactive diff/patch review flow: permission-gated file patch proposal, TUI review, rollback metadata, artifact lineage and inspectable session state.
