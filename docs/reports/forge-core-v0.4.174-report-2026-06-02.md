# Forge Core v0.4.174 Report - 2026-06-02

Run id: `run_8c9d3e943a204d03a16a1d4b33f71e63`
Workflow id: `wf_bcbca0bc466649c2a1322ae30220f420`
Cycle: 4

## Product Decision

This cycle fixes commit-message quality before adding more publication machinery. The product and business decision is that self-evolution output must be understandable from Git history: operators, contributors and report generators should see what changed and why it mattered without opening every cycle artifact.

## Change

- Replaced the generic self-evolution commit subject `chore: forge self evolution cycle N` with semantic message generation based on staged changed files.
- Added commit bodies that include cycle id, required validation commands, changed-file summary and v0.5 impact.
- Added tests proving executor-policy changes get a semantic `feat:` subject and self-evolution changes do not regress to cycle-number-only messages.
- Bumped Forge Core to `0.4.174` and updated the changelog.

## Validation Plan

- `cargo test self_evolution_commit_message`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`
- `forge plan --goal "Create a delivery platform" --output json`
- `forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-8c9d3e-cycle4`

## Validation Result

Passed:

- `cargo test self_evolution_commit_message`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test` with 84 unit tests, 242 CLI contract tests and doctests.
- `cargo build --release`
- `./target/release/forge plan --goal "Create a delivery platform" --output json`
- `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-8c9d3e-cycle4-20260602`

Post-validation install:

- `cargo install --path . --force` was attempted and failed because this sandbox cannot write `/home/arthur/.cargo/.crates.toml` (`Read-only file system`, os error 30).

Executor/model evidence from the smoke:

- Codex was installed, configured, human-authorized and non-interactive ready.
- OpenCode was installed and human-authorized, but model listing failed with `PRAGMA wal_checkpoint(PASSIVE)`, so quota policy marked OpenCode candidates as `skipped_interactive_hang_risk`.
- Gemini was installed, but `GEMINI_API_KEY` was not present in the smoke environment, so Gemini was not configured for non-interactive use.

Forge artifact lineage:

- Attached this report to workflow `wf_bcbca0bc466649c2a1322ae30220f420` as `artifact_0bf454f5b7a84a01929864ea84d29cf6`; workflow revision advanced to 8.

Publication:

- `gh auth token` succeeded without exposing the token in the report.
- `git remote get-url origin` returned `https://github.com/cardozoarthur/forge-core.git`.
- `git add CHANGELOG.md Cargo.lock Cargo.toml src/self_evolve.rs docs/reports/forge-core-v0.4.174-report-2026-06-02.md` was attempted after validation and failed because Git could not create `/home/arthur/projects/forge-core/.git/index.lock` (`Sistema de ficheiros só de leitura`).
- No commit or push was attempted after that blocker because there was no validated local commit to publish.

## v0.5 Movement

This improves governed self-evolution and publication quality. Stronger commit subjects make the periodic GitHub/Telegram reports easier to synthesize, make AI-assisted publication less dependent on hidden context, and preserve product/business rationale in Git history.

## Remaining Work

- Finish full required validation and local install with `cargo install --path . --force`.
- Commit with the new semantic self-evolution message behavior and publish through the GitHub CLI contract when validation passes.
- Continue native scheduled GitHub/Telegram publication with AI-assisted Markdown report metadata.
- Continue making Product/PM CLI-TUI the main entry point and turn OpenCode/Gemini repair goals into runnable validation tasks.
