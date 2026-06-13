# Forge Benchmark Technical Closure

Data: 2026-06-13

This report closes the benchmark exploration into implementation decisions for Forge.

## Purpose

The goal is not to copy benchmark code. It is to extract operating contracts, UI/UX patterns, and workflow semantics that Forge should own.

## Technical Decisions

1. `/resume` stays a first-class Forge contract.
2. Closing the chat must emit a unique chat code for later resume.
3. Workflows are the unit of execution, including file creation.
4. Benchmarks fall into `Core`, `Addon-first`, and `dual-use`.
5. SDKs must share one workflow model across TypeScript, Python, Go and Rust.
6. The installer must preserve the same `forge` entrypoint on Linux, macOS and Windows.

## Benchmark-by-Benchmark Closure

### Codex

Pros:
- fast local execution loop;
- clear `resume` story;
- review/execute separation.

Cons:
- can overfit to code execution and not to broader operations;
- model and shell policy can be implicit if not surfaced.

Forge decision:
- keep Codex as a Core execution reference;
- mirror the `resume` and chat-code flow;
- expose execution policy visibly in the TUI.

### Gemini CLI

Pros:
- shell-first interaction;
- interactive by default;
- session continuity.

Cons:
- approval and shell mode can feel hidden unless surfaced carefully.

Forge decision:
- preserve chat-first UX with shell mode as an explicit state;
- keep resume state durable and human-readable.

### OpenCode

Pros:
- clean TUI entrypoint;
- session continue/fork;
- provider/agent visibility;
- server mode exists.

Cons:
- a very compact UI can hide state if the operator surface is overloaded.

Forge decision:
- use OpenCode as the default TUI UX reference;
- keep suggestions on demand, not permanently visible;
- preserve a headless/server escape hatch.

### Claude CLI

Pros:
- long-running session memory;
- visible plan mode;
- named session resume.

Cons:
- plan/edit boundaries can feel tool-specific if not generalized.

Forge decision:
- adopt session naming, plan/approval boundaries and chat persistence;
- make workflow mutation explicit instead of hidden.

### LangGraph

Pros:
- stateful graphs;
- checkpoint/resume;
- subgraph reuse.

Cons:
- graph semantics can become overly abstract if the UI does not show them well.

Forge decision:
- every Forge workflow is a graph;
- subworkflow reuse is a first-class contract;
- checkpoints and resume are durable identities, not implementation detail.

### LangChain

Pros:
- middleware composition;
- context engineering;
- tool/error handling as primitives.

Cons:
- easy to over-abstract the agent and lose operational clarity.

Forge decision:
- use middleware as the pattern for routing, guardrails and context shaping;
- keep the orchestrator authoritative.

### OpenClaw

Pros:
- asynchronous operation;
- multi-channel collaboration;
- durable handoffs.

Cons:
- channel sprawl can obscure ownership if not graph-backed.

Forge decision:
- treat channels as Addons or adapters;
- preserve durable workflow identity across surfaces.

### Hermes

Pros:
- file-first memory;
- semantic retrieval;
- scope-aware promotion.

Cons:
- memory can become a dumping ground unless governance is strict.

Forge decision:
- keep project memory in `.forge`;
- split global/project/processing scopes;
- use file references, not giant context loads.

### Open Design

Pros:
- artifact-centric thinking;
- visual flows and outputs.

Cons:
- can be too broad if the kernel tries to own design execution itself.

Forge decision:
- keep design workflows as Addons;
- own the artifact contract in Core.

### Penpot

Pros:
- strong design system primitives;
- collaboration around tokens/components/layouts.

Cons:
- design UIs can become too heavy for operational use.

Forge decision:
- absorb tokens/components/layouts as Forge artifacts;
- keep operational UI dense and utilitarian.

### n8n

Pros:
- node/edge workflow clarity;
- schedules and triggers are obvious;
- visual automation is easy to reason about.

Cons:
- node graphs can become visually noisy without hierarchy.

Forge decision:
- treat n8n as dual-use:
  - Core: graph semantics, nodes, triggers, schedules;
  - Addon: interoperability and external automation;
- give Forge a workflow UI that can render graphs n8n-style.

### Paperclip

Pros:
- document-first operations;
- queues and audit trail;
- secure processing.

Cons:
- document systems can become rigid if workflows are not composable.

Forge decision:
- treat file/document creation as workflow;
- attach audit and security at the workflow boundary.

### Obsidian

Pros:
- backlinks and graph navigation;
- local-first note UX;
- canvas for visual organization.

Cons:
- can become a passive note vault if relationships are not actionable.

Forge decision:
- expose linked context, canvas and graph views for workflows and artifacts;
- keep the local-first mental model.

### OpenSquad

Pros:
- visible multi-agent collaboration;
- shared board/handoff model;
- human + AI coordination.

Cons:
- collaboration becomes hard to audit if ownership is hidden.

Forge decision:
- use OpenSquad-style collaboration as a dual-use benchmark;
- show ownership, handoffs and waiting states.

### superpowers

Pros:
- disciplined process;
- brainstorming, debugging, verification;
- worktree-aware execution.

Cons:
- skills can degrade if treated like hidden magic instead of explicit workflow.

Forge decision:
- make process gates part of runtime policy;
- use skills as workflow adapters, not the only source of behavior.

### installed skills and plugins

Pros:
- reusable capability packs;
- structured operator instructions.

Cons:
- skill behavior can be inconsistent across CLIs if not normalized.

Forge decision:
- convert skills into Forge-owned workflows, context packs or adapters;
- keep a registry and versioning contract.

### credential-vault

Pros:
- safe secret access;
- brokered terminal injection;
- visible contract boundary.

Cons:
- if the boundary is loose, secrets leak into prompts and logs.

Forge decision:
- secrets are workflow dependencies accessed through the vault contract only.

### telegram-delivery

Pros:
- clear completion handoff;
- document and message receipts;
- useful for reporting.

Cons:
- delivery should remain optional and not become a kernel dependency.

Forge decision:
- keep Telegram delivery as an Addon/workflow output;
- always attach receipts when delivery is used.

### Remotion

Pros:
- reusable templates;
- parameterized rendering;
- preview/render separation.

Cons:
- if taken literally, could over-push Forge toward media tooling.

Forge decision:
- use it as an artifact pipeline benchmark, not a media runtime benchmark.

### headroom

Pros:
- context compression;
- token economy;
- wrapper interception.

Cons:
- compression can hide critical facts if the policy is too aggressive.

Forge decision:
- keep context economy in Core;
- compress only when the brain contract can still operate safely.

## Final Forge Implementation Surface

The implementation backlog that follows this benchmark work should focus on:

1. a stable `/resume` and chat code UX;
2. workflow graphs for conversations, artifacts and files;
3. a Core/Addon/dual-use registry;
4. cross-language SDK transport;
5. a release installer for Linux/macOS/Windows;
6. UI surfaces for nodes, handoffs, approvals, documents and linked context.

## Outcome

The benchmark exploration is now technically closed enough to drive implementation.
The remaining work is execution, transport and distribution, not more naming.
