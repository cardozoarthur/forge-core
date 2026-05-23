# Forge Core Agent Guide

Forge Core is a Rust workflow runtime. Treat it as operational infrastructure, not as a chatbot wrapper or a human-flow builder.

## Product Rules

- Preserve Forge as the orchestration authority. CLIs and model providers are execution engines, even when they integrate tightly with Forge for usability.
- Treat skill/plugin/native CLI coupling as an adoption layer, not as the source of truth for workflow state.
- Support both integration directions: CLIs call Forge for planning/context/validation, and Forge calls CLIs through bounded executor adapters for long-running tasks.
- Preserve validation-before-promotion semantics.
- Keep self-improvement controlled: generate experiments, benchmark, compare and promote only after validation.
- Do not add unrestricted self-modification.
- Keep models provider-agnostic.
- Support autonomous and mixed workflows that can combine AI tasks, deterministic non-AI tasks, cron/wait tasks and notifications without requiring a human decision at every step.
- Prefer deterministic, testable behavior over clever prompt logic.

## Code Rules

- Core language: Rust.
- CLI binary name: `forge`.
- Persistence: SQLite through `rusqlite`.
- Config and artifacts should remain human-readable where practical.
- Keep modules small and purpose-specific:
  - `intent`: objective parsing;
  - `graph`: workflow/task structures;
  - `context`: bounded context packages;
  - `execution`: execution runtime;
  - `validation`: validation gates;
  - `artifact`: artifact persistence/listing;
  - `improve`: controlled optimization loop;
  - `skill`: Codex/OpenCode skill install.

## Required Validation

Run before claiming completion:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

For CLI smoke:

```bash
forge plan --goal "Create a delivery platform" --output json
forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke
```
