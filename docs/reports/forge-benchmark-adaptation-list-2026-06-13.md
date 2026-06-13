# Forge Benchmark Adaptation List

Data: 2026-06-13

This is a technical inventory of behaviors Forge should absorb from the benchmark CLIs, agent runtimes, and local skills.

Explicit benchmark sources in this inventory:
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
- Obsidian
- OpenSquad
- Paperclip
- Remotion
- headroom
- superpowers
- installed skills and plugins

## Placement Split

Forge should separate the benchmark set into three operating classes:

- Core benchmarks: terminals, brains, router behavior, workflow control, resume, approvals and execution contracts.
- Addon-first benchmarks: design systems, automation graphs, file-first memory, async operator channels and optional delivery surfaces.
- Dual-use benchmarks: tools that shape both the workflow graph and the optional surface that visualizes or interoperates with it.

That split matters because not every benchmarked capability must be loaded into the core TUI or kernel path all the time. The benchmark inventory should make reusability visible, but the runtime should prefer on-demand Addons when the feature is optional, and it should keep dual-use benchmarks explicit so Core and Addon responsibilities do not blur together.

OpenClaw and Hermes are treated here as first-class benchmark references, not just side notes. OpenClaw informs asynchronous operator surfaces and multi-channel durability. Hermes informs file-first memory and semantic retrieval.
Open Design and Penpot are first-class benchmark references for addon-style artifact and design systems. n8n is first-class in a different way: it is dual-use, because it informs both trigger/action graphs, schedules and node marketplace semantics and the Core workflow graph plus the workflow UI direction.
Obsidian is a first-class benchmark for knowledge UX: backlinks, graph/canvas navigation, local-first notes, plugin-driven composition and visual organization of linked artifacts.
OpenSquad is a first-class benchmark for visible multi-agent collaboration: handoffs, shared task boards and collaboration-aware workflow graphs.
Paperclip is a first-class benchmark for document operations: secure client folders, straight-through processing, workflow queues, audit-ready document handling, encryption-in-use and data digitization.
Remotion is a first-class benchmark for artifact pipelines: reusable templates, parameterized rendering and preview-before-render discipline.
headroom is a first-class benchmark for context economy: reversible compression, wrapper interception and token-budget awareness.
The superpowers skill family and the installed skills/plugins set are also benchmarks, but the translation target is not to rely on them as magical behavior. Forge should instead expose the same capabilities as first-party workflow, context and routing contracts so the system stays predictable even when skills are absent or suboptimal.

File creation itself is also a benchmarked workflow pattern: gather data, organize it, render the target schema, validate the result and persist the artifact with session context. For Forge this belongs in Core artifact/workflow contracts, with optional Addons only for domain-specific exporters.

## 1. Codex

Forge placement:
- Core routing and execution brain reference.
- Not an Addon by default; its capabilities become Forge-owned execution policy and harness contracts.

What to adapt:
- `/resume` picker semantics and `--last` continuation.
- `exec` vs `review` separation.
- model-aware non-interactive execution.
- sandbox and approval boundaries as first-class controls.
- MCP and plugin management as visible operational surfaces.

Example shape:
```bash
codex exec -m gpt-5.5 --sandbox read-only --output-last-message /tmp/last.txt "route this request"
codex resume --last
```

Forge target:
- chat session resume codes
- explicit session browser
- workflow creation only when the brain chooses it
- executor/brainer policy visible in `/brains`, `/sessions`, `/harness`

## 2. Gemini CLI

Forge placement:
- Core interactive brain reference.
- Addon only for optional integrations around sessions, approvals or provider adapters.

What to adapt:
- interactive-first default.
- `-p/--prompt` headless mode.
- session list / session resume / session UUID control.
- `--approval-mode` and `--raw-output`.
- worktree-first launch option.

Example shape:
```bash
gemini -p "summarize this request" --output-format text --raw-output --accept-raw-output-risk
gemini --resume latest
```

Forge target:
- keep the TUI conversational by default
- add deterministic resume of chat sessions
- preserve approval modes as visible runtime policy
- expose project-scoped session control

## 3. OpenCode

Forge placement:
- Core TUI/brain UX reference.
- Addon only where renderer or provider extensions should be optional.

What to adapt:
- TUI as the default entrypoint.
- headless `run` mode with a pure/no-plugin profile.
- session continue / session fork.
- provider, agent, model, stats, and web surfaces.
- shell-first feel and compact navigation.

Example shape:
```bash
opencode
opencode run --pure --prompt "inspect the repo"
opencode session --continue <id>
```

Forge target:
- `forge` should open the orchestrator TUI directly
- `/resume <chat-code>` should restore conversation state
- the surface should stay compact, with commands discovered on demand
- shell mode stays separate from normal chat

## 4. LangGraph

Forge placement:
- Core workflow/state-machine reference.
- Subworkflow and checkpoint contracts belong in Core; execution adapters can become Addons later.

What to adapt:
- thread_id / checkpoint_id / run_id as durable execution identities.
- interrupt / resume as first-class state transitions.
- subgraphs as reusable units.
- durability modes and resumable streaming.
- explicit state machine behavior instead of hidden loops.

Example shape:
```python
# conceptual shape, not copied from upstream
result = graph.invoke(
    {"messages": messages},
    config={"configurable": {"thread_id": thread_id}},
)
# on resume, reuse the same thread_id / checkpoint_id
```

Forge target:
- every workflow is resumable
- chat sessions should behave like a thread/checkpoint pair
- the router should inspect reusable workflows before creating a new one
- subworkflows should remain visible and composable

## 5. LangChain

Forge placement:
- Core agent-middleware design reference.
- Tooling and middleware patterns may become Addon templates when domain-specific.

What to adapt:
- `create_agent` as a thin harness around model + tools + middleware.
- middleware for context injection, guardrails, summarization, and tool limits.
- human-in-the-loop interrupts.
- tool-call validation and response shaping.
- per-step hooks for logging, retries, and early termination.

Example shape:
```python
from langchain.agents import create_agent

agent = create_agent(
    model="gpt-5.5",
    tools=[search_tool],
    middleware=[...],
)
```

Forge target:
- keep brain routing in the orchestrator
- move expensive or repeated behaviors into workflow/skill middleware
- use human approval only when a workflow is truly mutating or risky
- keep context engineering explicit and inspectable

## 6. superpowers

Forge placement:
- Core process-discipline reference.
- The translated behavior should live in Forge runtime gates, not as an optional Addon.

What to adapt:
- brainstorming before implementation.
- systematic debugging before patching.
- test-driven execution for feature work.
- verification-before-completion.
- using worktrees and parallel agents only when independent.

Forge target:
- each implementation should start from a plan or candidate set
- regressions should trigger focused debugging loops
- parallelism should be controlled by workflow structure, not improvisation

## 7. OpenClaw

Forge placement:
- Addon-first benchmark.
- Its async multi-channel operator surface should become a Forge Addon capability, not Core.

What to adapt:
- asynchronous operation across different interfaces.
- persistent operator surface.
- memory that survives beyond a single prompt.
- skills and channels as operational surfaces instead of hard-coded modes.

Example shape:
```text
operator -> channel -> workflow -> interrupt -> resume -> audit
```

Forge target:
- each channel becomes an Addon or event adapter
- workflow state survives across UI surfaces
- human and AI can work in different views on the same workflow

## 8. Hermes

Forge placement:
- Addon-first benchmark.
- File-first memory and semantic retrieval should be exposed through governed memory/search capabilities.

What to adapt:
- file-first memory.
- semantic search over file content.
- learning loop for skills that improve over time.
- execution that can work with persistent artifacts and refs.

Example shape:
```text
memory scope:
  global
  organization
  project
  processing
```

Forge target:
- split memory by scope and audience
- keep project memory in `.forge`
- keep processing memory temporary when appropriate
- promote only evidence-backed reusable skills

## 9. credential-vault

Forge placement:
- Core security boundary reference.
- This is a runtime dependency contract, not a feature Addon.

What to adapt:
- visible contract for secret access.
- safe terminal injection.
- local encrypted data file with controlled reads.
- no secret values in chat or logs.

Forge target:
- all credential-dependent workflows should be routed through the vault contract
- skill/tool surfaces should ask Forge for secrets only through controlled channels

## 10. telegram-delivery

Forge placement:
- Addon/workflow delivery reference.
- Delivery is optional by workflow, never a Core always-on dependency.

What to adapt:
- artifact delivery as a first-class completion step.
- document/message delivery with returned ids.
- the handoff should happen after verification, not before.

Forge target:
- keep Telegram delivery as a workflow output, not an ad hoc side effect
- return message/document ids to the operator

## 11. Forge-native SDKs

Forge placement:
- Core contract, distributed as language bindings.
- SDKs are not Addons; they are consumption surfaces for the same Forge model.

What to build:
- TypeScript SDK for workflows and subworkflows.
- Python SDK for async orchestration and graph composition.
- Go SDK for service-side orchestration.
- Rust SDK for native workflows and embedded runtime control.

Required shape:
```ts
// conceptual shape
const flow = forge.workflow("demo");
await flow.run(input);
await flow.call("rust_subflow", payload);
```

Target behavior:
- any language can start a flow
- flows can call subflows in other languages
- async functions should be first-class
- parallel subflows should converge into a final aggregator

## 12. Installer

Forge placement:
- Distribution layer, not a runtime Addon.
- The installer packages Core and SDKs but should not change workflow semantics.

What to build:
- Linux, macOS, and Windows install path.
- release packaging for `forge`.
- one command install/update path.

Target behavior:
- the same Forge binary entrypoint should be installable on every target OS
- the install path should preserve the current shell-visible executable contract

## 11. Current status

Already landed in this worktree:
- chat session persistence for the Forge TUI
- unique chat session code generation
- `/resume` in the TUI for saved chat sessions
- chat session save/load tests
- brain routing that keeps workflows, addons, and skills visible to the decision layer

Next step:
- run the full regression suite and then push the report through Telegram.
