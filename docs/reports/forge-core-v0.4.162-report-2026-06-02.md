# Forge Core v0.4.162 Report - 2026-06-02

## What Changed

- Added a persisted Markdown self-evolution cycle report path beside the existing JSON cycle report.
- The Markdown report exposes quota-aware executor/model decision evidence for human review and publication workflows.
- The report includes requested and selected executor, validation status, JSON/validation artifact paths, candidate provider/model/locality/quota/cost/status/reason rows, executor attempts, quota-preservation rationale and repair goals.

## Product Impact

This moves Forge closer to v0.5 by making executor/model policy visible as a durable artifact instead of burying the decision only in JSON. It supports business-quality runtime decisions because humans can inspect why Forge chose OpenCode, Gemini, Codex or local Ollama, what quota/cost assumptions were used, and what repair goals remain.

## Validation

- Added contract coverage for `forge self run --dry-run --output json` proving the Markdown report path is exposed and the file contains quota-aware executor policy evidence.
- Focused validation passed for `cargo test self_run_` and `cargo test executor_policy`.
- Full required validation passed for this cycle: `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, and `cargo build --release`.
