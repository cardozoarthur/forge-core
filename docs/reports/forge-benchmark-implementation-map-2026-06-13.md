# Forge Benchmark Implementation Map

Data: 2026-06-13

This map turns benchmark exploration into Forge implementation ownership.

## Core Ownership

### `src/opencode_tui.rs`

Owns:
- `/resume` and chat session codes;
- orchestrator-first shell/TUI;
- on-demand autocomplete and command discovery;
- benchmark surface in the TUI;
- workflow visibility, history, and compact operator UX.

Benchmark contracts absorbed here:
- Codex: resume and clear interactive/execution split.
- Gemini CLI: shell-first session continuity.
- OpenCode: default TUI entrypoint and compact operator surface.
- Claude CLI: named session resume and visible plan boundary.
- n8n: workflow graph visibility in the operator surface; Core graph semantics and UI inspiration, not only an Addon reference.
- Obsidian: linked context and local-first navigation cues.

### `src/interactive.rs`

Owns:
- task board, DAG, schedules, artifacts, approvals, costs;
- operational cockpit views;
- workflow mutation and guided operation;
- context memory and file/session views.

Benchmark contracts absorbed here:
- LangGraph: stateful graphs, checkpoints, resume and subgraphs.
- LangChain: middleware-like context shaping and tool boundaries.
- n8n: node graph semantics and operator-visible workflow states; the core workflow graph should be able to render and reuse n8n-style structures.
- Paperclip: document queues, audit trail and workflow queues.
- OpenSquad: visible multi-agent collaboration and handoffs.

### `src/executor.rs`

Owns:
- brain router, executor selection and session lifecycle;
- provider/brain visibility;
- shell session ownership and hot-swap policy.

Benchmark contracts absorbed here:
- Codex and Gemini: replaceable brains under Forge authority.
- Claude: session persistence and resume by identity.
- OpenCode: provider and agent selection remain visible.
- headroom: context economy for what reaches a brain.

### `src/harness.rs`

Owns:
- token headroom;
- wrapper plans;
- shims;
- forge-first harness policies;
- platform-specific command interception.

Benchmark contracts absorbed here:
- headroom: compression/interception/context budgeting.
- superpowers: verification discipline and process gates.
- credential-vault: controlled dependency access.

### `src/memory.rs`

Owns:
- global/project/processing memory governance;
- promotion/retention policy;
- scope-aware retrieval.

Benchmark contracts absorbed here:
- Hermes: file-first memory and semantic retrieval.
- Obsidian: local-first linked notes and graph-aware context.

### `src/addon.rs`

Owns:
- capability registry;
- optional domain surfaces;
- permissions and lifecycle.

Benchmark contracts absorbed here:
- Penpot and Open Design: design system artifacts as optional capabilities.
- n8n: dual-use node ecosystem and external automation interoperability; keep the Addon boundary explicit but preserve the Core graph model.
- telegram-delivery: optional handoff/delivery output.

### `src/workflow.rs` and `src/request.rs`

Own:
- workflow identity;
- run state;
- resume and continuation;
- async execution and lifecycle control.

Benchmark contracts absorbed here:
- LangGraph: durable state machine and subworkflow reuse.
- Claude/OpenCode: session continuation and fork/resume semantics.
- OpenClaw/OpenSquad: multi-channel collaboration and visible handoffs.

### `src/artifact.rs`

Owns:
- artifact identity;
- hash/provenance;
- generated output lifecycle.

Benchmark contracts absorbed here:
- Remotion: artifact-first templates and preview/render separation.
- Paperclip: document workflow, auditability and structure.

## Addon-First Ownership

### `sdk/`

Owns:
- TypeScript, Python, Go and Rust workflow clients;
- subworkflow calls across language boundaries;
- async fan-out/fan-in composition;
- stable resume ids.

Benchmark contracts absorbed here:
- LangGraph: subgraphs and checkpoints.
- LangChain: lightweight harness and composition.
- n8n: node/trigger mental model for graph composition, especially where Forge nodes should interoperate with external n8n workflows.

### `installer/`

Owns:
- shell-visible `forge` installation;
- platform-neutral release consumption;
- version pinning and update semantics.

Benchmark contracts absorbed here:
- OpenCode/Gemini/Claude: installable, terminal-first operator toolchains.

## Explicit UX Rules To Preserve

1. `/resume` is discoverable and stable.
2. Chat close prints the unique code needed for resume.
3. Commands and suggestions are on-demand, not visually noisy.
4. Workflow graphs stay visible where workflows are used.
5. File creation is treated as workflow, not a raw write.
6. Addons must remain optional and discoverable.
7. The same workflow model must be reusable across languages.

## Final Implementation Priorities

1. Wire SDK stubs to a real transport.
2. Keep the TUI/chat code stable.
3. Expand workflow graph views for files, documents and collaboration.
4. Keep benchmark-derived behavior in Core only when it shapes the workflow kernel.
5. Move domain-specific surfaces into Addons.
