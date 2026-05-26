# Forge 0.5 Creative Runtime Source Research

Date: 2026-05-26
Status: validated research baseline for Forge 0.5 milestone gating
Prompt packet: `forge.self_evolution.prompt.v2`

This artifact converts the Forge 0.5 creative-runtime research requirement into source-grounded runtime rules. It does not make Penpot, Stitch, v0, AG-UI, Impeccable, Figma, Remotion or OBS orchestration authorities. Forge remains the workflow owner; external tools are references, import/export paths or bounded executor adapters.

## Source Set

| Source | Evidence used | Forge implication |
| --- | --- | --- |
| [Penpot data model](https://help.penpot.app/technical-guide/developer/data-model/) | Pages and components share a container abstraction, with shape trees and shapes as editable design layers. | Forge creative IR must preserve identity, hierarchy and render/export metadata instead of flattening creative work into screenshots. |
| [Penpot data guide](https://help.penpot.app/technical-guide/developer/data-guide/) | Data evolution, optional attributes and component synchronization are compatibility-sensitive. | Forge migrations, patch diffs and token/component propagation need backward-compatible defaults and explicit sync/touched state. |
| [Penpot design tokens](https://help.penpot.app/user-guide/design-systems/design-tokens/) | Tokens align with the W3C DTCG format and integrate with components and layout. | Forge tokens remain source-of-truth artifacts with import/export adapters, semantic aliases and impact previews. |
| [Google Stitch real-time design](https://blog.google/innovation-and-ai/models-and-research/google-labs/stitch-updates/) | Stitch handles text, voice, codebase and design-file inputs as real-time canvas iteration. | Forge should model prompt-to-design as brief, variants, critique, patch, validation and export nodes. |
| [v0 docs](https://v0.app/docs) | v0 uses prompts to create high-fidelity UIs, full-stack code, live prototypes, PRs and deployment. | Forge should route product generation through workflow state, validation gates and retention policy before generated products are trusted. |
| [AG-UI protocol](https://github.com/ag-ui-protocol/ag-ui) and [AG-UI overview](https://docs.ag-ui.com/introduction) | AG-UI emphasizes event streams, shared state, frontend tools, interrupts, steering and cancellation. | Forge should expose AGUI-style surfaces as transport adapters while retaining authoritative state, permissions and audit. |
| [Impeccable docs](https://impeccable.style/docs/impeccable/) | Design quality is shaped by explicit product/design files and anti-pattern guidance before code. | Forge creative workflows need design-system discovery, anti-generic gates and scoped persona/taste routing. |
| [Figma MCP developer docs](https://developers.figma.com/docs/figma-mcp-server/) | Agents can read design context and write frames, components, variables and auto-layout through MCP. | Forge MCP should exchange structured IR patches and token/component references rather than whole-artifact rewrites. |
| [Remotion fundamentals](https://www.remotion.dev/docs/the-fundamentals) and [Sequence](https://www.remotion.dev/docs/sequence) | Video is explicit frame/timeline metadata with composable sequences. | Forge media plans should use deterministic timeline metadata and renderer adapters, without hard-depending on Remotion. |
| [OBS Studio overview](https://obsproject.com/kb/obs-studio-overview) | OBS composition is scenes, sources, ordering, filters and transitions. | Forge media/composition IR can borrow scene/source/filter/transition concepts while staying lightweight. |

## Local Skill Inputs

| Local source | Evidence used | Forge implication |
| --- | --- | --- |
| `/home/arthur/.codex/plugins/cache/openai-curated/superpowers/6188456f/skills/brainstorming/SKILL.md` | Requires design exploration, alternatives and approval before implementation. | Creative ambiguity should become human decision/form nodes with durable approval evidence. |
| `/home/arthur/.codex/skills/stitch-design/SKILL.md` | Separates prompt enhancement, design-system synthesis and screen generation/editing. | Forge should route generation and edit operations as separate workflow nodes with preserved design-system context. |
| `/home/arthur/.codex/skills/.system/imagegen/SKILL.md` | Distinguishes generated bitmap assets from repo-native vector/code artifacts and requires project-bound persistence. | Forge should keep bitmap generation as artifact lineage, not as a replacement for editable IR. |
| `/home/arthur/.codex/plugins/cache/openai-curated/figma/6188456f/skills/figma-generate-design/SKILL.md` | Requires component, variable and style discovery before mutating Figma screens. | Forge product workflows should inspect design systems before generation and reject hardcoded-token drift. |
| `/home/arthur/.codex/skills/remotion/SKILL.md` | Uses frame/time primitives, sequences and explicit render metadata. | Forge can borrow timeline discipline while keeping video rendering adapters optional. |

## Runtime Findings

1. Editable creative artifacts need stable identity and hierarchy.
   Forge rule: every creative patch must target stable artifact/object IDs and preserve token/component references unless replacement is explicit.

2. Tokens are executable creative configuration.
   Forge rule: token patches are high-impact changes with resolution, impact preview and validation before promotion.

3. Prompt-to-UI is multi-stage workflow work.
   Forge rule: brief intake, variant generation, critique, human approval, patching, validation and export must be distinct nodes.

4. Agent UI needs durable events and shared state.
   Forge rule: AGUI-style streaming can be an adapter, but Forge owns event persistence, permissions, cancellation and replay.

5. Design taste is a validation input.
   Forge rule: creative workflows should include anti-generic design review, accessibility checks and scoped persona/soul routing where taste matters.

6. Media output should start from portable timeline IR.
   Forge rule: video/animation workflows must define scenes, sources, timeline, dimensions, fps and duration before a renderer is selected.

## Forge Validation Gates

| Gate | Validates | Failure condition |
| --- | --- | --- |
| `creative_ir_round_trip_fidelity` | AI and human edits preserve IDs, hierarchy, comments, token references and audit history. | A patch rewrites unrelated content or destroys human-edited fields without explicit approval. |
| `design_token_source_of_truth` | Raw tokens, semantic aliases, modes and overrides resolve deterministically across artifacts. | An artifact embeds hardcoded values where token references are required. |
| `agent_ui_event_audit` | Slash commands, web actions and MCP calls produce replayable event records with origin and permission state. | An agent-visible action mutates state without a durable event. |
| `collaboration_conflict_replay` | Concurrent human/AI patches expose conflict state, chosen resolution and rollback evidence. | A conflict is silently resolved or loses either participant's intent. |
| `anti_generic_design_review` | Generated output is checked for generic UI patterns, accessibility and responsive text overflow. | A creative artifact passes with unreviewed generic style, inaccessible contrast or clipped text. |
| `media_timeline_determinism` | Media artifacts declare scenes, sources, timeline, dimensions, fps and duration before rendering. | A video or animation export cannot be reproduced from stored Forge state. |
| `export_fidelity_accessibility` | Markdown/PDF/slides/web exports preserve source artifact meaning, structure and accessibility metadata. | An export becomes the source of truth or cannot trace back to editable IR. |

## Workflow Templates

### `prompt_to_screen_with_tokens`

Stages: brief intake -> design-system discovery -> token proposal or reuse -> screen variant generation -> human direction choice -> patch-by-intent -> accessibility/export validation.

Deterministic nodes: token resolution, component dependency scan, text overflow checks.
AI nodes: variant generation, design critique.
Human gates: approve design-system baseline, choose visual direction.

### `ai_first_whiteboard_brainstorm`

Stages: goal framing -> idea generation -> duplicate detection -> semantic clustering -> vote/decision recording -> task/subflow conversion -> board export.

Deterministic nodes: duplicate detection, decision trace export, Markdown/PDF export.
AI nodes: alternative generation, assumption challenge.
Human gates: approve clusters, approve decisions, approve task conversion.

### `structured_deck_document_export`

Stages: outline -> narrative validation -> asset selection -> slide/document IR assembly -> export -> fidelity check.

Deterministic nodes: outline schema validation, link/image checks, PDF/Markdown export.
AI nodes: narrative synthesis, visual brief generation.
Human gates: approve outline, approve final delivery constraints.

### `long_video_storyboard_plan`

Stages: media brief -> scene/source/timeline planning -> script and beat sheet -> asset manifest -> render adapter selection -> frame/sample validation.

Deterministic nodes: timeline duration checks, asset hash manifest, sample frame checks.
AI nodes: script summarization, scene direction options.
Human gates: approve script, approve render budget.

### `agent_visible_component_patch`

Stages: component lookup -> intent-to-prop mapping -> token dependency impact preview -> bounded patch -> human review if high impact -> status/inspect evidence.

Deterministic nodes: component manifest parse, action registry validation, token impact preview.
AI nodes: patch wording normalization.
Human gates: approve high-impact component changes.

## Lean Governance Decisions

| Decision | Accepted complexity | Rejected complexity | Evidence metric |
| --- | --- | --- | --- |
| Forge-owned IR before vendor adapter | Compact schemas for screens, whiteboards, documents, slides, media plans, tokens, components and collaboration events. | Hard dependency on Penpot, Figma, Stitch, v0, Remotion or OBS to own workflow state. | Round-trip patch fidelity and fewer whole-artifact rewrites. |
| Deterministic gates before AI review | Schema checks, token resolution, dependency scans, text overflow checks, artifact hashing and export checks. | Model calls for stable parsing, hashing, listing, PDF generation or Telegram delivery. | Lower cost per recurring workflow and fewer retries after AI review. |
| Event-stream adapter, not orchestrator | Event schema mapping and permission-aware command routing. | Frontend event protocols mutating workflow state without Forge revisioning. | Durable replay, pause/resume and cross-surface decision consistency. |

## Promotion Impact

The required source-grounded research baseline is now present as this artifact and as the structured runtime surface `forge milestone research --version 0.5 --output json` plus MCP tool `forge.milestone.research`.

This validates the research gate for Forge 0.5, but it does not by itself bump the package to 0.5. A future release action still needs an explicit version change, release artifact bundle and human-controlled promotion decision.
