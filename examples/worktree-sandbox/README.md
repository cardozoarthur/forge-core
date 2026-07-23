# Worktree Sandbox Example

This example creates a Forge-owned Git worktree, binds a workflow to it, and runs a guarded test. Replace every placeholder with an absolute path or returned id.

## 1. Define Stable Paths

```bash
FORGE_WORKTREE_STORE=/absolute/path/to/central-forge.sqlite
FORGE_REPOSITORY_ROOT=/absolute/path/to/repository
FORGE_WORKTREE_ROOT=/absolute/path/to/worktrees/forge-preview
FORGE_WORKTREE_ID=wt_replace_from_create_output
FORGE_WORKFLOW_ID=wf_replace_from_plan_output
FORGE_TASK_ID=task-001
FORGE_APPROVER=operator-id
```

Keep `FORGE_WORKTREE_STORE` absolute and outside `FORGE_WORKTREE_ROOT`. Use the same value for every command.

## 2. Discover And Create

```bash
forge --store "$FORGE_WORKTREE_STORE" worktree discover \
  --repository "$FORGE_REPOSITORY_ROOT" \
  --output json

forge --store "$FORGE_WORKTREE_STORE" worktree create \
  --repository "$FORGE_REPOSITORY_ROOT" \
  --path "$FORGE_WORKTREE_ROOT" \
  --branch feature/forge-preview \
  --start-point main \
  --allow-repository-mutation \
  --output json
```

Copy `worktree.id` from the create response into `FORGE_WORKTREE_ID`.

For an existing checkout, use `forge worktree register --path "$FORGE_WORKTREE_ROOT"` instead of `create`.

## 3. Initialize And Review Policy

```bash
forge --store "$FORGE_WORKTREE_STORE" worktree init \
  --worktree "$FORGE_WORKTREE_ID" \
  --allow-worktree-write \
  --output json
```

Review `$FORGE_WORKTREE_ROOT/.forge/worktree.toml`. For a local process test, keep the bounded but non-isolating runtime explicit:

```toml
schema_version = "forge.worktree.config.v1"

[guardrails]
require_clean = false
allow_detached_head = false
allowed_branches = ["feature/*"]
allowed_commands = ["cargo", "sh"]
modifiable_paths = ["."]
protected_paths = [".git/", ".forge/worktree.toml"]
require_workflow_binding = true
max_command_seconds = 900
max_output_bytes = 1048576

[sandbox]
enabled = true
name = "internal"
root = ".forge/sandboxes/internal"
runtime = "process"
working_directory = "."
purposes = ["preview", "test"]
network = "inherit"
inherit_environment = ["PATH", "HOME", "LANG", "CARGO_HOME", "RUSTUP_HOME"]

[sandbox.commands]
preview = ["cargo", "check"]
test = ["cargo", "test"]
```

Both sandbox paths and all modification scopes must remain relative and inside the worktree. Protected scopes take precedence over modifiable scopes; the defaults prevent ordinary tasks from changing Git metadata or the policy file itself. Absolute paths, `..` escapes and existing symlink components are rejected. The manifest is not a place for secrets.

`init` approved the generated file, but the review above changed its content. Approve that exact new hash before a guard or sandbox plan:

```bash
forge --store "$FORGE_WORKTREE_STORE" worktree approve-config \
  --worktree "$FORGE_WORKTREE_ID" \
  --allow-guardrail-update \
  --approved-by "$FORGE_APPROVER" \
  --origin forge_cli \
  --output json
```

Editing the manifest again invalidates this approval. Review and repeat `approve-config`; never reuse the old hash implicitly.

## 4. Plan And Bind In One Command

```bash
forge --store "$FORGE_WORKTREE_STORE" plan \
  --goal "Implement the change and pass the worktree test gate" \
  --worktree "$FORGE_WORKTREE_ID" \
  --output json
```

Copy `workflow_id` into `FORGE_WORKFLOW_ID`. To override one task with another registered worktree, bind that task explicitly:

```bash
forge --store "$FORGE_WORKTREE_STORE" worktree bind \
  --worktree <task-worktree-id> \
  --workflow "$FORGE_WORKFLOW_ID" \
  --task "$FORGE_TASK_ID" \
  --origin forge_cli \
  --output json
```

The task-specific binding wins over the workflow default.

## 5. Guard Intended Modifications

Check the narrow files or directories that the task intends to write:

```bash
forge --store "$FORGE_WORKTREE_STORE" worktree guard check \
  --worktree "$FORGE_WORKTREE_ID" \
  --operation modify \
  --path src/lib.rs \
  --path tests/worktree_contract.rs \
  --reason "Implement and verify the worktree behavior" \
  --workflow "$FORGE_WORKFLOW_ID" \
  --task "$FORGE_TASK_ID" \
  --output json
```

Continue only with `status=modification_allowed` and `allowed=true`. A blocked check exits non-zero and returns path-level reasons plus its remediation. An unapproved hash points back to `approve-config`; a binding or unsafe-path error must be corrected rather than delegated.

When a protected or non-modifiable scope is genuinely required, create the returned objective predecessor explicitly:

```bash
forge --store "$FORGE_WORKTREE_STORE" worktree guard create-predecessor \
  --worktree "$FORGE_WORKTREE_ID" \
  --workflow "$FORGE_WORKFLOW_ID" \
  --task "$FORGE_TASK_ID" \
  --path .forge/worktree.toml \
  --goal "Update .forge/worktree.toml for the reviewed policy and validate the resulting hash" \
  --allow-workflow-mutation \
  --approved-by "$FORGE_APPROVER" \
  --origin forge_cli \
  --output json
```

This creates a pending predecessor, adds it as a dependency, sets the current task to `Blocked`, and records the approval/revision lineage. It does not edit the file. Validated completion of the predecessor returns the current task to `Pending` once every dependency is complete.

Run that exception branch only when the protected change is actually required. If created, execute and validate the returned predecessor before continuing with the original task. Because this example delegates the manifest itself, review its result and run `approve-config` again before the next guard or sandbox plan.

## 6. Use The Test Receipt As A Blocking Gate

Plan first:

```bash
forge --store "$FORGE_WORKTREE_STORE" worktree sandbox plan \
  --worktree "$FORGE_WORKTREE_ID" \
  --purpose test \
  --workflow "$FORGE_WORKFLOW_ID" \
  --task "$FORGE_TASK_ID" \
  --output json
```

With no command after `--`, the manifest supplies `cargo test`. Require `status=sandbox_ready` and `allowed=true` before continuing.

Then authorize one real execution:

```bash
forge --store "$FORGE_WORKTREE_STORE" worktree sandbox run \
  --worktree "$FORGE_WORKTREE_ID" \
  --purpose test \
  --workflow "$FORGE_WORKFLOW_ID" \
  --task "$FORGE_TASK_ID" \
  --allow-exec \
  --output json
```

Keep the validation task pending or in rework when the plan has blockers, execution was not approved, the command timed out, or the receipt has a non-zero exit. Only hand off or promote the dependent implementation task after `status=sandbox_completed` and `exit_code=0`, with the receipt referenced as validation evidence.

Forge blocks the sandbox run when guardrails fail. The workflow still needs an explicit task dependency/validation rule if this receipt must block another arbitrary task.

## 7. Opt Into Stronger Local Isolation

On a Linux host with `bwrap` available, change the manifest to:

```toml
[sandbox]
enabled = true
name = "internal"
root = ".forge/sandboxes/internal"
runtime = "bubblewrap"
working_directory = "."
purposes = ["preview", "test"]
network = "deny"
inherit_environment = ["LANG"]
```

Approve the changed manifest hash, then verify the actual boundary with a system-mounted shell:

```bash
forge --store "$FORGE_WORKTREE_STORE" worktree sandbox run \
  --worktree "$FORGE_WORKTREE_ID" \
  --purpose test \
  --workflow "$FORGE_WORKFLOW_ID" \
  --task "$FORGE_TASK_ID" \
  --allow-exec \
  --output json \
  -- sh -c 'test -r Cargo.toml && test ! -w Cargo.toml && test -w .forge/sandboxes/internal'
```

The plan embedded in a successful receipt reports both isolation fields as true, `runtime_worktree_root=/workspace`, `runtime_sandbox_root=/workspace/.forge/sandboxes/internal`, and `forge_store_path_mounted=false`. Bubblewrap mounts the host worktree read-only at that guest root, binds only the internal sandbox root writable, maps its `home` to `/home/forge` and `tmp` to `/tmp`, and intentionally omits host-home toolchains. `FORGE_STORE_PATH` remains a host locator for lineage; the external SQLite file is not mounted, so nested Forge mutations belong to the parent workflow outside the guest. To run an actual build, its executable/runtime must exist in the read-only system mounts and all outputs/caches must target the runtime sandbox root. The `process` runtime reports both enforcement fields as false and cannot enforce `network="deny"`.

## 8. Inspect The Recorded State

```bash
forge --store "$FORGE_WORKTREE_STORE" worktree inspect \
  --worktree "$FORGE_WORKTREE_ID" \
  --output json

forge --store "$FORGE_WORKTREE_STORE" worktree list \
  --workflow "$FORGE_WORKFLOW_ID" \
  --output json
```

See [the full contract](../../docs/worktrees-and-sandboxes.md) for all guardrails, receipts and isolation boundaries.
