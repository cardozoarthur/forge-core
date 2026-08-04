---
name: forge-core-agent
description: Forge Core executor discovery, explicit authorization, readiness and adapter diagnostics.
license: MIT
compatibility: codex, opencode, agy, claude
---

## Agent and Executor Contract

Forge owns workflow state, policy, leases, validation and promotion. Codex, Agy
and other CLIs are bounded execution engines. A CLI being present on disk does
not authorize Forge to use it.

An executor is usable only when all four gates are true:

1. `installed`: Forge found the executable;
2. `configured`: Forge found provider/CLI configuration evidence;
3. `allowed`: a human explicitly authorized the canonical executor;
4. `non_interactive_ready`: the bounded readiness probe succeeded.

`antigravity`, `antigravity-cli` and `agy-cli` are compatibility names for the
single canonical executor `agy`. They must not create independent policy,
quota, brain or lease state.

## Discover And Authorize Executors

Use the operator's real home when probing configuration:

```bash
forge sync executors --home "$HOME" --allow agy --allow codex --no-prompt --output json
forge executors --output json
forge brains --output json
```

`--allow` is the explicit human authorization persisted by Forge. `--no-prompt`
only disables the interactive question; it does not authorize an executor by
itself. Use `--deny <executor>` to persist an explicit denial.

Do not use a temporary smoke-test home to establish production readiness. The
executor report records the probed home so operators can distinguish the two.

## Adapter Credentials and Quotas

```bash
forge harness doctor --executor agy --shim-dir "$HOME/.forge/bin" --project-root <project-root> --output json
forge harness doctor --executor codex --shim-dir "$HOME/.forge/bin" --project-root <project-root> --output json
forge executor-quota ai-limits --ai-limits-cmd ai-limits --output json
```

Provider credentials remain in the provider's supported environment or config
store. Never place secret values in Forge skills, reports, command history or
human-readable workflow artifacts.

Before handoff, confirm the canonical executor is present in `usable`. Explicit
handoff fails closed and does not acquire a lease when any readiness or policy
gate is false. Quota evidence may independently stop the handoff or recommend a
fallback before capacity is spent.
