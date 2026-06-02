# Forge Core v0.4.176 Report - Attempt-Level Executor Fallback Evidence

## What Changed

- Added `next_fallback_reason` to self-evolution executor attempt records.
- Propagated the quota-aware selection trace into actual executor attempts so failures and fallback handoffs explain why the next provider/model was tried.
- Updated the persisted Markdown cycle report executor-attempt table with the fallback reason.
- Added CLI contract coverage proving an OpenCode failure records provider/model/locality evidence and that Gemini is attempted because an earlier quota-aware executor failed.

## Why It Matters

This improves the required quota-aware executor policy by connecting the planned selection trace to runtime evidence. Forge can now show not only which OpenCode/Gemini/Codex/local candidate was preferred, but why a later candidate actually ran after a failed attempt.

## Validation

- `cargo test self_run_falls_back_to_gemini_before_codex_if_previous_executor_fails --test forge_cli_contract`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`
- `target/release/forge plan --goal "Create a delivery platform" --output json`
- `target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`

## v0.5 Impact

- Real-time agent runtime: executor switch/fallback evidence is available in self-run cycle artifacts.
- Advanced CLI/TUI: status/report views can display why a fallback was selected without recomputing policy.
- Governed mutations: provider/model/locality and fallback rationale remain attached to validation artifacts.
- Quota-aware executor policy: non-local quota decisions and local fallback use are traceable at both policy and attempt levels.
- Business/product decisions: Forge can explain why it spent or preserved model capacity for a product-improving cycle.
