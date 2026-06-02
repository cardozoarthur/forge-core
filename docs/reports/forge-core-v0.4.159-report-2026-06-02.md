# Forge Core 0.4.159 Self-Evolution Report

## Summary

This cycle makes executor/model selection visible as a quota-aware product decision instead of an implicit fallback list. `forge self run` now reports the candidate executor policy for each cycle before execution, including non-local quota-bound paths and local capacity fallbacks.

## Behavior Added

- Added `forge.self_evolution.executor_policy.v1` to self-run cycle reports.
- The policy records requested chain, provider, model, local vs non-local, free/paid/quota classification, remaining quota assumption, rate-limit risk, monetary/token cost, latency, expected quality, business/product reasoning suitability and fallback risk.
- The report includes explicit non-interactive requirements for OpenCode, Gemini and Codex.
- The report includes repair goals for Gemini non-interactive detection and OpenCode provider/model classification.

## Behavior Changed

- Executor fallback strategy is derived from the same policy candidates that are serialized in reports.
- Gemini and Codex are classified as non-local quota-bound capabilities, not free executors.
- OpenCode local/Ollama is represented as a local-capacity option that is useful when quotas are low or work is repetitive, not as a blanket priority over all non-local options.

## Validation Evidence

- RED observed first with `cargo test self_run_reports_quota_aware_executor_policy_for_cycle -- --nocapture`: `executor_policy` was missing from the cycle report.
- Targeted GREEN passed for `cargo test self_run_reports_quota_aware_executor_policy_for_cycle -- --nocapture`.
- Fallback behavior remained green with `cargo test self_run_falls_back_to_gemini_before_codex_if_previous_executor_fails -- --nocapture`.
- Self-run regression contracts passed with `cargo test self_run_ -- --nocapture`.

## Product Boundary

This moves Forge toward v0.5 by making executor choice auditable and business-aware: high-value PM/business/creative reasoning can justify non-local quota, while deterministic or low-value work can preserve quota. The next step is to connect this policy to live executor probing, status surfaces and scheduled publication artifacts.

## Safety

The change is scoped to Forge Core source, tests, changelog and report artifacts. It does not install models, mutate Docker/Kubernetes/Knative resources, send Telegram messages or modify external infrastructure.
