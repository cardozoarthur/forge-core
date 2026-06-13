# Forge Goal Gap Analysis

Data: 2026-06-13

## Fonte Da Verdade

Documentos oficiais lidos integralmente:

- `/home/arthur/Downloads/goal1.md`
- `/home/arthur/Downloads/goal2.md`
- `/home/arthur/Downloads/goal3.md`

## Leitura Consolidada

Os três documentos convergem para a mesma tese:

- Forge é uma infraestrutura operacional AI-native, não um wrapper de LLM.
- Forge é workflow-first, event-driven, multi-tenant, domain-agnostic e human-in-the-loop.
- O Core deve permanecer mínimo e universal.
- Domain specificity deve viver em Addons.
- A interface deve ser uma TUI de classe mundial, comparável ou superior a Gemini CLI, Codex CLI, Claude CLI e OpenCode.
- Context routing, personality routing, memory governance, handoffs, checkpoints, waits e resume são contratos centrais, não detalhes auxiliares.

## Estado Atual Observado

### Já existe e está forte

- Workflow store, DAG, waits, schedules, resume, checkpoints e rework validation.
- Context routing versionado com lineage e contrato de personalidade.
- Identity, memory governance e tenant policy com organização, marca, produto, usuário e canal.
- Addon registry, capability resolution, lifecycle, permissions, marketplace local e runtime contracts.
- Harness Forge-first com wrapper plans, shims, headroom e session lifecycle.
- Observability rica com costs, events, structured logs, release gates e architecture compass.
- TUI fullscreen orchestrator-first com fallback nativo e smoke de TTY/JSON.

### Ainda parcial ou futuro

- Renderer nativo OpenTUI ainda não é o backend principal; o Forge continua com fallback `crossterm` e uma estratégia de ponte.
- Algumas heurísticas first-party continuam no Core como compatibilidade transitória.
- A camada event-driven já é forte, mas nem todos os canais citados na spec têm adaptadores produtivos completos.
- Remote execution, remote registry mirrors, auto-update de Addons, WASM plugins e transports específicos seguem como futuro.
- A modelagem organizacional já existe, mas certos foreign keys explícitos por workflow/run/artifact/event ainda aparecem como próximos passos em documentos técnicos.

## Conflitos E Dependências

1. Core mínimo versus heurísticas legadas.
   - O caminho correto é manter Addons first-party de compatibilidade e mover o que for específico para extensões declarativas.

2. TUI bonita versus dependência pesada.
   - O Forge precisa continuar funcional sem Bun/Zig, então o renderer avançado deve entrar por ponte, não como requisito duro.

3. Multi-tenant forte versus conveniência local.
   - O ambiente precisa derivar organização/marca/produto/identidade do contexto, mas sem quebrar uso local simples.

4. Headroom e wrappers versus execução direta.
   - O Forge deve continuar podendo operar como runtime central mesmo quando o usuário escolhe Codex, Claude, Gemini ou OpenCode como brain de execução.

## Oportunidades De Reuso

- `src/interactive.rs`: architecture compass, task board, event runtime, operational cockpit, release gates, add-on surfaces.
- `src/harness.rs`: token headroom, wrapper plans, adoption plan, bootstrap, activation profile, executor compatibility.
- `src/memory.rs`: organization/project/processing scope, retention, promotion, privacy and shareability.
- `src/addon.rs`: capability discovery, runtime contracts, lifecycle and permissions.
- `src/mcp.rs`: external agent surface and stable contract distribution.

## Benchmark De Ideias

Os benchmarks mais úteis não são copiáveis; eles viram contratos Forge-owned:

- Gemini CLI: terminal-first UX, project context, streamability, checkpoint-oriented interaction.
- Codex CLI: local execution pragmática, edição/testes e sandbox.
- Claude CLI: long-running command flow, dynamic workflow feel and operator-friendly interaction.
- OpenCode: provider abstraction plus a polished shell UX.
- OpenClaw: asynchronous, multi-channel, persistent operator surface.
- Hermes Agents: file-first memory and semantic retrieval.
- OpenSquad: multi-agent collaboration cards, boards and parallel handoffs.
- Open Design / Penpot / Paperclip: design tokens, component composition, brand/product context and company-operating thinking.
- Remotion: programmable pipeline and artifact-oriented generation.
- n8n: trigger/action model, schedules, webhooks and marketplace concepts.
- headroom: reversible compression, wrapper interception and token-budget awareness.

### SDK And Installer Gap

The goal also asks for cross-language SDKs and a distributable installer. The current repo is Rust-first, so the next step is to make the Forge contract language-agnostic before building bindings:

- TypeScript SDK: workflow creation, subworkflow composition, async orchestration and browser/client-friendly helpers.
- Python SDK: async flow invocation, parallel subflows and durable resume primitives.
- Go SDK: service-side flow control, fan-out/fan-in composition and integration with existing infrastructure.
- Rust SDK: native embedding, lower-level runtime control and local composition for server or desktop tools.
- Installer: one install/update story for Linux, macOS and Windows that preserves the shell-visible `forge` entrypoint and does not fork product semantics per OS.

The first implementation should keep the contract in one place and make every language consume the same workflow model, not invent its own surface.

## Incremental Plan

### 1. Stabilize The Kernel Contract

- Keep workflow/state/event/memory/identity/context contracts small and universal.
- Preserve dynamic workflows, checkpoints, waits, resume and human approvals.
- Avoid reintroducing domain-specific heuristics into Core.

### 2. Finish The TUI Contract

- Keep the orchestrator-first fullscreen TUI as the default shell entrypoint.
- Bridge to OpenTUI later for richer rendering while keeping a zero-dependency fallback.
- Expand visuals for DAG, handoffs, checkpoints, waits, costs and timeline without hiding workflow state.

### 3. Strengthen The Harness

- Treat token headroom as a first-class guard for large outputs and CLI wrappers.
- Keep wrapper plans, shims, activation profiles, session lifecycle and lineage explicit.
- Use Forge-first policies to make external CLIs behave like controlled brains under Forge authority.

### 4. Push Domain Behavior To Addons

- Move specialized workflows, triggers, views, artifacts and runtime contracts into Addons.
- Keep built-in compatibility layers only where needed for migration.
- Favor capability-driven planning over domain-specific branch logic.

### 5. Expand Event And Multi-Tenant Boundaries

- Continue pulling channel adapters, inbox semantics and tenant isolation into explicit contracts.
- Make organization/brand/product context unavoidable in workflows and artifacts.
- Ensure approvals, handoffs and memory governance remain auditable at runtime.

### 6. Scaffold SDK And Installer Contracts

- Define a small, shared workflow transport contract that every SDK can encode.
- Add a repository-owned distribution layout for language SDKs, even if the first version is thin.
- Define installer/package metadata early so CI and release automation can target the same binaries and archives.

## Practical Next Step

The next implementation increment should be small, architecture-safe and visible:

- add or refine one capability or adapter that improves Forge-first CLI adoption;
- preserve the current TUI and harness contracts;
- publish the change with tests and a push so the repo stays in a releasable state.
