---
name: forge-core-workflow
description: Forge Core workflow management, including context updates, artifact management, task creation/prioritization, and dependency resolution.
license: MIT
compatibility: codex, opencode, gemini, claude
---

## Workflow Contract

Workflows in Forge are mutable at runtime but must remain fully auditable and revision-controlled. Every workflow mutation must be tracked in the Forge state store.

## Creating Workflows

Create a workflow using a strategic goal description. This decomposes the goal into tasks and initializes the workflow graph:
```bash
forge plan --goal "Design and build a Rust operational runtime" --output json
```

This returns a workflow ID and a list of structured tasks.

## Updating Context

Before invoking any task execution node, load and bind context data. Bounded context packages must stay small and respect local budget limits:
```bash
forge context --workflow <workflow-id> --task <task-id> --project-root <project-root> --budget 1200 --strict --output json
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
forge workflow add-task --workflow <workflow-id> --description "Implement schema validation" --priority high --output json
```

## Prioritizing Tasks and Subtasks

Task scheduling follows priority rules (`high`, `medium`, `low`) and explicit graph dependencies.
Use the CLI to adjust priority:
```bash
forge workflow set-priority --workflow <workflow-id> --task <task-id> --priority high --output json
```

## Managing Dependencies and Impediments

### Graph Dependencies
A task cannot begin execution until all of its parent dependencies are marked `DONE`.
Add dependency links:
```bash
forge workflow add-dependency --workflow <workflow-id> --task <task-id> --depends-on <parent-task-id> --output json
```

### Impediments
If a task is blocked (e.g. requires human authorization or resource configuration), mark it with an impediment:
```bash
forge workflow set-impediment --workflow <workflow-id> --task <task-id> --reason "Awaiting Docker policy allowance" --output json
```
Removing the impediment clears the block, making the task ready for execution.
```bash
forge workflow clear-impediment --workflow <workflow-id> --task <task-id> --output json
```
