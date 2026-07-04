---
name: forge-core-executors
description: Forge Core executor/brain routing, sessions, harness, quota policy, ai-limits and CLI factory.
license: MIT
compatibility: codex, opencode, agy, claude
---

## Executor Contract

Forge chooses and audits execution engines. Do not use a detected CLI until executor policy marks it allowed.

Use:

```bash
forge sync all --home "$HOME" --output json
forge brains --output json
forge mcp call forge.brain_router --output json
forge sessions --output json
forge harness doctor --executor codex --project-root <project-root> --output json
forge executor-quota ai-limits --ai-limits-cmd ai-limits --output json
forge request switch-executor --run <run-id> --executor opencode --fallback-executor codex --summary "quota-aware fallback" --origin codex --output json
forge cli create --name <name> --goal "<goal>" --source <source> --command <command> --output json
```

Use `ai-limits` evidence to stop or fall back before burning exhausted Codex/Antigravity quota. Model providers remain interchangeable; Forge keeps workflow state and validation gates.
