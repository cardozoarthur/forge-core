# Forge Core

Forge Core is a high-performance AI-native workflow runtime for transforming large objectives into validated, context-controlled atomic execution graphs.

Forge is not an LLM wrapper and not a human-flow builder. It treats models as interchangeable execution resources and can run workflows that mix AI steps, deterministic non-AI steps, waits/cron and notifications.

The intended architecture is hybrid:

- CLIs such as Codex, OpenCode and Gemini CLI can call Forge directly for simpler adoption.
- Forge can also call those CLIs as bounded execution engines for long-running tasks.
- Native integrations/plugins are useful when they make the developer experience simpler, but the operational authority remains in Forge: graph state, context routing, retries, validation, scheduling, costs and persistence.

- decomposition;
- scheduling;
- context routing;
- validation;
- retries;
- artifact persistence;
- operational memory;
- controlled self-improvement.

## Status

Current version: `0.4.118`

This is the first functional CLI + Skill version:

- Rust CLI binary: `forge`
- SQLite persistence
- deterministic atomic task graph generation
- versioned, sharded bounded context package generation with subflow-aware routing
- strict context readiness gates for executor handoff
- validation gates
- simulated execution runtime
- autonomous mixed AI/non-AI workflow planning
- native cron/wait task representation with timezone, next-run, missed-run policy, run history and scale-to-zero metadata
- explicit loop primitives for loop-over-items, bounded repeat, retry/backoff, while/until and infinite recurring subflow semantics
- notification payloads with final workflow cost reporting
- artifact listing
- workflow registry listing with lifecycle state and `running`/`non-running` filters
- workflow registry context-action catalog discovery for handoff/resume/retry filters
- workflow registry per-task context-action refs for handoff/resume/retry triage
- workflow registry quality-action catalog discovery for Context Routing Engine triage filters
- terminal workflow DAG inspection with lifecycle, dependency, persona, context-route, execution-policy, next-action, focused task views and recursive child-subflow annotations
- handoff readiness summaries in workflow inspection and async request status
- proposed child-subflow links for compatible deterministic code-node reuse
- revisioned child-subflow validation gates before workflow promotion
- context routing with deterministic shard manifests, deterministic code-node and long-running cognition goals
- minimum-correct context section receipts for executor adapters and budget repair
- context routing quality scores and warnings for budget pressure, missing required context and profile filtering
- registry-level context quality summaries and workflow `quality_action` recommendations
- Forge-owned execution policy metadata for deterministic local Python/Node.js code nodes
- node-scoped Personality/Soul Routing profiles, metadata and validation gates for human-facing artifacts
- controlled improvement proposal generation
- Codex/OpenCode-compatible `forge-core` skill
- executor sync that detects installed/configured CLIs and persists human authorization policy
- runtime sync that detects Docker/Kubernetes/Knative and persists human authorization policy
- local cluster node registry with capability/trust metadata, dry-run placement decisions and distributed handoff manifests
- cluster registry scheduling posture that exposes per-node active/expired lease pressure without remote execution
- lease-aware cluster placement that exposes active lease counts and prefers idle eligible nodes
- remote AI cluster placement is blocked until explicit authorization enables cluster cognitive executors
- n8n-aware research planning that catalogs workflow primitives and evaluates Forge primitive candidates before graph promotion
- goal-oriented tasks with subtasks, impediments, acceptance criteria and rework readiness checks
- runtime workflow mutation for goals and artifacts with origin trace from `codex`, `opencode`, `forge_cli` or skills
- async workflow substrate policy with scope guards for Forge-owned resources
- async request handoff for skill callers: submit a goal, receive `run_id`, continue later with Forge
- MCP tool manifest and call surface for agent workflows: list, inspect, start/resume/status, schedule create/update/list/summary, loop inspect/summary, task handoff, context request, validation status and bounded artifact fetch
- 0.5 milestone status and promotion manifest surfaces for release-gate inspection
- native daily Goal research workflow planning and smoke execution for `hackathon` reports with Markdown/PDF artifacts and redacted Telegram delivery records
- persisted task leases so two executors cannot acquire the same workflow task concurrently
- executor handoff packets that combine strict context readiness, lease metadata, routing cache keys, checksums and validation gates
- cluster handoff packets that choose an eligible node, lease the task to that node and return a content-addressed sync manifest without remote execution
- cluster sync manifests with deterministic manifest-level SHA-256 checksums for distributed handoff auditing
- executor response validation for adapter outputs before Forge accepts completion evidence
- self-evolution runner for bounded Codex/OpenCode cycles until a stop date
- versioned self-evolution prompt packets with SHA-256 checksums in cycle reports
- self-evolution prompt packets load the persisted Forge workflow goal before generic guidance, so runtime `workflow update-goal` changes drive future cycles
- self-evolution operating modes (`lean`, `balanced`, `strict`) with overhead ledger and a decision gate that can stop terminal goals or reject low-value bloat cycles
- versioned improvement artifacts with strong changelog generation

## Install

```bash
cargo install --path .
```

## CLI Quickstart

```bash
forge plan --goal "Create a delivery platform" --output json
```

Use the returned `workflow_id`:

```bash
forge list --output json
forge list --lifecycle running --output json
forge list --lifecycle non-running --output json
forge list --context-actions --output json
forge list --context-action wait_for_dependencies --output json
forge list --quality-actions --output json
forge list --quality-action increase_context_budget --output json
forge inspect <workflow-id> --verbose --output json
forge inspect <workflow-id> --task task-008 --verbose --output json
forge status --workflow <workflow-id> --output json
forge workflow validate-subflow --workflow <workflow-id> --task task-011 --child-workflow <child-workflow-id> --child-task task-011 --origin codex --output json
forge schedule create-daily-goal-research --goal hackathon --timezone America/Sao_Paulo --cron "0 8 * * *" --origin codex --output json
forge schedule update --workflow <workflow-id> --task task-009 --cron "0 8 * * *" --timezone America/Sao_Paulo --next-run-at 2026-05-26T11:00:00Z --origin codex --output json
forge schedule pause --workflow <workflow-id> --task task-010 --origin codex --output json
forge schedule resume --workflow <workflow-id> --task task-010 --origin codex --output json
forge schedule run-due --workflow <workflow-id> --output json
forge task validate-response --workflow <workflow-id> --task task-001 --response ./executor-response.json --output json
forge context --workflow <workflow-id> --task task-001 --budget 1200 --output json
forge run --workflow <workflow-id> --simulate --output json
forge validate --workflow <workflow-id> --output json
forge improve --workflow <workflow-id> --output json
forge artifacts --workflow <workflow-id> --output json
forge milestone manifest --version 0.5 --output json
```

`forge context` emits a versioned context packet (`forge.context.v30`) with a deterministic
`task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30` routing policy.
The packet keeps the legacy `content` body for executors, and also returns workflow
revision, artifact count, persona routing metadata for human-facing nodes, a versioned
persona profile and persona contract, executor profile metadata, a versioned routing contract, execution policy metadata, dependency readiness summaries, proposed
child-subflow bindings, lineage hashes and a shard manifest with included/omitted sections, profile exclusions,
compression flags, required/missing-required markers, source labels, priorities,
content-addressed shard IDs, source hashes, remaining-budget before/after values,
byte counts, minimum-routable byte counts, per-shard selected-cost basis points,
selection savings, summaries and SHA-256 checksums. The packet also exposes `context_ready`,
`required_sections`, `missing_required_sections`, `handoff_ready`, `handoff_status`,
`handoff_blockers`, a `routing_summary`, a versioned `routing_contract`, a versioned
`routing_repair` budget recommendation, a versioned `routing_quality` score/warning contract,
`minimum_correct_set` section receipt, a node-scoped `persona_profile` and `persona_contract` for human-facing artifacts, a versioned `next_action`
resume/handoff decision, a versioned `routing_economy` ledger with selected, compressed,
omitted and no-AI model-call avoidance metrics, a versioned `prompt_packet`
contract for executor adapters, a versioned `replay_manifest`, a versioned
`continuation_plan` for checkpoint resume/refresh/partial-retry decisions, and a versioned `routing_fingerprint`
with component hashes and a cache key so executor adapters can reuse or invalidate
bounded context without reparsing full packets. Adapters can block handoff when the
minimum correct context was omitted or dependency tasks are not ready.
`minimum_correct_set` lists every required section with its inclusion/compression state,
selected and original byte counts, hashes, routing decision and repair action, so adapters
can audit the exact missing floor without re-deriving it from the full shard manifest.
Deterministic
command and wait nodes receive a smaller no-AI context
envelope that preserves local objective, execution policy, proposed subflow reuse and
validation context before lower-priority narrative sections, while AI and mixed nodes
keep richer reasoning context. When a goal explicitly calls for repeated local Python
or Node.js work without AI, Forge marks the deterministic step as a `local_code_node`,
records the selected runtime and routes that policy into the task context without
executing external code during planning. If the registry attaches a compatible child
subflow, the context packet carries both structured `child_subflows` metadata and a
compact `child_subflows` shard so the executor sees Forge's reuse decision without
reconstructing it from history. Runtime goal, artifact and persona routing state are
included in the context lineage so executors can detect stale context before resuming
work.
The packet also includes a versioned `replay_manifest` with the minimal replay
command, selector version, route budget, context checksum and shard refs. The prompt
packet binds the replay manifest checksum, and inspection projects the same checksum,
so long-running executor adapters can pause, compare and resume against the exact
context route without reparsing unrelated packet fields.

`forge inspect --output json` projects compact `context_route` and `execution_policy`
contracts for every DAG node and expands proposed child-subflow links into auditable
path metadata.
The route reuses the same versioned context package and includes the executor profile,
effective budget, context checksum, routing fingerprint schema, routing cache key,
lineage hash, handoff status, resume status, missing required sections and routing
summary. It also reuses the context packet's versioned `next_action` projection
(`forge.inspect_context_action.v1`) so operators can see whether a node should start
handoff, wait for dependencies, raise context budget, refresh stale context or retry
from a checkpoint with fresh context. The execution policy projection
(`forge.inspect_execution_policy.v1`) exposes the mode, AI allowance, deterministic
flag, reuse hint, selection reason, validation gate and optional local code runtime
fields before a handoff packet is requested. Human terminal diagrams also show the
profile, handoff state, selected/effective context bytes, short routing cache key,
next action and compact execution policy for each node. When a node has proposed
child subflows, inspection also reports each subflow's parent node, depth, path,
reachability, terminal status and loaded child workflow/task counts so operators can
audit recursive reuse without executing or promoting the child flow.
Use `forge inspect <workflow-id> --task <task-id>` when an operator or adapter needs a
bounded terminal view of one node. Focused inspection keeps the same context-route,
persona, execution-policy, handoff and child-subflow projections, adds a `focus`
block and `workflow_task_count`, and limits the node list, handoff summary and
terminal diagram to the selected task.

Use strict context mode when handing a package to an executor:

```bash
forge context --workflow <workflow-id> --task task-001 --budget 1200 --strict --output json
```

Strict mode still prints the replayable context package, but exits non-zero if
`handoff_ready=false`.

Acquire an executor handoff packet when a bounded adapter is ready to work:

```bash
forge task handoff --workflow <workflow-id> --task task-001 --executor codex --budget 1200 --ttl-seconds 900 --output json
```

The command reuses the strict context readiness contract, acquires a Forge task
lease only when `handoff_ready=true`, and returns `forge.executor_handoff.v8`
with the selected executor, task executor kind, lease id, context SHA-256,
routing fingerprint schema, routing cache key, lineage hash, expected output,
context routing quality, execution policy mode, full execution policy and validation gate. Human-facing
persona nodes also carry a versioned `persona_contract` with the derived profile id,
profile checksum, node-scoped mode, voice, tone, instruction source, source model
summaries, persona validation gate and lineage hashes so adapters do not have to
infer soul/personality routing from the nested context body. The handoff packet
also reuses the context `continuation_plan` as its `resume_plan`, so adapters see
the same validation-gated decision in `forge context`, `forge inspect` and
`forge task handoff`.

Before an adapter result is treated as usable completion evidence, validate its
response contract:

```bash
forge task validate-response --workflow <workflow-id> --task task-001 --response ./executor-response.json --output json
```

The response must use `forge.executor_response.v1`, match the task id, include a
replayable `trace_ref`, report non-negative cost/token values and, when marked
`completed`, include at least one passing validation evidence item. The command is
read-only with respect to task state: it records an audit event and exits non-zero
for rejected responses instead of silently promoting work.

Skill-style async handoff:

```bash
forge request start --goal "Improve Forge Core" --origin codex --output json
forge request status --run <run-id> --output json
forge request resume --run <run-id> --origin codex --output json
```

Codex/OpenCode should prefer this pattern when using Forge as a skill: make a short request, receive a `run_id`, and let Forge own the asynchronous workflow state.
`forge request start` uses the same registry-derived reuse pass as `forge plan`, returning `reuse_candidates`, `attached_subflows` and `forge.agent_handoff_contract.v1` when Forge can attach a compatible deterministic child subflow before persisting the async workflow.
`forge request status` resolves the run id back to the current Forge workflow state, including the current goal, original requested goal, latest revision, artifact count, task status summary and context handoff summary for every task.
The handoff summary includes aggregate routing quality counts and each task's quality contract, so async callers can distinguish dependency waits from context budget/profile pressure without opening full context packets.
`forge list` exposes the workflow registry across planned and async workflows, including stable workflow ids, associated run ids, initial request, current goal, lifecycle state, task summary, execution-policy route counts and deterministic code-node subflows that can be reused by compatible future workflows. Completed finite workflows are projected as `scaled_to_zero` when there is no remaining task work. Operators can use `forge list --context-actions` to discover valid handoff/resume/retry filter values, then combine lifecycle slices with `--context-action <action>` to find workflows whose next context route includes a specific handoff action such as `wait_for_dependencies`, `increase_context_budget` or `partial_retry_with_fresh_context`. Each registry row also includes `context_action_refs`, a per-task list with the task id, title, executor, next action, handoff status, blocker refs, checkpoint refs and current routing cache key, so operators can jump directly from a filtered registry row to the affected tasks without opening a full inspection first.

Agent-facing MCP surface:

```bash
forge mcp tools --output json
forge mcp call forge.run.start --input '{"goal":"Improve Forge Core","origin":"codex"}' --output json
forge mcp call forge.run.status --input '{"run_id":"<run-id>"}' --output json
forge mcp call forge.run.resume --input '{"run_id":"<run-id>","origin":"opencode"}' --output json
forge mcp call forge.workflow.inspect --input '{"workflow_id":"<workflow-id>","verbose":true}' --output json
forge mcp call forge.context.request --input '{"workflow_id":"<workflow-id>","task_id":"task-001","budget":1200}' --output json
forge mcp call forge.task.handoff --input '{"workflow_id":"<workflow-id>","task_id":"task-001","executor":"codex","budget":1200}' --output json
forge mcp call forge.schedule.create_daily_goal_research --input '{"goals":["hackathon"],"timezone":"America/Sao_Paulo","cron":"0 8 * * *","origin":"codex"}' --output json
forge mcp call forge.schedule.summary --output json
forge mcp call forge.schedule.loop_summary --output json
forge mcp call forge.loop.inspect --input '{"workflow_id":"<workflow-id>"}' --output json
forge mcp call forge.workflow.attach_artifact --input '{"workflow_id":"<workflow-id>","path":"./report.md","kind":"report","origin":"codex"}' --output json
forge mcp call forge.artifact.fetch --input '{"workflow_id":"<workflow-id>","path":"artifacts/<workflow-id>/attached-report-report.md","max_bytes":4096}' --output json
```

The MCP call surface is a stable local adapter layer over the existing Forge CLI and SQLite state. It does not introduce a second source of truth: mutations still flow through Forge-owned workflow, schedule and artifact APIs, validation remains explicit, and artifact reads are bounded to Forge-owned artifact refs.
The registry-level `execution_policy` summary uses schema `forge.registry_execution_policy.v1` and aggregates AI, mixed, deterministic, no-AI, model-call-required, model-call-avoided, local-code and reusable local-code route counts for both the filtered global summary and every workflow row.
The registry also includes compact `context_handoff`, `context_actions` and `context_quality` projections for every workflow and for the filtered global summary, so operators can see ready tasks, missing-context blockers, dependency blockers, routing quality pressure and the workflow-level `quality_action` recommendation without inspecting each task individually.
`forge plan` and `forge request start` report `reuse_candidates` when the registry already contains a compatible reusable deterministic subflow, and persist the best attachable candidate per requested task as a proposed child subflow before duplicating local Python/Node.js work.
`forge inspect <workflow-id>` renders the current DAG as terminal text and also exposes the same graph as structured JSON when `--output json` is used. `--verbose` includes task goals, expected outputs, validation rules, subtasks and proposed child-subflow links. `--task <task-id>` focuses the terminal and JSON inspection on one node while preserving the full workflow task count. Persona-aware nodes are annotated with their node-scoped persona mode, and every inspected node carries the context handoff status and next operational action derived from the same readiness contract used by `forge context --strict`.
`forge workflow validate-subflow` turns a proposed child-subflow binding into a revisioned `validated` binding only when the child workflow/task is present and the child flow is scaled to zero.
`forge validate` blocks promotion when a task declares persona routing that is not node-scoped, auditable, source-model backed and gated by `persona_routing_required`; it also blocks promotion while child-subflow bindings remain proposed, non-promotable or missing validation metadata.

Sync local execution engines before Forge uses external CLIs:

```bash
forge sync executors --home "$HOME" --output json
forge sync executors --home "$HOME" --allow codex --allow opencode --output json
forge executors --output json
```

Forge detects known CLIs, checks whether they appear configured and asks for human authorization when run interactively. A detected CLI is not usable until the policy is explicitly allowed. On this machine, `codex` and `opencode` can be authorized for Forge self-improvement with the second command above.

Sync async run substrates separately:

```bash
forge sync runtimes --home "$HOME" --output json
forge runtimes --output json
```

Forge can detect Docker, Kubernetes and Knative. If Docker and Kubernetes are available but Knative is missing, Forge reports a Knative install suggestion that requires human approval. Forge does not install or mutate infrastructure by itself.

Register LAN or SSH-reachable cluster nodes before scheduling distributed work:

```bash
forge cluster register \
  --node-id lan-linux-ai \
  --name "LAN Linux AI Worker" \
  --endpoint ssh://forge@lan-linux \
  --os linux \
  --arch x86_64 \
  --cpu-cores 16 \
  --memory-gb 64 \
  --software python3 \
  --capability python \
  --python \
  --network-reachable \
  --status online \
  --trust trusted_lan \
  --sandbox local_process_no_network \
  --output json
forge cluster list --output json
forge cluster place --workflow <workflow-id> --task task-009 --output json
forge cluster handoff --workflow <workflow-id> --task task-001 --budget 1200 --ttl-seconds 900 --output json
forge cluster leases --output json
forge cluster leases --node-id lan-linux-ai --output json
```

The cluster registry records reported CPU, memory, OS, GPUs, installed software,
Python/Node/Docker/GPU availability, network reachability, status,
cost/latency/reliability, trust level and sandbox permissions. `forge cluster list`
also returns `forge.cluster_registry.v2` scheduling posture with one
`forge.cluster_node_scheduling.v1` row per registered node. Those rows expose
whether the node is schedulable from local registry policy, active/expired lease
pressure, blockers and explicit `remote_execution_enabled=false` /
`external_mutation_allowed=false` markers. Placement is a read-only policy
decision: Forge can select a node that satisfies deterministic task requirements,
but it does not connect over SSH, execute remote code or mutate external machines.
Placement candidates also expose active node lease counts and penalize busy
eligible nodes, so a compatible idle node is preferred before handoff.
Each placement report includes a `forge.cluster_placement_policy.v1` receipt
with the authorized scope `placement_metadata_only`, explicit no-remote-execution
and no-external-mutation flags, the required trust policy and deterministic
hashes for the requirements and policy. That receipt is the audit boundary before
Forge creates a node lease or any future remote adapter asks for authorization.
`forge cluster handoff` layers that placement decision over the
normal executor handoff contract: it leases the task to the selected node id and
returns `forge.cluster_task_handoff.v1` with the placement report, executor handoff
packet, node-scoped lease ref and `forge.cluster_sync_manifest.v1`. The sync
manifest carries context, checkpoint, artifact and shard hashes plus a
deterministic `manifest_sha256`, so a future distributed adapter can copy or
verify only content-addressed inputs after an explicit remote-execution policy
exists. `forge cluster leases` provides the
read-only audit surface for those node-scoped leases, including active/expired
state, workflow/task identity, trust level, sandbox permissions and explicit
`remote_execution_enabled=false` / `external_mutation_allowed=false` markers.

Runtime resources are scope-guarded:

```bash
forge runtime guard --substrate knative --resource service/forge-node --namespace forge --action update --owner forge --output json
forge runtime guard --substrate knative --resource service/existing-api --namespace default --action update --owner external --output json
```

Forge may update/delete resources it created. External resources require explicit human authorization, even when the substrate is available.

Workflows can be changed while running:

```bash
forge workflow update-goal --workflow <workflow-id> --goal "new goal" --origin codex --output json
forge workflow attach-artifact --workflow <workflow-id> --path ./report.md --kind report --origin opencode --output json
```

This is how Codex/OpenCode act as the human interface for Forge: the CLI session can update goals, attach artifacts and keep a revision trail without bypassing Forge's persistent runtime state.

Run Forge self-evolution:

```bash
forge self run \
  --repo /home/arthur/projects/forge-core \
  --until 2026-05-25T10:00:00-03:00 \
  --executor codex \
  --executor opencode \
  --mode balanced \
  --max-cycles 1 \
  --output json
```

`forge self run` creates a run id and workflow id, writes prompt/report artifacts for every cycle, runs validation before committing, and only pushes when `--push` is passed.
Each self-evolution cycle report includes the prompt packet version and SHA-256 checksum so executor runs can be replayed and audited against the exact instructions given to Codex/OpenCode. Prompt packet `forge.self_evolution.prompt.v2` includes the current persisted workflow goal, the initial goal and the workflow revision before the generic strategic backlog, so human goal mutations such as clusterization or n8n node research are carried into subsequent self-evolution runs.
The self-evolution report also includes `forge.self_evolution.overhead_ledger.v1` and `forge.self_evolution.decision_gate.v1`. `--mode lean` rejects governance-heavy cycles when expected value is below orchestration cost; `balanced` is the default; `strict` allows more overhead only for audit, safety or distributed-execution needs. When the persisted terminal goal already has the mode boundary, ledger and decision gate, Forge returns `terminal_goal_reached` and creates no new cycle prompts.

Example autonomous mixed objective:

```bash
forge plan --goal "Execute research now, continue every Friday at 09:00, calculate costs without AI, and email the final workflow cost to finance@example.com" --output json
```

When a goal mentions n8n research, `forge plan` adds a research catalog task and
a separate Forge primitive evaluation task. The atomic graph build depends on
that recommendation, so concepts such as loop-over-items, IF/Switch routing,
Merge, Wait, Code, Execute Sub-workflow, triggers, retries, errors, transforms
and human approval patterns stay outside native Forge semantics until they
improve validated DAG execution, context routing, resumability, observability or
operator clarity.

## Skill Install

Install the Forge Core skill for Codex and OpenCode:

```bash
forge skill install --target codex --target opencode --output json --home "$HOME"
```

The installer writes:

- Codex: `~/.codex/skills/forge-core/SKILL.md`
- OpenCode: `~/.config/opencode/skills/forge-core/SKILL.md`
- Shared agent-compatible path: `~/.agents/skills/forge-core/SKILL.md`

The repository also includes project-local skill definitions:

- `.agents/skills/forge-core/SKILL.md`
- `.opencode/skills/forge-core/SKILL.md`
- `skills/forge-core/SKILL.md`

## Validation

Run the full local gate:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

The current test suite validates:

- planning creates a persistent atomic graph;
- validation blocks promotion until tasks are complete;
- context packages stay task-local, budget-bounded, executor-profiled, versioned and sharded;
- strict context mode blocks executor handoff when required sections are omitted;
- controlled improvement never auto-promotes without validation;
- artifact listing returns SHA-256 hashed outputs;
- workflow registry listing preserves initial requests and lifecycle state;
- workflow inspection renders terminal DAGs with dependency, lifecycle, persona and context next-action annotations;
- context routing carries proposed child-subflow bindings for reusable deterministic nodes;
- simulated execution can complete the graph and unlock validation;
- skill installation works for Codex and OpenCode paths.

## Self-Improvement Model

Forge Core does not perform unrestricted self-modification.

The current loop is:

```text
execute workflow
→ collect validation state
→ generate improvement experiment artifact
→ benchmark and validate externally
→ promote only when validation passes
```

`forge improve` generates a controlled experiment artifact and keeps `auto_promoted=false`.

Every improvement can target a version and generates a Markdown changelog:

```bash
forge improve --workflow <workflow-id> --target-version 0.3.0 --output json
```

Current structural improvement domains:

- task structure: backlog state, subtasks, impediments, owner role and acceptance criteria;
- prompt system: versioned prompt/task packets that can be benchmarked and rolled back;
- process runtime: Scrum/SAFe-style blocked work and promotion readiness;
- validation governance: goals must be definitively ready before promotion;
- executor policy: installed/configured CLIs require saved human authorization;
- runtime substrates: Docker/Kubernetes/Knative require authorization and resource ownership checks;
- runtime mutation: goals/artifacts can change while running with origin trace and revisions.
- async request handoff: skill callers receive a `run_id` and do not need to wait for the full run.
- context routing: proposed child-subflow reuse decisions are included in bounded task context.

## Evolution Direction

Forge should evolve as an operational kernel for agentic systems, not as a subordinate extension of a single agent CLI.

The practical path still includes close CLI coupling where it helps adoption:

- Codex/OpenCode/Gemini invoke `forge plan`, `forge context`, `forge run`, `forge validate` and `forge artifacts` from inside their normal workflows.
- Forge invokes Codex/OpenCode/Gemini/Claude/Ollama adapters for bounded tasks using a strict task packet with allowed context, expected output and validation rules.
- Open-source CLIs can receive deeper native integration over time so their interactive experience can be backed by Forge's persistent workflow runtime.

See [docs/evolution-roadmap.md](docs/evolution-roadmap.md) for the planned integration path.

## Project Scope

This release intentionally does not implement:

- SaaS frontend;
- full provider execution adapters;
- WASM plugin runtime;
- distributed execution;
- unrestricted autonomous code mutation.

The current focus is the portable runtime contract: decomposition, persistence, context minimization, validation, artifacts and controlled improvement.
