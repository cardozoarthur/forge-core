---
name: foundry-core-workspaces
description: Git worktree binding, approved paths, guardrails, predecessor blocking, and preview/internal-test sandboxes.
license: MIT
compatibility: codex, opencode, agy, claude
---

## Workspace Contract

Git owns checkout state; Foundry owns revisioned bindings and receipts. Keep one absolute store outside disposable worktrees. Tasks inherit the workflow binding unless a task binding overrides it; rebinding replaces that scope and records a revision.

## Required Flow

```bash
STORE=/absolute/path/to/foundry.sqlite
foundry --store "$STORE" worktree create --repository <repo> --path <path> --allow-repository-mutation
foundry --store "$STORE" worktree init --worktree <wt> --allow-worktree-write
foundry --store "$STORE" worktree approve-config --worktree <wt> --allow-guardrail-update --approved-by <operator>
foundry --store "$STORE" worktree bind --worktree <wt> --workflow <wf>
foundry --store "$STORE" worktree bind --worktree <wt> --workflow <wf> --task <task>
foundry --store "$STORE" worktree guard check --worktree <wt> --operation modify --path <relative-path> --reason "<objective>" --workflow <wf> --task <task>
foundry --store "$STORE" worktree guard create-predecessor --worktree <wt> --workflow <wf> --task <task> --path <relative-path> --goal "<path goal and validation>" --allow-workflow-mutation --approved-by <operator>
foundry --store "$STORE" worktree sandbox plan --worktree <wt> --purpose test --workflow <wf> --task <task> -- <command>
foundry --store "$STORE" worktree sandbox run --worktree <wt> --purpose test --workflow <wf> --task <task> --allow-exec -- <command>
```

Review `.foundry/worktree.toml`; edits invalidate its approved SHA-256. Paths must be relative, contained and symlink-free; protected scopes override modifiable scopes. Config, path, branch, command, binding, runtime and network gates must pass.

## Guardrails and Blocking

Run `guard check` before writes. It is a read-only policy gate, not filesystem enforcement. Delegate a protected or non-modifiable path only through the objective predecessor returned by the guard. Creation requires `--allow-workflow-mutation`; Foundry blocks the current task and returns it to `Pending` after dependencies validate. Foundry does not edit or approve the path. Fix binding, path and manifest errors directly, and reapprove changed manifests.

## Parallel Worktrees

`foundry teamwork --lane frontend=agy:3 --lane backend=codex:5 --max-parallel-agents 8` persists independent lanes. Dry-run `worktree prepare-teamwork`, then repeat with `--allow-repository-mutation`. Each external task gets one binding; launch still requires `request execute-wave --allow-exec`.

For joins, dry-run `worktree integrate-dependencies --workflow <wf> --task <join>`, then repeat with `--allow-repository-mutation --approved-by <operator> --reason "<reason>"`. The receipt must match the clean destination, frozen source HEADs and task bindings. Conflicts roll back; concurrent fan-in fails before mutation.

## Preview and Internal-Test Isolation

Use `--purpose preview` for previews and `--purpose test` for internal tests. The `process` runtime bounds cwd, environment, time, output and evidence, but is not a security boundary. `bubblewrap` mounts the worktree read-only and only its sandbox root writable; use `network="deny"` to deny network.

Treat plan/run receipts as separate gates. Block handoff on plan blockers, missing `--allow-exec`, timeout or non-zero exit. Persistent preview/test uses `sandbox start --allow-exec`, read-only `status`, and `stop --allow-stop`.

All mutations require shown approvals. Approver/origin is provenance, not authentication. Keep secrets out of manifests; Foundry has no worktree removal command.
