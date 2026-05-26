# Forge Core v0.4.150 Report - 2026-05-26

Run: `run_35ef1be524e640b98b117d39b2d37998`
Workflow: `wf_19f0b1b0a91a4e8698a234932d9fa9e3`
Cycle: 27
Status: validated 0.5 groundwork, not a Forge 0.5 promotion claim.

## Increment

Forge now has a native `forge patch plan` surface for replacement-grade CLI groundwork. The command creates a Forge-owned, plan-only file-editing contract before an agent edits files:

- repo-relative path permission gate;
- blocked absolute, parent-directory and `.git` paths;
- target file snapshots with SHA-256 hashes;
- bounded `forge context --strict` handoff instruction;
- diff-review and validation commands;
- rollback notes;
- workflow artifact lineage through an attached `patch_plan` artifact;
- explicit `applies_changes=false` and `external_resources_mutated=false`.

The same capability is exposed to agents as MCP tool `forge.patch.plan`, and the packaged Forge skill now teaches both CLI and MCP entrypoints.

## Validation

Red/green evidence:

- RED: `cargo test --test forge_cli_contract patch_plan -- --nocapture` failed because `patch` was not a known subcommand and the skill did not mention patch planning.
- GREEN: the same target passed after implementation with 3 new tests.

Required validation:

- `cargo fmt --check` passed after formatting.
- `cargo clippy --all-targets --all-features -- -D warnings` passed.
- `cargo test` passed: 58 unit tests, 224 integration tests, 0 doctests.
- `cargo build --release` passed.

Smokes:

- `./target/release/forge --version` returned `forge 0.4.150`.
- `./target/release/forge --store /tmp/forge-core-smoke-0.4.150.sqlite plan --goal "Create a delivery platform" --output json` returned a planned workflow.
- `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.150-27` installed Codex/OpenCode skills and synced local executor/runtime visibility without mutating Docker, Kubernetes or Knative resources.

Install:

- `cargo install --path . --force` was blocked by `/home/arthur/.cargo/.crates.toml` on a read-only filesystem.
- Fallback install succeeded: `CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --locked --offline`.
- `.forge/local-install/bin/forge --version` returned `forge 0.4.150`.

## Lean Overhead Ledger

- prompt_bytes_estimated: 62000
- prompt_tokens_estimated: 15500
- required_validation_command_count: 4
- total_validation_and_smoke_command_count: 15
- new_report_artifacts: 1
- changed_source_modules: 6
- changed_contract_tests: 1
- report_metadata_bytes: 3443

Value evidence: the increment replaces ad hoc agent file-edit intent with a durable Forge-owned planning surface. It improves future replacement-grade CLI work without adding an apply engine, shell automation or TUI bloat in this cycle.

## Safety

No source file is edited by `forge patch plan`; it only reads target files for hashes and writes Forge-owned artifacts. The implementation does not mutate Docker, Kubernetes, Knative, Telegram, cameras, microphones, screen/input devices, model caches or external user resources.

## Next Cycle

Extend the patch-plan contract toward a guarded apply/revert proposal flow: structured patch format, human approval node, dry-run apply, rollback artifact and inspect/status visibility. Keep it 0.5 groundwork until in-TUI diff approval and end-to-end Forge-first coding flow are validated.
