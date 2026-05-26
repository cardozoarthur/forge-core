# Forge Core v0.4.144 Self-Evolution Report

Run id: `run_35ef1be524e640b98b117d39b2d37998`  
Workflow id: `wf_19f0b1b0a91a4e8698a234932d9fa9e3`  
Executor: `codex`  
Cycle: `21`  
Version line: `0.4.x` groundwork for Forge 0.5 runtime readiness

## Increment

Forge now exposes deterministic replacement-grade CLI demo evidence:

- `forge milestone cli-demo --origin <origin> --output json`
- MCP tool `forge.milestone.cli_demo`

The demo creates Forge-owned state for three flows: a coding task with bounded context, handoff and diff-review artifact lineage; a research/artifact task that produces Markdown, PDF and Telegram delivery records for the canonical `hackathon` Goal; and a long-running async run with a fresh heartbeat visible through Forge request/status/list/inspect semantics.

This is still `0.5 groundwork`. It does not claim that Forge can replace Codex/OpenCode yet, because native file editing, in-TUI diff/patch review, provider/session management and permission-gated shell UX still need validated implementation.

## Validation Evidence

RED:

- `cargo test --test forge_cli_contract milestone_cli_demo_generates_replacement_grade_cli_flow_evidence -- --exact`
  - failed because `forge milestone cli-demo` did not exist.
- `cargo test --test forge_cli_contract mcp_exposes_replacement_cli_demo_tool_and_skill_guidance -- --exact`
  - failed because the packaged skill and MCP manifest did not mention the new CLI demo surface.
- `cargo test --test forge_cli_contract milestone_boundary_document_matches_validated_export_demo_runtime_state -- --exact`
  - failed because the visible 0.5 milestone boundary did not point to replacement-grade CLI demo evidence.
- `cargo test --test forge_cli_contract milestone_status_surfaces_05_boundary_and_promotion_gate -- --exact`
  - failed because the replacement-grade CLI capability evidence did not mention `forge milestone cli-demo`.

Targeted GREEN:

- `cargo test --test forge_cli_contract milestone_cli_demo_generates_replacement_grade_cli_flow_evidence -- --exact` passed.
- `cargo test --test forge_cli_contract mcp_exposes_replacement_cli_demo_tool_and_skill_guidance -- --exact` passed.
- `cargo test --test forge_cli_contract milestone_status_surfaces_05_boundary_and_promotion_gate -- --exact` passed.
- `cargo test --test forge_cli_contract milestone -- --nocapture` passed 10 milestone contract tests.

Required validation:

- `cargo fmt --check` passed.
- `cargo clippy --all-targets --all-features -- -D warnings` passed.
- `cargo test` passed: 53 unit tests, 219 CLI contract tests and 0 doc tests.
- `cargo build --release` passed.

Release smokes:

- `./target/release/forge plan --goal "Create a delivery platform" --output json` passed.
- `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.144-2` passed.
- `./target/release/forge --store /tmp/forge-cli-demo-smoke-0.4.144.sqlite milestone cli-demo --origin codex --output json` passed.
- `./target/release/forge mcp tools --output json` passed and exposed `forge.milestone.cli_demo`.

Install:

- `cargo install --path . --force` was blocked by read-only `/home/arthur/.cargo/.crates.toml`.
- `CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --locked --offline` passed.
- `/home/arthur/projects/forge-core/.forge/local-install/bin/forge --version` returned `forge 0.4.144`.

Publication:

- `gh auth token >/dev/null` passed.
- `git remote get-url origin` returned `https://github.com/cardozoarthur/forge-core.git`.
- `git add ...` was blocked because `/home/arthur/projects/forge-core/.git/index.lock` could not be created on a read-only filesystem.
- `git push` was attempted and failed because the sandbox could not resolve `github.com`.

## Lean Overhead Ledger

- Prompt packet bytes: approximately `65000`
- Estimated prompt tokens: approximately `16250`
- Validation command count before full gate: `8`
- Full validation and smoke command count: `9`
- Artifact count changed: `12`
- Metadata/report bytes added: approximately `4500`
- Useful value: replaces a vague replacement-grade CLI gap with a Forge-owned, scriptable and MCP-visible demo contract that creates inspectable workflow/run/artifact lineage without adding a separate agent shell.
- Cost control evidence: the new surface is deterministic Rust and reuses existing request, schedule, artifact and milestone primitives; it performs no model calls, installs, external sends or device access.

## Files Changed

- `Cargo.toml`
- `Cargo.lock`
- `README.md`
- `src/main.rs`
- `src/mcp.rs`
- `src/milestone.rs`
- `src/skill.rs`
- `skills/forge-core/SKILL.md`
- `tests/forge_cli_contract.rs`
- `CHANGELOG.md`
- `docs/forge-0.5-milestone.md`
- `docs/reports/forge-core-v0.4.144-report-2026-05-26.md`

## Safety

No Docker, Kubernetes, Knative, Telegram send, camera, microphone, screen, mouse, keyboard, peripheral, model download, Blender execution or external user resource was mutated. The new command creates only Forge-owned local workflow/run/artifact demo state in the selected store.

## Next Cycle

Move from demo evidence to real replacement-grade behavior: add a native Forge CLI file-editing and diff-review contract with permission gates, rollback metadata, validation hooks and inspectable session state, while preserving script-safe JSON automation.
