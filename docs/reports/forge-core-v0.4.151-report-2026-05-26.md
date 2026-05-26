# Forge Core v0.4.151 Report - 2026-05-26

Run: `run_35ef1be524e640b98b117d39b2d37998`  
Workflow: `wf_19f0b1b0a91a4e8698a234932d9fa9e3`  
Cycle: 29  
Status: validated 0.5 groundwork, not a Forge 0.5 promotion claim.

## Increment

Forge now treats patch rollback as a guarded workflow proposal instead of a silent destructive file restore.

The new `forge.patch_revert.v1` output records:

- `status=patch_revert_proposed`;
- `restore_executed=false`;
- `requires_human_approval=true`;
- `external_resources_mutated=false`;
- an explicit approval command such as `git checkout -- <path>`;
- safety notes and a Forge-owned rollback artifact.

`forge patch apply` remains a record step for current file snapshots, validation output and rollback lineage. `forge patch revert` now records the rollback proposal without running `git checkout`; future cycles can connect that proposal to TUI diff review and explicit human approval execution.

The packaged Forge skill now teaches agents to use `forge patch apply`, `forge patch revert`, `forge.patch.apply` and `forge.patch.revert` so Codex/OpenCode can stay inside Forge-owned workflow semantics for file-edit lifecycle records.

## Validation

Red/green evidence:

- RED: `cargo test --test forge_cli_contract patch_revert_records_proposal_without_restoring_files_automatically -- --exact` failed because `PatchRevertReport` did not expose `restore_executed`, `requires_human_approval` or `approval_command`.
- RED: `cargo test --test forge_cli_contract mcp_exposes_patch_apply_and_revert_tools -- --exact` failed because the packaged Forge skill did not mention apply/revert patch surfaces.
- GREEN: `cargo test --test forge_cli_contract patch_ -- --nocapture` passed with 9 patch-related tests.

Required validation in this cycle:

- `cargo fmt --check` passed.
- `cargo clippy --all-targets --all-features -- -D warnings` passed.
- `cargo test` passed: 58 unit tests, 229 integration tests, 0 doctests.
- `cargo build --release` passed.

Smokes:

- `./target/release/forge --version` returned `forge 0.4.151`.
- `./target/release/forge --store /tmp/forge-core-smoke-0.4.151.sqlite plan --goal "Create a delivery platform" --output json` returned a planned workflow.
- `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.151-cycle29` installed Codex/OpenCode skills and synced local executor/runtime visibility without mutating Docker, Kubernetes or Knative resources.

Install:

- `cargo install --path . --force` was blocked by `/home/arthur/.cargo/.crates.toml` on a read-only filesystem.
- Fallback install succeeded: `CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --locked --offline`.
- `.forge/local-install/bin/forge --version` returned `forge 0.4.151`.

## Lean Overhead Ledger

- prompt_bytes_estimated: 65000
- prompt_tokens_estimated: 16250
- required_validation_command_count: 4
- total_validation_and_smoke_command_count: 22
- new_report_artifacts: 1
- changed_source_modules: 4
- changed_contract_tests: 1
- report_metadata_bytes: 2960

Value evidence: the increment removes a risky rollback behavior from the replacement-grade CLI path. Forge now records rollback intent, safety status and approval instructions before any destructive restore can happen, preserving human control while keeping patch lifecycle lineage durable.

## Safety

`forge patch revert` no longer runs `git checkout` automatically. It records a proposal and approval command only. The implementation does not mutate Docker, Kubernetes, Knative, Telegram, cameras, microphones, screen/input devices, peripherals, model caches or other external user resources.

## Next Cycle

Add a first TUI/interactive diff-review node for patch proposals: show planned/apply/revert artifacts, request approve/reject/refine from the human, persist the decision as a human interaction node and only then allow an explicit restore execution path with validation evidence.
