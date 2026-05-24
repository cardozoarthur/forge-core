# Forge Core v0.4.83 Self-Evolution Report

Run id: `run_33b736cf419e46f49594b76a7267345c`  
Workflow id: `wf_feb0e9936b7b40aabec02a88d79731dd`  
Prompt packet: `forge.self_evolution.prompt.v2`

## Increment

`forge task validate-response` now advances workflow state when an executor
response is accepted. A completed response with passing validation evidence marks:

- the task status as `completed`;
- all task subtasks as `completed`;
- the work item backlog state as `done`;
- the task goal readiness as definitively ready.

Accepted promotions also append a workflow revision with:

- origin `executor_response`;
- change type `executor_response_promoted`;
- a summary naming the promoted task and old/new task and workflow states.

Forge records a separate `executor_response_promoted` event with the validated
response hash and revision number.

## Why It Matters

The GRACE hackathon factory workflow needed to move from idea and artifact
generation into development planning. Before this increment, Forge could record
that a response was valid, but the workflow still appeared fully pending. That
made async execution look stuck even when evidence existed.

This change makes validation the promotion boundary: tasks advance only after
evidence is accepted.

The audit addendum matters because runtime state mutations must be revisioned
and traced. A validated executor response is now both an advancement signal and
a durable lineage record for async executor cycles.

## Safety

Forge still does not mark a task complete from output alone. The response must
use the executor response schema and include at least one passing validation
evidence item. Failed and retry-needed responses keep the task out of definitive
readiness.

The promotion event stores the content hash of the accepted response. It does
not execute external commands, open SSH sessions, mutate remote machines, touch
Docker/Kubernetes/Knative resources, or authorize any external executor.

## TDD Evidence

- RED: `cargo test task_validate_response_accepts_completed_executor_response_with_passing_evidence -- --exact` failed because the promoted workflow had zero revisions.
- GREEN: the same focused test passed after adding revision and event tracing to accepted executor-response promotion.

## Validation

Validation passed for this cycle:

- RED: `cargo test task_validate_response_accepts_completed_executor_response_with_passing_evidence -- --exact`
- GREEN: `cargo test task_validate_response_accepts_completed_executor_response_with_passing_evidence -- --exact`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`
- CLI smoke: `target/release/forge plan --goal "Create a delivery platform" --output json`
- CLI smoke: `target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.83-audit`

`cargo test` ran 110 integration tests plus unit/doc-test harnesses with zero
failures.

## Installation Note

`cargo install --path . --force` was attempted after validation, but the sandbox
blocked writes to `/home/arthur/.cargo/.crates.toml` with a read-only filesystem
error.

A scoped offline install succeeded with:

```bash
cargo install --path . --force --root /tmp/forge-install-0.4.83-audit --offline
```

`/tmp/forge-install-0.4.83-audit/bin/forge --version` returned `forge 0.4.83`.

## Publication Note

`gh auth token >/dev/null` succeeded and `git remote get-url origin` returned
`https://github.com/cardozoarthur/forge-core.git`.

Creating the commit was blocked before any index mutation:

```text
fatal: Unable to create '/home/arthur/projects/forge-core/.git/index.lock': Sistema de ficheiros só de leitura
```

No `git push` was run because there was no validated commit to publish.

## Related Previous Run Note

The pre-existing report draft also recorded a GRACE task-advancement run:

Run id: `run_65dd78cebf3748f4b8b02d75f2079bb3`  
Workflow id: `wf_710fe1d41a324dd4b22af04a65f53711`  
Prompt packet: `forge.task_advancement.v1`

That active hackathon workflow goal was updated to use:

`GRACE (Green Routing And Collaborative Efficiency)`

The workflow then received a GRACE development-readiness artifact and progressed
through validated task responses up to the OSM/OSRM MVP technical planning
stage, leaving only MVP/pitch validation and the continuous improvement loop
pending.
