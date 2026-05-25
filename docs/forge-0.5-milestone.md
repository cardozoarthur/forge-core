# Forge 0.5 Milestone Boundary

Status vocabulary: `implemented`, `validated`, `groundwork`, `planned`, `blocked`.

Forge 0.5 is the first milestone allowed to claim the AI-first creative runtime, editable creative artifacts, live collaboration, whiteboards, design systems/tokens, componentization, AGUI-style interaction integration, web/CLI/MCP editing and human+AI creative workflows. The 0.4.x line may ship enabling infrastructure only.

| Capability | Status | Evidence | Gap before 0.5 promotion |
| --- | --- | --- | --- |
| Interactive Forge CLI baseline | validated | 0.4.97-0.4.109 validate `forge` TTY home, slash-command catalog, conversational routing, retention decisions and script-safe non-TTY behavior. | Full terminal TUI loop, autocomplete and richer inline mode remain before a 0.5 promotion claim. |
| Human decision/form nodes | validated | 0.4.98-0.4.109 validate choice prompts, form schemas, durable decisions, timeout state, pause/resume, list/status/inspect visibility and MCP create/list/answer/expire bridges. | Web UI, repeated-answer default promotion and richer TUI rendering remain planned. |
| Scheduler/loop/subflow foundation | validated | 0.4.92-0.4.112 validate cron nodes, loop state, due execution, missed-run reconciliation, native scale-to-zero decisions, daily Goal research smoke artifacts, schedule visibility and MCP aggregate schedule/loop summaries. | Production executor adapters for real research/page inspection remain planned. |
| Milestone governance/status surface | validated | 0.4.100 adds `forge milestone status --version 0.5 --output json` and MCP tool `forge.milestone.status` with promotion blockers. | Keep this surface updated as creative runtime capabilities move from planned to implemented or validated. |
| Creative artifact IR baseline | validated | 0.4.102 validates structured creative artifact IR for screens, whiteboards, documents, slide decks and component specs with serde round-trip, CLI attach/list/inspect coverage and workflow integration. | Declarative import/export, rendering adapters and full runtime editing flows remain for 0.5. |
| Design systems/tokens | validated | 0.4.102 validates token collection and semantic alias data structures with workflow storage and CLI set/get coverage. | Token resolution, inheritance, propagation and human edit preservation demos remain for 0.5. |
| Componentization and AI-first UI surfaces | validated | 0.4.102 validates component specs with props, variants, states, slots, token dependencies and patch-by-intent schema as portable IR. | Rendered previews, action registry generation, token dependency resolution and patch-by-intent execution remain for 0.5. |
| Live collaboration | groundwork | Human decision audit, durable interaction state, MCP human interaction bridge and creative MCP tools provide collaboration groundwork without claiming full live editing. | Need presence, cursors/selections, patch streams, comments, conflict handling and rollback demo before 0.5 promotion. |
| Research artifact baseline | planned | Research topics are listed in prompt v2, this milestone document and the scheduler/loop/subflow validation reports. | Need source-grounded comparison of Penpot, Stitch, v0, Impeccable/AGUI-style protocols, Superpowers, Remotion/Figma capabilities and OBS/media composition lessons before 0.5 promotion. |
| Export/demo baseline | validated | MCP creative artifact/token tools and scheduler worker-status evidence validate the agent-facing export/demo baseline while daily Goal smoke continues to produce Markdown/PDF artifacts through Forge-owned workflow semantics. | Need richer rendered design/tokens/component demos before a final 0.5 promotion claim. |

Promotion rule: a future `0.5` manifest must fail promotion if any required creative/runtime capability is only a goal, prompt item or partial infrastructure without working demo evidence.

Use `forge milestone manifest --version 0.5 --output json` or MCP tool `forge.milestone.manifest` to generate the release-gate manifest with requirements, completed capabilities, missing capabilities, validation evidence, demos, known gaps and the current promotion decision.
