---
name: forge-core-context
description: Forge Core bounded context, memory scope, brand identity, personality routing, deferred discovery and node-scoped context routing.
license: MIT
compatibility: codex, opencode, agy, claude
---

## Context Contract

Do not rediscover all MCP servers, skills, memory or CRM records for every node. Ask Forge for a bounded context packet and obey its router.

Use:

```bash
forge context --workflow <workflow-id> --task <task-id> --project-root <project-root> --budget 1200 --strict --view compact --output json
forge mcp call forge.context.request --input '{"workflow_id":"<workflow-id>","task_id":"<task-id>","project_root":"<project-root>","view":"compact"}' --output json
forge memory policy --project-root <project-root> --output json
forge memory search --workflow <workflow-id> --query "<query>" --scope project --audience manager --memory-level short_term --output json
```

In the compact view, read `selected_source_ids`, `deferred_source_ids`, `expand_commands`, `instruction_contract` and `guardrail` before loading more context. Use the full view only when an audit needs `forge.context.router.v1`, `context_router`, `deferred_discovery` or `search_tags`. CRM/user records require a bound subject marker such as `bound_crm_subject`, `user_id`, `lead_id`, `contact_id` or `account_id`.

Use the compact view for executor handoff. It preserves bounded `content`, the instruction contract, selected/deferred source IDs, routing economy, continuation state and an actionable `guardrail`, without loading the full audit manifest. When blocked, work only the bounded predecessor frontier returned by `guardrail.next_commands`; after those tasks change state, request compact context again instead of handing off the still-blocked current task. Request the full view only for routing audits, replay diagnostics or lineage inspection.

## Context Memory Scope

Forge context supports multiple memory tiers:
- **Short-Term Memory**: Node and task-specific execution history. Transient, cleared or compressed after execution.
- **Workflow Memory**: Persistent across the lifetime of a specific workflow instance. Holds task states, inputs, and intermediate conclusions.
- **Project Memory**: Codebase patterns, environment constraints, and project rules defined in `PROJECT.md` and `AGENTS.md`.
- **Global Memory**: User-level preferences, cross-project policies, and credentials.

## Brand Identity and Personality Routing

### Brand Identity
All execution outputs, messages, and reports must adhere to the Forge brand identity:
- **Tone**: Direct, technical, precise, and objective. Avoid friendly chatbot filler or conversational fluff.
- **Authority**: Maintain Forge as the ultimate orchestrator. CLI executors are execution substrates, not the core authority.
- **Designation**: Identify as a structured execution module of Forge Core.

### Personality Routing
Forge routes tasks to specific agent profiles (souls) based on the context requirements:
- **Analytical**: Best for planning, architecture design, and strategic reporting tasks.
- **Implementer**: Best for code writing, Rust programming, database queries, and script execution.
- **QA/Auditor**: Best for testing, clippy checking, validation, and contract conformance.

Personality is routed from the revisioned workflow task. Read `instruction_contract.persona_mode` and `instruction_contract.persona_profile_id` in compact responses; do not invent a CLI override outside workflow state.
