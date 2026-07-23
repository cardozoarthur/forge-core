# Validation report — worktrees and internal sandboxes

## Required repository gates

| Gate | Result |
| --- | --- |
| `cargo fmt --check` | PASS |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS |
| `cargo test` | PASS — 735 tests, 12 suites |
| `cargo build --release` | PASS |

## Worktree contracts

| Suite | Result | Coverage |
| --- | --- | --- |
| `forge_worktree_contract` | 5/5 | register/create/bind/context/handoff/sandbox plan-run |
| `forge_worktree_guard_contract` | 9/9 | path precedence, symlinks, predecessor DAG, retry and drift |
| `forge_worktree_lifecycle_contract` | 14/14 | timeout, launch failure, environment, lifecycle, MCP and redaction |

## Security and process regressions

- `setsid` daemon that retains captured pipes is killed at timeout without an unbounded reader join.
- redirected `setsid` daemon is persisted in `payload_descendant_pids` and killed by stop.
- SIGKILL of the supervisor is detected by status; payload group and tracked descendants are killed before failure is persisted.
- a secret in a neutral environment variable name is blocked before execution.
- a generic high-entropy secret is blocked in argv and redacted in stdout/stderr.
- MCP rejects task-scoped plan/run/start when `workflow_id` is absent.
- stop is a normal terminal path: receipt and lifecycle are `sandbox_stopped` with no synthetic error.

## CLI smokes

- `forge plan --goal "Create a delivery platform" --output json`: PASS.
- `forge skill install --target codex --target opencode --output json --home <isolated>`: PASS; all modular skills installed.
- process, Bubblewrap, lifecycle and MCP smokes: PASS.

The active workflow validation is intentionally performed only after its eight evidence tasks are promoted in dependency order.
