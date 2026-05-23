# Forge Core v0.4.11 Report - 2026-05-23

## Objective

Advance the Context Routing Engine with runtime mutation propagation: context packets should show the current Forge workflow state, not only the task snapshot created during planning.

## Change

`forge context` now emits schema `forge.context.v2` and routing policy `task_local_revisioned_budget_v2`.

Each context response includes:

- `workflow_revision`, derived from the latest workflow mutation revision;
- `artifact_count`, derived from Forge-owned artifact records;
- `lineage.workflow_goal_sha256` for the current workflow goal;
- `lineage.task_goal_sha256` for the selected task goal;
- `lineage.artifact_manifest_sha256` for the current artifact manifest;
- `lineage.revision_sources` for the origins that changed the workflow;
- `lineage.lineage_sha256` for replayable stale-context checks;
- a new `workflow_goal` shard in the executor-facing `content`.

The legacy `content`, `included_sections`, `omitted_sections` and `shards` fields remain present for existing Codex/OpenCode callers.

## Operator Impact

When a long-running executor resumes work after `forge workflow update-goal` or `forge workflow attach-artifact`, it can compare the lineage fields from its prior context packet with a fresh `forge context` response. A changed lineage hash means the executor should refresh its task packet before continuing.

## Safety

- No Docker, Kubernetes or Knative resources are read or mutated.
- No installed CLI is used as an execution engine.
- Context lineage is derived only from Forge's persisted workflow and artifact state.
- The change is additive for executor payload compatibility, while the schema version records the new contract.

## Validation

Required validation passed:

- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test`: passed, including 35 CLI contract tests.
- `cargo build --release`: passed.

CLI smoke passed with the release binary first in `PATH`:

- `forge plan --goal "Create a delivery platform" --output json`: passed.
- `forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`: passed.

## Installation And Publication

The required global install command was attempted:

```bash
cargo install --path . --force
```

It failed because `/home/arthur/.cargo` is read-only in this execution environment:

```text
error: failed to open: /home/arthur/.cargo/.crates.toml

Caused by:
  Read-only file system (os error 30)
```

The GitHub CLI publication contract was started:

- `gh auth token`: passed with stdout redirected.
- `git remote get-url origin`: returned `https://github.com/cardozoarthur/forge-core.git`.
- `git add ...`: blocked because `.git` is mounted read-only and cannot create `index.lock`.
- `git push`: blocked because this environment cannot resolve `github.com`.

Current `.git` mount state:

```text
/home/arthur/projects/forge-core/.git /dev/nvme0n1p2[/home/arthur/projects/forge-core/.git] ext4 ro,nosuid,nodev,relatime,errors=remount-ro
```

## Next Recommended Cycle

Add persisted context summaries keyed by workflow id, task id and lineage hash, then let `forge context` reuse validated summaries before adding raw artifact or history shards.
