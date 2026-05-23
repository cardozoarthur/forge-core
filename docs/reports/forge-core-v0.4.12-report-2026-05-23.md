# Forge Core v0.4.12 Report - 2026-05-23

## Objective

Persist the new Personality/Soul Routing goal so future Forge self-evolution cycles
can study and implement persona-aware execution for human-facing artifacts.

## Change

The self-evolution prompt now includes a strategic goal for Personality/Soul Routing.
Future cycles are directed to inspect how Codex handles developer/personality
instructions and how Paperclip models soul, voice, tone or persona, then design a
Forge-native contract for controlled persona switching.

The technical definition and roadmap now define the safety boundary:

- persona is selected per node, not as hidden global behavior;
- selected profile metadata must be included in context lineage;
- persona shifts are explicit, auditable and validation-gated;
- personality must not override Forge goals, validation rules, safety constraints or source-of-truth state.

## Validation

The CLI contract test for `forge self run --dry-run` now asserts that executor
prompt packets contain the Personality/Soul Routing goal and its audit boundary.

## Next Recommended Cycle

Implement the first persona profile data model and a dry-run `forge context` field
that records persona profile id, rationale and checksum for human-facing nodes.
