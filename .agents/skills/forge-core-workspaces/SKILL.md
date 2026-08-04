---
name: forge-core-workspaces
description: Git worktree binding, approved path policy, and preview/test sandbox execution for Forge Core.
license: MIT
compatibility: codex, opencode, agy, claude
---

## Workspace Contract

Git owns checkout state. Forge's central store owns worktree bindings, revisions, context, handoffs and sandbox receipts. Keep one absolute store outside disposable worktrees; a task binding overrides the workflow binding.

## Required Flow

```bash
STORE=/absolute/path/to/forge.sqlite
WT=<worktree-id-or-path>
WF=<workflow-id>
TASK=<task-id>

forge --store "$STORE" worktree discover --repository <repository-root> --output json
forge --store "$STORE" worktree create --repository <repository-root> --path <worktree-root> --branch <branch> --start-point <git-ref> --allow-repository-mutation --output json
forge --store "$STORE" worktree register --path <existing-worktree> --output json
forge --store "$STORE" worktree init --worktree "$WT" --allow-worktree-write --output json
forge --store "$STORE" worktree approve-config --worktree "$WT" --allow-guardrail-update --approved-by <operator-id> --output json
forge --store "$STORE" plan --goal "<goal>" --worktree "$WT" --output json
forge --store "$STORE" worktree bind --worktree "$WT" --workflow "$WF" --task "$TASK" --origin codex --output json
forge --store "$STORE" worktree guard check --worktree "$WT" --operation modify --path <relative-path> --reason "<objective>" --workflow "$WF" --task "$TASK" --output json
forge --store "$STORE" worktree guard create-predecessor --worktree "$WT" --workflow "$WF" --task "$TASK" --path <relative-path> --goal "<path-specific goal and validation>" --allow-workflow-mutation --approved-by <operator-id> --origin codex --output json
forge --store "$STORE" worktree sandbox plan --worktree "$WT" --purpose test --workflow "$WF" --task "$TASK" --output json -- <command>
forge --store "$STORE" worktree sandbox run --worktree "$WT" --purpose test --workflow "$WF" --task "$TASK" --allow-exec --output json -- <command>
forge --store "$STORE" worktree inspect --worktree "$WT" --output json
```

Primary `forge plan` and `forge request start` flows can materialize the same
graph with `--lane frontend=agy:3`, `--lane backend=codex:5` and
`--max-parallel-agents 8`; the normalized contract is persisted at
`core_orchestration.parallel_team`. Without lanes the generic graph remains
serial, and ambiguous or `auto` lane routing must fail closed.

For an elastic teamwork graph, prepare all external-agent worktrees as an
explicit, reviewable repository mutation:

```bash
forge --store "$STORE" teamwork \
  --goal "Deliver frontend and backend independently" \
  --lane frontend=agy:3 --lane backend=codex:5 \
  --max-parallel-agents 8 --detached --output json

forge --store "$STORE" worktree prepare-teamwork --workflow "$WF" \
  --repository <repository-root> --worktree-root <dedicated-sibling-root> \
  --branch-prefix forge/teamwork --output json
# Review the dry-run report, then apply it to the same workflow:
forge --store "$STORE" worktree prepare-teamwork --workflow "$WF" \
  --repository <repository-root> --worktree-root <dedicated-sibling-root> \
  --branch-prefix forge/teamwork --allow-repository-mutation --output json
```

The preparation subcommand neither creates a new workflow nor a new run. It is
idempotent and binds one Git worktree directly to each
pending external-agent task. One implementation branch is reported per declared
worker (eight in this example), separately from later integrator/auditor
worktrees. Worktree creation never
implies executor authorization; process launch still uses the explicit
`request execute-wave --allow-exec` gate.

After each join's dependencies have completed, converge their frozen branches
through a separate review/apply boundary:

```bash
forge --store "$STORE" worktree integrate-dependencies \
  --workflow "$WF" --task <join-task-id> --output json
# Review the dry-run plan, then apply it with explicit provenance:
forge --store "$STORE" worktree integrate-dependencies \
  --workflow "$WF" --task <join-task-id> \
  --allow-repository-mutation --approved-by <operator-id> \
  --reason "integrate validated dependency branches" --output json
```

Repeat this for both lane joins and the final auditor. Forge keeps those tasks
blocked on `git_dependency_fan_in` until a successful, current receipt matches
the clean destination, every frozen source HEAD and the task-scoped bindings.
Conflicts roll the Forge-owned destination back and require rework; they are not
accepted as a partial merge. Concurrent fan-in attempts for the same destination
fail before mutation; retry only after the active attempt finishes and review the
new dry-run.

Review `.forge/worktree.toml`; any edit invalidates its approved SHA-256. Paths must be relative, contained and symlink-free; protected scopes override modifiable scopes. Config, path, branch, command, binding, runtime and network decisions must all pass. `forge --store "$STORE" request start --goal "<goal>" --worktree "$WT" --origin codex --output json` is the asynchronous alternative to `plan`.

## Isolation And Blocking Rules

The `process` runtime bounds cwd, environment, time, output and evidence, but is not a security boundary and enforces neither filesystem nor network isolation. `bubblewrap` mounts the worktree read-only at `/workspace` and only its internal sandbox root writable; HOME/tmp map to sandbox directories. Payloads must use system mounts and write outputs under the runtime sandbox root. `FORGE_STORE_PATH` is an unmounted host locator (`forge_store_path_mounted=false`), so nested Forge mutations run outside. Add `network="deny"` for network isolation.

Check paths before writes. Delegate a protected/non-modifiable scope only through the approved, objective predecessor. Forge blocks the current task, then returns it to `Pending` after validated dependencies complete; it does not edit or approve the path. Reapprove the manifest if predecessor work changes it.

Use preview/test receipts as separate validation gates. Block handoff on plan blockers, missing `--allow-exec`, timeout or non-zero exit.

`sandbox start --allow-exec`, approval-free `status`, and `stop --allow-stop` manage persistent preview/test via `forge.worktree.sandbox_lifecycle.v1`. `status` does not launch commands, but it reconciles a dead supervisor by killing the persisted payload group and tracked descendants before recording `sandbox_execution_failed`. Receipts expose `execution_attempted`, sanitized `error`, and stream `redaction_count`.

All mutating commands above require their shown approvals. Approver/origin values are provenance, not authentication. Keep secrets out of the manifest; Forge has no worktree removal command.
