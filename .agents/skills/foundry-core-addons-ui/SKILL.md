---
name: foundry-core-addons-ui
description: Foundry Core Addons, renderer events, TUI/web operational panels and interactive surfaces.
license: MIT
compatibility: codex, opencode, agy, claude
---

## Addon And UI Contract

Domain features belong in Addons. Core exposes stable runtime, permission, renderer, event and inspection contracts.

Use:

```bash
foundry ops snapshot --project-root <project-root> --output json
foundry ops renderer-event --workflow <workflow-id> --addon <addon-id> --view <view-id> --event-kind refresh_requested --payload '{"refresh":true}' --output json
foundry mcp call foundry.ops.snapshot --input '{"project_root":"<project-root>"}' --output json
foundry mcp call foundry.ops.addon_renderer_event --input '{"workflow_id":"<workflow-id>","addon_id":"<addon-id>","view_id":"<view-id>","event_kind":"refresh_requested","payload":{"refresh":true}}' --output json
foundry interactive home --project-root <project-root> --output json
foundry mcp call foundry.interactive.home --input '{"project_root":"<project-root>"}' --output json
foundry interactive action-dispatch --action <action-id> --project-root <project-root> --payload '{"goal":"Run the action hook workflow"}' --origin foundry_cli --output json
foundry mcp call foundry.interactive.action_dispatch --input '{"action_id":"<action-id>","project_root":"<project-root>","payload":{"goal":"Run the action hook workflow"}}' --output json
```

Before rendering CRM or other product UIs, resolve Addon capability boundaries and keep mutations routed through Foundry workflows, permissions and events. Use action dispatch only for Foundry-owned hook routing: workflow hooks start Foundry workflows, while CLI brain hooks are routed as Foundry harness plans and are not executed directly.
