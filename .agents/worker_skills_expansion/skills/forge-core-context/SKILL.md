---
name: forge-core-context
description: Forge Core bounded context, memory, deferred discovery and node-scoped context routing.
license: MIT
compatibility: codex, opencode, gemini, claude
---

## Context Contract

Do not rediscover all MCP servers, skills, memory or CRM records for every node. Ask Forge for a bounded context packet and obey its router.

Use:

```bash
forge context --workflow <workflow-id> --task <task-id> --project-root <project-root> --budget 1200 --strict --output json
forge mcp call forge.context.request --input '{"workflow_id":"<workflow-id>","task_id":"<task-id>","project_root":"<project-root>","strict":true}' --output json
forge memory policy --project-root <project-root> --output json
forge memory search --workflow <workflow-id> --query "<query>" --scope project --audience manager --memory-level short_term --output json
```

Read `forge.context.router.v1`, `context_router`, `deferred_discovery`, `selected_source_ids`, `deferred_source_ids`, `search_tags` and `expand_commands` before loading more context. CRM/user records require a bound subject marker such as `bound_crm_subject`, `user_id`, `lead_id`, `contact_id` or `account_id`.
