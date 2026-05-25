# Forge Core v0.4.88 Self-Evolution Report

Date: 2026-05-25
Run: `run_278d6f1f0f264d04babac872fa460573`
Workflow: `wf_376336612169440a82e7f86b38b5760f`
Executor: Codex

## Goal

Optimize Forge Core as an agent-integration runtime. The increment targets a stable local MCP surface, generated Codex/OpenCode skill guidance and an async handoff flow where an agent starts work, receives a `run_id` immediately and polls status/artifacts later.

## Increment

Added `forge mcp tools --output json` with schema `forge.mcp.tools.v1`. The manifest exposes stable tools for:

- `forge.workflow.list`
- `forge.workflow.inspect`
- `forge.run.start`
- `forge.run.resume`
- `forge.run.status`
- `forge.workflow.update_goal`
- `forge.workflow.attach_artifact`
- `forge.context.request`
- `forge.validation.status`
- `forge.artifact.fetch`

Added `forge mcp call <tool> --input <json> --output json` with schema `forge.mcp.call.v1`. The call layer delegates to existing Forge modules instead of creating a separate state path.

Added `forge request resume --run <run-id> --origin <origin> --output json` and `forge.agent_handoff_contract.v1` on `request start`, binding:

- `run_id`
- `workflow_id`
- Forge authority policy
- bounded context tool and command
- validation rules
- status polling tool and command

Updated generated Codex/OpenCode skills to document MCP discovery, async run start/status/resume, artifact attach and bounded context requests.

## Value Evidence

- Agents now have a compact tool manifest rather than needing to infer CLI commands from documentation.
- MCP calls preserve Forge as the source of truth: run creation, goal mutation, artifact attach, validation and artifact fetch all route through existing Forge APIs.
- Async handoff is executable from the agent surface: `forge.run.start` returns a `run_id` and `forge.run.status` later returns workflow/task/handoff state.
- Artifact fetch is bounded and restricted to Forge-owned artifact refs, reducing accidental context overexposure.

## Lean Overhead Ledger

- Prompt packet bytes: approximately 9 KB from the provided human self-evolution prompt.
- Estimated prompt tokens: approximately 2.3K.
- Required validation command count: 4.
- Additional smoke command count: 5.
- New artifact count: 1 report document.
- Metadata bytes: 5,297 bytes in this report; 783,654 bytes across touched tracked files at validation time.

## Tests

Focused tests:

```bash
cargo test mcp_tools_manifest_exposes_stable_agent_runtime_surface --test forge_cli_contract
cargo test mcp_call_starts_resumes_and_polls_async_run_for_agent_handoff --test forge_cli_contract
cargo test mcp_call_mutates_workflow_and_fetches_bounded_artifact_content --test forge_cli_contract
cargo test skill_install_creates_codex_and_opencode_compatible_skill_files --test forge_cli_contract
```

Required validation:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

CLI smoke:

```bash
target/release/forge --store /tmp/forge-core-v0.4.88-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json
target/release/forge --store /tmp/forge-core-v0.4.88-skill-smoke-2.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v0.4.88-2
target/release/forge --store /tmp/forge-core-v0.4.88-mcp-smoke.sqlite mcp tools --output json
target/release/forge --store /tmp/forge-core-v0.4.88-mcp-flow-smoke.sqlite mcp call forge.run.start --input '{"goal":"Exercise MCP async handoff smoke","origin":"codex"}' --output json
target/release/forge --store /tmp/forge-core-v0.4.88-mcp-flow-smoke.sqlite mcp call forge.run.status --input '{"run_id":"run_ab01e08c723f4458b3ad704631f1528e"}' --output json
target/release/forge --version
```

Result: all required validation and CLI smoke commands passed. The release binary reported `forge 0.4.88`.

## Local Install and Publication

The required default install command was attempted:

```bash
cargo install --path . --force
```

It failed because `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem in this sandbox. The local workspace install then succeeded offline:

```bash
cargo install --path . --force --root .forge/local-install --offline
.forge/local-install/bin/forge --version
```

The installed workspace binary reported `forge 0.4.88`.

GitHub publication preflight passed for `gh auth token` and `git remote get-url origin`, which resolved to `https://github.com/cardozoarthur/forge-core.git`. Commit/push could not be completed because writing `.git/index.lock` failed with a read-only filesystem error.

## Safety

- No Docker, Kubernetes, Knative or external resource was mutated.
- No remote executor was authorized.
- MCP artifact fetch reads only artifact paths found under Forge-owned artifact listings and applies a `max_bytes` cap.
- MCP mutations are revisioned through existing workflow mutation functions.

## Next Recommended Cycle

Implement an actual MCP stdio server mode over the same tool registry, reusing the deterministic `forge.mcp.tools.v1` and `forge.mcp.call.v1` contracts added here. Keep it read/write bounded to the same Forge APIs and add contract tests that exercise JSON-RPC initialize, tools/list and tools/call without duplicating workflow state.
