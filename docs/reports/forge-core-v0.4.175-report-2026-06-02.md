# Forge Core v0.4.175 Report - Product/PM Entrypoint Decision Route

## What Changed

- Added `/pm <goal>` handling to `forge interactive route`.
- The PM route now creates a durable async workflow before executor handoff.
- The route records an initial product decision with rationale, alternatives, trade-offs, success metrics and backlog mutation.
- The JSON route report exposes `product_decision_id` and `product_decision_revision` for downstream status/report tooling.

## Why It Matters

This moves the Product/PM CLI-TUI closer to being Forge's main entry point for human-guided product and workflow creation. PM intent is no longer a transient slash-command hint: it becomes revisioned workflow state with durable product/business rationale before execution begins.

## Validation

- `cargo test interactive_pm_route_creates_workflow_with_initial_product_decision`
- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`
- `target/release/forge plan --goal "Create a delivery platform" --output json`
- `target/release/forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`

## Operational Note

Local installation with `cargo install --path . --force` was attempted after validation, but the sandbox could not write to `/home/arthur/.cargo/.crates.toml` and returned `Read-only file system`.

## v0.5 Impact

- Real-time agent runtime: PM-created workflows start as durable async requests with run/workflow ids.
- Advanced TUI: `/pm` becomes an actionable workflow creation route, not only a command catalog entry.
- Governed mutations: the initial product decision is revisioned and recorded as an event.
- Business/product decisions: product rationale, alternatives, trade-offs, metrics and backlog mutation are captured before executor work.
