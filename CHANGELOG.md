# Changelog

## 0.4.3 - 2026-05-23

### Added

- Added source-of-truth async request status projection:
  - `forge request status` now loads the current workflow behind the run id;
  - status output includes the current workflow goal, original requested goal, workflow status, latest revision, artifact count and task status summary.

### Changed

- `forge request status` no longer behaves as a stale run-record lookup for Codex/OpenCode skill callers. The run id now resolves to the current workflow state after runtime mutations such as `workflow update-goal` and `workflow attach-artifact`.

### Safety

- The original request goal is preserved as `requested_goal`, while `goal` reflects the current Forge workflow goal. This keeps Forge as the source of truth without losing the initial request intent.

## 0.4.2 - 2026-05-23

### Added

- Added persisted task leases:
  - `forge task acquire`;
  - `forge task release`;
  - SQLite-backed `task_leases` records keyed by workflow task;
  - JSON lease conflict reports when a second executor attempts to acquire an unexpired task lease.
- Added explicit self-evolution cycle report fields and non-dry execution for local Forge install updates and GitHub publication contract commands after validation, using `gh auth token` as the local credential gate.

### Safety

- Lease acquisition is guarded by Forge-owned workflow state and records acquisition, conflict and release events.
- Expired task leases may be replaced, but active leases block concurrent executor ownership until released or expired.
- Self-evolution prompts now declare post-validation local install and GitHub publication obligations instead of leaving them implicit.
- Public project publishing uses `gh auth status`, `gh repo view --json url,visibility` and a timed `git push`; non-public repositories are not pushed by that path.

## 0.4.1 - 2026-05-23

### Added

- Added versioned self-evolution prompt packets:
  - prompt packet version: `forge.self_evolution.prompt.v1`;
  - required validation commands embedded in each executor prompt;
  - SHA-256 prompt checksum persisted in each cycle report.

### Changed

- `forge self run --dry-run` now emits replayable executor prompt metadata so Codex/OpenCode runs can be audited against the exact prompt packet they received.

## 0.4.0 - 2026-05-23

### Added

- Added async request handoff:
  - `forge request start`;
  - `forge request status`.
- Added run records with stable `run_id` identifiers.
- Added `forge self run` for bounded Forge self-evolution cycles.
- Added self-evolution prompt/report artifacts per cycle.
- Added stop-date validation for autonomous work windows.

### Changed

- Codex/OpenCode skill flow now prefers returning a `run_id` instead of waiting for long work inline.
- Forge self-evolution can alternate authorized Codex/OpenCode executors while preserving validation gates.
- Fixed Codex self-evolution invocation to pass approval policy as a top-level Codex CLI option.

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
