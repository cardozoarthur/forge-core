# Changelog

## 0.3.0 - 2026-05-23

### Added

- Added runtime substrate sync for Docker, Kubernetes and Knative.
- Added `forge sync runtimes`, `forge sync all` and `forge runtimes`.
- Added Knative install suggestion when Docker and Kubernetes are available but Knative is missing.
- Added runtime ownership guard through `forge runtime guard`.
- Added async policy metadata on tasks that target Docker/Kubernetes/Knative-style execution.
- Added runtime workflow mutation commands:
  - `forge workflow update-goal`;
  - `forge workflow attach-artifact`.
- Added workflow revision history with mutation origin tracing for Codex, OpenCode, Forge CLI and skills.

### Safety

- Forge may mutate resources it created.
- Pre-existing Docker/Kubernetes/Knative resources require explicit human authorization before update/delete/patch/apply.
- Attached runtime artifacts are copied into Forge workflow storage instead of depending on external loose files.

## 0.2.0 - 2026-05-23

### Added

- Added executor sync with persisted local policy for Codex, OpenCode, Gemini, Claude and Ollama.
- Added explicit human authorization before Forge may use an installed/configured CLI as an execution engine.
- Added `forge sync executors` and `forge executors`.
- Added `opencode_codex_bridge` policy metadata when both OpenCode and Codex are authorized.
- Added goal-oriented task metadata: task goal, subtasks, definition of done, backlog state, impediments, acceptance criteria and owner role.
- Added goal readiness validation and `rework_tasks` output so unfinished goals return to work instead of being promoted.
- Added structural self-improvement domains: task structure, prompt system, process runtime, validation governance and executor policy.
- Added `--target-version` to `forge improve`.
- Added Markdown changelog generation for every improvement candidate.

### Changed

- `forge skill install` now runs executor sync as part of installation and includes the sync report in JSON output.
- Simulated execution now marks subtasks complete and task goals definitively ready.
- `forge improve` now creates both a JSON experiment artifact and a changelog artifact.

### Validation

- Test suite expanded from 9 to 15 CLI contract tests.
- New tests cover executor detection, saved human authorization, OpenCode/Codex bridge policy, goal-oriented task metadata, rework validation and changelog generation.
