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

Current version: `0.4.10`

This is the first functional CLI + Skill version:

- Rust CLI binary: `forge`
- SQLite persistence
- deterministic atomic task graph generation
- versioned, sharded bounded context package generation
- validation gates
- simulated execution runtime
- autonomous mixed AI/non-AI workflow planning
- cron/wait task representation
- notification payloads with final workflow cost reporting
- artifact listing
- workflow registry listing with lifecycle state
- context routing with deterministic shard manifests, deterministic code-node and long-running cognition goals
- controlled improvement proposal generation
- Codex/OpenCode-compatible `forge-core` skill
- executor sync that detects installed/configured CLIs and persists human authorization policy
- runtime sync that detects Docker/Kubernetes/Knative and persists human authorization policy
- goal-oriented tasks with subtasks, impediments, acceptance criteria and rework readiness checks
- runtime workflow mutation for goals and artifacts with origin trace from `codex`, `opencode`, `forge_cli` or skills
- async workflow substrate policy with scope guards for Forge-owned resources
- async request handoff for skill callers: submit a goal, receive `run_id`, continue later with Forge
- persisted task leases so two executors cannot acquire the same workflow task concurrently
- self-evolution runner for bounded Codex/OpenCode cycles until a stop date
- versioned self-evolution prompt packets with SHA-256 checksums in cycle reports
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
forge status --workflow <workflow-id> --output json
forge context --workflow <workflow-id> --task task-001 --budget 1200 --output json
forge run --workflow <workflow-id> --simulate --output json
forge validate --workflow <workflow-id> --output json
forge improve --workflow <workflow-id> --output json
forge artifacts --workflow <workflow-id> --output json
```

`forge context` emits a versioned context packet (`forge.context.v2`) with a deterministic
`task_local_revisioned_budget_v2` routing policy. The packet keeps the legacy `content`
body for executors, and also returns workflow revision, artifact count, lineage hashes
and a shard manifest with included/omitted sections, source labels, priorities, byte
counts, summaries and SHA-256 checksums so runs can be replayed against the exact
context that was selected. Runtime goal and artifact mutations are included in the
context lineage so executors can detect stale context before resuming work.

Skill-style async handoff:

```bash
forge request start --goal "Improve Forge Core" --origin codex --output json
forge request status --run <run-id> --output json
```

Codex/OpenCode should prefer this pattern when using Forge as a skill: make a short request, receive a `run_id`, and let Forge own the asynchronous workflow state.
`forge request status` resolves the run id back to the current Forge workflow state, including the current goal, original requested goal, latest revision, artifact count and task status summary.
`forge list` exposes the workflow registry across planned and async workflows, including stable workflow ids, associated run ids, initial request, current goal, lifecycle state and task summary. Completed finite workflows are projected as `scaled_to_zero` when there is no remaining task work.

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
  --max-cycles 1 \
  --output json
```

`forge self run` creates a run id and workflow id, writes prompt/report artifacts for every cycle, runs validation before committing, and only pushes when `--push` is passed.
Each self-evolution cycle report includes the prompt packet version and SHA-256 checksum so executor runs can be replayed and audited against the exact instructions given to Codex/OpenCode.

Example autonomous mixed objective:

```bash
forge plan --goal "Execute research now, continue every Friday at 09:00, calculate costs without AI, and email the final workflow cost to finance@example.com" --output json
```

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
- context packages stay task-local, budget-bounded, versioned and sharded;
- controlled improvement never auto-promotes without validation;
- artifact listing returns SHA-256 hashed outputs;
- workflow registry listing preserves initial requests and lifecycle state;
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
