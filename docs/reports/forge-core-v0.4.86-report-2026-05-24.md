# Forge Core v0.4.86 Self-Evolution Report

Run id: `run_0293799c9ae848629e6c26ce12281b20`  
Workflow id: `wf_846a25a6680c4fd1988ba2d57c17703d`  
Prompt packet: `forge.self_evolution.prompt.v2`

## Increment

Forge self-evolution now has an explicit lean stop boundary instead of only a
time boundary.

`forge self run` accepts:

```bash
--mode lean|balanced|strict
```

The default is `balanced`. The modes are operational boundaries:

- `lean`: minimal governance; reject cycles where metadata/protocol expansion is
  not justified by useful throughput, cost, retry, deterministic-execution or
  artifact-delivery value.
- `balanced`: default bounded governance for small validated increments.
- `strict`: higher auditability for concrete safety, audit or distributed-runtime
  needs.

Every run and cycle report now includes:

- `forge.self_evolution.overhead_ledger.v1`;
- `forge.self_evolution.decision_gate.v1`.

The ledger records prompt bytes, estimated prompt tokens, validation command
count, artifact count, metadata bytes and an orchestration cost score. The gate
can run a bounded cycle, reject low-value governance bloat or stop immediately
when the persisted terminal goal has already been satisfied.

## Why It Matters

The current persisted goal says the self-evolution program must stop when Forge
has a validated lean/balanced/strict mode boundary, measurable overhead ledger
and automated value-vs-cost gate. Before this increment, the loop could stop by
date or max cycle count, but it could not stop because the terminal self-evolution
goal was complete.

This change puts the stop decision in the self-evolution runtime itself. Future
cycles no longer need to continue proposing architecture just because time
remains before the stop date.

## Safety

This increment only changes Forge-owned CLI/report/prompt behavior. It does not
call a model, authorize executors, run Docker/Kubernetes/Knative mutation, change
external infrastructure, or bypass validation-before-commit.

The new ledger and gate are intentionally compact and reuse existing
self-evolution report artifacts instead of adding a new persistence table,
manifest layer or projection surface.

## TDD Evidence

- RED: `cargo test self_run_ --test forge_cli_contract` failed because
  `forge self run` rejected the new `--mode` argument.
- GREEN: the same focused command passed after adding operating modes, the
  overhead ledger and the decision gate.

## Validation

Validation passed for this cycle:

- RED: `cargo test self_run_ --test forge_cli_contract` failed because
  `forge self run` rejected the new `--mode` argument.
- GREEN: `cargo test self_run_ --test forge_cli_contract` passed.
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`
- CLI smoke: `target/release/forge --store /tmp/forge-core-v0.4.86-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json`
- CLI smoke: `target/release/forge --store /tmp/forge-core-v0.4.86-skill-smoke.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v0.4.86`

`cargo test` ran 114 integration tests plus unit/doc-test harnesses with zero
failures.

## Installation Note

`cargo install --path . --force` was attempted after validation, but the sandbox
blocked writes to `/home/arthur/.cargo/.crates.toml` with:

```text
Read-only file system (os error 30)
```

A scoped offline install succeeded with:

```bash
cargo install --path . --force --root /tmp/forge-install-0.4.86 --offline
```

`/tmp/forge-install-0.4.86/bin/forge --version` returned `forge 0.4.86`.

## Publication Check

`gh auth token >/dev/null` succeeded and `git remote get-url origin` returned:

```text
https://github.com/cardozoarthur/forge-core.git
```

Creating the commit was blocked before publication:

```text
fatal: Unable to create '/home/arthur/projects/forge-core/.git/index.lock': Sistema de ficheiros só de leitura
```

`git push` was attempted and failed under restricted network/DNS:

```text
Could not resolve host: github.com
```

## Next Cycle Recommendation

No default next architecture cycle is recommended once v0.4.86 is installed and
the terminal goal is active. The next run should verify that `forge self run`
returns `terminal_goal_reached` for the persisted final goal, then pause until a
human provides a new explicit goal.
