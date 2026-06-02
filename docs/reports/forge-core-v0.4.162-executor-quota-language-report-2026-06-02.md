# Forge Core v0.4.162 Executor Quota Language Report

Date: 2026-06-02
Run id: run_9ff8a6cdf43a4539a9d3245c5d4d403a
Workflow id: wf_dfa9a20f8ade43a69fb82cef22d0ba1a

## Product Decision

This cycle tightened executor/model policy language so Forge does not imply that Gemini, Codex, or default OpenCode non-local paths are free. The product value is clearer governance: humans inspecting `forge executors` or `forge self run` now see quota/cost uncertainty explicitly, which supports better PM and business decisions before spending scarce non-local capability.

## Change

- Replaced OpenCode non-local "free" wording with explicit quota/cost-bound wording in executor sync policy reports.
- Replaced self-evolution OpenCode non-local "no-cost" wording with provider-dependent cost semantics.
- Added a CLI contract assertion that the executor quota policy reports `unknown_or_configured_non_local_quota_bound`, `provider_config_dependent`, and no misleading "free" reason for the default OpenCode non-local candidate.

## Validation Evidence

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`
- `cargo test sync_persists_human_allowed_executor_policy --test forge_cli_contract`
- `cargo test self_evolve::tests::test_executor_`
- `./target/release/forge plan --goal "Create a delivery platform" --output json`
- `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`

Validation result: passed.

Operational install result: `cargo install --path . --force` was attempted after validation and failed because the current sandbox cannot write `/home/arthur/.cargo/.crates.toml` (`Read-only file system`). CLI smoke validation used the freshly built `./target/release/forge` binary instead.

Workflow artifact evidence: this report was attached to `wf_dfa9a20f8ade43a69fb82cef22d0ba1a` from Codex; the final attachment advanced the workflow to revision 15.

Publication status: `gh auth token` and `git remote get-url origin` succeeded, with remote `https://github.com/cardozoarthur/forge-core.git`. `git commit -m "fix: clarify executor quota cost policy"` was attempted after validation and failed because the current filesystem refused `.git/index.lock` creation as read-only. No push was attempted because no validated commit could be created in this sandbox.

## v0.5 Movement

This advances quota-aware executor policy and business-quality decision support. It reduces the risk that Forge selects or explains a model path using vague free/local labels, and gives future self-evolution reports cleaner evidence for why a quota-bound non-local or local model was chosen.

## Next Cycle

Add a concrete non-interactive probe artifact for Gemini and OpenCode attempts that records provider, model, prompt-risk classification, and repair-goal creation before any executor handoff.
