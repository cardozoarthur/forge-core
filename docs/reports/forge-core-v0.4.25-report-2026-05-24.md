# Forge Core v0.4.25 Report - 2026-05-24

## Objective

Make Personality/Soul Routing promotion-safe by validating declared persona switches
instead of only carrying them through graph and context metadata.

## Change

`forge validate` now checks every task that declares `persona` metadata. Promotion is
blocked with `failed_rules.kind="persona_routing"` when a persona switch is not:

- explicit through a non-empty mode, voice and tone;
- scoped to the node;
- auditable;
- gated by `persona_routing_required`;
- backed by both `codex_developer_personality_instructions` and
  `paperclip_soul_voice_tone_persona` source model references.

Invalid persona routing also creates a rework task so the node returns to work instead
of being promoted after execution.

## Safety

This is a read-only validation gate over Forge-owned workflow metadata. It does not
run models, select providers, authorize CLIs, execute local Python/Node.js code, mutate
Docker/Kubernetes/Knative resources, or promote workflows.

Persona-free tasks remain governed by the existing task-status, graph and goal-readiness
rules. The new gate only applies when a workflow node explicitly declares a persona
switch.

## TDD Evidence

- RED: `cargo test validation_blocks_promotion_when_persona_routing_is_not_auditable`
  failed because a completed workflow with corrupted persona routing still validated
  as promotable.
- GREEN: the focused test passed after `forge validate` started emitting a
  `persona_routing` failure and rework task for incomplete or non-auditable persona
  metadata.

## Validation

- `cargo fmt --check`: passed after formatting the new Rust changes.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo test`: passed, including 51 CLI contract tests.
- `cargo build --release`: passed.
- Release-binary smoke `./target/release/forge --store /tmp/forge-core-v0425-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json`: passed.
- Release-binary smoke `./target/release/forge --store /tmp/forge-core-v0425-skill-smoke.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`: passed.

## Installation

- `cargo install --path . --force`: blocked by this execution environment because
  `/home/arthur/.cargo/.crates.toml` is read-only.
- `cargo install --path . --force --root .forge/local-install`: blocked by restricted
  network while trying to refresh crates.io.
- `cargo install --path . --force --root .forge/local-install --offline`: passed.
- `.forge/local-install/bin/forge --version`: `forge 0.4.25`.
- Installed-binary smoke with `.forge/local-install/bin` first in PATH:
  - `forge plan --goal "Create a delivery platform" --output json`: passed with `status="planned"`.
  - `forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`: passed with `skill="forge-core"`.
- Default `forge --version` still resolves to `/home/arthur/.cargo/bin/forge` and reports
  `forge 0.4.24` until the global Cargo directory is writable.

## Next Recommended Cycle

Expose persona-routing validation status in `forge inspect --verbose` so operators can
see which human-facing node would fail promotion before running full workflow validation.
