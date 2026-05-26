# Forge Core v0.4.133 Self-Evolution Report

Run id: `run_35ef1be524e640b98b117d39b2d37998`  
Workflow id: `wf_19f0b1b0a91a4e8698a234932d9fa9e3`  
Cycle: `9`  
Date: `2026-05-26`

## Outcome

Forge now has a first Forge-owned experimental multimodal capability surface. The feature is intentionally disabled by default and exposes planning and guard decisions only; it does not install models, mutate local devices, access camera/microphone/screen/input, or change external resources.

## Runtime Increment

- Added `forge multimodal status --output json` with schema `forge.multimodal.status.v1`.
- Added `forge multimodal install-plan --capability <id> --output json` with schema `forge.multimodal.install_plan.v1`.
- Added `forge multimodal guard --capability <scope> --action <action> --output json` with schema `forge.multimodal.guard.v1`.
- Added MCP tools:
  - `forge.multimodal.status`;
  - `forge.multimodal.install_plan`;
  - `forge.multimodal.guard`.
- Updated the packaged Forge skill so agents can discover the multimodal status, install-plan and guard flows.

## Capability Coverage

The inventory covers image understanding, OCR, object detection, segmentation, image generation/editing, video generation/editing, audio transcription, speech synthesis, audio understanding, realtime vision, screen understanding, computer-use actions, mouse/keyboard automation, peripheral/device access, avatar/camera emulation, 3D generation/adaptation and Blender-assisted asset processing.

## Safety Boundary

- Experimental multimodal state defaults to `experimental_disabled`.
- Install plans are `plan_only` and report `installs_performed: false`.
- Runtime guard decisions deny access unless `--enable-experimental` and `--allow` are both present.
- Guardrails include dry-run/simulation first, scoped target, kill switch, secrets redaction, audit log and rollback/uninstall planning.

## Lean Overhead Ledger

Prompt packet bytes: approximately `64300`  
Estimated prompt tokens: approximately `16075`  
Validation command count run: `18`  
Artifact count: `2` (`CHANGELOG.md`, this report)  
Metadata bytes added: approximately `5400`

Expected value: the track converts a broad multimodal ambition into a small deterministic runtime surface and prevents ad hoc camera/screen/device access. The useful delivery is a stable CLI/MCP/skill contract that future cycles can extend with model manifests, benchmarks and safe demo workflows.

## Validation Status

Focused red/green tests passed:

- `cargo test multimodal_ --test forge_cli_contract`
- `cargo test packaged_skill_mentions_experimental_multimodal_agent_surface --test forge_cli_contract`

Required full validation passed:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`

CLI smoke passed:

- `./target/release/forge --version` returned `forge 0.4.133`.
- `./target/release/forge plan --goal "Create a delivery platform" --output json` returned a planned workflow with atomic tasks.
- `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v04133` installed Codex/OpenCode skill surfaces and synced executors/runtimes without mutating external infrastructure.

Install status:

- `cargo install --path . --force` was attempted and failed because `/home/arthur/.cargo/.crates.toml` is read-only in the current sandbox.
- Workspace-local fallback install passed with `CARGO_NET_OFFLINE=true CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --offline`.
- `/home/arthur/projects/forge-core/.forge/local-install/bin/forge --version` returned `forge 0.4.133`.
- The workspace-local installed binary returned `experimental_disabled` for both `forge multimodal status --output json` and `forge mcp call forge.multimodal.status --output json`.

Publication status:

- `gh auth token` succeeded with token output suppressed.
- `git remote get-url origin` returned `https://github.com/cardozoarthur/forge-core.git`.
- Commit creation was blocked because `.git/index.lock` could not be created in the current sandbox (`Read-only file system`).
- `git push` was attempted and failed because the sandbox could not resolve `github.com`.

## Next Recommended Cycle

Add a persisted multimodal capability manifest artifact and benchmark template workflow for one safe local image-recognition path, still disabled by default and guarded by explicit human approval.
