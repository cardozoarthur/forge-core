---
name: foundry-core-runtime
description: Foundry Core runtime workflows, request lifecycle, handoff, schedules, validation and rework.
license: MIT
compatibility: codex, opencode, agy, claude
---

## Runtime Contract

Foundry owns workflow state. CLIs and models are execution engines.

Use:

```bash
foundry request start --goal "<objective>" --origin codex \
  --lane frontend=agy:3 --lane backend=codex:5 \
  --max-parallel-agents 8 --output json
foundry request step --run <run-id> --executor codex --ttl-seconds 300 --origin codex --output json
foundry request execute-wave --run <run-id> --executor auto --context-budget 4096 \
  --max-parallel <admitted-task-count> --allow-exec --approved-by <operator> \
  --reason "execute dependency-ready task worktrees" --output json
foundry task handoff --workflow <workflow-id> --task <task-id> --executor codex --view compact --output json
foundry mcp call foundry.task.handoff --input '{"workflow_id":"<workflow-id>","task_id":"<task-id>","executor":"codex"}' --output json
foundry request complete-task --run <run-id> --task <task-id> --executor codex --summary "<validated evidence>" --evidence-command "<passing gate>" --evidence-exit-code <observed-exit-code> --origin codex --output json
foundry schedule worker-status --output json
foundry validate --workflow <workflow-id> --output json
```

Explicit request lanes use the same Core materializer as `foundry plan` and MCP
`foundry.run.start`, and persist under `core_orchestration.parallel_team`.
Omitting lanes preserves the serial generic DAG. Reject ambiguous declarations
and `auto` lane executors rather than guessing. Declared agent capacity is only
an admission ceiling: Foundry dispatches dependency-ready tasks, and every
mutating task still needs its own guarded worktree.

Completion means the task goal is definitively ready and validation has no rework tasks. If validation fails, return the task to work with the rework reason.

`foundry request step` is a supervised routing boundary, not evidence that the
task ran. Every task must be executed by its real executor and completed with
`foundry request complete-task --evidence-command "<passing gate>"
--evidence-exit-code <observed-exit-code>`. The exit code must be the value
actually observed by the caller; Foundry never fabricates `0`. Foundry returns
`handoff_required` while execution evidence is absent.

`foundry request execute-wave` is the bounded real-process bridge for an admitted
dispatch frontier. It can start several Codex and Agy tasks concurrently only
after explicit process authorization, executor policy/quota admission and a
distinct task-scoped Git worktree claim for every mutating task. Runtime
receipts remain separate from `complete-task`: process success never promotes a
task without reviewed validation evidence.
