# Worktrees And Sandboxes

Foundry can register Git worktrees, bind them to workflows or individual tasks, approve versioned path policy, guard intended modifications, include the effective worktree configuration in context/handoff packets, and plan or run bounded `preview` and `test` commands inside the selected checkout.

This is an execution-workspace contract. It does not turn a Git worktree into a container, and it does not make a process an operating-system security boundary.

## Model And Authority

| Concept | Meaning | Source of truth |
| --- | --- | --- |
| Repository root | The primary Git repository that owns the shared Git common directory. | Git |
| Git worktree | A concrete checkout, branch or detached HEAD, and filesystem path. | Git plus a Foundry worktree record |
| Worktree binding | The revisioned association from a workflow, or one workflow task, to a registered worktree. | The central Foundry SQLite store |
| Worktree manifest | Guardrails and sandbox settings loaded from `.foundry/worktree.toml` in that worktree. | The worktree filesystem, captured by SHA-256 and approved in central state |
| Sandbox | A planned or executed `preview`/`test` command rooted in the worktree. | The manifest plus a versioned Foundry plan/receipt |
| Visual workspace | A Foundry UI/canvas concept. It is not a Git worktree or an execution sandbox. | Foundry visual artifacts and UI state |

Foundry remains the orchestration authority. Git owns checkout mechanics, while the central Foundry store owns registration, binding, policy approval, workflow revision, predecessor, event, plan and receipt lineage.

## Central Store Requirement

Use one absolute store path for every command in the lifecycle:

```bash
FOUNDRY_WORKTREE_STORE=/absolute/path/to/foundry.sqlite
foundry --store "$FOUNDRY_WORKTREE_STORE" worktree list --output json
```

Do not rely on the default relative `.foundry/foundry.sqlite` while changing directories or starting detached work. Do not put the central store inside a disposable worktree. Sandbox execution exports its resolved host path as `FOUNDRY_STORE_PATH`. A `process` child can use that locator to retain lineage; Bubblewrap receives it only as metadata because the external store is intentionally not mounted in the guest. Foundry interaction for an isolated payload must therefore happen through the parent workflow, not by creating or opening a second store inside the checkout.

## End-To-End Flow

### 1. Discover Or Create A Worktree

Discovery is read-only:

```bash
foundry --store "$FOUNDRY_WORKTREE_STORE" worktree discover \
  --repository /absolute/path/to/repository \
  --output json
```

Creation runs `git worktree add -b`. It requires explicit repository-mutation authorization, a new valid branch name and a destination that does not already exist:

```bash
foundry --store "$FOUNDRY_WORKTREE_STORE" worktree create \
  --repository /absolute/path/to/repository \
  --path /absolute/path/to/worktrees/feature-preview \
  --branch feature/preview \
  --start-point main \
  --allow-repository-mutation \
  --output json
```

To adopt an existing Git worktree without claiming that Foundry created it:

```bash
foundry --store "$FOUNDRY_WORKTREE_STORE" worktree register \
  --path /absolute/path/to/existing-worktree \
  --output json
```

`create` reports `created_by_foundry=true`; `register` reports it as false unless the worktree was already known as Foundry-created.

### 2. Initialize The Manifest

Initialization writes `.foundry/worktree.toml` and creates `.foundry/sandboxes/internal/{artifacts,cache,tmp,home}`. It is blocked unless worktree writes are explicitly authorized:

```bash
foundry --store "$FOUNDRY_WORKTREE_STORE" worktree init \
  --worktree <worktree-id-or-path> \
  --allow-worktree-write \
  --output json
```

If the manifest exists, initialization fails. `--force` permits replacement and should only be used after reviewing the current file.

`init` records the exact generated manifest SHA-256 as approved. Any later edit changes that hash and blocks modification checks and sandbox plans until an operator approves the new policy:

```bash
foundry --store "$FOUNDRY_WORKTREE_STORE" worktree approve-config \
  --worktree <worktree-id-or-path> \
  --allow-guardrail-update \
  --approved-by <operator-id> \
  --origin foundry_cli \
  --output json
```

`--approved-by` and `--origin` are recorded provenance labels; this local command does not authenticate the asserted identity. Review the full manifest before authorizing its current hash.

### 3. Bind The Worktree

`foundry plan --worktree` and `foundry request start --worktree` accept either a registered worktree id or an existing path. A path is registered and bound; an id is bound directly:

```bash
foundry --store "$FOUNDRY_WORKTREE_STORE" plan \
  --goal "Implement and validate the preview" \
  --worktree <worktree-id-or-path> \
  --output json

foundry --store "$FOUNDRY_WORKTREE_STORE" request start \
  --goal "Run the validated work asynchronously" \
  --worktree <worktree-id-or-path> \
  --origin codex \
  --output json
```

An existing workflow can be bound explicitly. Add `--task` when only one task should use that worktree:

```bash
foundry --store "$FOUNDRY_WORKTREE_STORE" worktree bind \
  --worktree <worktree-id-or-path> \
  --workflow <workflow-id> \
  --task <task-id> \
  --origin foundry_cli \
  --output json
```

A task-specific binding takes precedence over the workflow-level default. Rebinding the same workflow/task scope removes that scope from the previously selected worktree and records a workflow revision.

### 4. Check Intended Modifications

Before an executor writes files, evaluate every intended file or directory against the approved manifest:

```bash
foundry --store "$FOUNDRY_WORKTREE_STORE" worktree guard check \
  --worktree <worktree-id-or-path> \
  --operation modify \
  --path src/lib.rs \
  --path tests/worktree_contract.rs \
  --reason "Implement and verify the worktree contract" \
  --workflow <workflow-id> \
  --task <task-id> \
  --output json
```

`check` is read-only and exits successfully only when every path is allowed. It returns one decision per path, the matched modifiable/protected scopes, `delegable_to_predecessor`, `current_task_action`, and remediation commands. It is a Foundry policy gate, not an operating-system filesystem enforcement layer; callers must run it before performing the write.

The remediation depends on the blocker:

- an unapproved manifest hash returns `approve-config` as the next command and does not propose a predecessor;
- a protected path, or a path outside `modifiable_paths`, returns an objective `required_task_spec` and a `guard create-predecessor` command;
- a binding mismatch, unsafe path or invalid manifest must be corrected directly and is not delegable as a protected-path task.

Creating the predecessor is a separate, explicit workflow mutation:

```bash
foundry --store "$FOUNDRY_WORKTREE_STORE" worktree guard create-predecessor \
  --worktree <worktree-id-or-path> \
  --workflow <workflow-id> \
  --task <current-task-id> \
  --path .foundry/worktree.toml \
  --goal "Update .foundry/worktree.toml for the approved sandbox policy and validate the resulting hash" \
  --allow-workflow-mutation \
  --approved-by <operator-id> \
  --origin foundry_cli \
  --output json
```

Foundry creates a pending, path-specific predecessor with a validation rule, adds it as a dependency, marks the current task `Blocked`, and records a workflow revision and event. When the predecessor is completed through the validated executor-response path and all dependencies are satisfied, Foundry returns the current task to `Pending`, sets `backlog_state=ready_after_worktree_guard_predecessor`, and removes the guard impediment. The command does not edit the protected path, approve a changed manifest, execute a sandbox or complete either task. If predecessor work changes `.foundry/worktree.toml`, review and approve that resulting hash before the dependent task performs another guard check or sandbox plan.

### 5. Plan Before Execution

The plan is non-executing and should always be inspected before `run`:

```bash
foundry --store "$FOUNDRY_WORKTREE_STORE" worktree sandbox plan \
  --worktree <worktree-id-or-path> \
  --purpose test \
  --workflow <workflow-id> \
  --task <task-id> \
  --output json \
  -- cargo test
```

If no command follows `--`, Foundry uses `sandbox.commands.<purpose>` from the manifest. The command executable basename must still be present in `guardrails.allowed_commands`.

### 6. Run With Explicit Approval

`run` evaluates the same plan again and records a receipt even when execution is blocked. Real execution requires `--allow-exec`:

```bash
foundry --store "$FOUNDRY_WORKTREE_STORE" worktree sandbox run \
  --worktree <worktree-id-or-path> \
  --purpose test \
  --workflow <workflow-id> \
  --task <task-id> \
  --allow-exec \
  --output json \
  -- cargo test
```

The receipt records the binding, runtime, config and command hashes, timing, timeout state, exit code, and bounded stdout/stderr. `execution_attempted` reports whether Foundry reached the launcher, while `executed` reports whether the payload child actually started; a launch failure therefore remains structured evidence instead of disappearing as a CLI-only error. Optional `error` and captured stream content are secret-sanitized, and each stream reports `redaction_count`. Full stream hashes cover all bytes even when displayed content is truncated or sanitized. Foundry reloads the manifest immediately before launch and rejects execution if its SHA-256 changed after planning.

### 7. Manage A Persistent Preview

For a long-running preview, `start` evaluates the same approved plan and requires the same explicit execution authorization as `run`. It persists `foundry.worktree.sandbox_lifecycle.v1` in the central store and returns a stable `sandbox_id` with the current startup/running state:

```bash
foundry --store "$FOUNDRY_WORKTREE_STORE" worktree sandbox start \
  --worktree <worktree-id-or-path> \
  --purpose preview \
  --workflow <workflow-id> \
  --task <task-id> \
  --allow-exec \
  --output json \
  -- <long-running-preview-command>

foundry --store "$FOUNDRY_WORKTREE_STORE" worktree sandbox status \
  --sandbox <sandbox-id> \
  --output json

foundry --store "$FOUNDRY_WORKTREE_STORE" worktree sandbox stop \
  --sandbox <sandbox-id> \
  --allow-stop \
  --output json
```

`status` requires no mutation approval and never launches a command. It does reconcile stale runtime state: when a persisted supervisor is dead, Foundry kills the payload process group and every tracked descendant before atomically recording `sandbox_execution_failed`. `stop` requires `--allow-stop`, records the stop request, terminates the payload process group plus tracked descendants, waits for the supervisor to persist the terminal state, and forcibly terminates a supervisor that does not cooperate. It returns `sandbox_stopped` only after the managed processes are gone. Natural completion, launch failure and timeout also become persistent lifecycle states linked to the normal sandbox receipt. A timeout is failure even when the direct shell happens to report exit code zero.

The MCP surface is contract-equivalent: `foundry.worktree.sandbox.plan`, `foundry.worktree.sandbox.run`, `foundry.worktree.sandbox.start`, `foundry.worktree.sandbox.status` and `foundry.worktree.sandbox.stop`. Execution inputs use `allow_exec`; stop uses `allow_stop`; lifecycle lookup uses the canonical `sandbox_id` field. MCP does not bypass manifest approval, binding checks or explicit execution/termination authorization.

## Manifest Contract

`foundry worktree init` writes schema `foundry.worktree.config.v1`. A practical manifest is:

```toml
schema_version = "foundry.worktree.config.v1"

[guardrails]
require_clean = false
allow_detached_head = false
allowed_branches = ["feature/*"]
allowed_commands = ["cargo", "pnpm"]
modifiable_paths = ["."]
protected_paths = [".git/", ".foundry/worktree.toml"]
require_workflow_binding = true
max_command_seconds = 900
max_output_bytes = 1048576

[sandbox]
enabled = true
name = "internal"
root = ".foundry/sandboxes/internal"
runtime = "process"
working_directory = "."
purposes = ["preview", "test"]
network = "inherit"
inherit_environment = ["PATH", "HOME", "LANG", "CARGO_HOME", "RUSTUP_HOME"]

[sandbox.commands]
preview = ["cargo", "check"]
test = ["cargo", "test"]

[sandbox.environment]
CI = "true"
```

The manifest is human-readable policy, not a secret store. Do not place credentials in `sandbox.environment`.

### Path Guardrails

- `--repository`, `--path` and registered selectors are resolved to concrete Git worktree state before persistence.
- `--branch` is validated through Git, and `--start-point` cannot be empty or start with `-`.
- A create destination must not exist. The CLI does not impose a global worktree parent directory, so the operator must choose the intended absolute destination.
- `sandbox.root` and `sandbox.working_directory` must be relative to the worktree. Absolute paths, `..`, root prefixes and other escape components are rejected.
- Modification scopes are also relative. `modifiable_paths=["."]` allows the worktree generally, while `protected_paths` always takes precedence.
- A scope ending in `/` covers that directory and its descendants; a scope without `/` names one file. A broad directory request is blocked when it contains any protected scope, so pass the narrow paths an operation actually intends to change.
- The generated policy protects `.git/` and `.foundry/worktree.toml`. Changing the manifest invalidates its approval hash; review it and run `approve-config` before checking modifications or planning a sandbox again.
- Existing symlink components, absolute paths and parent traversal are rejected instead of being resolved outside the worktree.
- A worktree selector can be its registered id or its canonical filesystem path.
- A custom worktree id may contain only ASCII letters, digits, `_` and `-`.
- Branch allowlists support `*` wildcard matching. Command allowlists compare the executable basename, not the entire argument string.

## Modification Guard Decisions

`worktree guard check` emits `foundry.worktree.modification_guard.v1`. Its `allowed` value is true only when the manifest hash is approved, the requested workflow/task resolves to this worktree, every path is safely contained, every path matches a modifiable scope, and no path overlaps a protected scope. Each path decision exposes whether its denial is `delegable_to_predecessor`; this is true only for a scope denial after config, binding and containment checks pass. A blocked check exits non-zero without mutating either the filesystem or workflow.

`worktree guard create-predecessor` emits `foundry.worktree.predecessor_task.v1` only when every blocked path is delegable; a mixed set containing any config, binding or containment denial is rejected atomically. It requires an objective goal of at least 20 characters that names a blocked file or directory, `--allow-workflow-mutation`, and `--approved-by`. The asserted approver and `--origin` are stored in the report/event; they are audit provenance rather than identity authentication.

## Sandbox Guardrail Decisions

Every sandbox plan returns explicit decisions and a `blockers` list. Execution is allowed only when all of these gates pass:

- manifest present and schema supported;
- current manifest hash explicitly approved;
- sandbox enabled;
- purpose is `preview` or `test` and is allowed by the manifest;
- command is present and allowlisted;
- command arguments contain no detected inline secret;
- clean-worktree policy passes;
- branch or detached-HEAD policy passes;
- required workflow/task binding is unambiguous;
- runtime is `process` or `bubblewrap` and is available;
- network policy is `inherit` or `deny` and is enforceable by the selected runtime;
- configured timeout/output limits do not exceed the administrative ceilings of 3,600 seconds and 16 MiB.

A missing or unapproved manifest, disallowed command, dirty worktree under `require_clean=true`, missing binding, unsupported runtime, unavailable `bwrap`, or unenforceable network policy produces `status=sandbox_blocked`, `allowed=false`, and no child execution.

## Isolation Boundary

| Runtime | Filesystem enforcement | Network enforcement | Intended use |
| --- | --- | --- | --- |
| `process` | `false` | `false` | Bounded cwd, explicit environment, timeout, output limits and receipts. It is not a security boundary. |
| `bubblewrap` + `network="inherit"` | `true` | `false` | Minimal read-only runtime mounts and a read-only worktree; only the internal sandbox root is writable. Host networking remains available. |
| `bubblewrap` + `network="deny"` | `true` | `true` | The same filesystem boundary plus a separate network namespace. |

The `process` runtime always reports `filesystem_isolation_enforced=false` and `network_isolation_enforced=false`. Setting `network="deny"` with `process` blocks the plan because a normal child process cannot enforce that policy.

The `bubblewrap` runtime requires `bwrap` on `PATH`. Foundry mounts only required runtime directories (`/usr`, available binary/library paths and selected `/etc` files) read-only, mounts the host worktree read-only at `/workspace`, and rebinds only its configured internal sandbox root writable at the matching `/workspace/<relative-sandbox-root>`. It binds `sandbox/tmp` to `/tmp` and `sandbox/home` to `/home/foundry`, then creates process/IPC/UTS namespaces; `network="deny"` adds a network namespace.

The plan makes host and guest paths explicit:

| Plan/environment value | `process` | `bubblewrap` |
| --- | --- | --- |
| `worktree_root`, `sandbox_root`, `working_directory` | Host paths | Host paths used to construct mounts |
| `runtime_worktree_root` / `FOUNDRY_WORKTREE_ROOT` | Host worktree path | `/workspace` |
| `runtime_sandbox_root` / `FOUNDRY_SANDBOX_ROOT` | Host sandbox path | `/workspace/<relative-sandbox-root>` |
| `runtime_working_directory` | Host working path | Matching path below `/workspace` |
| `FOUNDRY_STORE_PATH` | Usable host locator | Host locator metadata; store is not mounted |
| `foundry_store_path_mounted` | `true` | `false` |

Build tools must direct caches and outputs into `runtime_sandbox_root`. Payload executables and runtimes must already exist in the read-only system mounts; host-home toolchains are not exposed. When `foundry_store_path_mounted=false`, nested Foundry mutations must be performed outside Bubblewrap by the parent workflow. Docker, Kubernetes and Knative remain separately authorized asynchronous substrates; this command neither installs nor mutates them.

## Blocking Task Flow

The safe workflow order is:

```text
register/create worktree
→ initialize and review manifest
→ approve the current manifest hash if it was edited
→ bind workflow/task
→ guard check every intended modification
→ create and complete an approved predecessor for any delegable protected scope
→ approve the resulting manifest hash again if that predecessor changed it
→ sandbox plan returns allowed=true
→ sandbox run returns sandbox_completed with exit_code=0, or a persistent preview is started, inspected and stopped explicitly
→ hand off or promote the dependent implementation task
```

The modification guard creates a predecessor only when the operator explicitly invokes `create-predecessor`. The current task remains `Blocked` while that path-specific task is incomplete. Validated completion automatically returns the dependent task to `Pending` only after all of its dependencies are complete.

Separately, model preview/test as a blocking predecessor when later work must not proceed without its receipt. Keep that validation task pending or in rework while the plan has blockers, the run is not explicitly approved, the command times out, or the receipt exits non-zero. Attach or otherwise reference the successful receipt as validation evidence before promoting the dependent task.

The current worktree binding automatically routes the selected root and approved config state into context and handoff packets, and sandbox guardrails automatically block sandbox execution. Neither a binding nor a receipt injects an arbitrary dependency or completes a task; only the explicit guard mutation creates the path-specific predecessor, and normal validation rules still govern completion.

## Mutation And Ownership

| Command | Mutation | Required explicit flag |
| --- | --- | --- |
| `worktree discover` | None | None |
| `worktree create` | Creates a Git branch/worktree and registers it | `--allow-repository-mutation` |
| `worktree register` | Writes only central Foundry state | None |
| `worktree bind` | Writes central Foundry state and a workflow revision | None |
| `worktree init` | Writes the manifest/internal directories and approves the generated hash | `--allow-worktree-write` |
| `worktree approve-config` | Approves the current manifest hash in central Foundry state | `--allow-guardrail-update` and `--approved-by` |
| `worktree guard check` | None; returns a policy decision | None |
| `worktree guard create-predecessor` | Adds a task/dependency, blocks the current task and records a workflow revision/event | `--allow-workflow-mutation` and `--approved-by` |
| `worktree sandbox plan` | Records no child execution | None |
| `worktree sandbox run` | Starts the configured child command | `--allow-exec` |
| `worktree sandbox start` | Persists a supervised preview/test lifecycle and starts its payload process group | `--allow-exec` |
| `worktree sandbox status` | Reads one persisted lifecycle and reconciles a dead supervisor fail-closed | None |
| `worktree sandbox stop` | Records a stop request and terminates the persisted payload group and tracked descendants | `--allow-stop` |

The current CLI has no worktree removal command. Foundry therefore does not delete registered or external worktrees through this surface. Use normal Git administration deliberately, and preserve the central record/receipt history needed for audit.

## Inspection And Schemas

```bash
foundry --store "$FOUNDRY_WORKTREE_STORE" worktree list --output json
foundry --store "$FOUNDRY_WORKTREE_STORE" worktree list --workflow <workflow-id> --output json
foundry --store "$FOUNDRY_WORKTREE_STORE" worktree inspect --worktree <worktree-id-or-path> --output json
```

Versioned contracts include:

- `foundry.worktree.config.v1`;
- `foundry.worktree.discovery.v1`;
- `foundry.worktree.record.v1`;
- `foundry.worktree.binding.v1`;
- `foundry.worktree.modification_guard.v1`;
- `foundry.worktree.predecessor_task.v1`;
- `foundry.worktree.sandbox_plan.v1`;
- `foundry.worktree.sandbox_receipt.v1`;
- `foundry.worktree.sandbox_lifecycle.v1`.

Context and executor handoff include the effective `worktree` object with repository/worktree roots, branch, HEAD, dirty state, config status/path/SHA-256, approval state and approved hash, guardrails, sandbox settings, project settings and bindings. This lets a receiving brain verify the execution workspace instead of inferring it from process cwd.

See [the runnable example](../examples/worktree-sandbox/README.md) for a compact operator walkthrough.
