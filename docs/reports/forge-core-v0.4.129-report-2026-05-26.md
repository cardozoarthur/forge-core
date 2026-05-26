# Forge Core v0.4.129 Self-Evolution Report

Run id: `run_35ef1be524e640b98b117d39b2d37998`
Workflow id: `wf_19f0b1b0a91a4e8698a234932d9fa9e3`
Prompt packet: `forge.self_evolution.prompt.v2`
Cycle: 5
Status: validated 0.5 groundwork

## Summary

This cycle implements the source-grounded Forge 0.5 creative-runtime research baseline as a Forge-owned runtime surface.

The research gate no longer depends on scattered report prose. It is now available through:

- `docs/research/forge-0.5-creative-runtime-source-research.md`
- `forge milestone research --version 0.5 --output json`
- MCP tool `forge.milestone.research`

The milestone manifest remains correctly blocked on `export_demo_baseline`; Forge 0.5 is not promoted by this patch. The next blocker is rendered demo evidence for a design/tokens/component workflow plus a structured document/slide/whiteboard workflow.

## Changed Files

- `Cargo.toml`
- `Cargo.lock`
- `src/milestone.rs`
- `src/main.rs`
- `src/mcp.rs`
- `tests/forge_cli_contract.rs`
- `docs/research/forge-0.5-creative-runtime-source-research.md`
- `docs/forge-0.5-milestone.md`
- `CHANGELOG.md`
- `docs/reports/forge-core-v0.4.129-report-2026-05-26.md`

## Runtime Behavior

- Added `MilestoneResearchReport` with sources, local skill inputs, findings, validation gates, workflow templates and lean governance decisions.
- Added `forge milestone research`.
- Added MCP tool `forge.milestone.research`.
- Updated `research_artifact_baseline` to `validated`.
- Kept `export_demo_baseline` as `groundwork`, so `forge milestone manifest --version 0.5 --output json` still returns a failed promotion decision with `export_demo_baseline` in `blocked_by`.

## Source-Grounded Research Inputs

External sources inspected:

- Penpot data model, data guide and design token docs.
- Google Stitch real-time design announcement.
- v0 docs.
- AG-UI GitHub/docs.
- Impeccable docs.
- Figma MCP developer docs.
- Remotion fundamentals and Sequence docs.
- OBS Studio overview.

Local skill inputs inspected:

- Superpowers brainstorming.
- `stitch-design`.
- `imagegen`.
- Figma generate-design.
- Remotion best practices.

## Validation

RED:

- `cargo test milestone_research_baseline_is_source_grounded_and_agent_visible --test forge_cli_contract` initially failed because `build_milestone_research` did not exist.

GREEN:

- `cargo test milestone_research_baseline_is_source_grounded_and_agent_visible --test forge_cli_contract`
- `cargo test milestone --test forge_cli_contract`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`
- `/home/arthur/projects/forge-core/.local-install/bin/forge plan --goal "Create a delivery platform" --output json`
- `/home/arthur/projects/forge-core/.local-install/bin/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v04129`

Full test result: 224 tests passed (`27` unit, `197` integration) plus doc-tests.

## Install/Publish Status

- Global `cargo install --path . --force` was attempted and blocked by the sandbox because `/home/arthur/.cargo/.crates.toml` is read-only in this execution environment.
- Fallback install succeeded with `cargo install --path . --force --offline --root /home/arthur/projects/forge-core/.local-install`.
- The installed fallback binary reported `forge 0.4.129` and was used for CLI smoke tests.
- The generated `.local-install/` directory was removed after smoke validation to avoid leaving an untracked binary artifact.
- Commit and push were blocked by environment constraints after validation: `.git/index.lock` could not be created because `.git` is read-only, and `git push` could not resolve `github.com` from this sandbox.

## Lean Overhead Ledger

- Prompt bytes: estimated 59000.
- Estimated prompt tokens: estimated 14500.
- Validation command count: 10.
- Source artifact count created this cycle: 2.
- Runtime/source files changed: 6.
- Documentation/report files changed or added: 4.
- Metadata bytes added in structured research output: approximately 17000.

## Safety

- No Docker, Kubernetes, Knative or external user resources were mutated.
- No Telegram secret or credential value was exposed.
- External creative tools remain references/adapters; Forge owns workflow semantics, milestone state and agent-facing surfaces.

## Next Cycle

Implement the remaining `export_demo_baseline` blocker:

1. Generate a rendered design/tokens/component demo artifact from Forge creative IR.
2. Generate one structured document/slide/whiteboard workflow demo.
3. Expose both demos through `forge milestone manifest` evidence.
4. Keep the package on 0.4.x until an explicit human-controlled 0.5 release promotion is requested.
