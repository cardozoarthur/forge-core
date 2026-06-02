# Forge Core 0.4.160 Self-Evolution Report

## Summary

This cycle fixes the shape of quota-aware executor/model selection for self-evolution. Forge no longer lets the requested primary executor alone override the product decision that high-value self-evolution should evaluate authorized non-local capabilities before local/Ollama fallback.

## Behavior Changed

- `forge.self_evolution.executor_policy.v1` candidates now sort by quota-aware capability class before requested-chain order.
- High-value self-evolution candidates are ordered as:
  1. OpenCode non-local provider path.
  2. Gemini non-local quota-bound path.
  3. Codex non-local quota-bound path.
  4. OpenCode local/Ollama capacity path.
- The requested executor chain remains recorded, but it is no longer allowed to put `codex` primary ahead of the stronger non-local decision sequence for this workflow.
- Policy text now states that selection considers expected value, and quota-preservation evidence explicitly calls out cheap repetitive or low-value work.

## Business/Product Decision

The useful product decision is to stop wasting self-evolution cycles on accidental executor ordering. For Forge v0.5, executor choice is a business decision: strong non-local quota should be spent when PM/business/creative reasoning value justifies it, while local/Ollama capacity should handle cheap, repetitive, privacy-sensitive or low-value work. This change makes that decision deterministic and auditable in the cycle report.

## Validation Evidence

- RED: `cargo test test_executor_policy_prefers_non_local_quota_aware_capabilities_for_self_evolution` failed because `codex` primary sorted ahead of OpenCode/Gemini non-local candidates.
- GREEN: the same targeted test passed after introducing quota-aware selection tiers.
- Required validation passed:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test`
  - `cargo build --release`
- CLI smoke passed:
  - `./target/release/forge plan --goal "Create a delivery platform" --output json`
  - `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`
- Local install:
  - `cargo install --path . --force` was blocked by read-only `/home/arthur/.cargo/.crates.toml`.
  - `CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --locked --offline` succeeded.
  - `.forge/local-install/bin/forge --version` returned `forge 0.4.160`.
- Workflow validation:
  - `./target/release/forge validate --workflow wf_dfa9a20f8ade43a69fb82cef22d0ba1a --output json` returned `blocked` because persisted workflow tasks are still `Pending`; those tasks must return to work before promotion.

## Product Boundary

This moves Forge closer to v0.5 by improving reliable agent switching and quota-aware executor policy. It is still not promotion-complete: Forge still needs live provider probing, Gemini non-interactive failure classification, OpenCode provider/model availability evidence, scheduled GitHub/Telegram publication as a native workflow, and richer PM/TUI decision flows.

## Safety

No external Docker, Kubernetes, Knative or model resources were modified. No Telegram message was sent in this code change. The patch is limited to Forge Core source, tests, version metadata and this report artifact.
