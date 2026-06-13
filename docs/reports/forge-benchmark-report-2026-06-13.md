# Forge Benchmark Report

Data: 2026-06-13

This report captures the current benchmark split and the TUI rendering update.

## Summary

- Core benchmarks now cover runtime control: Codex, Gemini CLI, OpenCode, LangGraph, LangChain and the Forge orchestrator itself.
- Addon-first benchmarks now cover optional capability classes: OpenClaw, Hermes, Open Design, Penpot and n8n.
- File creation is treated as a workflow, not a raw write.
- The TUI now wraps panel text when lines exceed the available width, instead of clipping them.

## Core Benchmarks

Core benchmarks are the ones that should stay in the default workflow path:

- Codex: direct execution, review and apply flow.
- Gemini CLI: interactive-first session flow and shell-first ergonomics.
- OpenCode: orchestrator-style TUI and compact operator experience.
- LangGraph: resumable state, checkpoints and subworkflow composition.
- LangChain: tool/middleware shaping and context injection discipline.

These belong in Forge core because they affect routing, execution and workflow state.

## Addon-First Benchmarks

These should be explicit but not always loaded:

- OpenClaw: async operator surface and durable multi-channel handoff.
- Hermes: file-first memory and semantic retrieval.
- Open Design: artifact-centric creative workflows.
- Penpot: design tokens, components and layout systems.
- n8n: triggers, actions, schedules and node-marketplace semantics.

These are benchmark references, but they are better treated as optional Addons unless a workflow requires them.

## File Workflow

File creation is a workflow when it has structure.

The flow is:

1. collect source data
2. organize and normalize it
3. choose the target schema
4. render the output
5. validate the structure
6. persist the file
7. attach it to the session or workflow

That contract is already captured in `docs/reports/forge-file-workflow-contract-2026-06-13.md`.

## TUI Rendering

The TUI now uses line wrapping for boxed panels. Long benchmark lines and other long surfaces will break across lines instead of being truncated.

That matters for:

- `/benchmark`
- long workflow labels
- long context strings
- long file or addon names

## Current Direction

The benchmark inventory should keep listing everything the user asked for, but the runtime should only surface Core items by default and load Addon-first items when they are actually needed.
