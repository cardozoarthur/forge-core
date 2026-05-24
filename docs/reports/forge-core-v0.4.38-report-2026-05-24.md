# Forge Core v0.4.38 Report - Persona Handoff Contract

## Increment

Forge Core now projects node-scoped Personality/Soul Routing into executor handoff
packets.

`forge task handoff` emits `forge.executor_handoff.v4`. Human-facing tasks with
persona routing keep the compact `persona_mode` field and now also include a
versioned `persona_contract`:

- `schema_version = forge.persona_handoff.v1`;
- persona mode, node scope, instruction source, voice and tone;
- source model references for Codex developer/personality instructions and
  Paperclip-style soul/voice/tone/persona modeling;
- `persona_routing_required` validation gate and auditable flag;
- workflow context lineage SHA-256 and persona mode SHA-256.

This moves persona enforcement closer to executor adapters. Adapters can now
validate the human-facing mode before work starts without parsing unrelated
context shards from the nested context package.

## Runtime Impact

- Bounded Codex/OpenCode adapters receive persona routing as an explicit handoff
  contract, not only as context narrative.
- The Context Routing Engine remains the source of lineage. The handoff contract
  references the same lineage hashes already produced by `forge context`.
- Existing compact operator checks can continue reading `persona_mode`.

## Safety

The contract is read-only metadata derived from Forge-owned workflow/task state.
It does not authorize executors, complete tasks, promote workflows, execute local
Python/Node.js code, install Knative, or mutate Docker/Kubernetes/Knative
resources.

Promotion safety remains enforced by `forge validate`, which rejects persona
switches that are not node-scoped, auditable, source-model backed and gated by
`persona_routing_required`.

## Validation Evidence

- RED: `cargo test task_handoff_packet_carries_node_scoped_persona_contract --test forge_cli_contract` failed first because the packet still reported `forge.executor_handoff.v3`.
- GREEN: `cargo test task_handoff --test forge_cli_contract` passed with 4 handoff tests.
- Required validation passed:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test` with 64 CLI contract tests passing
  - `cargo build --release`
- CLI smokes passed with `target/release/forge`:
  - `target/release/forge --version` reported `forge 0.4.38`
  - `target/release/forge --store /tmp/forge-core-smoke-plan.sqlite plan --goal "Create a delivery platform" --output json`
  - `target/release/forge --store /tmp/forge-core-smoke-skill-exact.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`

## Installation Note

Global installation was attempted after validation with:

```bash
cargo install --path . --force
```

The sandbox rejected writes to `/home/arthur/.cargo/.crates.toml` with
`Read-only file system (os error 30)`.

Checkout-local installation succeeded offline with:

```bash
cargo install --path . --force --root .forge/local-install --offline
.forge/local-install/bin/forge --version
```

The checkout-local binary reports `forge 0.4.38`.

## Publication Note

The GitHub CLI contract was started after validation:

- `gh auth token >/dev/null` succeeded.
- `git remote get-url origin` returned `https://github.com/cardozoarthur/forge-core.git`.
- `git diff --check` succeeded.
- Branch creation failed because Git could not create
  `.git/refs/heads/codex-persona-handoff-contract.lock`: `Sistema de ficheiros
  só de leitura`.
- `git add ...` failed because Git could not create `.git/index.lock`:
  `Sistema de ficheiros só de leitura`.
- `git push` was attempted and failed with `Could not resolve host:
  github.com` under restricted network access.

## Next Cycle

Add artifact-level persona validation evidence: after a human-facing artifact is
generated, `forge validate` should be able to link the artifact back to the
handoff `persona_contract` and report whether the artifact satisfied the selected
mode, audience, factuality and validation gate.
