---
name: foundry-core-executors
description: Foundry Core executor/brain routing, sessions, harness, quota policy, ai-limits and CLI factory.
license: MIT
compatibility: codex, opencode, agy, claude
---

## Executor Contract

Foundry chooses and audits execution engines. Do not use a detected CLI until executor policy marks it allowed.

Use:

```bash
foundry sync all --home "$HOME" --output json
foundry brains --output json
foundry mcp call foundry.brain_router --output json
foundry sessions --output json
foundry harness doctor --executor codex --project-root <project-root> --output json
foundry executor-quota ai-limits --ai-limits-cmd ai-limits --output json
foundry request switch-executor --run <run-id> --executor opencode --fallback-executor codex --summary "quota-aware fallback" --origin codex --output json
foundry cli create --name <name> --goal "<goal>" --source <source> --command <command> --output json
```

Use `ai-limits` evidence to stop or fall back before burning exhausted Codex/Antigravity quota. Model providers remain interchangeable; Foundry keeps workflow state and validation gates.
