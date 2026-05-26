# Forge Core v0.4.141 Self-Evolution Report

## Cycle

- Prompt packet: `forge.self_evolution.prompt.v2`
- Run: `run_35ef1be524e640b98b117d39b2d37998`
- Workflow: `wf_19f0b1b0a91a4e8698a234932d9fa9e3`
- Executor: `codex`
- Cycle: `17`
- Version line: `0.4.x` groundwork for Forge 0.5 runtime readiness

## Increment

Forge 0.5 milestone governance now incorporates the latest terminal-phase scope. The runtime no longer reports Forge 0.5 as promotable while replacement-grade CLI behavior and experimental multimodal runtime evidence remain only groundwork.

## User-Visible Surfaces

- `forge milestone status --version 0.5 --output json`
  - adds `replacement_grade_cli` as `groundwork`
  - adds `experimental_multimodal_runtime` as `groundwork`
  - reports `promotion_decision.decision = "fail"`
  - includes both capabilities in `promotion_decision.blocked_by`
- `forge milestone manifest --version 0.5 --output json`
  - places both new capabilities in `missing_capabilities`
  - emits their required evidence and next actions through `requirements` and `known_gaps`
- MCP tools `forge.milestone.status` and `forge.milestone.manifest` return the same gate state to agent callers.
- `docs/forge-0.5-milestone.md` now matches runtime truth and says current promotion fails as of `0.4.141`.

## Lean Economics

- Prompt bytes estimate: `110000`.
- Estimated prompt tokens: `27500`.
- Validation command count: `1` intentional malformed test command, `1` intentional red milestone test run, `1` green focused milestone test run, `7` successful validation/smoke/install checks and `1` blocked default install.
- Artifact count changed: `8` (`Cargo.toml`, `Cargo.lock`, `README.md`, `src/milestone.rs`, `tests/forge_cli_contract.rs`, `CHANGELOG.md`, `docs/forge-0.5-milestone.md`, this report).
- Metadata bytes estimate: `9800`.
- Useful value: prevents a false 0.5 promotion claim after the scope expanded to replacement-grade CLI and multimodal runtime readiness.
- Accepted complexity: two milestone capabilities, focused required-evidence text, two next-action rules and contract assertions.
- Rejected complexity: no new scheduler, no device/model runtime, no new TUI dependency and no external resource mutation.

## Validation Evidence

- RED observed first:
  - `cargo test --test forge_cli_contract milestone` failed because milestone status still reported `promote`, requirements had only the old capability set and the visible milestone doc lacked replacement-grade CLI and multimodal runtime rows.
- GREEN targeted validation:
  - `cargo test --test forge_cli_contract milestone` passed: `9` milestone contract tests passed.
- Required validation:
  - `cargo fmt --check` passed after `cargo fmt`.
  - `cargo clippy --all-targets --all-features -- -D warnings` passed.
  - `cargo test` passed: `47` unit tests and `213` CLI/MCP contract tests.
  - `cargo build --release` passed.
- CLI smoke:
  - `./target/release/forge plan --goal "Create a delivery platform" --output json` passed.
  - `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-0.4.141` passed.
- Local install:
  - Default `cargo install --path . --force` was blocked because `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem.
  - Repo-local offline install passed with `CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --offline`.
  - `.forge/local-install/bin/forge --version` returned `forge 0.4.141`.
  - User-visible `forge --version` remains `forge 0.4.140` until the default install path can be updated outside this sandbox.
- GitHub contract:
  - `gh auth token` succeeded with token output suppressed and discarded.
  - `git remote get-url origin` returned `https://github.com/cardozoarthur/forge-core.git`.
  - `git add ...` was blocked because `/home/arthur/projects/forge-core/.git/index.lock` cannot be created on the read-only `.git` filesystem.
  - `git push` was attempted and failed because `github.com` DNS resolution is unavailable in this environment.

## Safety

- No Docker, Kubernetes, Knative, Telegram, camera, microphone, screen, mouse, keyboard, peripheral, model download or external user resource was mutated.
- Multimodal remains disabled by default; this cycle only changes governance/manifest output.
- The promotion gate is stricter after this change, so it reduces overclaiming risk instead of broadening runtime authority.

## Next Cycle Recommendation

Move from governance to evidence: add a replacement-grade CLI demo contract where `forge` completes a small coding task through its own session/run model with bounded file editing, diff review, validation and inspectable artifact lineage.
