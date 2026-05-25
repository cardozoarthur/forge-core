# Forge 0.5 Milestone Boundary

Status vocabulary: `implemented`, `validated`, `groundwork`, `planned`, `blocked`.

Forge 0.5 is the first milestone allowed to claim the AI-first creative runtime, editable creative artifacts, live collaboration, whiteboards, design systems/tokens, componentization, AGUI-style interaction integration, web/CLI/MCP editing and human+AI creative workflows. The 0.4.x line may ship enabling infrastructure only.

| Capability | Status | Evidence | Gap before 0.5 promotion |
| --- | --- | --- | --- |
| Interactive Forge CLI baseline | groundwork | `forge` TTY home, slash-command catalog, conversational routing and retention decisions were validated in 0.4.97. | Full terminal TUI loop, autocomplete and inline mode still need implementation evidence. |
| Human decision/form nodes | groundwork | 0.4.98 adds choice prompts, form schemas, durable decisions, timeout state, pause/resume and list/status/inspect visibility. 0.4.104 exposes create/list/answer/expire through MCP for agent-facing approval bridges. | Web UI, repeated-answer default promotion and richer TUI rendering remain planned. |
| Scheduler/loop/subflow foundation | validated | 0.4.92-0.4.106 validate cron nodes, loop state, due execution, missed-run reconciliation, native scale-to-zero decisions, daily Goal research smoke artifacts and schedule visibility. | Production executor adapters for real research/page inspection remain planned. |
| Milestone governance/status surface | validated | 0.4.100 adds `forge milestone status --version 0.5 --output json` and MCP tool `forge.milestone.status` with promotion blockers. | Keep this surface updated as creative runtime capabilities move from planned to implemented or validated. |
| Creative artifact IR baseline | groundwork | 0.4.102 adds structured creative artifact IR for screens, whiteboards, documents, slide decks and component specs with CLI attach/list/inspect coverage. | Need declarative import/export, rendering adapters and full runtime editing flows. |
| Design systems/tokens | groundwork | 0.4.102 adds token collection and semantic alias data structures with workflow storage and CLI set/get coverage. | Need token resolution, inheritance, propagation and human edit preservation demos. |
| Componentization and AI-first UI surfaces | groundwork | 0.4.102 adds component specs with props, variants, states, slots and token dependencies as portable IR. | Need rendered previews, action registry generation, token dependency resolution and patch-by-intent execution. |
| Live collaboration | planned | Human decision audit groundwork exists in 0.4.98. | Need presence, cursors/selections, patch streams, comments, conflict handling and rollback demo. |
| Research artifact baseline | planned | Research topics are listed in prompt v2. | Need source-grounded comparison of Penpot, Stitch, v0, Impeccable/AGUI-style protocols, Superpowers, Remotion/Figma capabilities and OBS/media composition lessons. |
| Export/demo baseline | planned | Daily Goal smoke produces Markdown/PDF as scheduler validation, not creative runtime proof. | Need one design/tokens/component workflow demo and one structured document/slide/whiteboard workflow demo. |

Promotion rule: a future `0.5` manifest must fail promotion if any required creative/runtime capability is only a goal, prompt item or partial infrastructure without working demo evidence.
