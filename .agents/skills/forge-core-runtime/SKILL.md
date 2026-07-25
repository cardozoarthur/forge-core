---
name: forge-core-runtime
description: Forge Core runtime workflows, request lifecycle, handoff, schedules, validation and rework.
license: MIT
compatibility: codex, opencode, agy, claude
---

## Runtime Contract

Forge owns workflow state. CLIs and models are execution engines.

Use:

```bash
forge request start --goal "<objective>" --origin codex --output json
forge request step --run <run-id> --executor codex --ttl-seconds 300 --origin codex --output json
forge task handoff --workflow <workflow-id> --task <task-id> --executor codex --view compact --output json
forge mcp call forge.task.handoff --input '{"workflow_id":"<workflow-id>","task_id":"<task-id>","executor":"codex"}' --output json
forge request complete-task --run <run-id> --task <task-id> --executor codex --summary "<validated evidence>" --origin codex --output json
forge schedule worker-status --output json
forge validate --workflow <workflow-id> --output json
```

Completion means the task goal is definitively ready and validation has no rework tasks. If validation fails, return the task to work with the rework reason.

`forge request step` is a supervised routing boundary, not evidence that the
task ran. Command, wait, and notification tasks must be executed by their real
executor and completed with `forge request complete-task --evidence-command
"<passing gate>"`; Forge returns `handoff_required` when that receipt is absent.
