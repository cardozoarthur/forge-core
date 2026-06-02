# Forge Core v0.4.177 Report - Publication AI Synthesis Metadata

## What Changed

- Added persisted publication metadata for previous Markdown report path, previous/current commit ranges, pushed commit and report artifact path.
- Added `ai_synthesis` metadata to self-evolution publication JSON with provider/model fields, synthesis status and fallback reason.
- Updated the 2-hour publication Markdown report to state when deterministic fallback was used instead of native AI-assisted report generation.
- Added unit coverage for publication report synthesis metadata and previous-report/commit-range carry-forward.

## Why It Matters

The live publication requirement says periodic GitHub/Telegram reports must be AI-assisted and must persist report metadata. Forge does not yet have the native AI synthesis node, so this increment makes the gap visible and auditable instead of pretending the deterministic bridge is AI-generated.

## Validation

- `cargo test self_evolve::tests::test_publication_markdown_records_ai_synthesis_fallback_metadata`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`
- `target/release/forge plan --goal "Create a delivery platform" --output json`
- `target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-177`

The skill-install smoke completed and surfaced the current executor policy evidence: Codex was usable, Gemini was not configured for non-interactive use, and OpenCode was marked as non-interactive readiness risk because the `opencode models` probe failed.

## v0.5 Impact

- Real-time agent runtime: publication state now carries report lineage and commit ranges between recurring cycles.
- Advanced CLI/TUI: status/report views can expose whether publication synthesis was AI-assisted or deterministic fallback.
- Governed mutations: publication artifacts record what was pushed and how the report was generated.
- Quota-aware executor policy: native AI report generation remains a visible future decision instead of silently consuming or skipping model quota.
- Better business/product decisions: Forge can explain reporting quality, remaining gaps and how publication work supports v0.5 adoption evidence.
