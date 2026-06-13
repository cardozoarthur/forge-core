# Forge Benchmark Matrix

Data: 2026-06-13

This matrix captures the best ideas to absorb from the benchmark set named in the official Forge goals. It is not a cloning plan. It is a translation into Forge-owned contracts.

## Terminal And Brain CLIs

### Gemini CLI

- Strength to absorb: project-aware terminal UX and long-lived operator flow.
- Forge translation: default fullscreen entrypoint, compact/detail modes, project-scoped context and streamable workflow state.

### Codex CLI

- Strength to absorb: local execution pragmatism, code changes, tests and operator speed.
- Forge translation: Codex as a replaceable brain under Forge workflow authority, with bounded context and structured validation.

### Claude CLI

- Strength to absorb: long-running interactive flow and dynamic task handling.
- Forge translation: workflow evolution during execution, checkpoints, waits, resume and approval gates.

### OpenCode

- Strength to absorb: polished shell UX and brain/provider abstraction.
- Forge translation: orchestrator-first terminal surface with interchangeable execution brains.

## Agent Platforms

### OpenClaw benchmark

- Strength to absorb: asynchronous surfaces, multi-interface operation and persistent collaboration.
- Forge translation: event-driven workflows, durable views and a human+AI operational cockpit.

### Hermes benchmark

- Strength to absorb: file-first memory and semantic retrieval.
- Forge translation: memory governance by scope, tenant and audience, with reversible processing memory and retrieval refs.

### OpenSquad

- Strength to absorb: agent collaboration boards, parallel handoffs and visible task ownership.
- Forge translation: task-board and workflow-DAG views with per-node brains, handoffs and checkpoints, plus a collaboration surface that shows who owns what and what is waiting on whom.

## Design And Product Systems

### Open Design benchmark

- Strength to absorb: local design workflows and bridgeable artifact output.
- Forge translation: design as an Addon-owned artifact system, not a kernel responsibility.

### Penpot benchmark

- Strength to absorb: design tokens, components, shared design systems and collaboration.
- Forge translation: composable UI artifacts, dynamic views and brand-aware operation.

### Paperclip

- Strength to absorb: company-work framing and organizational thinking.
- Forge translation: Forge as an organizational copilot with product, finance, admin, marketing and communication context, plus secure document workflows, audit trails, queues and encryption-in-use.

### Obsidian

- Strength to absorb: local-first knowledge UX, backlinks, graph/canvas navigation and composable note workflows.
- Forge translation: session and artifact graphs, linked context views, note-backed workflows and a visual knowledge surface that stays local-first.

## Media And Automation

### Remotion

- Strength to absorb: programmable pipelines for generated media, reusable templates and previewable rendering.
- Forge translation: artifact/render pipelines owned by Addons, with media as a domain capability and a shared template/render contract for other artifact types.

### n8n benchmark

- Strength to absorb: trigger/action graph, webhooks, schedules, node marketplace thinking and a visual workflow UI that makes nodes and edges obvious.
- Forge translation: capability registry, event adapters, schedules, human-in-the-loop nodes and Addon marketplace semantics, plus a Core workflow surface that can present graphs in an n8n-like way.

### superpowers benchmark

- Strength to absorb: process discipline, brainstorming, systematic debugging, TDD, verification-before-completion and worktree-aware execution.
- Forge translation: workflow design gates, debugging loops, validation gates and context/parallelization discipline that are enforced by the runtime rather than left to memory.

### installed skills benchmark

- Strength to absorb: reusable operator procedures, task-specific capability packs and structured instructions.
- Forge translation: skills become Forge-owned workflows, context packs or capability adapters instead of the only place where behavior lives.

## Harness And Context

### headroom

- Strength to absorb: reversible context compression, wrapper interception and budget awareness.
- Forge translation: token headroom as a first-class harness contract with retrieval refs and CLI wrapper policies.

## Reusable Patterns

Across the whole benchmark set, the recurring patterns are:

- operator-first terminal UX;
- bounded context;
- recoverable state;
- explicit approvals;
- dynamic workflows;
- multi-channel events;
- visible progress and auditability;
- separation between orchestration and execution brain;
- artifact-first delivery;
- themeable, composable UI surfaces.

## Do Not Copy

- Do not copy provider-specific orchestration loops.
- Do not hard-code software development as the core domain.
- Do not collapse the workflow OS into an LLM wrapper.
- Do not make the renderer or brain provider a hard dependency of the kernel.
