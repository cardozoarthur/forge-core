# Operational report — Git worktrees and internal preview/test sandboxes

## Outcome

Forge now treats Git worktrees as explicit execution workspaces while keeping Git authoritative for checkout state and the central SQLite store authoritative for workflow bindings, approved guardrail revisions, lifecycle state and receipts. Preview/test commands can use a bounded process runtime or a Bubblewrap boundary with a read-only worktree, an isolated writable sandbox and optional network denial.

The internal Rust module formerly named `harness` is now `cli_integration`, which describes its actual responsibility without implying that one integration layer is the whole product. The public `forge harness` CLI and `forge.harness.*` MCP names remain compatibility contracts.

## Reproducible version

The public source is `https://github.com/cardozoarthur/forge-core` on branch `agent/worktree-sandbox-hardening`. The reproducible revision is the Git commit containing this report; resolve it after checkout with `git log -1 --format=%H -- docs/reports/worktree-sandbox-hardening-2026-07-22/operational-report.md`.

The implementation is split across purpose-specific modules:

- `src/worktree.rs`: discovery, registration, manifest policy, binding, guard decisions, sandbox plans/receipts, lifecycle supervision, process cleanup and output redaction;
- `src/storage.rs`: atomic worktree, lifecycle, event and receipt persistence;
- `src/context.rs` and `src/handoff.rs`: task/workflow worktree policy routed into bounded executor context;
- `src/security.rs`: command, environment and captured-output secret detection/redaction;
- `src/main.rs` and `src/mcp.rs`: CLI and contract-equivalent MCP surfaces;
- `src/cli_integration.rs`: the renamed native-CLI adoption layer.

## Operator replay

Use one absolute store outside disposable worktrees throughout the lifecycle:

```bash
FORGE_WORKTREE_STORE=/absolute/path/to/forge.sqlite
REPOSITORY=/absolute/path/to/repository
WORKTREE=/absolute/path/to/worktrees/feature-preview
```

Discover existing checkouts, create a new Git worktree with explicit repository-mutation authorization, or register a pre-existing checkout:

```bash
forge --store "$FORGE_WORKTREE_STORE" worktree discover \
  --repository "$REPOSITORY" \
  --output json

forge --store "$FORGE_WORKTREE_STORE" worktree create \
  --repository "$REPOSITORY" \
  --path "$WORKTREE" \
  --branch feature/preview \
  --start-point main \
  --allow-repository-mutation \
  --output json

forge --store "$FORGE_WORKTREE_STORE" worktree register \
  --path "$WORKTREE" \
  --output json
```

Initialize the human-readable policy. `init` records the generated manifest hash as approved; run `approve-config` only after reviewing an intentional edit, so central state approves the exact resulting SHA-256:

```bash
forge --store "$FORGE_WORKTREE_STORE" worktree init \
  --worktree "$WORKTREE" \
  --allow-worktree-write \
  --output json

forge --store "$FORGE_WORKTREE_STORE" worktree approve-config \
  --worktree "$WORKTREE" \
  --allow-guardrail-update \
  --approved-by <operator-id> \
  --origin forge_cli \
  --output json
```

Bind the approved worktree to a workflow or a narrower task scope, then check every intended modification before writing:

```bash
forge --store "$FORGE_WORKTREE_STORE" worktree bind \
  --worktree "$WORKTREE" \
  --workflow <workflow-id> \
  --task <task-id> \
  --origin codex \
  --output json

forge --store "$FORGE_WORKTREE_STORE" worktree inspect \
  --worktree "$WORKTREE" \
  --output json

forge --store "$FORGE_WORKTREE_STORE" worktree list \
  --workflow <workflow-id> \
  --output json

forge --store "$FORGE_WORKTREE_STORE" worktree guard check \
  --worktree "$WORKTREE" \
  --operation modify \
  --path src/lib.rs \
  --path tests/worktree_contract.rs \
  --reason "Implement and validate the worktree contract" \
  --workflow <workflow-id> \
  --task <task-id> \
  --output json
```

For a protected or non-modifiable scope, use the returned objective task specification. Creating the predecessor is a separate authorized mutation; it does not edit the path:

```bash
forge --store "$FORGE_WORKTREE_STORE" worktree guard create-predecessor \
  --worktree "$WORKTREE" \
  --workflow <workflow-id> \
  --task <blocked-task-id> \
  --path .forge/worktree.toml \
  --goal "Update .forge/worktree.toml for the approved sandbox policy and validate the resulting hash" \
  --allow-workflow-mutation \
  --approved-by <operator-id> \
  --origin forge_cli \
  --output json
```

Inspect a non-executing sandbox plan before explicitly authorizing a bounded test:

```bash
forge --store "$FORGE_WORKTREE_STORE" worktree sandbox plan \
  --worktree "$WORKTREE" \
  --purpose test \
  --workflow <workflow-id> \
  --task <task-id> \
  --output json \
  -- cargo test

forge --store "$FORGE_WORKTREE_STORE" worktree sandbox run \
  --worktree "$WORKTREE" \
  --purpose test \
  --workflow <workflow-id> \
  --task <task-id> \
  --allow-exec \
  --output json \
  -- cargo test
```

Proceed only when the plan reports `status=sandbox_ready`, `allowed=true` and no blockers. A successful run reports `status=sandbox_completed`, `executed=true`, `exit_code=0`, `timed_out=false` and `error=null`; retain its config, binding, command and receipt hashes as validation evidence.

Manage a persistent preview by its stable lifecycle identifier:

```bash
forge --store "$FORGE_WORKTREE_STORE" worktree sandbox start \
  --worktree "$WORKTREE" \
  --purpose preview \
  --workflow <workflow-id> \
  --task <task-id> \
  --allow-exec \
  --output json \
  -- <long-running-preview-command>

forge --store "$FORGE_WORKTREE_STORE" worktree sandbox status \
  --sandbox <sandbox-id> \
  --output json

forge --store "$FORGE_WORKTREE_STORE" worktree sandbox stop \
  --sandbox <sandbox-id> \
  --allow-stop \
  --output json
```

Poll the JSON lifecycle state, not only the CLI exit code: `status` deliberately exits successfully after returning an inspected terminal failure. Accept the receipt only after checking its structured state and exit/timeout/error fields.

Equivalent MCP tools are `forge.worktree.sandbox.plan`, `.run`, `.start`, `.status` and `.stop`. A task-scoped MCP request must include `workflow_id`; execution and stop retain their `allow_exec` and `allow_stop` authorization gates.

## Minimal approved manifest

`forge worktree init` writes `.forge/worktree.toml` with schema `forge.worktree.config.v1`. A practical policy is:

```toml
schema_version = "forge.worktree.config.v1"

[guardrails]
require_clean = false
allow_detached_head = false
allowed_branches = ["feature/*"]
allowed_commands = ["cargo"]
modifiable_paths = ["src/", "tests/"]
protected_paths = [".git/", ".forge/worktree.toml"]
require_workflow_binding = true
max_command_seconds = 900
max_output_bytes = 1048576

[sandbox]
enabled = true
name = "internal"
root = ".forge/sandboxes/internal"
runtime = "bubblewrap"
working_directory = "."
purposes = ["preview", "test"]
network = "deny"
inherit_environment = ["PATH", "LANG", "CARGO_HOME", "RUSTUP_HOME"]

[sandbox.commands]
test = ["cargo", "test"]

[sandbox.environment]
CI = "true"
```

The manifest is policy, not a secret store. Any edit invalidates the approved SHA-256; review and approve the new content before the next guard check or sandbox plan.

## Guardrail and dependency semantics

- Protected paths override modifiable paths. Absolute paths, parent traversal, root prefixes and existing symlink escape components are rejected.
- `guard check` is read-only. It returns one decision per path, the matching scopes, whether the denial is delegable, the current-task action and exact remediation commands.
- Only an explicitly authorized `guard create-predecessor` mutation creates a path-specific task and dependency. It atomically blocks the current task and records approval/origin lineage.
- Validated predecessor completion returns the dependent task to `Pending` only after every dependency is done. If predecessor work changes the manifest, the resulting hash requires a new approval.
- A sandbox receipt does not complete arbitrary work. When preview/test must gate promotion, model it as a validation predecessor and require `sandbox_completed` with `exit_code=0`.

## Safety and lifecycle model

- `process` bounds cwd, environment, time, output and evidence, but is not a filesystem or network security boundary.
- `bubblewrap` requires an existing `bwrap` executable, mounts the checkout read-only at `/workspace`, and leaves only the managed sandbox, its home and tmp writable. `network="deny"` creates a separate network namespace; build caches and outputs must target `FORGE_SANDBOX_ROOT`.
- `FORGE_STORE_PATH` remains a host locator and is intentionally not mounted into Bubblewrap. Nested Forge mutations stay with the parent workflow.
- Planning never executes. `run` and `start` require `--allow-exec`; `stop` requires `--allow-stop`.
- Lifecycle mutation uses an immediate SQLite transaction, so concurrent stop/status/supervisor writers cannot each publish conflicting terminal state.
- Status launches nothing. If the persisted supervisor is dead, it kills the payload group and tracked descendants before atomically recording `sandbox_execution_failed`.
- Stop records the request, kills the process group and tracked descendants, waits for cooperative persistence and only returns `sandbox_stopped` after the bounded processes are gone. A clean stop has `error=null`.
- Natural exit, launch failure and timeout remain persisted after the initiating CLI exits. A timeout is always failure even if a shell reports zero.
- Inline secrets detected in command arguments or environment values block the plan before execution. Captured stdout, stderr and error material are redacted with format-aware and high-entropy detection; receipts retain bounded evidence, redaction counts and hashes of the original streams.

## Validation evidence

The required repository gates passed on the final implementation state used for this workflow:

| Gate | Result |
| --- | --- |
| `cargo fmt --check` | PASS |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS |
| `cargo test` | PASS — 735 tests in 12 suites |
| `cargo build --release` | PASS |
| Worktree contract | PASS — 5/5 |
| Modification guard contract | PASS — 9/9 |
| Sandbox lifecycle contract | PASS — 14/14 |

Real smokes also passed for the process runtime, Bubblewrap read-only/network-denied isolation, CLI start plus MCP stop, timeout reconciliation, supervisor-loss reconciliation, `forge plan`, and modular skill installation. See the [validation gate matrix](validation-report.md) and [replayable execution trace](execution-trace.md).

## Workflow evidence

- Workflow: `wf_cf8a1698f266442981479112786f00b0`.
- Tasks: eight dependency-ordered evidence nodes from intent parsing through this operational report.
- Task 007 manifest: `artifact-manifest.json`, containing repository-relative paths and verified SHA-256 values for the six stable source artifacts.
- Task 008 context: revision 15, budget 2,233 bytes, `context_ready=true`, context SHA-256 `40cfd053ed1459a6dd3f477b3fdcfb077ed840cec68ad0852e6aa31b82172a81`.
- Final workflow gate: `forge --store /home/arthur/projects/forge-core/.forge/forge.sqlite validate --workflow wf_cf8a1698f266442981479112786f00b0 --output json`.

## Deliberate boundaries

Forge does not automatically delete worktrees, mutate pre-existing external resources, install Bubblewrap, or install/mutate Docker, Kubernetes or Knative. The `process` runtime is evidence-bounded rather than isolated. Bubblewrap payloads must write caches/build outputs below the managed sandbox, and tools must already exist in the exposed read-only runtime paths.

The [full operator contract](../../worktrees-and-sandboxes.md) and [smaller runnable example](../../../examples/worktree-sandbox/README.md) provide the long-form reference. Forge has no worktree removal command; deliberate Git cleanup remains the repository operator's responsibility.
