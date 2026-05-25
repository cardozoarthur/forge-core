# Forge 0.5 Milestone Boundary

Status vocabulary: `implemented`, `validated`, `groundwork`, `planned`, `blocked`.

Forge 0.5 is the first milestone allowed to claim the AI-first creative runtime, editable creative artifacts, live collaboration, whiteboards, design systems/tokens, componentization, AGUI-style interaction integration, web/CLI/MCP editing and human+AI creative workflows. The 0.4.x line may ship enabling infrastructure only.

| Capability | Status | Evidence | Gap before 0.5 promotion |
| --- | --- | --- | --- |
| Interactive Forge CLI baseline | groundwork | `forge` TTY home, slash-command catalog, conversational routing and retention decisions were validated in 0.4.97. | Full terminal TUI loop, autocomplete and inline mode still need implementation evidence. |
| Human decision/form nodes | groundwork | 0.4.98 adds choice prompts, form schemas, durable decisions, timeout state, pause/resume and list/status/inspect visibility. | Web UI, MCP approval bridge, repeated-answer default promotion and richer TUI rendering remain planned. |
| Scheduler/loop/subflow foundation | validated | 0.4.92-0.4.96 validate cron nodes, loop state, due execution, missed-run policy and daily Goal research smoke artifacts. | Production executor adapters for real research/page inspection remain planned. |
| Creative artifact IR baseline | planned | Requirements captured in self-evolution prompt v2. | Need working structured IR for screens, whiteboards, documents/slides and component manifests. |
| Design systems/tokens | planned | Requirements captured in self-evolution prompt v2. | Need token schema, semantic resolution, inheritance, propagation and human edit preservation demos. |
| Componentization and AI-first UI surfaces | planned | Requirements captured in self-evolution prompt v2. | Need component manifest, variants/states/actions, token dependencies and patch-by-intent demo. |
| Live collaboration | planned | Human decision audit groundwork exists in 0.4.98. | Need presence, cursors/selections, patch streams, comments, conflict handling and rollback demo. |
| Research artifact baseline | planned | Research topics are listed in prompt v2. | Need source-grounded comparison of Penpot, Stitch, v0, Impeccable/AGUI-style protocols, Superpowers, Remotion/Figma capabilities and OBS/media composition lessons. |
| Export/demo baseline | planned | Daily Goal smoke produces Markdown/PDF as scheduler validation, not creative runtime proof. | Need one design/tokens/component workflow demo and one structured document/slide/whiteboard workflow demo. |

Promotion rule: a future `0.5` manifest must fail promotion if any required creative/runtime capability is only a goal, prompt item or partial infrastructure without working demo evidence.
