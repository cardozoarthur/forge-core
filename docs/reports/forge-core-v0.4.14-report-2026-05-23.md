# Forge Core v0.4.14 Report - 2026-05-23

## Objective

Add the first read-only terminal inspection surface for persisted Forge workflows so operators can inspect a DAG without reading raw JSON.

## Change

`forge inspect <workflow-id>` now loads a workflow from Forge's SQLite source of truth and renders a terminal DAG view. The report includes:

- workflow id, lifecycle state, initial request, current goal, revision, artifact count and task count;
- task rows with status, dependency edges and executor kind;
- node-scoped persona annotations for human-facing tasks;
- structured JSON nodes for adapters and tests;
- reserved subflow fields so the next registry increment can add recursive subflows without changing the top-level shape.

`forge inspect <workflow-id> --verbose --output json` adds task goals, expected outputs, validation rules and subtasks. Human output prints the terminal diagram directly; JSON output returns the same diagram plus structured fields.

## Safety

The command is read-only. It derives lifecycle state through the existing registry projection and does not mutate workflows, executor policy, Docker, Kubernetes, Knative or any external runtime substrate.

## Validation

Added a CLI contract test for:

- lifecycle projection;
- terminal DAG text;
- dependency edge rendering;
- persona annotations;
- verbose validation rules and subtasks.

Fresh validation run in this cycle:

- `cargo fmt --check`: passed;
- `cargo clippy --all-targets --all-features -- -D warnings`: passed;
- `cargo test`: passed, 38 CLI contract tests;
- `cargo build --release`: passed;
- CLI smoke `forge plan --goal "Create a delivery platform" --output json`: passed with the release binary and a temporary store;
- CLI smoke `forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`: passed with the release binary and a temporary store;
- CLI smoke `forge inspect <workflow-id> --verbose --output json`: passed with the release binary against the planned workflow from the plan smoke.

Post-validation local installation:

- `cargo install --path . --force`: blocked because `/home/arthur/.cargo` is read-only in this execution environment;
- `cargo install --path . --force --root .forge/local-install --offline`: passed;
- `.forge/local-install/bin/forge --version`: `forge 0.4.14`.

## Next Recommended Cycle

Add persisted recursive subflow records and extend `forge inspect --verbose` to render child subflows, including finite versus infinite lifecycle metadata.
