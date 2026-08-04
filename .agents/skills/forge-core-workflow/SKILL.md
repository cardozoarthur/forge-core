---
name: forge-core-workflow
description: Forge Core workflow management, including context updates, artifact management, task creation/prioritization, and dependency resolution.
license: MIT
compatibility: codex, opencode, agy, claude
---

## Workflow Contract

Runtime workflow mutations must be revisioned, auditable, and tracked in the Forge state store.

## Creating Workflows

Plan a strategic goal into its task graph:
```bash
forge plan --goal "Design and build a Rust operational runtime" --output json
```

It returns the workflow ID and tasks.

Declare an independent N:N team in the plan:

```bash
forge plan --goal "Deliver frontend and backend" \
  --lane frontend=agy:3 --lane backend=codex:5 \
  --max-parallel-agents 8 --output json
```

`core_orchestration.parallel_team` records it. No `--lane` keeps the serial DAG.
Never infer lanes from prose or assign `auto`; agents start only through
explicitly selected detached execution.

## Updating Context

Before invoking any task execution node, load and bind context data. Bounded context packages must stay small and respect local budget limits:
```bash
forge context --workflow <workflow-id> --task <task-id> --project-root <project-root> --budget 1200 --strict --view compact --output json
```

This commands Forge to route relevant memory, environment variables, and state markers to the executing agent.

## Adding/Attaching Artifacts

Artifacts are the concrete outputs of workflow tasks. Always attach artifacts to register them in the workflow lineage:
```bash
forge workflow attach-artifact --workflow <workflow-id> --artifact <path> --kind report --tag <tag> --origin codex --output json
```

Tags must be used to indicate artifact type, account/customer context, and search intent.

## Adding and Managing Tasks and Subtasks

You can dynamically add tasks or subtasks to an active workflow:
```bash
forge workflow add-task --workflow <workflow-id> --description "Implement schema validation" --priority high --expected-revision <revision> --origin codex --output json
forge workflow update-task --workflow <workflow-id> --task <task-id> --goal "Reach the revised task goal" --expected-revision <revision> --origin codex --output json
```

Use the current workflow revision when several operators or agents may mutate
the same graph. A stale `--expected-revision` fails without changing state.

## Prioritizing Tasks and Subtasks

Task scheduling follows priority rules (`high`, `medium`, `low`) and explicit graph dependencies.
Use the CLI to adjust priority:
```bash
forge workflow set-priority --workflow <workflow-id> --task <task-id> --priority high --expected-revision <revision> --origin codex --output json
```

## Managing Dependencies and Impediments

### Graph Dependencies
A task cannot begin execution until all of its parent dependencies are marked `DONE`.
Add dependency links:
```bash
forge workflow add-dependency --workflow <workflow-id> --task <task-id> --depends-on <parent-task-id> --expected-revision <revision> --origin codex --output json
forge workflow remove-dependency --workflow <workflow-id> --task <task-id> --depends-on <parent-task-id> --expected-revision <revision> --origin codex --output json
```

### Impediments
If a task is blocked (e.g. requires human authorization or resource configuration), mark it with an impediment:
```bash
forge workflow set-impediment --workflow <workflow-id> --task <task-id> --reason "Awaiting Docker policy allowance" --expected-revision <revision> --origin codex --output json
```
Without `--impediment`, clearing removes only `manual` impediments. Resource,
authorization and policy impediments require their explicit id so an operator
cannot accidentally bypass a runtime or governance gate.
```bash
forge workflow clear-impediment --workflow <workflow-id> --task <task-id> --expected-revision <revision> --origin codex --output json
```
