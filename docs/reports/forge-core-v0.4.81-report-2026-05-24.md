# Forge Core v0.4.81 Self-Evolution Report

Run id: `run_65dd78cebf3748f4b8b02d75f2079bb3`  
Workflow id: `wf_710fe1d41a324dd4b22af04a65f53711`  
Prompt packet: `forge.hackathon_mvp_factory.v1`

## Increment

Forge Core now detects hackathon, ideathon and maratona requests that ask for an
MVP or software factory and expands them into a regulation-first execution graph.

The new graph adds:

- regulation parsing;
- buffered deadline calculation;
- viability decision against the regulation;
- weighted brainstorm using the judging rubric;
- final idea and MVP scope selection;
- PDF artifact generation;
- Telegram delivery node;
- MVP software factory backlog;
- OSM/OSRM technical planning;
- MVP and pitch validation;
- recurring improvement until the buffered deadline.

## Why It Matters

ForgeFlow's legacy hackathon flow was useful for guided human decision making.
This increment moves that operating pattern into Forge as a durable workflow
runtime primitive: the user can submit a regulation plus a build idea, receive a
run id, and let the graph control viability, artifact generation, implementation
planning, tests and improvement loops.

The important change is that Forge does not blindly build the user's first idea.
It first checks the idea against the event rules and judging weights. If the idea
is weakly aligned, the workflow must reframe it or produce a stronger
alternative before building the MVP backlog.

## Hackathon Factory Shape

The accepted workflow for the Ideathon Energia para Todos run has 19 tasks. The
hackathon-specific tasks start at `task-009`:

- `task-009`: parse the hackathon regulation;
- `task-010`: calculate a customizable buffered deadline;
- `task-011`: evaluate the user's idea against the regulation;
- `task-012`: brainstorm and score MVP concepts;
- `task-013`: select the final idea and MVP scope;
- `task-014`: generate the final idea PDF and explanation artifact;
- `task-015`: send the PDF to Telegram;
- `task-016`: build the MVP software factory backlog;
- `task-017`: prepare the OSM/OSRM MVP technical plan;
- `task-018`: validate the MVP, pitch and judging package;
- `task-019`: run continuous improvement until the buffered deadline.

## Validation

Required validation passed for this cycle:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- `cargo build --release`
- `cargo install --path . --force`
- release CLI smoke: `forge --version` returned `forge 0.4.81`
- release CLI smoke: `forge plan` produced the hackathon factory graph with PDF,
  Telegram, OSM/OSRM and improvement-loop tasks
- release CLI smoke: `forge request start --origin codex --output json`
  returned async run `run_65dd78cebf3748f4b8b02d75f2079bb3`

## Safety

This release changes Forge-owned planning metadata, graph construction,
validation rules and documentation. It does not mutate external hackathon
systems, submit forms, create production cloud resources or expose Telegram
credentials.

The Telegram node points at `configured_telegram_chat`; the actual delivery is
performed by the operator environment using the existing Kubernetes secret, with
bot token and raw chat id kept out of logs.

## First Artifact Decision

The first Ideathon artifact chooses to keep the user's GreenRoute AI concept,
but with a stricter regulation fit:

`GreenRoute Energia Compartilhada: logística colaborativa para reduzir consumo
energético e emissões em pequenas operações.`

The idea is marked `viable_with_reframe`, because a generic logistics
marketplace would be too distant from the challenge, while a route and vehicle
capacity optimization MVP framed around energy consumption reduction, carbon
footprint reduction, inclusion digital for small businesses and sustainable
habits fits the regulation.

## Next Recommended Cycle

Add an execution adapter that can advance the first deterministic hackathon
tasks automatically and attach task-level evidence without needing the operator
to manually produce the first PDF artifact.
