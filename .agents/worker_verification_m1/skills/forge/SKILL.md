---
name: forge
description: Skill to interact with Forge (the advanced workflow orchestrator and operability runtime) using its CLI.
---

# Forge Core Integration Skill

This skill allows Google Antigravity agents to interact with **Forge Core**, a Rust-based workflow runtime. Forge Core serves as the central orchestration authority, while Antigravity acts as a local execution engine for bounded tasks.

## Orchestration Guidelines

When operating within a project that uses Forge, you **MUST** follow these steps to preserve Forge as the orchestration authority:

### 1. Project Planning and Objective Decomposition
Before starting any multi-step task, decompose the user's objective using `forge plan`.
```bash
forge plan --goal "Your high-level goal" --output json
```
This produces an auditable workflow graph with deterministic task nodes and validation requirements.

### 2. Context Retrieval and Sharding
Do not read files or load the codebase blindly. Retrieve the minimal, correct context package for the current task:
```bash
forge context --workflow <workflow-id> --task <task-id> --project-root <project-root> --budget 120000 --output json
```
This respects context budgets, filters out irrelevant files, and avoids token waste.

### 3. Monitoring and Observability
To inspect the graph state, active leases, or scheduled tasks:
- **List workflows**: `forge list --output json`
- **Inspect Graph DAG**: `forge inspect <workflow-id>`
- **Check active sessions**: `forge sessions`

### 4. Running Actions & Progressing Graph
If a deterministic code step or action needs execution:
```bash
forge run --workflow <workflow-id> --output json
```

### 5. Milestone Validation & Promotion
Before claiming a task is done or requesting human approval to promote a milestone:
1. Run local test gates defined in the context.
2. Run `forge validate`:
   ```bash
   forge validate --workflow <workflow-id> --output json
   ```
3. Attach required evidence/receipts:
   ```bash
   forge milestone collect-ready-evidence --project-root <project-root> --approved-by <your-agent-id> --output json
   ```

---
> [!IMPORTANT]
> Always run validations before task promotion. Do not bypass the Forge Core release gates.
