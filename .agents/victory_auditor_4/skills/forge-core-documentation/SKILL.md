---
name: forge-core-documentation
description: Forge Core documentation standard for workflows, tasks, and nodes, including schemas and contract definitions.
license: MIT
compatibility: codex, opencode, gemini, claude
---

## Documentation Contract

All workflows, tasks, and nodes in Forge must be clearly and formally documented. Proper documentation ensures auditability, trace preservation, and precise agent execution interfaces.

## Documenting Workflows

A workflow represents the top-level orchestration structure. When documenting workflows, you must specify:
1. **Goal**: The high-level intent/objective of the workflow.
2. **Context**: Bounded context limits and memory boundaries.
3. **Artifacts**: Expected generated deliverables and their tags.
4. **Lineage**: The provenance and flow of information.

Use the `forge` CLI to plan and view workflow metadata:
```bash
forge plan --goal "<goal>" --output json
forge workflow status --workflow <workflow-id> --output json
```

## Documenting Tasks and Subtasks

Tasks are goal-oriented units within a workflow. Documenting tasks requires:
- **Task ID**: Unique Identifier (e.g., `task-101`).
- **Description**: Clear definition of done. What defines successful completion?
- **Dependencies**: Explicit IDs of prior tasks that must complete first.
- **Priority**: Relative priority of the task.
- **Impediments**: Blockers or issues preventing completion.

Format task definitions using the following structure:
```json
{
  "id": "task_id",
  "description": "Clear statement of goal and success criteria",
  "dependencies": ["dependency_task_id"],
  "priority": "high|medium|low",
  "impediments": []
}
```

## Documenting Nodes (Execution & Code Contracts)

Nodes are the execution targets within a workflow graph. They can be code-nodes (Rust execution) or cognitive nodes (LLM executors).

### Adding Descriptions
Every node must contain a `description` field that defines the exact operation performed. Avoid vague descriptions.

### Output Schemas
Any node producing JSON structured output must specify an explicit JSON Schema defining key names, types, and required fields.
Example of a Node Output Schema contract:
```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "NodeOutputSchema",
  "type": "object",
  "properties": {
    "status": { "type": "string", "enum": ["success", "failure"] },
    "output_paths": { "type": "array", "items": { "type": "string" } }
  },
  "required": ["status", "output_paths"]
}
```

### Code-Node Contracts
For Rust-based code nodes:
- Specify the input struct and output struct types.
- Detail the expected errors, constraints, and deterministic side-effects.
- Document any shared memory or database state mutations (e.g., SQLite rusqlite changes).
