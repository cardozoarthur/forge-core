# Forge Core v0.4.168 Report - 2026-06-02

Run id: `run_9ff8a6cdf43a4539a9d3245c5d4d403a`
Workflow id: `wf_dfa9a20f8ade43a69fb82cef22d0ba1a`
Cycle: 17

## What Changed

- `forge executors` quota policy now exposes workload routes for high-value PM/business/creative reasoning, deterministic validation/reporting and privacy-sensitive or low-value repetitive work.
- The workload routes state when Forge should spend non-local quota because better reasoning changes product or business outcome, and when it should preserve quota with command nodes or local OpenCode/Ollama.
- `forge workflow decision` now records alternatives, trade-offs, success metrics and backlog mutation, not only title and rationale.
- Self-evolution next-goal decisions now persist the same richer product-decision fields so the recurring loop carries product rationale and backlog impact.

## Product Decision

This cycle prioritized quota-aware executor decision reporting and durable product decision artifacts before new PM/TUI surface area. The business reason is that Forge needs auditable judgement about when to spend scarce non-local model quota and how each product choice changes the backlog. Technical progress alone is not enough for v0.5; Forge must show why a decision improves user value, cost control, speed, risk and product leverage.

## How It Was Worked

- Added report-facing workload classes instead of changing live executor handoff behavior without a broader selection contract.
- Completed an already-started product-decision artifact change by adding CLI fields, serialization defaults and contract assertions.
- Kept the implementation deterministic and testable through CLI JSON output rather than live provider calls.

## Validation Results

- Targeted test passed: `cargo test workflow_decision_records_revisioned_product_state`.
- Targeted test passed: `cargo test sync_persists_human_allowed_executor_policy`.
- Targeted test passed: `cargo test executor_report_surfaces_persisted_quota_observations`.
- Required validation passed: `cargo fmt --check`.
- Required validation passed: `cargo clippy --all-targets --all-features -- -D warnings`.
- Required validation passed: `cargo test` with 80 unit tests, 240 CLI contract tests and doc-tests.
- Required validation passed: `cargo build --release`.
- CLI smoke passed: `./target/release/forge plan --goal "Create a delivery platform" --output json`.
- CLI smoke passed: `./target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v04168-2`.
- The skill smoke exposed `workload_routes`, classified OpenCode as `skipped_interactive_hang_risk` after its model probe failed, classified Gemini as not configured, and left Codex as the usable non-local fallback.
- Default `cargo install --path . --force` was blocked by read-only `/home/arthur/.cargo/.crates.toml`.
- Workspace fallback install passed with `CARGO_INSTALL_ROOT=/home/arthur/projects/forge-core/.forge/local-install cargo install --path . --force --locked --offline`.
- The fallback binary reports `forge 0.4.168`.

## Safety

- No Docker, Kubernetes or Knative resources were installed or mutated.
- No Telegram token, chat id or secret is recorded in this report.
- The change only affects Forge-owned executor policy reporting, product decision artifacts, self-evolution decision persistence, tests, version metadata and report artifacts.

## Publication Status

- GitHub auth check passed with `gh auth token` redirected away from logs.
- Git remote check passed: `https://github.com/cardozoarthur/forge-core.git`.
- Commit/push publication is blocked in this sandbox because writing `.git/index.lock` fails with `Sistema de ficheiros só de leitura`.
- Telegram publication should send this Markdown report as a document through the existing allowed local publisher or a future Forge-owned scheduled notification node.

## v0.5 Movement

- Real-time agent runtime: executor choices now include workload-specific quota reasoning that can guide live handoff policy.
- Advanced TUI/status readiness: Product/PM surfaces can show alternatives, trade-offs, success metrics, backlog mutation and quota routing without inventing new schema.
- Governed mutations: product decisions remain revisioned workflow state and legacy records remain loadable.
- Quota-aware executor policy: Forge distinguishes high-value non-local reasoning from deterministic or local work using explicit workload routes.
- Better business/product decisions: decisions now capture user value rationale, alternatives, trade-offs and success metrics as first-class workflow artifacts.

## Next Cycle

Use the workload routes to drive concrete executor handoff selection and status/inspect evidence: classify task workload value, apply quota observations, select or skip OpenCode/Gemini/Codex/local candidates, and persist the selected route with validation evidence.
