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

Current version: `0.1.0`

This is the first functional CLI + Skill version:

- Rust CLI binary: `forge`
- SQLite persistence
- deterministic atomic task graph generation
- bounded context package generation
- validation gates
- simulated execution runtime
- autonomous mixed AI/non-AI workflow planning
- cron/wait task representation
- notification payloads with final workflow cost reporting
- artifact listing
- controlled improvement proposal generation
- Codex/OpenCode-compatible `forge-core` skill

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
forge status --workflow <workflow-id> --output json
forge context --workflow <workflow-id> --task task-001 --budget 1200 --output json
forge run --workflow <workflow-id> --simulate --output json
forge validate --workflow <workflow-id> --output json
forge improve --workflow <workflow-id> --output json
forge artifacts --workflow <workflow-id> --output json
```

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
- context packages stay task-local and budget-bounded;
- controlled improvement never auto-promotes without validation;
- artifact listing returns SHA-256 hashed outputs;
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
