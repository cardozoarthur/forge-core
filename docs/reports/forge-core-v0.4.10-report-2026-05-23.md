# Forge Core v0.4.10 Report - 2026-05-23

## Objective

Advance the Context Routing Engine with a small production increment: make `forge context` packets versioned, replayable and sharded without changing executor compatibility.

## Change

`forge context` now emits schema `forge.context.v1` and routing policy `task_local_priority_budget_v1`.

Each context response includes:

- `context_sha256` for the exact executor-facing context body;
- `included_sections` for full sections selected within the budget;
- `omitted_sections` for sections skipped by the budget gate;
- `shards` with section name, source, priority, inclusion decision, byte count, first-line summary and SHA-256 checksum.

The legacy `content` field remains the executor-facing payload, so existing Codex/OpenCode callers can keep consuming the context body while newer adapters inspect the manifest for provenance and replay.

## Routing Order

The first policy uses deterministic task-local priority:

1. local objective;
2. context requirements;
3. validation rules;
4. dependencies;
5. work item metadata;
6. workflow constraints.

This is intentionally conservative. It avoids broad project history and gives future cycles a stable place to add persisted summaries, artifact shards, subflow context and validation-gate-specific packets.

## Safety

- No Docker, Kubernetes or Knative resources were read or mutated.
- No model provider behavior was hardcoded.
- Context package changes are additive JSON fields.
- The previous `content` and `included_sections` fields remain present.

## Validation

Required validation passed:

- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test`: passed, 34 CLI contract tests.
- `cargo build --release`: passed.

CLI smoke passed with the release binary first in `PATH`:

- `forge plan --goal "Create a delivery platform" --output json`: passed.
- `forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`: passed.

## Installation

The required global install command was attempted:

```bash
cargo install --path . --force
```

It failed because `/home/arthur/.cargo` is read-only in this execution environment:

```text
error: failed to open: /home/arthur/.cargo/.crates.toml

Caused by:
  Read-only file system (os error 30)
```

A workspace-local install was then completed offline:

```bash
cargo install --path . --force --root .forge/local-install --offline
.forge/local-install/bin/forge --version
```

The installed workspace-local binary reports `forge 0.4.10`.

## Publication

The GitHub CLI publication contract was started:

- `gh auth token`: passed with stdout redirected.
- `git remote get-url origin`: returned `https://github.com/cardozoarthur/forge-core.git`.

Commit and push are blocked in this execution environment because `.git` is mounted
read-only:

```text
fatal: Unable to create '/home/arthur/projects/forge-core/.git/index.lock': Sistema de ficheiros só de leitura
```

The relevant mount state is:

```text
/dev/nvme0n1p2 on /home/arthur/projects/forge-core type ext4 (rw,nosuid,nodev,relatime,errors=remount-ro)
/dev/nvme0n1p2 on /home/arthur/projects/forge-core/.git type ext4 (ro,nosuid,nodev,relatime,errors=remount-ro)
```

The validated changes remain in the working tree but were not committed or pushed.

## Next Recommended Cycle

Add persisted context summaries keyed by workflow/task/artifact checksum, then have `forge context` reuse those summaries before including raw artifact or history shards.
