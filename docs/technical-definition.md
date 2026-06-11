# Forge Core Technical Definition

Forge Core is a workflow runtime that transforms large objectives into validated, context-controlled atomic execution graphs.

Forge Core is less human-dependent than ForgeFlow. ForgeFlow focuses on product creation workflows with explicit human decision paths. Forge Core focuses on executing operational graphs that can run with AI, without AI or with both.

## Runtime Authority

Forge Core should not be treated only as a plugin or skill that adds capability to another agent. A plugin runs inside the agent's operational model. Forge is intended to own the operational model.

Forge owns:

- objective decomposition;
- explicit goal hierarchy;
- atomic task graph state;
- context minimization;
- task scheduling and cron/wait continuation;
- validation gates;
- retries and recovery policy;
- artifacts and operational memory;
- workflow cost accounting;
- promotion and self-improvement gates.

Codex, OpenCode, Gemini CLI, Claude Code, Ollama and other engines should be usable as execution targets. They receive bounded task packets and return structured results. Forge decides what context they receive, what they are allowed to do, how their output is validated and whether the workflow can advance.

Close coupling is still valuable when it reduces friction. The target architecture supports both directions:

- CLI calls Forge: interactive agents use Forge commands/skills to create, inspect and validate workflow state.
- Forge calls CLI: Forge launches an executor adapter for long-running or specialized tasks.
- Native CLI integration: open-source CLIs may embed Forge-backed orchestration paths while still leaving Forge as the source of truth for workflow state.

## Core Modules

- Intent parser: extracts goal, constraints, deliverables, risks and unknowns.
- Requirement extractor: normalizes the objective into measurable execution needs.
- Workflow fragmentation engine: produces atomic retryable tasks with explicit goals.
- Work item controller: tracks backlog state, subtasks, impediments, owner role, acceptance criteria and definition of done.
- Atomic task graph: keeps dependency-aware execution state.
- Context routing engine: compresses, summarizes, selects, versions and shards the minimum correct context for each executor under a budget.
- Addon registry: loads first-party and project-local Addon manifests, exposes capabilities, permissions, views, workflow extensions, artifact types, event types and integrations.
- Capability registry: resolves goals into required capabilities and missing capabilities before workflow planning. The Core consumes capability ids and workflow-extension contracts; domain-specific behavior must live behind Addons.
- Execution runtime: coordinates task execution and trace collection.
- Executor policy: detects installed/configured CLIs and persists human authorization before use.
- Runtime substrate policy: detects Docker/Kubernetes/Knative and persists human authorization before use.
- Scheduled execution: represents future continuation with cron/wait tasks and exposes deterministic bounded-worker assignment plans before due work is leased or executed.
- Non-AI execution: runs deterministic command-style steps without requiring a live model call.
- Notification execution: creates final notification payloads such as email cost reports.
- Validation engine: blocks invalid promotion.
- Artifact system: stores reusable outputs with stable paths and hashes.
- Operational memory: persists workflows, events and generated artifacts.
- Self-improvement loop: generates experimental changes without unrestricted promotion.

## v0 Boundary

The current version is a local Rust CLI, MCP tool surface and skill package. It includes SQLite persistence, simulated execution, AI/non-AI/wait/notification task kinds, executor sync/policy, runtime substrate sync/policy, goal-oriented work items, rework validation, runtime goal/artifact mutation, MCP tools for workflow list/inspect/start/resume/status/context/validation/artifact fetch, cluster registry/placement metadata, per-node cluster scheduling posture, explicit remote-AI placement blocking, cluster handoff manifests, cost report generation, controlled improvement artifacts with changelog generation, Addon catalog inspection, persistent Addon lifecycle registry, SQLite-materialized Addon capability indexing, trusted local Addon package marketplace indexing with package fetch/import into a local cache, local-process and controlled external-API Addon worker execution over HTTP/HTTPS with env-backed or credential-vault-backed HMAC/Bearer auth through the runtime dispatch ledger, external planning-strategy execution with Core reference equivalence audits, token-headroom reports, Forge-first CLI wrapper planning, non-destructive PATH shim installation, reversible stdout/stderr headroom receipts and global timeline events for guarded CLI execution, capability-first intent resolution, project operating context loading with brand identity/design system/operating policy, a persisted identity registry for organization/brand/product/user/channel records, cross-channel identity links/resolution, role-derived identity membership permissions, a physical tenant index for workflows/runs/artifacts/events, tenant-aware event stream projection, a global inbound event inbox with route support for `start_workflow`, `continue_workflow`, `modify_workflow`, `pause_workflow`, `resume_workflow` and validation-gated `complete_workflow`, a first bounded HTTP webhook ingress listener for real POST requests, a managed event service registry plus bounded worker and webhook-ingress service execution with persistent lease/heartbeat/health, a first generic `http`/`webhook` egress emitter for declared Addon event adapters with optional HMAC-SHA256 signing or Bearer token injection from env or credential-vault over `http://` or `https://`, plus a governed Telegram Bot API egress transport for message/document/report delivery through the built-in notification Addon. It does not yet include remote distributed execution, real provider adapters, SaaS UI, remote Addon registry mirrors/auto-update, WASM plugins, automatic first-party builder replacement by external planners, production transport-specific WhatsApp/email adapters or fully decoupled production transport workers.

## Multimodal Safety Boundary

Experimental multimodal support is disabled by default and currently exists as Forge-owned planning, guard and evidence surfaces only. The capability is now declared through the first-party `forge.addon.multimodal` Addon as capability `multimodal_runtime`, permission `multimodal.runtime_benchmark`, view `multimodal.benchmark_center` and runtime contract `multimodal_runtime_benchmark.executor`; the Core command path remains a guarded compatibility executor while production Addon workers mature. The same guarded path can be driven through `forge addons dispatch-contract` / MCP `forge.addons.dispatch_contract` and `forge addons run-dispatch` / MCP `forge.addons.run_dispatch`, so Addon dispatch, policy recheck and runtime-processing lineage wrap the deterministic benchmark receipt. `forge multimodal status`, `install-plan`, `benchmark-template`, `demo-plan`, `guard` and their MCP equivalents perform no installs, model execution, device access, input automation or network access. A project can opt into experimental planning by adding approved `.forge/multimodal.json` with `experimental_enabled` and `approved_by`; CLI `--project-root` and MCP `project_root` inputs let agents inspect that config without relying on process cwd. `forge multimodal benchmark-result --approved-by <operator> --confirm-fixture-only --output json` and MCP `forge.multimodal.benchmark_result` record `forge.multimodal.benchmark_result.v1` fixture-only evidence for approved, secret-free benchmark fixtures while explicitly reporting `installs_performed=false`, `model_execution_performed=false`, `device_access_performed=false` and `network_access_performed=false`. `forge multimodal runtime-benchmark --approved-by <operator> --confirm-runtime-execution --allow-model --output json` and MCP `forge.multimodal.runtime_benchmark` record `forge.multimodal.runtime_benchmark.v1` guarded deterministic local runtime evidence only after experimental opt-in and model guard approval; it performs the in-process fixture model path while explicitly reporting `installs_performed=false`, `device_access_performed=false`, `filesystem_access_performed=false` and `network_access_performed=false`. `forge multimodal demo-receipt --demo <id> --fixture <id> --approved-by <operator> --confirm-local-fixture --allow-model --output json` and MCP `forge.multimodal.demo_receipt` record `forge.multimodal.demo_receipt.v1` guarded local fixture evidence only after experimental opt-in. The receipt runs the deterministic fixture path, records model guard approval if supplied, and emits a guard matrix proving camera, microphone, screen, input and filesystem access remain blocked and unperformed unless each scope receives separate guard approval.

These fixture and local receipt results are not promotion evidence for real image/audio/video/3D model execution. Promotion requires guard-approved runtime/model execution and benchmark/demo artifacts that prove camera, microphone, screen, input, filesystem and network access remain blocked unless the operator opts in through Forge policy.

## Core + Addons Direction

Forge Core must remain minimal, universal and domain-agnostic. Core-owned concepts are goals, workflows, events, state, context, memory, identity, permissions, human collaboration, artifacts, observability, scheduling, runtime, tool execution, UI composition and registries.

Domain-specific behavior belongs in Addons. An Addon manifest can declare:

- capabilities;
- workflow extensions;
- artifact types;
- event types;
- views/widgets;
- integrations;
- permissions;
- dependencies;
- lifecycle metadata.

`forge addons catalog --output json` exposes the active catalog. `forge addons resolve --goal "<goal>" --addon-dir .forge/addons --registry-source <index.json|https://...> --output json` resolves the required capabilities for a goal, including project-local manifests. When registry sources are explicitly supplied, it runs bounded registry sync first and includes the resulting `forge.addon_registry_sync.v1` reports in `registry_syncs`. `forge plan` loads `.forge/addons` by default and records `forge.intent.v2`, which includes workflow mode, event policy, operating context, required capabilities, active Addons and a `forge.capability_resolution.v1` report, but it does not sync registries during planning.

`forge.capability_resolution.v1` now includes top-level workflow-extension activations with source Addon, Addon version, source capability, kind and reason. It also emits `capability_suggestions` when required capability dependencies are missing but a known Addon can provide them after enablement, permission authorization or installation from a catalog source. The store-aware resolver behind `forge addons resolve` and MCP `forge.addons.resolve` additionally checks trusted installable packages in the local marketplace and includes package id, source, repository, channel, package hash, CLI commands and MCP tools when a package can satisfy the missing capability. With explicit registry sources, it syncs those indexes before suggestion building and returns `registry_syncs` as evidence of what was consulted. The planner consumes an internal workflow-extension planner registry for first-party builders such as n8n primitive research, hackathon factory, daily Goal research and async runtime policy. Each registry entry binds extension id, capability id, phase, legacy textual guard and builder function. Resolved capability/extension activations take precedence; textual guards are only a compatibility fallback for old intents without capability-resolution evidence. For manifest-declared extensions not owned by a first-party builder, Forge adds a generic auditable DAG task with Addon/capability/extension lineage and `addon_extension_validation`. This preserves compatibility while reducing scattered Core heuristics; external signed builder registration remains the next boundary.

Addon manifests also expose `context_providers`, `memory_providers`, `event_adapters`, `runtime_contracts` and `views`. Context providers declare provider id, source, scopes and context sections; memory providers declare provider id, provider type, scopes and supported memory levels; event adapters declare adapter id, transport, direction, origins, allowed actions, event types, schema name, auth mode, required permission ids and, for generic `http`/`webhook` egresso, endpoint/allowed-host/timeout/response-size metadata; runtime contracts declare planning, replanning, validator, executor and handoff entrypoints by capability/workflow extension plus required permission ids; views declare UI/TUI/ops-console surfaces and required permission ids for dynamic interface composition. These contracts let domain Addons advertise context, memory, event, runtime and UI surfaces without embedding domain code in Core. Ingress routing, bounded HTTP webhook ingress and generic HTTP/webhook egress over `http://` or `https://` with HMAC or Bearer env-backed auth are executable boundaries now; richer transports and signed/authenticated channel adapters remain Addon/runtime work.

`forge addons validate --addon-dir .forge/addons --output json` emits `forge.addon_validation.v1`. It blocks invalid catalogs at the CLI level by returning non-zero when duplicate Addon ids, duplicate capability ids, missing required Addon dependencies, unsatisfied dependency `version_req` clauses, missing required capability dependencies or undeclared permission references in runtime contracts, event adapters or views are detected. High-risk permissions without human approval gates are warnings.

`forge addons package --manifest ./addon.yaml --repository <repo> --channel stable --package-path ./dist/addon.package.json --output json` emits `forge.addon_package.v1`. Forge validates the candidate catalog, records raw and canonical manifest SHA-256 hashes, byte size, package id, repository/channel/source metadata, install/upgrade/downgrade commands, capability/dependency/permission/runtime-contract/view summaries and detached Ed25519 signature metadata. `forge addons trust-key --repository <repo> --channel stable --public-key <ed25519-public-key-hex> --output json` persists trusted package signing keys in `forge.addon_trust_store.v1`. `forge addons publish-package --package ./dist/addon.package.json --output json` indexes a package in `forge.addon_marketplace.v1` with current trust-policy evidence and uses the package path as the installable source when the caller does not supply a real source. `forge addons fetch-package --source <path|file://|https://...> --expected-sha256 <sha256> --allow-remote --lock .forge/addon-package-lock.json --output json` emits `forge.addon_package_fetch.v1`: it copies the package into Forge's local cache, applies a byte limit and optional SHA-256 precheck, optionally enforces `forge.addon_package_lock.v1`, then indexes the cached package through the same trust-policy path; HTTP(S) sources require explicit `--allow-remote`. `forge addons sync-registry --source <index.json|file://|https://...> --allow-remote --lock .forge/addon-package-lock.json --output json` emits `forge.addon_registry_sync.v1`: it reads a JSON/YAML index with package sources, fetches each bounded package through `forge.addon_package_fetch.v1`, optionally enforces the same lock per package, records per-package issues and indexes trusted packages into the local marketplace. `forge addons package-lock --write .forge/addon-package-lock.json --output json` emits `forge.addon_package_lock.v1`, a reproducible lock snapshot with package ids, Addon versions, repository/channel, sources, manifest hashes, package hashes, capability ids and current install-policy status. `forge addons marketplace --output json` lists packages with live signature/trust status. `forge addons install-package --package ./dist/addon.package.json --lock .forge/addon-package-lock.json --output json` returns `forge.addon_package_install.v1` and installs only when the schema, package identity, embedded manifest identity, canonical manifest hash, detached Ed25519 signature, repository/channel trust key and optional lock entry verify. Lock matches emit `forge.addon_package_lock_enforcement.v1`; mismatches block fetch, registry sync and install. These contracts still do not execute package code, maintain remote mirrors or apply automatic updates. MCP exposes the same surfaces as `forge.addons.package`, `forge.addons.trust_key`, `forge.addons.trust_store`, `forge.addons.publish_package`, `forge.addons.fetch_package`, `forge.addons.sync_registry`, `forge.addons.package_lock`, `forge.addons.marketplace` and `forge.addons.install_package`.

Addon manifests can declare a `compatibility` block with `forge_version_req`, `api_versions`, `runtimes`, `features`, `platforms` and `migrations`. Validation rejects unsupported Forge versions, Addon API versions, features, runtime names, runtime contracts outside declared runtime compatibility and platform tags that do not match the current OS/architecture. Install, upgrade, downgrade and trusted package install reuse the same catalog validation. Major version changes additionally require a matching migration entry from the installed version to the candidate version with `strategy` and `rollback` evidence, so package/lifecycle flows cannot silently cross a breaking boundary without an auditable migration/rollback plan.

`forge addons migration-workflow --from-manifest ./addon-v1.yaml --to-manifest ./addon-v2.yaml --action upgrade --output json` returns `forge.addon_migration_workflow.v1` and persists a real Forge workflow. The workflow is persistent, records a creation event and contains explicit tasks for installed-state backup, applying the declared migration plan, validating the migrated Addon state, preparing the rollback path and packaging the audit evidence. Major lifecycle operations that cross a declared migration boundary attach this same report to `forge.addon_lifecycle.v1`. This is the workflow-first migration boundary: Forge now owns the auditable operational plan and rollback readiness, while executing arbitrary Addon data migration code remains reserved for future signed runtime workers.

`forge addons install --manifest ./addon.yaml --output json` persists a manifest into SQLite and returns `forge.addon_lifecycle.v1`. `forge addons installed --output json` returns `forge.installed_addons.v1`. `forge addons upgrade --manifest ./addon-v2.yaml --output json` and `forge addons downgrade --manifest ./addon-v1.yaml --output json` replace an installed Addon manifest only when the candidate version is respectively higher or lower than the stored version. The change validates the resulting catalog, preserves the current lifecycle state, preserves authorization gates and rematerializes `addon_capabilities` with the candidate manifest version. `forge addons enable|disable|uninstall <addon-id> --output json` mutates lifecycle state through Forge-owned storage. Disabled Addons are still auditable but do not satisfy planning capability matching or capability dependency checks. The combined catalog merges built-ins, project-local manifests and installed Addons, with installed records controlling lifecycle for the same Addon id.

`forge addons permissions --output json` exposes `forge.addon_permission_authorizations.v1`, a persistent approval ledger keyed by Addon id and permission id. `forge addons authorize-permission --addon <addon-id> --permission <permission-id> --approved-by <human> --output json` records an approval, and `forge addons revoke-permission ...` records revocation. Installed Addons whose manifests declare `requires_human_approval: true` cannot be installed or re-enabled until the permission is approved. Store-aware catalog loading also projects any enabled Addon with missing human approval as `unauthorized`, so direct file manifests and installed manifests share the same operational gate. If approval is later revoked, capability index sync marks that Addon lifecycle as `unauthorized`, so capability resolution no longer treats its capabilities as active.

`forge addons capabilities --output json` exposes `forge.addon_capability_index.v1`. This is a materialized SQLite index derived from installed Addon manifests, with lifecycle-aware rows for each installed capability. It supports Addon id, capability id and lifecycle filters in CLI and MCP so planners, ops views and future marketplace/search surfaces can query capabilities without reparsing every manifest.

`forge addons contracts --type planning_strategy --output json` exposes `forge.addon_runtime_contracts.v1`, derived from the active Addon catalog. It lists declarative `planning_strategy`, `replanning_strategy`, `validator`, `executor` and `handoff` contracts with capability id, workflow extension id, runtime, entrypoint, inputs, outputs and required permissions. Each contract includes `permission_gate` using `forge.addon_permission_gate.v1`, projecting whether the contract is allowed, unauthorized for missing human approval, blocked by undeclared permissions, or disabled with the declared tools, resources, integrations, actions and tenant scopes. Built-in first-party builders are now visible as runtime contracts, and external Addons can declare validators or executors in YAML/JSON without adding hard-coded Core branches. `forge addons planners --output json` exposes `forge.addon_planner_registry.v1`, a narrowed registry over `planning_strategy` and `replanning_strategy` contracts. It distinguishes Core-owned first-party builders from external runtime-contract planners and returns policy status, dispatchability, commands and MCP tools. `forge addons dispatch-planner --contract <contract-id> --goal "<goal>" --output json` narrows dispatch to `planning_strategy`/`replanning_strategy` contracts and persists a standardized `forge.addon_planner_dispatch_input.v1` payload with goal, constraints, optional workflow/task ids, planner metadata and context. `forge addons execute-planner --addon <addon-id> --contract <contract-id> --worker <worker-id> --goal "<goal>" --output json` executes a registered planner worker through the same local-process or controlled external-API boundary, injects a Core reference workflow into `context.core_reference`, validates the returned task graph and emits a replacement-readiness equivalence audit. Valid but non-equivalent planner results stay review-required; Forge does not silently replace first-party builders.

`forge addons contract-policy --contract <contract-id> --output json` exposes `forge.addon_runtime_contract_policy.v1`. It evaluates matching runtime contracts before dispatch and reports `dispatch_allowed`, `status`, issues, runtime, entrypoint and the same permission gate. The current policy is read-only and blocks contracts that lack an enabled permission gate, runtime or entrypoint; it is the pre-execution boundary for future signed/WASM/external API dispatchers.

`forge addons dispatch-contract --contract <contract-id> --input '<json>' --output json` exposes `forge.addon_runtime_contract_dispatch.v1`. It first evaluates `forge.addon_runtime_contract_policy.v1`, then persists a dispatch envelope in SQLite with Addon id, contract id, runtime, entrypoint, input, policy snapshot, source and status (`queued`, `blocked` or `dry_run`). `forge addons dispatch-planner --contract <contract-id> --goal "<goal>" --constraint "<constraint>" --context '<json>' --output json` is a planner-specific wrapper over the same ledger: it rejects non-planner contracts and stores `forge.addon_planner_dispatch_input.v1` so workers receive consistent planning/replanning data instead of ad hoc JSON. `forge addons run-dispatch --dispatch <dispatch-id> --output json` rechecks the current runtime policy before processing the queued row, so permission revocation, lifecycle changes or contract shape drift after enqueue block the dispatch instead of using stale authorization. Forge Core only processes allow-listed `forge_core_builtin` entrypoints such as `builtin:echo`/`forge_core.echo`; external runtimes such as `wasm` and `external_api` are marked `needs_external_worker` with processing evidence. `forge addons register-worker --worker <id> --runtime wasm --trust-level signed --output json` and `forge addons workers --runtime wasm --status available --output json` expose `forge.addon_runtime_workers.v1`, an auditable registry of external runtime workers with status, trust level and metadata. Workers can declare `signature_scheme: "ed25519"` and `public_key_hex` in metadata; local executable workers declare `execution_mode: "local_process"`, an absolute `command`, optional `allowed_entrypoints`/`allowed_contracts` and a bounded timeout; API workers declare `execution_mode: "external_api"`, an explicit `http://` or `https://` endpoint, optional `allowed_hosts`, the same entrypoint/contract allowlists, `auth: none|bearer|hmac`, `secret_env`/`hmac_secret_env` or `credential_vault`, timeout and response-size limits. `run-dispatch` includes eligible workers in the processing evidence. `forge addons execute-dispatch --dispatch <id> --worker <worker-id> --output json` runs registered `local_process` or controlled `external_api` workers, claims the dispatch, sends a typed JSON request, reads a JSON completion, then reuses the normal completion policy, signature verification and ledger update. `external_api` supports explicit HTTP endpoints through bounded TCP and HTTPS endpoints through controlled `curl`, local hosts by default and non-local hosts only when listed in worker metadata, plus Bearer/HMAC auth from env or credential-vault without reporting secret values. `forge addons claim-dispatch --dispatch <id> --worker <worker-id> --output json` lets an available runtime-compatible worker claim a `needs_external_worker` dispatch only after the current policy is rechecked again; the claim stores the worker identity/key snapshot. `forge addons complete-dispatch --dispatch <id> --worker <worker-id> --result '<json>' --signature <ed25519-hex> --output json` records the claimed worker result with ownership validation, result SHA-256, attestation SHA-256 and signature verification status; signed/trusted workers must provide a valid Ed25519 signature over the canonical completion payload using the claim snapshot, so later worker key rotation cannot change the dispatch identity, and revoked policy blocks completion. This is a local-process plus controlled HTTP/HTTPS API worker boundary; WASM plugins and stronger trust/rotation policies are still future work. The same surfaces exist as MCP `forge.addons.dispatch_contract`, `forge.addons.dispatch_planner`, `forge.addons.dispatches`, `forge.addons.run_dispatch`, `forge.addons.execute_dispatch`, `forge.addons.claim_dispatch`, `forge.addons.complete_dispatch`, `forge.addons.register_worker` and `forge.addons.workers`.

`forge addons views --output json` exposes `forge.addon_views.v1`, derived from the active Addon catalog. It lists Addon-provided view ids, titles and target surfaces with Addon lifecycle metadata and `permission_gate` so the TUI, ops console and future dashboard composition engine can discover Addon UI contributions without hard-coded domain panels and avoid rendering actions that require unapproved scopes. View manifests support `type`, `component`, `route`, `layout`, `data_bindings`, `actions` and arbitrary `props`, allowing Addons to declare dashboards, widgets, visualizations, editors and specialized tools as generic UI contracts. Callers can still pass `--surface ops_console`, `--surface tui` or another surface when they need one lane.

`forge addons observability --output json` exposes `forge.addon_observability.v1`, derived from the active Addon catalog, persistent runtime dispatch ledger and global event timeline. It is the consolidated Addon operator view: lifecycle counts, enabled/disabled/unauthorized status, capability/dependency/permission/runtime-contract/view/artifact/event/integration totals, permission gates, declared event ingress/egress summaries, runtime consumed/emitted event counts with event types/transports, and dispatch status counts live behind one CLI/MCP contract. This keeps Addon observability domain-agnostic while giving the TUI, ops console and external brains enough context to inspect whether an Addon is merely installed, actually active, permission-blocked, emitting/consuming events or accumulating queued/failed runtime dispatches.

`forge harness token-headroom --content <payload> --kind log --budget-tokens <n> --persist --output json` and MCP `forge.harness.token_headroom` expose `forge.harness.token_headroom.v1`, a deterministic local-first compression report for logs, search-like results, JSON, code and text. It reports routing strategy, original/compressed hashes, byte counts, estimated token counts, saved tokens, savings percentage, budget status and a retrieval ref so an executor can pass smaller context without losing audit lineage. With `--persist` or `persist=true`, Forge stores the original and compressed payload in SQLite as a reversible local headroom blob; `forge harness retrieve-headroom --ref <retrieval-ref> --include-content --output json` and MCP `forge.harness.retrieve_headroom` expose `forge.harness.headroom_retrieval.v1` for metadata or content recovery by retrieval ref. `forge harness wrap-plan --executor codex|claude|gemini|opencode --cmd <arg> --project-root <project-root> --forge-first --workflow <workflow-id> --task <task-id> --run <run-id> --output json` and MCP `forge.harness.wrap_plan` expose `forge.harness.cli_wrapper_plan.v1`, a non-executing wrapper plan with normalized executor id, launch command and environment overlay for Forge-first operation. CLI `--project-root <project-root>` and MCP `project_root` let wrapper planning resolve another project's `.forge/harness.json` before shell execution while MCP calls without `project_root` keep the explicit MCP default. It preserves workflow/task/run lineage in `FORGE_WORKFLOW_ID`, `FORGE_TASK_ID`, `FORGE_RUN_ID` and the generated launch command while keeping executor-specific behavior such as Claude's `ENABLE_TOOL_SEARCH=true`. CLI operators can set `FORGE_HARNESS_DEFAULT_MODE=forge_first` or project `.forge/harness.json` with `{"default_mode":"forge_first"}` so harness `wrap-plan`, `install-shims` and `exec` default to Forge-first; the same project file can set `context_budget` or `default_context_budget`, `default_token_headroom` or `token_headroom`, and `require_token_headroom_for_forge_first` so wrapper plans and guarded executions inherit the project's context/headroom policy unless an explicit CLI/MCP override is supplied. Adding `"require_lineage_for_exec": true` to the same file blocks real child execution unless workflow, task and run lineage are supplied. `forge harness mode --project-root <project-root> --output json` and MCP `forge.harness.mode` with `project_root` expose `forge.harness.mode.v1` as a read-only audit of the effective default, source, project config path/status, exec policy status, `require_lineage_for_exec`, context budget source, token-headroom source, `require_token_headroom_for_forge_first`, safety checks and precedence before any shim install or CLI execution. `--observe-only` or MCP `observe_only=true` keeps one invocation in observation mode. Precedence is observe-only flag, explicit Forge-first flag, env default, project config, then observation mode. The wrapper, shim, mode and exec reports include `forge_first_source`, `context_budget_source` and `token_headroom_source`, and the child overlay includes `FORGE_HARNESS_MODE_SOURCE`, so event timelines and receipts can distinguish explicit flags, environment defaults, project config, observe-only overrides and MCP defaults. `forge harness install-shims --project-root <project-root>` and MCP `forge.harness.install_shims` expose `forge.harness.shim_install.v1`, writing PATH shims for Codex, Claude, Gemini or OpenCode that delegate to guarded Forge harness execution while refusing to overwrite existing non-Forge files unless forced. If no explicit `real_cmd` is supplied, Forge resolves the native CLI from `PATH` while excluding the target shim directory and records the resolution source/status in each shim report, preventing recursion through a stale Forge shim. `forge harness shim-status` and MCP `forge.harness.shim_status` expose `forge.harness.shim_status.v1`, a read-only audit that reports existence, Forge ownership, executable bit, PATH precedence, parsed real command/store/Forge binary and recursion risk before any shell relies on the shim. `forge sync executors --shim-dir <dir>` turns that audit into executor readiness by persisting `forge.executor_harness_status.v1`, `forge_first_ready` and Forge-first shell entrypoints; `forge brains`, `/brains` and `/shells` then show whether a brain shell will launch through the Forge harness or native CLI. `forge sessions --output json` and MCP `forge.sessions` expose `forge.brain_sessions.v1`, aggregating Forge-controlled providers, shell session specs, readiness, lifecycle state, recorded shell launch counts and recent `shell_launch_planned`/`brain_session_lifecycle` global events without starting child processes. `forge sessions lifecycle --session <id> --state opened|attached|closed --output json` and MCP `forge.session.lifecycle` record `forge.brain_session_lifecycle.v1` audit-only lifecycle receipts for known shell sessions, preserving workflow/task/run lineage and operator notes in the global event timeline. `forge shells --executor <executor> --workflow <workflow-id> --task <task-id> --run <run-id> --output json` and MCP `forge.shell.launch_plan` expose `forge.shell_launch_plan.v1`, a plan-only launch report for operators and agents with selected shell entrypoint, readiness, harness status, preflight commands, concrete context/handoff/heartbeat commands when workflow/task/run are supplied and Forge validation gates, without starting a child process. Each launch plan also exposes `prompt_packet_gate_policy` (`forge.shell.prompt_packet_gate_policy.v1`) with `organization_context_required`, `personality_decision_required` and `company_work_decision_required`, so a Forge-first brain shell has an explicit pre-launch prompt-packet checklist instead of relying only on a generic context warning. `forge shells --record-session` and MCP `forge.shell.record_plan` persist that intent as `forge.shell_session_receipt.v1` plus a `shell_launch_planned` global event, so operators can audit shell intent through the normal event timeline. `forge harness exec --project-root <project-root>` and MCP `forge.harness.exec` apply project policy first; with `require_lineage_for_exec`, missing workflow/task/run returns `harness_exec_blocked_by_project_policy` and `project_policy_status=lineage_required_missing` instead of launching the child process. Authorized execution applies the same headroom analysis to real guarded child stdout/stderr when token headroom is enabled, attaches `stdout_headroom`/`stderr_headroom` reports to `forge.harness.exec_receipt.v1`, and persists retrieval refs in SQLite so compressed output can be handed to a brain without losing the original stream. When workflow, task or run lineage is present, the receipt records `forge.harness.exec_event.v1` as a `forge_harness` global event with tenant context, receipt metadata, stdout/stderr headroom refs and a `global_event_id` that appears in `forge events timeline`.

`forge harness headroom-plan --executor codex|claude|gemini|opencode --project-root <project-root> --output json` and MCP `forge.harness.headroom_plan` expose `forge.harness.headroom_plan.v1`, a read-only plan that resolves the effective Forge-first mode, context budget, token-headroom source, project `require_token_headroom_for_forge_first` policy, wrapper env, `session_lifecycle_plan`, compression pipeline, reserve strategy, retrieval policy and next commands before a brain CLI receives large logs, tool output or child stdout. The nested `forge.harness.session_lifecycle_plan.v1` contract gives TUI, web and agent clients deterministic record-launch/opened/attached/closed commands for the selected brain shell without opening or attaching the shell. The report is intentionally separate from execution and shim installation so clients can inspect token/headroom and session readiness without mutating workflow state or launching external CLIs.

`forge harness adoption-plan --executor codex|claude|gemini|opencode --shim-dir <dir> --project-root <project-root> --output json` and MCP `forge.harness.adoption_plan` expose `forge.harness.adoption_plan.v1`, an ordered read-only adoption plan for making a project prefer Forge-first CLI infrastructure. It aggregates `mode`, `headroom-plan` and `doctor` evidence, recommends the project `.forge/harness.json` shape for Forge-first defaults, token headroom and lineage-required execution, and returns concrete command arrays for config preparation, shim installation, executor sync, wrapper planning and guarded `harness exec` with workflow/task/run lineage. The plan explicitly reports `mutates_state=false` and `executes_child=false`; operators or future UI flows must run the listed commands separately when they want to write config, install shims or launch a brain CLI.

`forge harness bootstrap --executor codex|claude|gemini|opencode --shim-dir <dir> --project-root <project-root> --output json` and MCP `forge.harness.bootstrap` expose `forge.harness.bootstrap.v1`, a governed bootstrap layer over the adoption plan. Dry-run is the default and reports `harness_bootstrap_planned` without writing files. With `--apply --approved-by <operator>`, Forge writes or updates `.forge/harness.json` with Forge-first defaults, token headroom and lineage-required execution, then installs the Forge-owned shim through the same non-destructive shim installer. `apply=true` without `approved_by` returns `harness_bootstrap_blocked_missing_approval`, so human/operator approval remains explicit before project policy or PATH shims change.

`forge identity context --project-root . --output json` exposes `forge.operating_context_load.v1`. A project can define `.forge/operating-context.yaml`, `.forge/operating-context.yml` or `.forge/operating-context.json` to bind workflows to organization, brand, product, user, channel, memory scope, personality scope, brand identity, design system, operating policy and tenant policy mode. If the file is absent, Forge emits explicit default context instead of omitting tenant fields. `tenant_policy_mode` defaults to `audit`; `enforce` makes `forge plan`, `forge request start` and inbound `start_workflow` routing run the operating-context preflight before creating workflow state.

`forge identity sync --project-root . --output json` exposes `forge.identity_sync.v1` and materializes the project operating context into SQLite `identity_registry` rows. It also creates an active `identity_memberships` row linking the operating-context user to the organization/brand/product scope as an `operator`. `forge identity registry --scope organization --output json` exposes `forge.identity_registry.v1` with optional scope/id filters, and `forge identity memberships --organization <id> --output json` exposes `forge.identity_memberships.v1`. `forge identity membership-update --subject <user> --organization <org> --brand <brand> --product <product> --grant workflow:mutate --deny patch:apply --expires-at <rfc3339> --output json` exposes `forge.identity_membership_update.v1`, updating role/status, grants, denies and validity windows without raw SQL or manual `data_json` edits. `forge identity link --left-scope telegram --left-id 123 --right-scope user --right-id arthur --output json` exposes `forge.identity_link.v1`, `forge identity unlink ...` marks the same edge as unlinked without deleting audit history, `forge identity links --status active --output json` exposes `forge.identity_links.v1`, and `forge identity resolve --scope telegram --id 123 --output json` exposes `forge.identity_resolve.v1` with the connected aliases and canonical subject. Tenant policy resolves active identity links before checking memberships, so a Telegram/Discord/Web/channel identity can authorize through the linked governed user while keeping separation reversible. Membership rows expose role-derived `permissions`, custom grants/denies, validity fields and `environments`; `operator` can create/execute/mutate/deliver workflows, while `viewer`/`auditor` are read-oriented. Explicit org/brand/product foreign keys on every workflow/run/artifact/event remain future work.

`forge identity tenant-index --output json` exposes `forge.tenant_index.v1`. It is a physical SQLite projection keyed by resource type and resource id for workflows, runs, artifacts and events, carrying organization, brand, product, user, channel, memory scope and personality scope. `save_workflow` updates workflow and artifact rows, `save_run` updates run rows and `record_event` updates event rows from the workflow operating context. This is still a projection/index rather than a full authorization boundary.

`forge identity tenant-audit --output json` exposes `forge.tenant_audit.v1`. It compares persisted workflows, workflow artifacts, workflow events and async runs against `tenant_index`; missing rows produce `tenant_index_missing_resources` and a non-zero CLI exit code. This gives the runtime an explicit pre-enforcement gate before multi-tenant mode starts blocking writes.

`forge identity tenant-policy --workflow <workflow-id> --mode audit|enforce --action "<action>" --output json` exposes `forge.tenant_policy.v1`. It evaluates four multi-tenant gates for a workflow: explicit organization/brand/product/user/channel context, active user membership for the workflow tenant scope, membership permission for the requested action and tenant-index coverage for workflow/run/artifact/event rows. The report includes `action`, `required_permission`, `membership_roles` and `granted_permissions`. In `enforce` mode the CLI exits non-zero when any gate denies the workflow. This is an optional policy gate that can be wired into workers, planners and response paths before the storage schema is migrated to mandatory tenant foreign keys.

When an operating context sets `tenant_policy_mode: enforce`, the current runtime blocks workflow creation unless the context is explicit and an active, currently valid membership grants `workflow:create`. Membership data can carry `permission_grants`, `permission_denies`, `expires_at`, `not_before` or `valid_from`; denies take precedence over grants and expired/not-yet-valid memberships are excluded from authorization. Existing workflows then use `ensure_workflow_policy` to block context packets, task handoff, task leases, async request drive/step/status/heartbeat/final-package/switch/cancel/resume/recover, final-audit generation, workflow goal/task/node/status/artifact/token/creative mutations, checkpoints, human interactions, schedule update/run-due, patch plan/diff/review/apply/revert/restore and ops modifier proposals when membership, action permission or tenant-index coverage is missing. Default/audit projects remain non-blocking for local development and legacy workflow stores.

Workflow roots now include `runtime` with schema `forge.workflow_runtime.v1`. It mirrors the intent workflow mode into first-class persisted fields: `lifecycle_kind` (`ephemeral_workflow` or `persistent_workflow`), expected lifetime, `persistent`, `ephemeral`, `can_become_persistent` and scale-to-zero policy. This keeps long-running workflow semantics auditable even when a caller does not inspect nested intent payloads.

`forge workflow update-goal --workflow <workflow-id> --goal "<new goal>" --origin <origin> --output json` is a live workflow mutation, not a display-only rename. It reparses `forge.intent.v2` with the active Addon catalog and the workflow's existing operating context, then persists the new intent, deliverables, capability resolution and revision event. The report includes added/removed deliverables plus previous/new capability ids, allowing operators and agents to see whether final delivery gates changed with the human goal update.

`forge list --output json` and the Ops snapshot project the same root contract into `forge.registry_workflow_runtime.v1`. The projection aggregates persistent/ephemeral counts, scale-to-zero policies and current runtime posture, and each workflow row exposes `operational_state`, `operator_action` and `reason` so a human operator or modifier AI can decide whether to monitor, repair, wake on event, keep an event listener ready, run due schedule work, sleep until the next schedule, archive/reuse or promote an ephemeral workflow to persistent runtime. Workflows with cron or one-shot `wait_until` schedule nodes are classified as schedule actions instead of event wakeups, preventing the event worker supervisor from being started for schedule-only rehydration.

`forge events list --workflow <workflow-id> --output json` exposes `forge.event_stream.v1`. This is a typed projection over the existing event table, preserving legacy event payloads while adding an event envelope with store sequence, category, severity, origin, correlation ids and the workflow operating context. Each envelope also carries `forge.event_observability.v1`, a normalized projection of node/task ref, Addon id, duration, retry, wait, context budget/usage/pressure and memory level/scope evidence when the raw payload already contains those values. It is an incremental bridge toward a universal event engine, not the final global event inbox.

`forge events timeline --organization <organization-id> --limit 50 --after-sequence <cursor> --output json` exposes `forge.event_timeline.v1`. It reads the append-only `global_events` projection when available, preserving workflow events and inbound events that exist before a workflow is created, and falls back to legacy workflow-event projection for older stores. The typed envelope supports workflow, organization, brand, product, latest-N, cursor filters and the same `forge.event_observability.v1` projection as event streams. The `page` block uses `forge.event_timeline.page.v1` and returns `after_sequence`, `limit`, `next_cursor` and `has_more`, giving the ops console and agents a tenant-aware observability stream without requiring each caller to inspect workflows one by one.

`forge events observability --workflow <workflow-id> --node <node-ref> --addon <addon-id> --output json` exposes `forge.event_observability_index.v1`. It reads the SQLite-materialized `event_observability_index` table, backfilled from `global_events` during migration and updated on every new global event, with a derived timeline fallback for legacy stores. It filters by workflow/organization/brand/product/node/Addons and returns tenant, workflow, node and Addon buckets with severity/category counts plus duration, retry, wait, context budget/usage/pressure and memory level/scope aggregates. This gives the Cost OS, dashboard renderers and improvement loop a queryable event-observability surface. Historical rollups now use the same materialized source through `forge events observability-history` and `forge.events.observability_history` without reparsing raw event payloads.

`forge events observability-history --project-root . --bucket day --group-by addon --output json` exposes `forge.event_observability_history.v1`. It derives hour/day buckets from the same materialized observability index, supports grouping by none, tenant, workflow, node or Addon, keeps the same duration/retry/wait/context/memory summary inside every bucket and includes structured `group` metadata for grouped historical views. In `tenant_policy_mode: enforce`, the CLI and MCP surface require `context:read`, apply the operating-context organization/brand/product filters when omitted and block explicit tenant filters outside that context. This gives operators and policy loops a tenant-safe historical rollup without adding a domain-specific analytics subsystem to Core.

`forge events improvement-policy --project-root . --workflow <workflow-id> --output json` exposes `forge.event_improvement_policy.v1`. It is the first automatic policy layer over event observability: it reads the normalized observability records, groups by node and Addon, applies operator-tunable thresholds for repeated events, total duration, retries, wait time and context pressure, then returns read-only recommendations such as `prefer_deterministic_node`, `tighten_context_routing`, `add_validation_or_rework_gate` and `supervise_wait_or_external_dependency`. In `tenant_policy_mode: enforce`, the CLI and MCP surface require `context:read`, apply the operating-context organization/brand/product filters when omitted and block explicit tenant filters outside that context before deriving recommendations. It does not mutate workflows automatically; promotion into `forge improve` or runtime mutation still requires a separate validated step.

`forge events ingest --origin <origin> --action start_workflow --input '{"goal":"..."}' --output json` stores `forge.event_ingest.v1` in the global `event_inbox` table before any workflow exists. `forge events inbox --output json` lists `forge.event_inbox.v1`. `forge events route --event <event-id> --project-root . --output json` emits `forge.event_route.v1` and supports `start_workflow` by creating a workflow with the same project Addons and operating context used by direct planning. It also supports `continue_workflow`, `modify_workflow`/`update_goal`, `pause_workflow`, `resume_workflow` and `complete_workflow`. `forge events scan --project-root . --limit 20 --output json` emits `forge.event_worker.v1`, a bounded single-pass inbox worker that routes pending events and marks failed events with worker error evidence. `forge events worker --project-root . --limit 20 --max-cycles 12 --interval-seconds 300 --idle-exit --stop-file .forge/run/event-worker.stop --output json` emits `forge.event_worker_loop.v1`, repeatedly running the same scan contract for a bounded number of cycles with optional sleep, idle early-exit and cooperative stop-file shutdown that reports `stop_requested` and `stopped_reason`. `forge events service-plan --kind worker|webhook_ingress --output json` emits `forge.event_service_plan.v1`, a plan-only managed service contract that captures the re-runnable command, settings, lease TTL, heartbeat interval, backoff policy, cooperative shutdown policy and health checks, and records an `event_service_plan` entry in `global_events`. `forge events service-run --kind worker --stop-file .forge/run/event-worker.stop --output json` emits `forge.event_service_run.v1`: it persists an `event_services` row, acquires a service lease, refreshes heartbeat and lease expiry after each worker cycle, records live health counters, executes the bounded worker loop and writes an `event_service_run` entry to `global_events`, including final service status `stopped` when the stop file is observed. `forge events service-run --kind webhook_ingress --stop-file .forge/run/webhook-ingress.stop --output json` emits the same schema for a bounded HTTP webhook listener, persists progress heartbeats while listening or waiting for requests, persists a `webhook_report`, saves final request/ingest/route health counters and writes the same global audit; it records final service status `stopped` when the stop file is observed between requests. `forge events service-supervise --kind worker --max-runs 12 --backoff-initial-seconds 5 --backoff-max-seconds 300 --stop-file .forge/run/event-supervisor.stop --output json` emits `forge.event_service_supervisor.v1`, a bounded supervisor over `event_service_run` that restarts managed service executions, applies executable backoff after failures, aggregates success/failure/stop health and writes `event_service_supervisor` to the global timeline while preserving `event_services` as the per-run registry. `forge events services-recover --project-root . --kind worker --output json` emits `forge.event_services_recovery.v1`, scanning `running` event service rows, marking expired leases as `stale`, preserving the latest data payload and adding a `forge.event_service_recovery_marker.v1` recovery marker before writing a global audit event. `forge events runtime-reconcile --project-root . --recover-stale-services --execute --scan-schedules --output json` emits `forge.event_runtime_reconcile.v1`, optionally applying the same stale-service recovery to worker leases before reading `forge.registry_workflow_runtime.v1`, pending inbox state and active event-service leases to recommend or optionally execute a bounded worker supervisor when persistent workflows require event wakeups or pending events need routing; with schedule scanning enabled it also includes `forge.schedule.worker_status.v1` and, when executing, `forge.schedule.scan_due.v1` for due cron or one-shot `wait_until` workflow rehydration. `forge events runtime-daemon --project-root . --recover-stale-services --execute --scan-schedules --continuous --cycle-retention 100 --stop-file .forge/run/event-runtime-daemon.stop --output json` emits `forge.event_runtime_daemon.v1`, running reconciliation cycles under a persisted `runtime_reconcile` event-service lease with heartbeat, stop-file shutdown, per-cycle reports, aggregate schedule execution counters and global audit. In continuous mode the daemon ignores `max_cycles`, exits by `idle_exit` or stop-file, stores retained/dropped cycle counters, keeps only the configured number of per-cycle reports in the final JSON while preserving aggregate health, and can run stale-worker recovery before each cycle's service recommendation. `forge events services --output json` and MCP `forge.events.services` inspect all event service kinds. MCP `forge.events.services_recover`, `forge.events.runtime_reconcile` and `forge.events.runtime_daemon` expose the same stale-service recovery contract through explicit `recover_stale_services` inputs where applicable. `forge events webhook-ingress --host 127.0.0.1 --port 8787 --path /webhook --origin partner_api --action start_workflow --schema partner.event.v1 --route --hmac-secret-env FORGE_WEBHOOK_SECRET --signature-header X-Forge-Signature --max-requests 100 --stop-file .forge/run/webhook-ingress.stop --output json` emits `forge.event_webhook_ingress.v1`, a bounded HTTP POST listener that normalizes JSON request bodies into the same inbox, adds `transport: webhook` and optional schema metadata, verifies optional HMAC-SHA256 signatures from an environment-backed secret, and can route immediately through declared ingress adapter policy. Before executing a route, Forge evaluates declared ingress Addon adapters and returns `adapter_policy` using `forge.event_adapter_policy.v1`; if an origin/transport matches a declared adapter, the route enforces declared actions, schema compatibility, `auth_verified` evidence and the adapter `permission_gate`. `continue_workflow` is a generic continuation dispatcher for `attach_artifact`, `checkpoint`, `answer_interaction`, `complete_task` and `drive_run`, including action inference from common payload fields. Completion is validation-gated by `validate_workflow`, so channel events cannot bypass task readiness and validation evidence.

Declared ingress routes also persist `inbound_event_routed` runtime metadata with `addon_id`, `adapter_id`, `direction`, `transport`, `event_type` and matched `adapter_policy`. This makes consumed Addon traffic visible in `forge events timeline`, `forge events observability` and `forge addons observability` without adding channel-specific Core code.

`forge events adapters --output json` exposes `forge.addon_event_adapters.v1`, derived from the active Addon catalog. It filters by Addon id, transport, direction and manifest directory, and gives agents a stable discovery surface for ingress/egress contracts such as the Core `forge.core.event_inbox` and Addon-provided webhook, Telegram, cron or API adapters. Each adapter includes `permission_gate` with the required permission ids and declared tool/resource/integration/action/tenant scopes. `forge events emit --adapter <adapter-id> --event-type <event-type> --action <action> --payload '<json>' --output json` exposes `forge.event_egress_emit.v1`: it selects a declared `egress` or `bidirectional` adapter, enforces direction, origin, action, event type, permission gate and endpoint host allowlist, and sends a typed `forge.event_egress_request.v1` to explicit `http://` or `https://` webhook/API endpoints or to the built-in `telegram` transport. `http://` uses Forge's bounded local HTTP client; `https://` uses controlled `curl` with JSON stdin, timeout, response hash reporting and `FORGE_EVENT_EGRESS_HTTPS_MODE=simulate` for deterministic non-network tests. Egress adapters declaring `auth: hmac` plus `secret_env`/`hmac_secret_env` or `credential_vault` sign the raw request body with HMAC-SHA256 and send `sha256=<hex>` in `signature_header` (default `X-Forge-Signature`). Egress adapters declaring `auth: bearer` use `secret_env` or `credential_vault` as a Bearer token in `Authorization` by default, or in `signature_header` when a custom header name is declared. Telegram adapters use `auth: bot_token`, read only the declared token env or credential-vault record, resolve chat from payload or `TELEGRAM_CHAT_ID`/`TELEGRAM_REPORT_CHAT_ID`, and send message/document/report payloads by the Bot API while reporting only `telegram://bot_api/sendMessage|sendDocument`, auth scheme, secret source, credential-vault metadata, env name, response size and hash. `FORGE_TELEGRAM_EGRESS_MODE=simulate` exercises the full non-dry-run path without calling the external API. Reports expose only auth scheme, secret source, env var name, credential-vault contract metadata and header, never the secret value. Each dry-run or delivery is also persisted as an `event_egress` record in `global_events`, returns `global_event_id` and appears in `forge events timeline` with the project operating context. Successful non-dry-run delivery with `payload.workflow_id` first runs workflow tenant enforcement for action `event egress delivery`, requiring `workflow:deliver` when `tenant_policy_mode: enforce`; only then does Forge write `forge.event_egress_delivery_evidence.v1` and attach it to that workflow as `telegram_delivery_record` or `event_egress_delivery`. This gives outcome evaluation a workflow-local artifact and prevents external delivery from crossing a tenant boundary before policy approval. Additional channel-specific adapters remain Addon/runtime work.

`forge memory configure --project-root <project-root> --memory-level <level> --default-scope project --default-scope processing --default-audience manager --privacy-mode private_by_default --retention-mode processing_auto_archive --approved-by <operator> --reason "<reason>" --output json` exposes `forge.memory_governance_config.v1` and writes `.forge/memory-governance.json` for that project. `forge memory policy --project-root <project-root> --output json` exposes the same project governance plus effective defaults inside `forge.memory_policy.v1`. This keeps project memory posture explicit and auditable instead of depending on hidden executor state.

`forge memory policy --output json` exposes `forge.memory_policy.v1`. It makes memory file-first and explicit: global, organization, project and processing roots; public/internal/private visibility; global/organization/project/manager/thread/non-shareable shareability; and memory levels `MEMORY_NONE`, `MEMORY_SESSION`, `MEMORY_SHORT_TERM`, `MEMORY_STANDARD`, `MEMORY_FULL` and `MEMORY_ADMIN`.

`forge memory search --workflow <workflow-id> --query "<query>" --memory-level <level> --scope global --scope organization --scope project --scope processing --organization <organization-id> --organization-root <path> --audience public|internal|manager|private --output json` exposes `forge.memory_search.v1`. It applies the memory level before reading files, then applies visibility/shareability gates and returns snippets with path and line ranges. Explicit CLI/MCP `memory_level`, `scope` and `audience` inputs win; if they are omitted and `--project-root` has `.forge/memory-governance.json`, Forge uses that project's default memory level, scopes and audience. The search governance object reports `project_governance_status`, `project_governance_config_path`, `memory_level_source`, `requested_scopes_source` and `audience_source`, so agents can distinguish explicit flags, project governance, workflow binding and defaults. `MEMORY_NONE` produces no roots, `MEMORY_SESSION` limits retrieval to processing memory, `MEMORY_SHORT_TERM` limits retrieval to project/processing memory, and standard/full/admin can inspect configured scopes subject to audience governance. When `--workflow` is provided, Forge derives the default organization and allowed memory scopes from the workflow operating context; in `tenant_policy_mode: enforce`, requested scopes outside the workflow `memory_scope` are rejected before file reads. Organization memory resolves to `organizations/<organization-id>/memory` under the Forge store by default, to the workflow organization when bound to a workflow, or to the explicit `--organization-root` when provided. The same contracts are available through MCP tools `forge.memory.configure`, `forge.memory.policy` and `forge.memory.search`.

`forge context --workflow <workflow-id> --task <task-id> --project-root <project-root> --output json`, MCP `forge.context.request` and executor handoff packets now include `forge.context.memory_policy.v1`. This is derived from the workflow operating context plus optional project `.forge/memory-governance.json`, not ad hoc executor state: it carries `memory_scope`, effective memory level, allowed scopes, source fields for memory level/scopes/audience, project-governance status/config path, tenant boundary, default audience, data classification, sharing policy, explicit-search requirement and a ready-to-use `forge memory search --workflow <workflow-id>` command. When project governance is configured, the default search command carries `--project-root` so memory search can resolve the current project policy. Inline broad memory remains disabled; brains must route historical lookup through Forge memory tools.

`forge cost ledger --project-root . --workflow <workflow-id> --output json` exposes `forge.cost_ledger.v1`. It aggregates planned task cost estimates and observed workflow-event executor costs/tokens by workflow, node, organization/brand/product tenant and detected Addon source. In `tenant_policy_mode: enforce`, the CLI and MCP surface require `context:read`, apply the operating-context organization/brand/product filters when omitted and block explicit tenant filters outside that context before returning costs. This gives operators and the improvement loop a tenant-safe direct Cost OS surface separate from candidate ranking, while preserving `forge improve candidates` as the decision-oriented recommendation layer. `forge cost materialize --project-root . --workflow <workflow-id> --output json` exposes `forge.cost_ledger_index.v1`, persisting a normalized SQLite index with `planned_task` and `observed_event` rows keyed by workflow, tenant, Addon, executor and model-call flags, so later dashboards and policies can inspect cost history without reparsing workflow JSON; it enforces the same `context:read` tenant boundary before writing and returning normalized rows. `forge cost incremental --project-root . --after-sequence <global-event-id> --output json` exposes `forge.cost_ledger_incremental.v1`, applying that boundary before scanning global events and materializing only affected workflows inside the project tenant. `forge cost history --project-root . --bucket day --group-by source_kind --output json` exposes `forge.cost_ledger_history.v1`, deriving hour/day buckets from that materialized index and grouping by none, tenant, workflow, source kind, Addon or executor while enforcing the same `context:read` tenant boundary when project policy is `enforce`. `forge cost maintain --project-root . --bucket day --group-by source_kind --retention-days 31 --output json` exposes `forge.cost_ledger_maintenance.v1`, applying that tenant boundary before materializing rows and before returning the scheduled maintenance rollup. `forge cost daemon --project-root . --bucket day --group-by workflow --max-cycles 2 --output json` exposes `forge.cost_ledger_daemon.v1`, applying the same boundary for each bounded cycle and recording the cycle event under the enforced tenant. `forge cost retention --project-root . --retention-days 31 --output json` exposes `forge.cost_ledger_retention.v1`, applying the same tenant boundary before planning or executing approval-gated physical deletion of stale index rows.

`forge interactive home --output json` and MCP `forge.interactive.home` expose the operator home surface as structured state, not only text. The dashboard now includes `workflow_focus`, `harness_mode_panel`, `navigation_panel`, `ui_composition_panel`, `patch_workbench_panel`, `permissions_panel`, `digital_twin_panel`, `dag_panel`, `task_board_panel`, `schedule_panel`, `event_panel`, `structured_logs_panel`, `cost_panel`, `context_memory_panel` and `addon_renderer_panel`, allowing a TUI, web console or external agent to render current workflow priorities, effective Forge-first harness mode/source/project-config status, keyboard navigation, compact/detailed/focus display modes, themes, UI composition regions, Git patch/diff review posture, permission/approval posture, operational digital twin state, workflow DAGs, task lanes, ready handoffs, checkpoint resume candidates, pending human waits, attached artifact counts, due waits/schedules, global event timeline, structured event logs, cost totals, memory/context posture and safe Addon UI renderer families from one call. The `harness_mode_panel` uses schema `forge.harness.mode.v1`, matching CLI `forge harness mode`, MCP `forge.harness.mode` and interactive slash command `/harness`; the `navigation_panel` uses schema `forge.interactive.navigation.v1` and carries default display mode, supported display modes, active theme, available themes and keyboard bindings for focus movement, command palette, theme cycling and display-mode cycling; the `ui_composition_panel` uses schema `forge.interactive.ui_composition.v1` and carries an operator-workspace layout with ordered regions, Core widgets, Addon widgets derived from safe enabled Addon renderers across `ops_console` and `tui` surfaces, Addon renderer families and refresh/inspection commands so clients compose dashboards without hard-coded domain panels; the `patch_workbench_panel` uses schema `forge.interactive.patch_workbench.v1` and carries repository path, clean/dirty counts, staged/unstaged/untracked files, bounded inline `diff_preview`, multi-file `diff_review_queue`, `edit_intake` required inputs/forms, ordered `operation_plan` lifecycle steps, diff stat/check status, `approval_flow` review/approval/rollback gates and direct permission-gated patch lifecycle commands for file editing and rich diff review clients; the `permissions_panel` uses schema `forge.interactive.permissions.v1` and carries tenant memberships, active/expired/not-yet-valid membership counts, Addon permission authorizations, pending/timed-out human approval items and direct membership/addon/interaction commands for granular permission-center UI; the `structured_logs_panel` uses schema `forge.interactive.structured_logs.v1` and carries recent event log entries with store sequence, workflow id, kind, category, severity, origin, source, timestamp, correlation, observability and truncated payload preview for TUI/web drill-down without parsing timeline strings; the `digital_twin_panel` reuses `forge.ops.operational_digital_twin.v1` so the TUI sees what is happening, done, remaining, validated, rejected and awaiting approval per workflow; the `dag_panel` uses schema `forge.interactive.workflow_dag.v1` and carries per-workflow nodes, dependency edges, ready root counts, blocked/human-wait counts and direct inspect/task-board/validate commands for a real-time DAG surface; the `task_board_panel` uses schema `forge.interactive.task_board.v1` and carries lane-level next-action commands plus operable `task_cards` with task id, title, status, dependencies, dependents, executor, context requirement count, validation rule count, estimated cost, cost model, workflow artifact count, task event history, latest history event, human-interaction state, handoff readiness, context action, checkpoint id/state and direct inspect/context/handoff/interaction commands. The patch workbench `edit_intake` uses schema `forge.interactive.patch_edit_intake.v1`; it is read-only and tells clients which workflow/task/intent/path/artifact/approval fields are missing before rendering `plan`, `review`, `diff`, `apply`, `revert` or `restore` actions as ready. The patch workbench `operation_plan` uses schema `forge.interactive.patch_operation_plan.v1`; it sequences lifecycle steps, dependencies, mutation flags and human-approval gates so TUI/web clients can render a coherent action lane before enabling apply/restore actions. `forge interactive readiness --output json` and MCP `forge.interactive.readiness` expose `forge.interactive.readiness.v1`, a dedicated shell/handoff preflight panel with executor, runtime, brain, shell, Forge-controlled surface, harness mode, harness doctor and next corrective command state without loading the full home dashboard. The patch workbench is exposed directly as `forge interactive patch-workbench --output json` and MCP `forge.interactive.patch_workbench`, the permission center is exposed directly as `forge interactive permissions --output json` and MCP `forge.interactive.permissions`, the same task-board contract is exposed directly as `forge interactive task-board --output json` and MCP `forge.interactive.task_board`, the same workflow-DAG contract is exposed directly as `forge interactive workflow-dag --output json` and MCP `forge.interactive.workflow_dag`, and the same structured-log contract is exposed directly as `forge interactive structured-logs --output json` and MCP `forge.interactive.structured_logs`, so agents and future TUI/web clients can render drill-down panels without requesting the full home dashboard. The human renderer prints matching sections so non-JSON terminal use remains useful.

`forge interactive harness --output json` and MCP `forge.interactive.harness` expose `forge.interactive.harness.v1`, a read-only harness center for one brain CLI. It aggregates the effective harness mode, doctor report, shim status, wrapper plan, `headroom_plan`, first-class `session_lifecycle_plan` and deterministic token-headroom preview, and it is also available as `dashboard.harness_panel` plus a Core widget in `dashboard.ui_composition_panel`. This gives TUI/web/agent clients one stable panel for Forge-first CLI readiness, wrapper/headroom controls and shell lifecycle audit commands without installing shims or launching child CLIs.

`forge interactive sessions --output json` and MCP `forge.interactive.sessions` expose `forge.interactive.sessions.v1`, a read-only session center over `forge.brain_sessions.v1`. It supports provider, lifecycle-state and readiness filters, projects session cards with `forge.brain_session_operation_plan.v1`, history/lifecycle/launch-plan commands, lineage completeness, context/handoff/heartbeat requirements and the recommended next session action, and is also available as `dashboard.sessions_panel` plus a Core widget in `dashboard.ui_composition_panel`. This gives TUI/web/agent clients one stable panel for provider/session lifecycle controls without opening, attaching or closing shells implicitly.

`forge ops snapshot --project-root <project-root> --addon-dir <dir> --output json` and MCP `forge.ops.snapshot` consume `forge.addon_views.v1` for `ops_console` views and `forge.addon_observability.v1` for lifecycle, permission gate, event flow and dispatch usage. They also emit `forge.ops.operational_digital_twin.v1`, a per-workflow operational twin with live-state lists for what is happening, already done, remaining, validated, rejected and awaiting approval, plus direct inspect/task-board/validate/events commands. They also emit `forge.ops.memory_context_governance.v1`, projecting the optional project `.forge/memory-governance.json` plus each workflow's organization/brand/product/user/channel context, memory scope, personality scope, effective memory level/source, allowed scopes, default audience and ready-to-run governed `forge context` / `forge memory search` commands for operators and modifier AIs. `forge.ops.addon_view_renderers.v1` remains a non-executing renderer projection over active Addon views. Each renderer classifies the view into safe families such as `dashboard_renderer`, `visualization_renderer`, `editor_renderer`, `data_list_renderer`, `timeline_renderer`, `canvas_renderer` or `document_renderer`, normalizes data sources, required capabilities, permissions, action risk, TUI affordance and HTML anchor, and attaches `forge.ops.addon_view_interaction_state.v1` with Forge-owned state keys, safe filters, chart hover policy, form fields, list sorting/pagination, timeline cursor policy, canvas tool palette or document outline mode according to the renderer family. Unsafe props disable the renderer instead of evaluating arbitrary component code, and `external_code_execution=false` remains part of the interaction contract. The local HTML console renders raw view contracts plus those safe renderer cards, interaction state, the operational digital twin table, projected `forge.ops.addon_view_runtime_state.v1` entries from workflow events, the memory/context governance table, a generic event form for each safe renderer, and a separate Addon observability table for enabled/unauthorized counts, queued/blocked/worker dispatch pressure and consumed/emitted events. `POST /api/addon-renderer/event`, CLI `forge ops renderer-event` and MCP `forge.ops.addon_renderer_event` record `forge.ops.addon_renderer_client_event.v1` after validating the requested event against `allowed_client_events` from the renderer state and persist it as `addon_renderer_client_event` in the workflow timeline; `addon_id` disambiguates repeated `view_id` values and becomes required when multiple Addons declare the same view id. The next snapshot folds those events back into runtime state per workflow with last event, actor, payload, hover, selection, filters, draft and refresh markers. External Addons can therefore advertise operational panels and remain visible as governed runtime components without becoming Core-specific code. `forge ops serve` accepts the same `--project-root` and `--addon-dir` options and defaults Addons to `.forge/addons`.

`forge.capability_resolution.v1` now includes `runtime_contracts` derived from Addon manifests whenever a required capability or activated workflow extension has matching `planning_strategy`, `replanning_strategy`, `validator`, `executor` or `handoff` contracts. It also includes `capability_suggestions`, which map missing dependencies to known disabled, unauthorized or inactive Addons and expose the suggested action, CLI command and MCP tool needed to make the capability available. On `forge addons resolve` and MCP `forge.addons.resolve`, the same report is enriched from the persistent marketplace with trusted installable packages that provide the missing capability, including package metadata and install/inspect commands; explicit registry sources are synced first and reported under `registry_syncs`. If the trusted package source is HTTP(S), the suggestion emits `fetch_package` with `--allow-remote` and the expected package SHA-256 before installation. Each activation preserves Addon/capability/extension lineage plus runtime, entrypoint, inputs, outputs, permissions, constraints and `permission_gate`. Generic manifest-driven workflow extension tasks add matching runtime contract references to their context requirements, while `forge.addon_planner_registry.v1` gives operators and agents the planner-registration view over those contracts before Forge runs signed external Addon code.

`forge memory promote --workflow <workflow-id> --from-scope processing --to-scope project|organization|global --source-path <path> --summary "<curated summary>" --approved-by <operator> --reason "<reason>" --output json` exposes `forge.memory_promotion.v1`. Promotion is classification-first: it requires an existing source file, curated summary, explicit approver, reason, target scope and compatible shareability. Forge writes a new Markdown memory file with promotion metadata, source path/line lineage, approval timestamp and hashes; it does not copy raw private processing memory by default. When bound to a workflow in `tenant_policy_mode: enforce`, promotion rejects source or target scopes outside the workflow `memory_scope` and derives the organization from the workflow unless an explicit matching organization is supplied. Successful promotions are indexed in SQLite as `memory_promotions`, carrying workflow/organization/brand/product/user/channel columns, and exposed through `forge memory promotions --workflow <workflow-id> --output json` / MCP `forge.memory.promotions` using schema `forge.memory_promotion_index.v1`. When `workflow_id` is supplied, the index call enforces tenant policy and applies physical workflow/tenant filters before returning rows.

`forge memory retention --workflow <workflow-id> --scope processing --scope project --output json` exposes `forge.memory_retention.v1`. It scans configured roots, classifies each Markdown memory file as `keep`, `classify_then_promote_or_delete` or `delete_after_final_packaging`, and records that no destructive action was performed. When `--workflow` is supplied, retention derives default scopes from the workflow and rejects explicit scopes outside the workflow `memory_scope` in enforcement mode. This gives operators and future cleanup workflows an auditable plan without deleting processing memory implicitly.

`forge memory cleanup --workflow <workflow-id> --scope processing --mode archive|delete --approved-by <operator> --reason "<reason>" --confirm --output json` exposes `forge.memory_cleanup.v1`. Cleanup is separate from retention: `--dry-run` plans archive/delete actions without approval, while non-dry-run execution requires approval, reason and confirm. It only acts on processing-memory files that retention classified as `delete_after_final_packaging`; promotable/private suggestions remain skipped until explicitly classified or promoted.

The current built-in Addons are compatibility Addons that keep existing planning behavior while the external Addon lifecycle matures. They are a transition path, not an excuse for domain logic to remain hidden in the Core.

`forge.addon.software_development` declares the software-specific `source_code_patch_lifecycle` capability. It owns the `software.patch_workbench` TUI surface, the `source_code.patch` permission and the `source_code_patch_lifecycle.executor` runtime contract. `forge.interactive.patch_workbench.v1` includes `forge.interactive.patch_addon_contract.v1` so clients can read that Addon/capability/permission/runtime lineage directly instead of inferring it from command names. `AddonViewAction` now carries optional operator metadata (`palette_group`, `source_panel`, `description`, `risk_level`, `mutates_workflow`, `command_template` and `keywords`) so `forge.interactive.command_palette.v1` can generate CLI actions from any enabled TUI Addon view; patch actions use that generic path, and command palette plus autocomplete preserve `forge.interactive.addon_action_contract.v1` with Addon id/name/version/lifecycle, capability, permission, permission-gate status, view id and action id through `addon_contract`, `addon_view_id` and `addon_view_action_id`. Addon action entries also carry `enabled`, `blocked_reason` and `operation_plan` with schema `forge.interactive.command_palette_action_plan.v1`; ready entries point to executable command arrays, while blocked entries remain diagnostic-only, expose empty command templates and point clients to Addon view inspection commands so invalid permission contracts do not look executable. `forge.interactive.action_registry.v1`, exposed through `forge interactive action-registry`, MCP `forge.interactive.action_registry`, interactive `/actions [query]` and `dashboard.action_registry_panel`, derives a strict-filtered read-only registry from those same action contracts with per-group readiness counts and operation plans for TUI/web/agent clients that should not depend on command-palette layout. `forge.interactive.action_invocation.v1`, exposed through `forge interactive action-invocation --action <id>`, MCP `forge.interactive.action_invocation` and interactive `/action <id>`, resolves one selected action into a ready command or diagnostic-only plan with `not_executed=true`, so action selection can be tested without hiding execution inside the Core UI layer. `forge.interactive.autocomplete.v1` now treats `/action <partial>` as action-id completion, using `action_registry` as the source, inserting `/action <action-id>` and keeping Addon/action lineage on the suggestion before any command is executed. The current executor delegates to `forge.patch.lifecycle` as a `forge_core_builtin` compatibility adapter, but the architectural boundary is explicit: source-code patch planning, review, apply, revert and restore are Addon capabilities, while Core only hosts the universal workflow, event, artifact, permission and runtime contracts.

## Executor Contract Direction

Agent integrations can discover the local MCP surface with:

```bash
forge mcp tools --output json
```

The current MCP manifest uses `forge.mcp.tools.v1` and exposes stable tools for
workflow listing, graph inspection, async run start/resume/status, revisioned
goal/artifact mutation, scheduled workflow create/update/list/run-due/scan-due,
worker-status assignment plans, aggregate schedule and loop summaries, bounded context requests, validation
status, milestone status/manifest inspection and bounded artifact fetch. `forge mcp call forge.run.start ...` returns
the same persisted run/workflow ids as `forge request start` plus
`forge.agent_handoff_contract.v1`, so Codex/OpenCode can return `run_id`
quickly and poll later without duplicating workflow state outside Forge.
`forge.schedule.scan_due` returns `forge.worker_pool.v1` evidence when bounded
parallel dispatch is used, and it reconciles idle scheduled workflows into
Forge-owned scale-to-zero state instead of leaving that behavior to external
loops or tmux wrappers.

Executor integrations should converge on a bounded packet:

```json
{
  "workflow_id": "wf_...",
  "task_id": "task-...",
  "executor": "codex|opencode|gemini|claude|ollama|command",
  "objective": "Implement JWT middleware",
  "allowed_context": [],
  "artifact_refs": [],
  "validation_rules": [],
  "expected_output": "",
  "cost_budget": {
    "max_usd": 0.0,
    "max_tokens": 0
  }
}
```

The executor response should be structured enough for validation, cost reporting and replay:

```json
{
  "task_id": "task-...",
  "status": "completed|failed|needs_retry",
  "artifacts": [],
  "trace_ref": "",
  "cost": {
    "estimated_usd": 0.0,
    "tokens_in": 0,
    "tokens_out": 0
  },
  "validation_evidence": []
}
```

Forge validates adapter outputs through `forge task validate-response`. The current
contract uses `forge.executor_response.v1` for executor output and
`forge.executor_response_validation.v1` for Forge's acceptance report. A completed
response must match the target task, include a replayable trace reference, report
finite non-negative cost/token values and carry at least one passing validation
evidence item. Rejected responses are audit events, not task promotion events.

## Goal-Oriented Work Contract

Every task and subtask must have a goal. A task is not promotable just because an executor returned output. Forge must evaluate whether the task is definitively ready.

The task work item includes:

- `goal`;
- `backlog_state`;
- `subtasks`;
- `impediments`;
- `acceptance_criteria`;
- `goal_validation.evidence_required`;
- `goal_validation.definitively_ready`;
- `goal_validation.rework_policy`.

If goal evidence is missing, validation reports `goal_readiness` failures and returns rework tasks. The workflow must go back to work instead of advancing as if it were complete.

## Executor Sync Contract

On install and on every sync, Forge should inspect known execution CLIs:

- Codex;
- OpenCode;
- Gemini;
- Claude;
- Ollama.

Forge records whether each CLI is installed and configured. Installed/configured does not mean usable. A CLI becomes usable only after a human explicitly allows it. The local policy is persisted in SQLite.

When Codex and OpenCode are both authorized, Forge records the `opencode_codex_bridge` integration so OpenCode and Codex can be coordinated as bounded execution engines.

## Cluster Registry Contract

Forge must know available cluster nodes before it schedules distributed work.
The first contract is a local registry, populated by explicit operator input, not
by automatic network scans or infrastructure mutation.

Each Forge node record includes:

- CPU cores and memory;
- operating system and architecture;
- GPU inventory and GPU availability;
- installed software;
- Python, Node.js and Docker availability;
- network reachability and endpoint metadata;
- lifecycle status;
- cost, latency and reliability estimates;
- trust level and sandbox permissions.

`forge cluster register` persists a reported node profile in SQLite.
`forge cluster list` returns `forge.cluster_registry.v2` with capability
summaries and scheduling posture. Each registered node gets a
`forge.cluster_node_scheduling.v1` row with schedulable/busy/idle/blocked state,
active and expired lease counts, local registry blockers and explicit
`remote_execution_enabled=false` / `external_mutation_allowed=false` markers.
`forge cluster place --workflow <id> --task <task-id>` derives deterministic
placement requirements from the task's executor and execution policy, then
returns a dry-run placement decision with candidate reasons. Candidates include
active node lease counts and the score penalizes busy eligible nodes, so a
compatible idle node is preferred before distributed handoff. A local Python code
node, for example, requires a registered online and reachable node with the
`python` capability, a trusted LAN/local trust class and the declared sandbox
permission.
Every placement report also includes `forge.cluster_placement_policy.v1`, a
read-only policy receipt with authorized scope `placement_metadata_only`,
`remote_execution_enabled=false`, `external_mutation_allowed=false`, the required
trust class, an explicit authorization requirement and deterministic hashes for
both the placement requirements and the policy receipt. This makes the dry-run
placement boundary auditable before Forge creates node leases or a later adapter
asks permission to execute remotely.
`forge cluster handoff --workflow <id> --task <task-id>` turns an eligible
placement into a bounded handoff packet. It acquires the normal Forge task lease
using the selected node id as the executor, returns a node-scoped lease ref and
emits `forge.cluster_sync_manifest.v1` with context, checkpoint, artifact and
context-shard hashes. The manifest also includes a deterministic
`manifest_sha256` over its sync fields, excluding the hash field itself. The
manifest is hash-only and declares remote execution and external mutation
disabled, so it is an auditable staging contract rather than a remote runner.
`forge cluster leases --output json` lists the node-scoped leases created by
cluster handoff. Each row includes the workflow/task identity, lease scope,
active/expired state, selected node metadata, trust level, sandbox permissions and
explicit `remote_execution_enabled=false` / `external_mutation_allowed=false`
markers. Operators can filter by `--node-id` to inspect a single LAN/SSH node
without touching the remote machine.

This stage intentionally does not open SSH sessions, run remote commands, mutate
Docker/Kubernetes/Knative resources or execute remote AI. It gives Forge an
auditable scheduling and handoff precondition so later remote adapters can build
on explicit capabilities, node leases, content hashes and trust policy.

## Runtime Substrate Contract

Forge separates cognitive executors from run substrates.

Cognitive executors:

- Codex;
- OpenCode;
- Gemini;
- Claude;
- Ollama.

Run substrates:

- Docker;
- Kubernetes;
- Knative.

Run substrates can execute asynchronous workflow nodes. They still require human authorization before use.

If Docker and Kubernetes are available but Knative is missing, Forge may suggest Knative installation. It must not install Knative or mutate a cluster without explicit user authorization.

## Resource Ownership Contract

Forge must not mutate resources outside its ownership scope.

Allowed without extra approval:

- create Forge-owned resources;
- update Forge-owned resources;
- delete Forge-owned resources.

Blocked without explicit approval:

- update pre-existing Docker/Kubernetes/Knative resources;
- delete pre-existing Docker/Kubernetes/Knative resources;
- patch resources that belong to another app, namespace or context.

Forge-owned resources should be labeled or recorded with ownership metadata. Until real substrate adapters exist, `forge runtime guard` provides the policy decision as a testable contract.

## Runtime Mutation Contract

Workflows are not frozen snapshots. Goals and artifacts can change while execution is active.

Mutation rules:

- every goal change records origin and revision;
- every artifact attachment copies the artifact into Forge workflow storage;
- origins can be `forge_cli`, `codex`, `opencode`, `skill` or another future adapter;
- mutation must not bypass validation;
- downstream tasks must see updated goals/artifacts through Forge context packages.

Codex CLI and OpenCode CLI are therefore human interfaces for Forge as well as possible executor adapters. They can update Forge state through CLI commands while Forge remains the persistent source of truth.

## Context Routing Engine

The context routing engine is a primary Forge differentiator. Forge should not pass broad project history to every executor. It should build minimal, correct context packets.

Responsibilities:

- compress large context into task-relevant summaries;
- select only the files, artifacts, decisions and constraints required by the current task;
- version context packets so executor results can be reproduced;
- shard context by task, subflow, artifact and validation gate;
- avoid redundant reasoning by reusing validated summaries and prior artifacts;
- reduce model cost and hallucination risk by excluding irrelevant history.

The goal is not simply smaller prompts. The goal is maximum relevance with traceable context lineage.

Current `forge context` packets use schema `forge.context.v30` and routing policy
`task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30`. Each packet
includes the executor-facing content, the full context checksum, workflow revision,
artifact count, project `operating_context`, node-scoped persona routing metadata plus a versioned persona profile
and persona contract for human-facing tasks with brand voice/tone/values, design token source and operating policy, executor
profile metadata, a versioned routing contract, execution policy metadata, dependency
readiness summaries, proposed child-subflow bindings, requested and effective budgets,
lineage hashes including `operating_context_sha256`, included and omitted sections,
profile-driven omissions, and a deterministic shard manifest with
source, priority, compression state, profile exclusion state, required/missing-required
state, remaining-budget before/after values, selected byte count, minimum-routable
byte count, selected-cost basis points, selection savings, summary and shard checksum.
Packets also include `context_ready`,
`required_sections`, `missing_required_sections`, `handoff_ready`, `handoff_status`,
`handoff_blockers`, aggregate `routing_summary` metrics and a versioned
`routing_contract`, `routing_repair` budget recommendation, `budget_plan` minimum-correct budget contract and
`minimum_correct_set` section receipt plus a `routing_quality` contract. Packets also carry a versioned `next_action` decision so executor adapters
can distinguish fresh handoff, dependency waits, context-budget repair, stale
checkpoint refresh and partial retry with fresh context without first asking for a
separate inspection projection. Packets now include a versioned `prompt_packet`
contract that binds the context schema, routing policy, executor profile, persona
mode/profile, instruction sources, validation gates, a compact versioned
`organization_context` with organization/brand/product/user/channel, memory/personality
scope, tenant policy, brand voice/tone/values, terminology, design token/component
sources, design guidelines and operating policy, plus a compact versioned
`personality_decision` with the Forge-owned routing owner, organization/brand/product/user/channel,
selected persona mode/profile, selected voice/tone, brand voice/tone/values, style sources,
fallback mode and audit flag, plus a compact versioned `company_work_decision` with the
multidisciplinary operating checklist for product, technical, financial, administrative,
marketing, communication and delivery work. The prompt packet also carries
`organization_context_sha256`, `personality_decision_sha256`, `company_work_decision_sha256`,
`organization_context_required`, `personality_decision_required` and
`company_work_decision_required`, so adapters cannot silently ignore organizational context,
personality routing or company operating discipline while still using a bounded task-local packet. These inputs, plus context
checksum and lineage checksum, are folded into a stable adapter-facing hash. Packets also include a versioned
`replay_manifest` that records the replay command, selector version, budget,
context checksum and content-addressed shard refs; prompt packets bind its checksum
so async executors can pause and resume against the exact route. Packets also
include a versioned `continuation_plan` that converts checkpoint state, context
delta and current route into an adapter-facing action: start fresh, resume from a
reusable checkpoint, refresh context before resume, or run a partial retry with
fresh context. The routing contract names the selector version, executor profile version,
profile id, selection strategy, requested and effective budget, minimum budget,
allowed/required/optional section set and profile hash. The repair contract turns
missing required sections into a deterministic action and recommended budget so
operators can retry with the smallest known budget increase instead of guessing. The
budget plan exposes the required context floor, selected bytes, optional pressure,
and recommended budget. The minimum-correct set records every required section's
included/compressed/missing state, source and content hashes, byte counts, routing
decision and repair action, then binds that section-level floor into the routing
fingerprint. The `routing_economy` contract records baseline bytes, selected bytes,
compression savings, budget omissions, profile-filtered omissions, total avoided bytes,
reduction basis points and whether a deterministic no-AI profile avoided a model call.
Together these contracts let executor adapters choose the smallest correct handoff
budget before spending model or runtime work. The persona profile gives the selected
profile id, routing rationale, source-model summaries, brand identity, design token
source, operating policy and profile checksum. The persona contract binds the node's
mode, scope, voice, tone, brand voice/tone/values, design token source, operating
policy, instruction source, source models, validation gate, audit flag and profile checksum to the context lineage hash and persona-mode hash before executor
handoff. The quality contract scores each packet and emits explicit warnings for missing required
context, budget pressure, compressed summaries and profile-filtered optional context,
so adapters and operators can audit routing risk without reconstructing shard
decisions. Handoff policy can still block incomplete context or pending dependencies
before an executor starts work.

Executor profiles let Forge route different envelopes without changing workflow
authority. Deterministic `command` and `wait` nodes use a no-AI profile that shrinks
the context budget and prioritizes local objective, validation rules, declared context
requirements and dependencies before lower-priority narrative context. Notification
nodes use a smaller deterministic profile that still allows persona routing. AI and
mixed nodes keep the richer reasoning profile. Execution policy metadata records
whether the node is allowed to use AI, whether it is deterministic, whether a local
Python/Node.js code runtime was selected and which validation gate controls the node.
`forge context --strict` emits the same replayable JSON packet but exits non-zero when
`handoff_ready=false`, giving adapters a deterministic readiness gate for missing
required sections and dependency-not-ready holds without hiding routing evidence.
`forge inspect` and `forge request status` project that same handoff decision as
read-only summaries, so operators and async callers can see which task is ready,
blocked by missing context or blocked by dependencies without reconstructing the
context package manually. Those summaries also carry routing quality aggregates and
per-task quality contracts for context-budget and profile-pressure triage.
`forge list --context-actions` exposes the valid registry filter values for
handoff, resume and retry actions, so operators can discover context-action filters
without memorizing summary field names or reading source code.
Every workflow row also exposes versioned `context_action_refs` entries with task id,
title, executor, next action, handoff status, blocker refs, checkpoint refs and the
current routing cache key. This keeps registry filtering actionable without forcing
operators or adapters to open a full `forge inspect` view before deciding which task
needs handoff, dependency wait, context repair, checkpoint resume or partial retry.
`forge inspect <workflow-id> --task <task-id>` provides a focused terminal and JSON
inspection view for one task. It preserves the selected node's context-route,
persona, execution-policy, handoff and child-subflow projections while limiting the
node list, handoff summary and diagram to the focused task. The JSON report includes
both the focused node count and the full workflow task count so adapters can bound
operator context without losing DAG scale metadata.
When the workflow registry has attached a proposed compatible child subflow, the
context package carries the structured binding plus a compact `child_subflows` shard
from `subflow_registry`, which lets executors reuse Forge's planning decision without
rebuilding it from irrelevant history. Runtime goal, artifact and persona routing state
remain part of the context lineage, which gives long-running executors a deterministic
stale-context signal while leaving room for persisted summaries, artifact shards and
active child-subflow execution gates.

Inspection expands those proposed child-subflow bindings as read-only topology
metadata. `forge inspect --output json` records the parent workflow/task, depth,
path, reachability, terminal flag and loaded child workflow/task counts for each
linked subflow, and the terminal diagram prints the same path. This makes recursive
reuse auditable before Forge schedules, executes or promotes a child flow.

Validation turns recursive reuse into an explicit promotion gate. A proposed
child-subflow binding is not promotable by itself: `forge validate` fails the parent
task until the binding is revisioned to a validation-ready state. `forge workflow
validate-subflow` performs that Forge-owned mutation, checks that the child
workflow/task exists, refuses child flows that are not scaled to zero,
stamps the current child lifecycle and validation gate onto the parent binding, and
records a workflow revision plus event. This keeps reuse as an auditable runtime
decision instead of silently treating a registry suggestion as completed work.

## Personality/Soul Routing

Some workflow outputs are not only machine artifacts. Reports, research summaries,
strategy documents, teaching material and operator updates are read by humans, so
Forge should be able to route a node through an explicit personality, voice or
"soul" profile when that improves clarity.

This capability must remain operationally bounded:

- the persona is a node-level execution setting, not hidden global behavior;
- the task graph records mode, scope, source models, voice, tone and validation gate;
- the context packet records which persona profile was selected, includes it as a shard and hashes it into lineage;
- the project operating context is also a shard, hashed into lineage and projected into persona profiles/contracts;
- executor handoff packets project the selected persona as a versioned contract so
  adapters can enforce the node mode without parsing unrelated context;
- prompt packets also project the Forge-owned `personality_decision`, which combines node persona,
  brand identity, organization/product/channel/user scope, fallback mode, style sources, audit status
  and `personality_decision_required` into a hashable executor decision;
- Codex-style developer/personality instructions and Paperclip-style soul, voice,
  tone or persona models are summarized as explicit inputs to the profile contract;
- the persona switch is included in lineage so results are replayable;
- promotion validation rejects persona switches that are not node-scoped,
  auditable, source-model backed and gated by `persona_routing_required`;
- validation gates can reject artifacts that drift away from the requested role,
  audience, constraints or factual content.

The intent is to improve human-facing artifacts without letting personality override
Forge goals, validation rules, safety constraints or source-of-truth state.

## Deterministic + AI Hybrid Graph

Forge workflows should mix AI and non-AI execution in one graph.

Supported graph node classes should include:

- AI executor tasks;
- deterministic local code tasks;
- Python or Node.js code nodes for repeated/frequent logic that does not need model reasoning;
- waits and cron continuation;
- approvals;
- validation gates;
- rollback;
- deployment;
- notifications and cost reports.

Forge should decide whether a node needs AI. If the work is stable, repeated or high-volume, Forge should prefer a deterministic local code node over a model call.

## Long-Running Cognition

Forge must make cognition durable over time.

Long-running workflow support should include:

- pause/resume;
- async continuation;
- durable execution records;
- checkpointing;
- partial retry from the failed node or subflow;
- resumable context packets;
- run state that survives crashes, CLI restarts and executor changes.

## Workflow Registry, Inspect And Subflows

Forge must expose the workflow registry as an operational runtime surface, not only as raw SQLite state.

Required user-facing goals:

- `forge list` lists workflows/runs that are currently running and workflows/runs that are not running.
- Each list row includes a stable id, lifecycle state and the original initial request description, even after later goal mutations.
- Non-infinite workflows should scale to zero when no runnable or scheduled work remains.
- Infinite workflows and infinite subflows remain eligible for scheduling instead of being treated as completed one-shot graphs.
- `forge inspect <id>` renders the current workflow graph in the terminal from persisted Forge state.
- `forge inspect <id> --verbose` includes task goals, expected outputs, validation rules, subtasks and proposed child-subflow links.
- `forge inspect <id> --task <task-id>` focuses inspection on one node while retaining full workflow task-count metadata.
- Workflows may contain subflows recursively. A flow can own many subflows, and each subflow can own many child subflows.
- Subflows can be finite or infinite. Infinite subflows require explicit lifecycle metadata so Forge can distinguish "idle but alive" from "completed".
- Running workflows must remain mutable: list gives stable ids, inspect shows the current graph, and goal/artifact mutations appear as revisions.

Before creating a new workflow from scratch, Forge should inspect available workflows and reusable flow definitions. If an existing flow can satisfy part of the new objective, Forge should propose or attach it as a child subflow instead of duplicating orchestration logic.

The first reuse contract is deterministic and registry-derived. `forge list` exposes reusable local code-node subflows with a compatibility key based on execution policy, language, entrypoint and validation gate, plus a context lineage hash derived from the task-local context requirements and validation rules. `forge plan` and `forge request start` report compatible `reuse_candidates` from existing workflows before saving the new workflow and persist the best attachable candidate per requested task as a proposed `child_subflows` link. This gives direct planning and skill-style async requests the same recursive subflow surface without spending a model call, executing local Python/Node.js work or automatically promoting reused subflows. Promotion requires a later `forge workflow validate-subflow` mutation so the parent workflow records when a reused child became validation-ready.

## Async Request Contract

When Codex/OpenCode use Forge as a skill, they should not hold the user interaction open for long-running work.

The preferred flow is:

```text
Codex/OpenCode/skill
→ forge request start
→ receives run_id
→ returns run_id to human
→ Forge continues asynchronously
→ human/agent checks forge request status later
```

`run_id` is distinct from `workflow_id`. The workflow is the operational graph; the run is the asynchronous execution instance that can continue, pause, resume and report progress.

`forge request status` must resolve the run id to the current workflow before reporting status. Runtime mutations performed through Forge, including goal updates and attached artifacts, are reflected in request status with the original request preserved as `requested_goal`.

## Self-Evolution Contract

Forge may work on Forge itself only through bounded cycles:

- stop date is mandatory;
- every cycle writes prompt/report artifacts;
- prompt packets are versioned and must load the persisted Forge workflow goal before generic strategic guidance;
- if a human or adapter mutates the self-evolution goal through `forge workflow update-goal`, the next cycle must carry that current goal, initial goal and workflow revision into the executor prompt;
- `forge self run --mode lean|balanced|strict` selects the self-evolution operating boundary:
  lean minimizes governance and rejects low-value bloat cycles, balanced is the
  default for small validated increments, and strict tolerates extra overhead
  only for concrete audit, safety or distributed-execution needs;
- every run reports `forge.self_evolution.overhead_ledger.v1` with prompt bytes,
  estimated prompt tokens, validation command count, artifact count, metadata
  bytes and an orchestration cost score;
- every run reports `forge.self_evolution.decision_gate.v1`, which can run one
  bounded cycle, reject a low-value cycle whose expected value is below
  orchestration cost, or stop immediately when the persisted terminal
  self-evolution goal is already satisfied;
- authorized executors are selected from local policy;
- validation must pass before commit;
- push is explicit;
- external Docker/Kubernetes/Knative resources remain out of scope unless explicitly authorized.

## Validation Contract

A workflow is only promotable when all tasks are completed and validation rules pass. Until then, `forge validate` returns a blocked status and non-zero exit code.

Self-improvement is intentionally conservative. `forge improve` generates an experiment artifact plus a version changelog and does not auto-promote.
