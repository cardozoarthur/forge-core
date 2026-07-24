---
name: forge-core-addons-ui
description: Forge Core Addons, renderer events, TUI/web operational panels and interactive surfaces.
license: MIT
compatibility: codex, opencode, gemini, claude
---

## Addon And UI Contract

Domain features belong in Addons. Core exposes stable runtime, permission, renderer, event and inspection contracts.

Use:

```bash
forge ops snapshot --project-root <project-root> --output json
forge ops renderer-event --workflow <workflow-id> --addon <addon-id> --view <view-id> --event-kind refresh_requested --payload '{"refresh":true}' --output json
forge mcp call forge.ops.snapshot --input '{"project_root":"<project-root>"}' --output json
forge mcp call forge.ops.addon_renderer_event --input '{"workflow_id":"<workflow-id>","addon_id":"<addon-id>","view_id":"<view-id>","event_kind":"refresh_requested","payload":{"refresh":true}}' --output json
forge interactive home --project-root <project-root> --output json
forge mcp call forge.interactive.home --input '{"project_root":"<project-root>"}' --output json
forge interactive action-dispatch --action <action-id> --project-root <project-root> --payload '{"goal":"Run the action hook workflow"}' --origin forge_cli --output json
forge mcp call forge.interactive.action_dispatch --input '{"action_id":"<action-id>","project_root":"<project-root>","payload":{"goal":"Run the action hook workflow"}}' --output json
```

Before rendering CRM or other product UIs, resolve Addon capability boundaries and keep mutations routed through Forge workflows, permissions and events. Use action dispatch only for Forge-owned hook routing: workflow hooks start Forge workflows, while CLI brain hooks are routed as Forge harness plans and are not executed directly.
