# Forge Benchmark Final Closure

Data: 2026-06-13

This document closes the benchmark exploration into a single implementation-oriented view.

## What Was Explored

The benchmark set was examined across code contracts, runtime semantics, UI/UX patterns, pros, cons and Forge-specific impact:

- Codex
- Gemini CLI
- OpenCode
- Claude CLI
- LangGraph
- LangChain
- OpenClaw
- Hermes
- Open Design
- Penpot
- n8n
- Paperclip
- Obsidian
- OpenSquad
- superpowers
- installed skills and plugins
- credential-vault
- telegram-delivery
- Remotion
- headroom

## Final Classification

### Core

These define Forge runtime, routing, resume, execution and default operator UX:

- Codex
- Gemini CLI
- OpenCode
- Claude CLI
- LangGraph
- LangChain
- headroom

### Addon-first

These are valuable, but should stay optional unless a workflow needs them:

- OpenClaw
- Hermes
- Open Design
- Penpot
- Paperclip
- Remotion
- telegram-delivery

### Dual-use

These shape both the Core graph and optional surfaces:

- n8n
- Obsidian
- OpenSquad

## Key Technical Decisions

1. `/resume` stays a first-class Forge contract.
2. Chat close emits a unique code for resume.
3. File creation is a workflow, not a raw write.
4. Workflows are the unit of execution.
5. Benchmarks are classified as `Core`, `Addon-first`, or `dual-use`.
6. SDKs share one workflow model across TypeScript, Python, Go and Rust.
7. The installer preserves the same `forge` entrypoint on Linux, macOS and Windows.
8. `n8n` is dual-use, not just Addon.
9. `OpenSquad` is dual-use and must expose ownership and handoffs explicitly.

## What Forge Should Absorb

- From Codex: exec/review split, direct execution loop, resume behavior.
- From Gemini CLI: shell-first mode and persistent interactive continuity.
- From OpenCode: compact TUI, session continue/fork, operator-friendly default entrypoint.
- From Claude CLI: named sessions and plan/approval boundaries.
- From LangGraph: checkpointed state, subworkflow reuse and durable graph semantics.
- From LangChain: middleware-like routing and context shaping.
- From headroom: token budgeting and reversible context projection.
- From OpenClaw: asynchronous handoffs and durable operator state.
- From Hermes: file-first memory with strict scope boundaries.
- From Open Design and Penpot: artifact and design-system thinking, not core kernel bloat.
- From n8n: trigger/action graphs, schedules, nodes and a readable workflow UI.
- From Paperclip: secure document queues, audit trail and structured file workflows.
- From Obsidian: linked context, local-first graph navigation and canvas-style surfaces.
- From OpenSquad: visible collaboration, ownership and waiting states.
- From superpowers: explicit process gates and verification discipline.
- From credential-vault: secrets accessed only through a brokered contract.
- From telegram-delivery: message/document receipts as workflow outputs.
- From Remotion: parameterized artifact pipelines and preview/render separation.

## Forge Implementation Backlog

1. Keep `/resume` and session codes stable in the TUI.
2. Treat file creation, document generation and report generation as workflows.
3. Build workflow graph views that can render n8n-style structures.
4. Keep optional capabilities behind Addons and registry gates.
5. Wire SDK stubs to a real transport.
6. Keep context economy in the harness/router.
7. Expose visible ownership, approvals, handoffs and waiting states.
8. Preserve compact TUI defaults with on-demand discovery.

## Closure

The benchmark exploration is technically closed enough to drive implementation.
The remaining work is implementation, transport and distribution, not further benchmark naming.
