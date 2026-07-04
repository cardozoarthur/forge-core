# Forge Ecosystem Strategic Report

**Date**: July 3, 2026  
**OS Version**: Linux  
**Status**: Milestone 1 (Strategic Analysis) Completed  

---

## 1. Executive Summary

The Forge ecosystem is a next-generation, AI-native, workflow-first, event-driven, and multi-tenant operational runtime designed to coordinate AI and non-AI task execution. Rather than acting as a simple chatbot wrapper, Forge is positioned as the central orchestration authority, keeping agents and tools bounded while retaining validation and promotion gates.

The ecosystem is split into three main components:
1. **`forge-core`**: The universal, domain-agnostic Rust workflow runtime. It owns objective decomposition, dependency graphs, context routing, execution, memory governance, and validation.
2. **`forge-flow`**: The historical alpha implementation (written in Python/FastAPI/Next.js/Go/n8n) that serves as a rich repository of automated digital product-creation flows, serving as a functional reference for the core's expansion.
3. **`forge-crm`**: The first reference product proving the extensibility of Forge Core. It is implemented as a Forge Addon declaring capabilities, permissions, event adapters, views, and workflows, demonstrating that CRM-specific features can be layered on top of Forge without polluting the universal kernel.

This report details the currently implemented capabilities of each repository, maps planned/missing features, analyzes architectural alignment, and proposes concrete next steps to guide the ecosystem toward production-level enterprise readiness.

---

## 2. Features Currently Implemented

### 2.1. Forge Core (`forge-core`)
`forge-core` has evolved from a local Rust CLI to an operational kernel with native SQLite persistence, an MCP tool surface, and a comprehensive TUI framework. The following key modules are implemented:
*   **Intent Parser & Requirements Extractor (`intent.rs`)**: Normalizes user-facing goals into `forge.intent.v2` specs, extracting required capabilities, event policies, deliverables, risks, and unknowns.
*   **Atomic Task Graph & Work Item Controller (`graph.rs`, `workflow.rs`)**: Decomposes workflows into dependency-aware retryable tasks with explicit goals, backlog states, impediments, and acceptance criteria.
*   **Context Routing Engine (`context.rs`)**: Compresses and shards context into minimal correct packets under a budget (`forge.context.v30`), containing versioned persona contracts, execution policies, and lineage hashes to reduce model hallucination and costs.
*   **Event Engine & Webhook Ingress (`event.rs`)**: Hosts a tenant-aware inbox (`event_inbox`) that routes events such as `start_workflow`, `continue_workflow`, and `complete_workflow`. Includes a real local HTTP POST webhook ingress listener and a generic egress emitter for Telegram (via Bot API) and HTTP(S) with env-backed or credential-vault Bearer/HMAC authentication.
*   **Addon Catalog & Registry Lifecycle (`addon.rs`)**: Resolves required capabilities to first-party/project-local Addons (`.forge/addons`), validates dependency version constraints (using common operators), persists authorizations (`addon_permission_authorizations`), and indexes capabilities in SQLite. Packages Addons (`forge addons package`) with signature verification (Ed25519) and trust store management.
*   **CLI Harness & Wrapper (`harness.rs`)**: Provides PATH shims and wrapper plans (`wrap-plan`) for Codex, Claude, Gemini, and OpenCode, enforcing project policies (like `require_lineage_for_exec` and token headroom compression).
*   **Cost OS & Materialization Ledger (`cost.rs`)**: Aggregates planned/observed costs and tokens. Materializes costs into a SQLite ledger index (`cost_ledger_index`) supporting daemon-based updates, retention policies, and time rollups.
*   **Interactive TUI & Dashboard (`interactive.rs`)**: Exposes structured UI panels (Task Board, Workflow DAG, Structured Logs, Patch Workbench, Identity Center, Permissions Center, and Harness Doctor) in CLI/MCP.
*   **Memory Governance (`memory.rs`)**: Governs organization/project/processing scopes, enforcing privacy and providing curation tools for promoting short-term processing memory to long-term memory.
*   **Controlled Improvement Loop (`improve.rs`)**: Automatically recommends optimizations (e.g. tightening context, adding validators) from historical timeline events, benchmark results, and rollback plans without auto-promoting changes.

### 2.2. ForgeFlow (`forge-flow`)
`forge-flow` serves as a product creation workspace focused on human-in-the-loop (HIL) coordination and visual automations.
*   **Next.js Structured Chat UI**: Serves as the interactive interface for product creation, providing rich messaging, inline previews, and human decision steps.
*   **Go Runtime API**: Manages operational calendars, resource bookings, availability filters, and cost/token accounting.
*   **n8n Visual Automation**: Integrates n8n as the workflow designer, executing subflows, triggers, and loops. Includes custom n8n nodes for querying Paperclip agents, tasks, workspaces, and costs.
*   **Orchestrator**: Integrates Telegram alerts for human approvals, packages reviews, records execution lineage, and enables replay.
*   **Hetzner Ephemeral Provisioning**: Spawns temporary virtual machines to build and run heavy frontend/backend demos, routing them to the user via Traefik.
*   **GitHub Integration**: Dynamically requests user repository tokens to automate repository bootstrapping, PR creation, and release tags.

### 2.3. Forge CRM (`forge-crm`)
`forge-crm` is a workflow-backed business application built entirely as a Forge Addon.
*   **Addon Configuration (`addons/forge-crm.json`)**: Declares CRM-specific capabilities (`source_code_patch_lifecycle`), permissions, views (`crm.ops-console`), and event adapters.
*   **CRM Worker (`runtime/crm-worker.mjs`)**: Implements an `external_api` worker responding to planner, executor, validator, and handoff contracts over HTTP.
*   **Tenant Bootstrapper & Pack (`scripts/crm-workflow-pack-lib.mjs`)**: Orchestrates the sales pipeline, customer enrichment, omnichannel communication, SLA triages, and goal commission settlements via Forge workflows.
*   **Operating Model Snapshot**: Generates a unified operating state (`crm_operating_snapshot`) showing relationships, Kaban pipelines, and document queues.
*   **Web Console (`web/`)**: Renders static dashboards, data relationship graphs, and Forge action triggers from the operating snapshot without a local database.
*   **TUI Operational Cockpit (`crm.operational-cockpit`)**: Integrates CRM actions into the Forge terminal UI for lead classification, message ingestion, proposal generation, and document validation.

---

## 3. Features Currently Missing or Planned

The following features represent gaps between the current local/simulated implementation and the ultimate target architecture:

1.  **WASM Sandbox & Plugins**:
    *   *Current State*: External Addon runtime contracts (`wasm` and `external_api`) are routed as `needs_external_worker`. `external_api` execution is supported via HTTP/HTTPS TCP sockets.
    *   *Planned*: Native in-process WASM executor to run untrusted Addon code directly within the Rust CLI shell under strict resource constraints.
2.  **Real Remote / Distributed Execution**:
    *   *Current State*: `forge cluster` performs dry-run placement and handoff manifest generation. Remote execution and external mutations are hardcoded to `disabled` in scheduling leases.
    *   *Planned*: SSH adapters, remote agent executors, and active node synchronization across physical machines.
3.  **Strict Database Multi-Tenant Schema**:
    *   *Current State*: Multi-tenancy is enforced using the `tenant_index` projection. Workflows, runs, events, and artifacts check permissions against this registry.
    *   *Planned*: Deep database schema refactoring to introduce physical `organization_id`, `brand_id`, `product_id`, and `user_id` foreign keys on every operational table, making data leaks physically impossible.
4.  **Production Decoupled Event Workers & Channels**:
    *   *Current State*: Webhook ingress is local, and Telegram egress relies on inline `curl` execution.
    *   *Planned*: Decoupled production-grade message workers supporting WhatsApp, Slack, and email (SMTP/IMAP) with queue backoff and worker pools.
5.  **Remote Addon Registry Mirrors & Auto-updates**:
    *   *Current State*: Marketplace catalog loads from a local JSON index file and downloads packages to a local cache.
    *   *Planned*: Bounded remote package indexes with signature-locked mirrors, secure checksum verification, auto-updates, and automated migration/rollback workflows.
6.  **Interactive TUI & Web Client Synchronization**:
    *   *Current State*: `ops_console` and TUI panels compose views safely via defined layout schemas and event posts.
    *   *Planned*: Two-way, real-time reactive state synchronization between the Rust daemon, web clients, and TUI boards.
7.  **Real CLI Adapters with Sandbox Policies**:
    *   *Current State*: Shims PATH execution to capture stdout/stderr, verifying lineage. CLI integrations operate in dry-run or observe-only mode.
    *   *Planned*: Bounded execution of local developer tools (like Codex or Claude Code CLI) with strict timeout limits, sandboxed filesystem access, and classified retry logic.
8.  **Autonomous Self-Evolution Loops**:
    *   *Current State*: `forge improve` suggests optimizations into a markdown changelog, requiring manual apply/promote.
    *   *Planned*: Bounded auto-evolution where the loop can apply small optimizations, run tests/benchmarks in ephemeral environments, and promote them based on validated efficiency gains.

---

## 4. Architectural Alignment

The Forge ecosystem is architected around three core design principles:

### 4.1. Separation of Concerns
*   **universal vs. Domain-Specific**: The Rust kernel (`forge-core`) does not contain domain-specific knowledge (like n8n, CRM relationships, or code-editing hooks). The kernel exposes universal abstractions: DAG state, context packets, permission registries, and event timeline ledgers. Domain logic is declared in Addons (like `forge-crm` and `forge.addon.software_development`) via JSON/YAML manifests.
*   **Built-in Compatibility Layer**: The legacy planning heuristics are routed into a capability-first planner. Built-ins exist as first-party Addons, ensuring they are cataloged and governed using the same permission gates as external packages.

### 4.2. Control of Workflow Authority vs. Local Task Execution
*   **Forge as the Authority**: Forge owns the decomposed goal hierarchy, state machine, task dependency, validation rules, and context sharding.
*   **CLIs/Workers as Executors**: Agent CLIs (like Codex, Claude Code) and Addon workers (like the CRM node API) act as stateless execution engines. They receive a bounded packet containing only the required file context, validation commands, and budgets. They execute the task and return a structured audit receipt (`trace_ref`, exit code, tokens used). Forge decides whether the task passes validation.
*   **Harness Shim Gate**: The harness wraps external CLI invocations and shims the `PATH`. If a project sets `require_lineage_for_exec`, the shim blocks execution unless it receives explicit workflow/task/run metadata. This ensures developer agents cannot run arbitrary model loops outside the knowledge of the orchestrator.

### 4.3. Self-Evolution Governance
*   Forge governs its own optimization loop through bounded cycles. Rather than allowing agents to mutate prompt files directly:
    *   `forge self` enforces a mandatory stop date and budget.
    *   `forge improve` rank-orders optimization candidates based on actual event timeline observations (e.g. identifying high retries or context pressure).
    *   Changes are written to isolated experimental artifacts with rollback metadata. Promotion requires a validation benchmark pass and explicit operator approval.

---

## 5. Concrete Next Steps

To bridge the gaps and advance the Forge ecosystem toward a production-grade orchestration runtime, the following actions are recommended:

1.  **Transition from Dry-Run to Real CLI Adapters**:
    *   Implement process-isolated execution for Codex and OpenCode adapters, including stderr log capturing, SIGTERM cancellation hooks, and exit-code translation.
2.  **Enforce Mandatory Tenant Foreign Keys**:
    *   Perform a database migration on `forge.sqlite` to add direct tenant foreign keys (`organization_id`, `brand_id`, etc.) across `workflows`, `runs`, `events`, `artifacts`, and `task_leases`.
    *   Upgrade the query layer to enforce tenant isolation at the database level rather than through application-level filters.
3.  **WASM Runtime Integration**:
    *   Integrate a lightweight WebAssembly runtime (e.g. `wasmer` or `wasmtime`) into the Rust kernel.
    *   Define the host-guest API bindings to allow WASM-packaged Addon validators and executors to run inside the CLI securely.
4.  **Stand-Alone Event Daemon Service**:
    *   Decouple `events runtime-daemon` from the user terminal thread.
    *   Package it as a systemd/Docker service that monitors the `event_inbox`, schedules cron jobs, and manages child worker leases automatically.
5.  **Multi-Channel Communication Adapters**:
    *   Migrate the Telegram ingress webhook pattern from `forge-flow` into a generic event adapter in `forge-core`.
    *   Introduce Slack and Email ingress/egress workers running under leased worker pools.
