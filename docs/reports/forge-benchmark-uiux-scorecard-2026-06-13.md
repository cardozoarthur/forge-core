# Forge Benchmark UI/UX Scorecard

Data: 2026-06-13

This scorecard condenses the benchmark exploration into a practical view: code/contract strengths, UI/UX strengths, risks, and the Forge decision.

## Reading Rules

- `Code/contract` means API shape, runtime model, execution semantics, and composition patterns.
- `UI/UX` means operator flow, discoverability, density, clarity, and session behavior.
- `Risk` means what can go wrong if Forge copies the benchmark too literally.
- `Decision` means what Forge should absorb and where it belongs.

## Core Benchmarks

| Benchmark | Code/contract strengths | UI/UX strengths | Risk | Forge decision |
|---|---|---|---|---|
| Codex | Clear exec/review split, `resume`, sandbox/approval boundaries | Fast local loop, direct action feel | Overfits to code-only work | Keep as Core execution brain reference |
| Gemini CLI | Persistent sessions, prompt/headless mode, approval policy | Shell-first, simple interaction | Hidden mode boundaries if not surfaced | Keep chat-first with explicit shell mode |
| OpenCode | `--continue`, `--session`, `--fork`, `serve`, provider/agent model | Clean default TUI, compact operator surface | Compact UI can hide state | Use as default TUI UX reference |
| Claude CLI | Named sessions, resume, plan mode, long session memory | Persistent assistant feel | Plan/edit boundary can be tool-specific | Absorb naming, resume and approval boundaries |
| LangGraph | Stateful graphs, checkpoints, subgraphs, time-travel | Strong mental model for resumable work | Can feel abstract in UI | Make every workflow a graph |
| LangChain | Middleware, context engineering, tool handling | Highly composable agent harness | Easy to over-abstract the agent | Use middleware-like routing and guardrails |

## Addon-First Benchmarks

| Benchmark | Code/contract strengths | UI/UX strengths | Risk | Forge decision |
|---|---|---|---|---|
| OpenClaw | Async, multi-channel, durable handoff | Collaboration across surfaces | Channel sprawl | Treat as Addon-first multi-channel surface |
| Hermes | File-first memory, semantic retrieval, scopes | Local-first memory behavior | Memory dump risk | Keep project/global/processing scopes |
| Open Design | Artifact-centric workflows | Visual artifact thinking | Kernel scope creep | Addon for creative/artifact workflows |
| Penpot | Tokens, components, systemized design | Strong design-system UX | Heavy UI if pulled into core | Addon for design-system artifacts |
| Paperclip | Document queues, audit trail, encryption-in-use, straight-through processing | Business workflow UX | Rigid document systems | Addon for document operations; Core keeps workflow contract |
| Remotion | Parameterized templates, preview/render separation | Artifact pipeline clarity | Media-tool drift | Benchmark artifact pipelines, not media runtime |

## Dual-Use Benchmarks

| Benchmark | Code/contract strengths | UI/UX strengths | Risk | Forge decision |
|---|---|---|---|---|
| n8n | Trigger/action graph, schedules, nodes | Node-based visual automation | Graph noise without hierarchy | Dual-use: Core workflow semantics + Addon interoperability + workflow UI inspiration |
| Obsidian | Backlinks, local-first graph/canvas model | Local knowledge UX, canvas, linked notes | Passive note vault if relationships are not actionable | Dual-use: linked context model + canvas/graph surfaces |
| OpenSquad | Multi-agent collaboration, explicit handoffs | Visible boards and ownership | Ownership can get hidden | Dual-use: collaboration surface and workflow graph |

## Process / Support Benchmarks

| Benchmark | Code/contract strengths | UI/UX strengths | Risk | Forge decision |
|---|---|---|---|---|
| superpowers | Brainstorming, debugging, verification, worktrees | Process discipline made explicit | Skills become inconsistent if hidden | Core process gates, not magic behavior |
| installed skills and plugins | Reusable capability packs | Task-specific surfaces | Behavior drift across CLIs | Convert to Forge-owned workflows/context packs/adapters |
| credential-vault | Brokered secret access | Controlled injection boundary | Secret leakage if ambient env is used | Core security dependency contract |
| telegram-delivery | Artifact handoff with receipts | Clear completion notification | Side-effect risk if always-on | Optional delivery Addon; workflow-owned |
| headroom | Context compression, wrapper interception, token budgeting | Keeps operator surfaces useful by shrinking noise | Over-compression can hide important facts | Core context policy in router/harness |

## Forge Technical Decisions

1. `/resume` and unique chat codes stay public-facing Forge behavior.
2. File creation is a workflow, not a raw write.
3. Benchmarks are classified as `Core`, `Addon-first`, or `dual-use`.
4. SDKs must share a single workflow model across TypeScript, Python, Go and Rust.
5. The installer must keep the same `forge` entrypoint across Linux, macOS and Windows.
6. UI must remain compact by default and reveal complexity on demand.

## Implementation Targets

- `src/opencode_tui.rs`: resume, chat code, on-demand autocomplete, benchmark surface.
- `src/interactive.rs`: workflow DAG, task board, artifacts, approvals, schedules.
- `src/executor.rs`: brain routing and executor visibility.
- `src/harness.rs`: token headroom, shims, wrapper policy.
- `src/memory.rs`: scope-aware memory and retrieval.
- `src/addon.rs`: optional capability surfaces.
- `sdk/`: cross-language workflow client contract.
- `installer/`: release-based distribution contract.

## Outcome

The benchmark exploration is now reduced to an implementation map.
The remaining work is execution, release packaging, and incremental product hardening.
