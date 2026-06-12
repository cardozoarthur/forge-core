# Forge Core

Forge Core is a high-performance AI-native operational and strategic workflow runtime for transforming large objectives into validated, context-controlled atomic execution graphs.

Forge is not an LLM wrapper and not a human-flow builder. It treats models as interchangeable execution resources and can run workflows that mix AI steps, deterministic non-AI steps, code/subworkflows, waits/cron, notifications and live human/AI steering.

The intended architecture is hybrid:

- CLIs such as Codex, OpenCode and Gemini CLI can call Forge directly for simpler adoption.
- Forge can also call those CLIs as bounded execution engines for long-running tasks.
- Native integrations/plugins are useful when they make the developer experience simpler, but the operational authority remains in Forge: graph state, context routing, retries, validation, scheduling, costs and persistence.

- decomposition;
- scheduling;
- context routing;
- validation;
- retries;
- artifact persistence;
- operational memory;
- addon and capability discovery;
- controlled self-improvement.

## Status

Current version: `0.4.177`

This is the first functional CLI + Skill version:

- Rust CLI binary: `forge`
- SQLite persistence
- deterministic atomic task graph generation
- versioned, sharded bounded context package generation with subflow-aware routing
- strict context readiness gates for executor handoff
- validation gates
- simulated execution runtime
- autonomous mixed AI/non-AI workflow planning
- native cron/wait task representation with timezone, next-run, missed-run policy, run history and scale-to-zero metadata
- explicit loop primitives for loop-over-items, bounded repeat, retry/backoff, while/until and infinite recurring subflow semantics
- notification payloads with final workflow cost reporting
- artifact listing
- workflow registry listing with lifecycle state and `running`/`non-running` filters
- workflow registry context-action catalog discovery for handoff/resume/retry filters
- workflow registry per-task context-action refs for handoff/resume/retry triage
- workflow registry quality-action catalog discovery for Context Routing Engine triage filters
- terminal workflow DAG inspection with lifecycle, dependency, persona, context-route, execution-policy, next-action, focused task views and recursive child-subflow annotations
- handoff readiness summaries in workflow inspection and async request status
- proposed child-subflow links for compatible deterministic code-node reuse
- revisioned child-subflow validation gates before workflow promotion
- context routing with deterministic shard manifests, deterministic code-node and long-running cognition goals
- minimum-correct context section receipts for executor adapters and budget repair
- context routing quality scores and warnings for budget pressure, missing required context and profile filtering
- registry-level context quality summaries and workflow `quality_action` recommendations
- Forge-owned execution policy metadata for deterministic local Python/Node.js code nodes
- node-scoped Personality/Soul Routing profiles, metadata and validation gates for human-facing artifacts
- controlled improvement proposal generation
- Codex/OpenCode-compatible `forge-core` skill
- executor sync that detects installed/configured CLIs and persists human authorization policy
- runtime sync that detects Docker/Kubernetes/Knative and persists human authorization policy
- local cluster node registry with capability/trust metadata, dry-run placement decisions and distributed handoff manifests
- cluster registry scheduling posture that exposes per-node active/expired lease pressure without remote execution
- lease-aware cluster placement that exposes active lease counts and prefers idle eligible nodes
- remote AI cluster placement is blocked until explicit authorization enables cluster cognitive executors
- n8n-aware research planning that catalogs workflow primitives and evaluates Forge primitive candidates before graph promotion
- goal-oriented tasks with subtasks, impediments, acceptance criteria and rework readiness checks
- runtime workflow mutation for goals and artifacts with origin trace from `codex`, `opencode`, `forge_cli` or skills
- local assisted operations console with workflow visibility, run controls and a strategic modifier lane for live goal/node proposals
- outcome status that separates support artifacts from user-facing final deliverables and requires final audit evidence before closing deliverable-driven workflows
- final delivery package generation for async runs, attaching user-facing Markdown plus machine-readable JSON summaries of readiness, deliverables, evidence, tasks and remaining gaps
- visual workflow surface for tasks, subtasks, whiteboards, screens, wireframes, flows, components, documents, design tokens and human+AI collaboration actions in the ops console
- orchestrator improvement candidate ranking using workflow events, run heartbeats, outcome evidence, parallel-ready tasks and avoidable AI-cost metrics for repetitive/deterministic work
- async workflow substrate policy with scope guards for Forge-owned resources
- async request handoff for skill callers: submit a goal, receive `run_id`, continue later with Forge
- process-liveness-aware run activity so a recorded live executor PID keeps long-running Forge handoffs active even after heartbeat TTL expiry
- Forge-owned patch planning for agent file edits, with bounded context contract, repo-relative path permission gates, file snapshots, diff-review commands, validation commands, apply artifacts and guarded revert proposals without silent destructive restores
- MCP tool manifest and call surface for agent workflows: list, inspect, interactive home/slash/route, start/resume/status, schedule create/update/list/summary, loop inspect/summary, task handoff, context request, validation status and bounded artifact fetch
- interactive home dashboard through `forge interactive home` and MCP `forge.interactive.home`, including `harness_mode_panel` and `/harness` quick action so TUI/web/agent dashboards can see the effective Forge-first mode, source and project config status before opening brain shells
- interactive task board through `forge interactive task-board` and MCP `forge.interactive.task_board`, using `forge.interactive.task_board.v1` to expose workflow lanes, operable per-task cards, ready handoffs, blocked/failed/running task counts, checkpoint resume candidates, pending human interactions, artifact counts and direct next-action commands for TUI/web/agent dashboards
- 0.5 milestone status, promotion manifest, release-gates panel, evidence-plan, collect-evidence and attached-evidence ledger surfaces for release-gate inspection, including project-root-aware secret-free `manifest_templates` for missing project evidence manifests such as `.forge/connected-brain-runtimes.json`, `.forge/multimodal.json` and `.forge/multimodal-runtimes.json`
- native daily Goal research workflow planning and smoke execution for `hackathon` reports with Markdown/PDF artifacts and redacted Telegram delivery records
- scheduler worker status includes a deterministic assignment plan that shows which due scheduled workflows fit the current bounded worker pool and which remain queued under backpressure
- bounded parallel schedule scanning reports `forge.worker_pool.v1` execution evidence and still reconciles idle scheduled workflows into persisted scale-to-zero state
- persisted task leases so two executors cannot acquire the same workflow task concurrently
- executor handoff packets that combine strict context readiness, lease metadata, routing cache keys, checksums and validation gates
- cluster handoff packets that choose an eligible node, lease the task to that node and return a content-addressed sync manifest without remote execution
- cluster sync manifests with deterministic manifest-level SHA-256 checksums for distributed handoff auditing
- executor response validation for adapter outputs before Forge accepts completion evidence
- self-evolution runner for bounded Codex/OpenCode cycles until a stop date
- versioned self-evolution prompt packets with SHA-256 checksums in cycle reports
- self-evolution prompt packets load the persisted Forge workflow goal before generic guidance, so runtime `workflow update-goal` changes drive future cycles
- `workflow update-goal` reparses `forge.intent.v2` with the current operating context and Addon catalog, so changed human goals also update deliverables, capability resolution and outcome gates instead of only replacing display text
- self-evolution operating modes (`lean`, `balanced`, `strict`) with overhead ledger and a decision gate that can stop terminal goals or reject low-value bloat cycles
- versioned improvement artifacts with strong changelog generation
- addon catalog and capability resolution commands for Core + Addons planning
- persistent SQLite Addon lifecycle registry with install, upgrade, downgrade, enable, disable, uninstall and installed-list commands
- deterministic Addon package reports for marketplace distribution, including raw/canonical manifest hashes, distribution metadata, capability catalog and detached signature metadata
- persistent Addon marketplace registry and trust store with Ed25519 package verification before package-based install
- Addon compatibility constraints for Forge version, Addon API versions, runtimes, features, platforms and major-version migration/rollback plans
- Addon major-version migration workflow creation with backup, apply, validation, rollback readiness and audit-package tasks
- SQLite-materialized Addon capability index with CLI/MCP filters by Addon, capability and lifecycle
- Addon compatibility validation for dependency `version_req` clauses
- persistent Addon permission authorizations, blocking install/enable and capability exposure when a manifest permission requires human approval
- granular Addon permission gates with declared tools, resources, integrations, actions and tenant scopes projected into runtime contracts, event adapters and UI views through `forge.addon_permission_gate.v1`
- Addon runtime contract dispatch policy reports that gate runtime, entrypoint, lifecycle and permissions before any external contract execution
- persistent Addon runtime dispatch ledger with queued/blocked/dry-run/completed/external-worker records and safe Core-side processing for `forge_core_builtin` contracts
- Addon runtime worker registry for auditable discovery of external workers by runtime, status, trust level and Ed25519 signature identity
- local Addon runtime worker execution for registered `local_process` workers and controlled `external_api` HTTP/HTTPS workers with allowlists, env-backed or credential-vault-backed HMAC/Bearer auth, typed JSON envelopes, timeout and ledger-backed claim/completion
- bounded inbound HTTP webhook listener that normalizes real POST requests into `event_inbox`, optionally routes them through declared ingress adapters and keeps routing validation inside Forge
- outbound Addon event adapter execution for declared `http`/`webhook` egress, with direction/action/event-type/permission checks, endpoint allowlist, tenant-policy delivery gates for workflow-bound and project-level sends, env-backed or credential-vault-backed HMAC/Bearer secrets, `http://` posting, `https://` posting through controlled curl, typed JSON envelope, timeout, response hash report and persisted global timeline audit
- built-in notification Addon declares Telegram ingress plus governed message, document and report egress adapters; `events emit --dry-run` validates delivery policy and non-dry-run Telegram transport can send message/document/report payloads through the Bot API via env-backed or credential-vault-backed token without exposing the token in reports, attaching `telegram_delivery_record` evidence to the workflow when `payload.workflow_id` is present
- Addon planner registry for `planning_strategy` and `replanning_strategy` contracts, separating first-party Core builders from external runtime-contract planners
- Addon planner execution for external `planning_strategy`/`replanning_strategy` workers, with Core reference plans, result validation and replacement-readiness equivalence audits
- Addon validator execution for external `validator` workers, with standardized subject/input/context envelopes, decision validation and result audit over the same dispatch ledger
- Addon executor execution for external `executor` workers, with standardized task/input/context envelopes, generic result validation and artifact/event/output audit over the same dispatch ledger
- harness utilities for token-headroom analysis, Forge-first CLI wrapper planning, non-destructive PATH shim installation, shim status auditing, reversible stdout/stderr headroom receipts and workflow/task/run timeline events across Codex, Claude, Gemini and OpenCode-style executors
- intent v2 records workflow mode, event policy, operating context, required capabilities, active addons and capability resolution
- capability resolution exposes workflow-extension activations, runtime contracts and missing-capability suggestions with source Addon/capability lineage, including trusted local marketplace packages on the store-aware resolver
- capability-first internal workflow-extension planner registry for first-party Addon builders and policy mutations, with textual matching kept only for legacy intents
- manifest-declared external workflow extensions become generic auditable DAG tasks when no first-party builder owns them
- Addon manifests can declare context providers and memory providers, so domain packs can advertise scoped context sections and file-memory sources without Core-specific code
- Addon manifests can declare event adapters, runtime contracts and UI views, with validation that referenced permissions exist before catalog promotion
- safe Addon UI renderer projections for Ops/TUI composition, classifying view contracts into renderer families with data-source, permission and action-risk metadata
- SQLite-materialized event observability index over the global event timeline, grouped by tenant, workflow, node and Addon with duration, retry, wait, context-pressure and memory-policy aggregates plus hour/day historical rollups
- project operating context from `.forge/operating-context.yaml|json`, covering organization, brand, product, user, channel, memory scope, personality scope, brand identity, design system and operating policy
- context packets route project operating context as a first-class shard and bind brand identity, design token source and operating policy into persona profiles/contracts for executor adapters
- context and executor handoff packets expose `forge.context.memory_policy.v1`, deriving the allowed memory scopes, memory level, tenant boundary, default audience and governed `forge memory search --workflow <workflow-id>` command from workflow operating context
- workflow-bound memory search, promotion, promotion-index access and retention evaluation through CLI/MCP, deriving organization and allowed scopes from the workflow when `--workflow` / `workflow_id` is provided
- persistent identity registry for organization, brand, product, user and channel records synced from project operating context
- cross-channel identity links and resolution for treating Telegram, Discord, Web or other ids as aliases of the same governed subject
- persisted identity memberships linking users to organization/brand/product scopes, with role-derived permissions and environment evidence
- physical tenant index for workflows, runs, artifacts and events with organization/brand/product/user/channel filters
- tenant index audit for detecting workflows, runs, artifacts or events missing physical tenant projection
- tenant policy gate with audit/enforce modes for explicit context, active/current membership, custom grants/denies, action permission and tenant-index coverage
- opt-in `tenant_policy_mode: enforce` in operating context, blocking `plan`, async `request start`, inbound `start_workflow`, context handoff, run drive/step, workflow mutations, task leases, checkpoints, interactions, schedule execution, external event-egress delivery and patch/artifact mutations when the active user lacks membership or the membership role lacks the required permission
- tenant-aware workflow event stream and global event timeline projection through CLI and MCP, with project operating-context enforcement for global timeline reads when `tenant_policy_mode: enforce` is active
- global inbound event inbox with route support for `start_workflow`, `continue_workflow`, `modify_workflow`, `pause_workflow`, `resume_workflow` and validation-gated `complete_workflow`
- file-first memory policy/search with `MEMORY_NONE`, `MEMORY_SESSION`, `MEMORY_SHORT_TERM`, `MEMORY_STANDARD`, `MEMORY_FULL` and `MEMORY_ADMIN` levels before scoped Markdown retrieval across global, organization, project and processing roots

## Install

```bash
cargo install --path .
```

## CLI Quickstart

```bash
forge plan --goal "Create a delivery platform" --output json
```

Use the returned `workflow_id`:

```bash
forge list --output json
forge list --lifecycle running --output json
forge list --lifecycle non-running --output json
forge list --context-actions --output json
forge list --context-action wait_for_dependencies --output json
forge list --quality-actions --output json
forge list --quality-action increase_context_budget --output json
forge harness token-headroom --content "$(cat logs/build.log)" --kind log --budget-tokens 1200 --source build-log --persist --output json
forge harness retrieve-headroom --ref forge://harness/headroom/<sha256> --include-content --output json
forge harness wrap-plan --executor codex --cmd codex --cmd --dangerously-bypass-approvals-and-sandbox --forge-first --workflow <workflow-id> --task <task-id> --run <run-id> --context-budget 8000 --output json
forge harness install-shims --shim-dir .forge/bin --executor codex --forge-first --workflow <workflow-id> --task <task-id> --run <run-id> --context-budget 8000 --output json
forge harness shim-status --shim-dir .forge/bin --executor codex --output json
forge sync executors --home "$HOME" --shim-dir .forge/bin --allow codex --output json
forge shells --executor codex --workflow <workflow-id> --task <task-id> --run <run-id> --context-budget 8000 --ttl-seconds 900 --output json
forge shells --executor codex --workflow <workflow-id> --task <task-id> --run <run-id> --record-session --origin forge_cli --output json
forge harness exec --executor codex --forge-first --workflow <workflow-id> --task <task-id> --run <run-id> --context-budget 8000 --output json -- codex --version
forge addons catalog --output json
forge addons installed --output json
forge addons capabilities --addon forge.addon.example --lifecycle enabled --output json
forge addons contracts --type planning_strategy --lifecycle enabled --output json
forge addons contract-policy --contract my.addon.executor --output json
forge addons dispatch-contract --contract my.addon.executor --input '{"work_item":"demo"}' --output json
forge addons dispatch-planner --contract my.addon.planning --goal "Criar workflow de roteirização" --constraint "preservar aprovação humana" --output json
forge addons execute-planner --addon my.addon --contract my.addon.planning --worker route-planner-api-worker --goal "Criar workflow de roteirização" --constraint "preservar aprovação humana" --output json
forge addons dispatches --status queued --output json
forge addons run-dispatch --dispatch <dispatch-id> --worker local-worker --output json
forge addons execute-dispatch --dispatch <dispatch-id> --worker local-worker --lease-seconds 300 --output json
forge addons register-worker --worker wasm-worker --runtime wasm --trust-level signed --data '{"endpoint":"local://wasm-worker","signature_scheme":"ed25519","public_key_hex":"..."}' --output json
forge addons workers --runtime wasm --status available --output json
forge addons claim-dispatch --dispatch <dispatch-id> --worker wasm-worker --lease-seconds 600 --output json
forge addons complete-dispatch --dispatch <dispatch-id> --worker wasm-worker --result '{"ok":true}' --signature sig:demo --attestation '{"signer":"demo"}' --output json
forge addons views --surface ops_console --lifecycle enabled --output json
forge addons observability --addon forge.addon.example --dispatch-limit 1000 --output json
forge addons resolve --goal "Criar roteirização agrícola" --addon-dir .forge/addons --registry-source ./forge-addon-registry.json --output json
forge addons package --manifest ./my-addon.yaml --repository https://example.com/my-addon.git --channel stable --package-path ./dist/my-addon.package.json --output json
forge addons trust-key --repository https://example.com/my-addon.git --channel stable --public-key <ed25519-public-key-hex> --approved-by arthur --output json
forge addons publish-package --package ./dist/my-addon.package.json --output json
forge addons fetch-package --source https://example.com/my-addon.package.json --allow-remote --expected-sha256 <sha256> --lock .forge/addon-package-lock.json --output json
forge addons sync-registry --source https://example.com/forge-addon-registry.json --allow-remote --lock .forge/addon-package-lock.json --output json
forge addons package-lock --write .forge/addon-package-lock.json --output json
forge addons marketplace --repository https://example.com/my-addon.git --channel stable --output json
forge addons install-package --package ./dist/my-addon.package.json --lock .forge/addon-package-lock.json --output json
forge addons migration-workflow --from-manifest ./my-addon-v1.yaml --to-manifest ./my-addon-v2.yaml --action upgrade --output json
forge addons install --manifest ./my-addon.yaml --output json
forge addons upgrade --manifest ./my-addon-v2.yaml --output json
forge addons downgrade --manifest ./my-addon-v1.yaml --output json
forge addons permissions --addon forge.addon.example --output json
forge addons authorize-permission --addon forge.addon.example --permission payments.charge --risk high --approved-by arthur --output json
forge addons revoke-permission --addon forge.addon.example --permission payments.charge --approved-by arthur --output json
forge addons disable forge.addon.example --output json
forge addons enable forge.addon.example --output json
forge addons uninstall forge.addon.example --output json
forge identity sync --project-root . --output json
forge identity registry --scope organization --output json
forge identity memberships --subject-scope user --subject arthur --organization digital-directive --output json
forge identity membership-update --subject arthur --organization digital-directive --brand digital-directive --product forge --grant workflow:mutate --source forge_cli --output json
forge identity link --left-scope telegram --left-id 123 --right-scope user --right-id arthur --reason "same person across channels" --output json
forge identity resolve --scope telegram --id 123 --output json
forge identity links --scope telegram --id 123 --status active --output json
forge identity tenant-index --organization digital-directive --resource-type workflow --output json
forge identity tenant-audit --output json
forge identity tenant-policy --workflow <workflow-id> --mode enforce --output json
forge memory policy --output json
forge memory search --workflow <workflow-id> --query "customer suggestion operations" --scope project --scope processing --audience manager --memory-level short_term --output json
forge memory search --workflow <workflow-id> --query "operating decisions" --scope organization --organization digital-directive --audience internal --memory-level standard --output json
forge memory promote --workflow <workflow-id> --from-scope processing --to-scope organization --source-path ./run-memory.md --summary "Curated product signal without private data." --approved-by arthur --reason "Useful operating memory for the organization." --organization digital-directive --output json
forge memory promotions --workflow <workflow-id> --to-scope organization --approved-by arthur --output json
forge memory retention --workflow <workflow-id> --scope processing --scope project --output json
forge memory cleanup --workflow <workflow-id> --scope processing --dry-run --output json
forge memory cleanup --workflow <workflow-id> --scope processing --mode archive --approved-by arthur --reason "Final packaging complete." --confirm --output json
forge cost ledger --workflow <workflow-id> --output json
forge cost materialize --project-root . --workflow <workflow-id> --output json
forge cost incremental --project-root . --after-sequence <global-event-id> --output json
forge cost history --workflow <workflow-id> --bucket day --group-by source_kind --output json
forge cost maintain --project-root . --workflow <workflow-id> --bucket day --group-by source_kind --retention-days 31 --output json
forge cost daemon --project-root . --workflow <workflow-id> --bucket day --group-by workflow --max-cycles 2 --interval-seconds 300 --retention-days 31 --output json
forge cost retention --project-root . --organization <organization-id> --retention-days 31 --apply --approved-by <operator> --reason "Validated retention window." --confirm --output json
forge events observability --workflow <workflow-id> --node task-001 --addon forge.addon.example --output json
forge events observability-history --workflow <workflow-id> --bucket day --group-by addon --output json
forge events improvement-policy --workflow <workflow-id> --min-events 3 --output json
forge events scan --project-root . --limit 20 --output json
forge events worker --project-root . --limit 20 --max-cycles 12 --interval-seconds 300 --idle-exit --stop-file .forge/run/event-worker.stop --output json
forge events service-plan --kind worker --project-root . --lease-seconds 300 --heartbeat-seconds 60 --output json
forge events service-plan --kind webhook_ingress --project-root . --origin partner_api --action start_workflow --schema partner.event.v1 --route --hmac-secret-env FORGE_WEBHOOK_SECRET --signature-header X-Forge-Signature --output json
forge events service-run --kind worker --project-root . --limit 20 --max-cycles 12 --interval-seconds 300 --stop-file .forge/run/event-worker.stop --lease-owner forge.event_service_manager --output json
forge events service-run --kind webhook_ingress --host 127.0.0.1 --port 8787 --path /webhook --origin partner_api --action start_workflow --schema partner.event.v1 --project-root . --route --max-requests 100 --stop-file .forge/run/webhook-ingress.stop --lease-owner forge.event_service_manager --output json
forge events service-supervise --kind worker --project-root . --limit 20 --max-cycles 1 --max-runs 12 --backoff-initial-seconds 5 --backoff-max-seconds 300 --stop-file .forge/run/event-supervisor.stop --lease-owner forge.event_service_supervisor --output json
forge events runtime-reconcile --project-root . --limit 20 --service-limit 20 --recover-stale-services --execute --scan-schedules --schedule-executor forge-runtime-scheduler --schedule-max-workers 2 --schedule-ttl-seconds 300 --max-cycles 1 --max-runs 1 --lease-owner forge.event_runtime_reconcile --output json
forge events runtime-daemon --project-root . --limit 20 --service-limit 20 --recover-stale-services --execute --scan-schedules --schedule-executor forge-runtime-scheduler --schedule-max-workers 2 --schedule-ttl-seconds 300 --continuous --cycle-retention 100 --interval-seconds 300 --idle-exit --lease-owner forge.event_runtime_daemon --stop-file .forge/run/event-runtime-daemon.stop --output json
forge events services --project-root . --kind worker --status completed --output json
forge events services-recover --project-root . --kind worker --origin forge_cli --output json
forge events webhook-ingress --host 127.0.0.1 --port 8787 --path /webhook --origin partner_api --action start_workflow --schema partner.event.v1 --project-root . --route --max-requests 100 --stop-file .forge/run/webhook-ingress.stop --output json
forge events emit --addon forge.addon.partner --adapter partner.webhook_egress --event-type partner.notification --action notify_partner --origin codex --payload '{"id":"demo"}' --output json
forge inspect <workflow-id> --verbose --output json
forge inspect <workflow-id> --task task-008 --verbose --output json
forge status --workflow <workflow-id> --output json
forge workflow validate-subflow --workflow <workflow-id> --task task-011 --child-workflow <child-workflow-id> --child-task task-011 --origin codex --output json
forge schedule create-daily-goal-research --goal hackathon --timezone America/Sao_Paulo --cron "0 8 * * *" --origin codex --output json
forge plan --goal "Execute intake now, wait until 2026-06-10T12:00:00Z, then run deterministic follow-up without AI" --output json
forge schedule update --workflow <workflow-id> --task task-009 --cron "0 8 * * *" --timezone America/Sao_Paulo --next-run-at 2026-05-26T11:00:00Z --origin codex --output json
forge schedule pause --workflow <workflow-id> --task task-010 --origin codex --output json
forge schedule resume --workflow <workflow-id> --task task-010 --origin codex --output json
forge schedule run-due --workflow <workflow-id> --output json
forge task validate-response --workflow <workflow-id> --task task-001 --response ./executor-response.json --output json
forge context --workflow <workflow-id> --task task-001 --budget 1200 --output json
forge run --workflow <workflow-id> --simulate --output json
forge validate --workflow <workflow-id> --output json
forge improve candidates --output json
forge improve --workflow <workflow-id> --output json
forge artifacts --workflow <workflow-id> --output json
forge milestone manifest --version 0.5 --output json
forge milestone evidence-plan --version 0.5 --capability replacement_grade_cli --project-root . --connected-brain <provider-id> --output json
forge milestone evidence-plan --version 0.5 --capability experimental_multimodal_runtime --project-root . --connected-runtime <runtime-id> --output json
forge milestone collect-evidence --version 0.5 --capability replacement_grade_cli --kind external_brain_provider_execution --project-root . --connected-brain <provider-id> --approved-by arthur --origin codex --output json
forge milestone collect-evidence --version 0.5 --capability replacement_grade_cli --kind broader_project_coding_research_workflow --project-root . --approved-by arthur --origin codex --output json
forge milestone collect-evidence --version 0.5 --capability replacement_grade_cli --kind terminal_file_editing_ux --project-root . --approved-by arthur --origin codex --output json
forge milestone collect-evidence --version 0.5 --capability experimental_multimodal_runtime --project-root . --connected-runtime <runtime-id> --approved-by arthur --output json
forge milestone attach-evidence --version 0.5 --capability experimental_multimodal_runtime --kind production_runtime_benchmark --summary "Operator-approved runtime receipt." --artifact ./runtime-receipt.json --approved-by arthur --output json
forge multimodal benchmark-template --capability image_understanding --output json
forge multimodal demo-plan --demo local_image_recognition --output json
```

`forge context` emits a versioned context packet (`forge.context.v30`) with a deterministic
`task_local_revisioned_persona_profile_compressed_executor_policy_subflow_checkpoint_dependencies_handoff_budget_summary_required_first_content_addressed_shards_budget_ledger_quality_contract_repair_budget_plan_minimum_correct_set_persona_contract_next_action_delta_economy_prompt_packet_replay_manifest_continuation_plan_shard_selection_audit_v30` routing policy.
The packet keeps the legacy `content` body for executors, and also returns workflow
revision, artifact count, project `operating_context`, persona routing metadata for
human-facing nodes, a versioned persona profile and persona contract that include
brand voice/tone/values, design token source and operating policy, executor profile
metadata, a versioned routing contract, execution policy metadata, dependency
readiness summaries, proposed child-subflow bindings, lineage hashes including
`operating_context_sha256`, and a shard manifest with included/omitted sections, profile exclusions,
compression flags, required/missing-required markers, source labels, priorities,
content-addressed shard IDs, source hashes, remaining-budget before/after values,
byte counts, minimum-routable byte counts, per-shard selected-cost basis points,
selection savings, summaries and SHA-256 checksums. The packet also exposes `context_ready`,
`required_sections`, `missing_required_sections`, `handoff_ready`, `handoff_status`,
`handoff_blockers`, a `routing_summary`, a versioned `routing_contract`, a versioned
`routing_repair` budget recommendation, a versioned `routing_quality` score/warning contract,
`minimum_correct_set` section receipt, a node-scoped `persona_profile` and `persona_contract` for human-facing artifacts, a versioned `next_action`
resume/handoff decision, a versioned `routing_economy` ledger with selected, compressed,
omitted and no-AI model-call avoidance metrics, a versioned `prompt_packet`
contract for executor adapters, a versioned `replay_manifest`, a versioned
`continuation_plan` for checkpoint resume/refresh/partial-retry decisions, and a versioned `routing_fingerprint`
with component hashes and a cache key so executor adapters can reuse or invalidate
bounded context without reparsing full packets. Adapters can block handoff when the
minimum correct context was omitted or dependency tasks are not ready.
`minimum_correct_set` lists every required section with its inclusion/compression state,
selected and original byte counts, hashes, routing decision and repair action, so adapters
can audit the exact missing floor without re-deriving it from the full shard manifest.
Deterministic
command and wait nodes receive a smaller no-AI context
envelope that preserves local objective, execution policy, proposed subflow reuse and
validation context before lower-priority narrative sections, while AI and mixed nodes
keep richer reasoning context. When a goal explicitly calls for repeated local Python
or Node.js work without AI, Forge marks the deterministic step as a `local_code_node`,
records the selected runtime and routes that policy into the task context without
executing external code during planning. If the registry attaches a compatible child
subflow, the context packet carries both structured `child_subflows` metadata and a
compact `child_subflows` shard so the executor sees Forge's reuse decision without
reconstructing it from history. Runtime goal, artifact and persona routing state are
included in the context lineage so executors can detect stale context before resuming
work.
The packet also includes a versioned `replay_manifest` with the minimal replay
command, selector version, route budget, context checksum and shard refs. The prompt
packet binds the replay manifest checksum, and inspection projects the same checksum,
so long-running executor adapters can pause, compare and resume against the exact
context route without reparsing unrelated packet fields.

`forge inspect --output json` projects compact `context_route` and `execution_policy`
contracts for every DAG node and expands proposed child-subflow links into auditable
path metadata.
The route reuses the same versioned context package and includes the executor profile,
effective budget, context checksum, routing fingerprint schema, routing cache key,
lineage hash, handoff status, resume status, missing required sections and routing
summary. It also reuses the context packet's versioned `next_action` projection
(`forge.inspect_context_action.v1`) so operators can see whether a node should start
handoff, wait for dependencies, raise context budget, refresh stale context or retry
from a checkpoint with fresh context. The execution policy projection
(`forge.inspect_execution_policy.v1`) exposes the mode, AI allowance, deterministic
flag, reuse hint, selection reason, validation gate and optional local code runtime
fields before a handoff packet is requested. Human terminal diagrams also show the
profile, handoff state, selected/effective context bytes, short routing cache key,
next action and compact execution policy for each node. When a node has proposed
child subflows, inspection also reports each subflow's parent node, depth, path,
reachability, terminal status and loaded child workflow/task counts so operators can
audit recursive reuse without executing or promoting the child flow.
Use `forge inspect <workflow-id> --task <task-id>` when an operator or adapter needs a
bounded terminal view of one node. Focused inspection keeps the same context-route,
persona, execution-policy, handoff and child-subflow projections, adds a `focus`
block and `workflow_task_count`, and limits the node list, handoff summary and
terminal diagram to the selected task.

Use strict context mode when handing a package to an executor:

```bash
forge context --workflow <workflow-id> --task task-001 --budget 1200 --strict --output json
```

Strict mode still prints the replayable context package, but exits non-zero if
`handoff_ready=false`.

Acquire an executor handoff packet when a bounded adapter is ready to work:

```bash
forge task handoff --workflow <workflow-id> --task task-001 --executor codex --budget 1200 --ttl-seconds 900 --output json
```

The command reuses the strict context readiness contract, acquires a Forge task
lease only when `handoff_ready=true`, and returns `forge.executor_handoff.v8`
with the selected executor, task executor kind, lease id, context SHA-256,
routing fingerprint schema, routing cache key, lineage hash, expected output,
context routing quality, execution policy mode, full execution policy and validation gate. Human-facing
persona nodes also carry a versioned `persona_contract` with the derived profile id,
profile checksum, node-scoped mode, voice, tone, brand voice/tone/values, design token source,
operating policy, instruction source, source model summaries, persona validation gate and lineage hashes so adapters do not have to
infer soul/personality routing from the nested context body. The handoff packet
also reuses the context `continuation_plan` as its `resume_plan`, so adapters see
the same validation-gated decision in `forge context`, `forge inspect` and
`forge task handoff`.

Before an adapter result is treated as usable completion evidence, validate its
response contract:

```bash
forge task validate-response --workflow <workflow-id> --task task-001 --response ./executor-response.json --output json
```

The response must use `forge.executor_response.v1`, match the task id, include a
replayable `trace_ref`, report non-negative cost/token values and, when marked
`completed`, include at least one passing validation evidence item. The command is
read-only with respect to task state: it records an audit event and exits non-zero
for rejected responses instead of silently promoting work.

Skill-style async handoff:

```bash
forge request start --goal "Improve Forge Core" --origin codex --output json
forge request status --run <run-id> --output json
forge request resume --run <run-id> --origin codex --output json
forge request switch-executor --run <run-id> --executor opencode --fallback-executor codex --summary "codex limit approaching; opencode continuing from Forge state" --origin codex --output json
```

Codex/OpenCode should prefer this pattern when using Forge as a skill: make a short request, receive a `run_id`, and let Forge own the asynchronous workflow state.
`forge request start` uses the same registry-derived reuse pass as `forge plan`, returning `reuse_candidates`, `attached_subflows` and `forge.agent_handoff_contract.v1` when Forge can attach a compatible deterministic child subflow before persisting the async workflow.
`forge request status` resolves the run id back to the current Forge workflow state, including the current goal, original requested goal, latest revision, artifact count, task status summary and context handoff summary for every task.
`forge request switch-executor` hot-swaps the active executor without cancelling the run, changing the workflow id, dropping checkpoints or weakening explicit user directives. Use `--fallback-executor` to persist an ordered executor recovery chain, for example OpenCode primary with Codex fallback. Use it when Codex/OpenCode/Gemini/other adapters approach model limits or need to hand work to another authorized executor while Forge remains the source of truth.
The handoff summary includes aggregate routing quality counts and each task's quality contract, so async callers can distinguish dependency waits from context budget/profile pressure without opening full context packets.
`forge list` exposes the workflow registry across planned and async workflows, including stable workflow ids, associated run ids, initial request, current goal, lifecycle state, task summary, execution-policy route counts and deterministic code-node subflows that can be reused by compatible future workflows. Completed finite workflows are projected as `scaled_to_zero` when there is no remaining task work. Operators can use `forge list --context-actions` to discover valid handoff/resume/retry filter values, then combine lifecycle slices with `--context-action <action>` to find workflows whose next context route includes a specific handoff action such as `wait_for_dependencies`, `increase_context_budget` or `partial_retry_with_fresh_context`. Each registry row also includes `context_action_refs`, a per-task list with the task id, title, executor, next action, handoff status, blocker refs, checkpoint refs and current routing cache key, so operators can jump directly from a filtered registry row to the affected tasks without opening a full inspection first.

Agent-facing MCP surface:

```bash
forge mcp tools --output json
forge mcp call forge.run.start --input '{"goal":"Improve Forge Core","origin":"codex"}' --output json
forge mcp call forge.run.status --input '{"run_id":"<run-id>"}' --output json
forge mcp call forge.run.resume --input '{"run_id":"<run-id>","origin":"opencode"}' --output json
forge mcp call forge.run.step --input '{"run_id":"<run-id>","executor":"codex","ttl_seconds":300,"origin":"codex"}' --output json
forge mcp call forge.run.complete_task --input '{"run_id":"<run-id>","task_id":"<task-id>","executor":"codex","summary":"executor finished the ready task with passing evidence","origin":"codex"}' --output json
forge mcp call forge.run.switch_executor --input '{"run_id":"<run-id>","executor":"opencode","fallback_executors":["codex"],"summary":"take over without stopping workflow","origin":"codex"}' --output json
forge mcp call forge.improve.candidates --input '{"limit":10}' --output json
forge mcp call forge.workflow.inspect --input '{"workflow_id":"<workflow-id>","verbose":true}' --output json
forge mcp call forge.events.list --input '{"workflow_id":"<workflow-id>","limit":50}' --output json
forge mcp call forge.events.timeline --input '{"project_root":".","organization_id":"digital-directive","limit":50}' --output json
forge mcp call forge.events.observability --input '{"workflow_id":"<workflow-id>","node_ref":"task-001","addon_id":"forge.addon.example","limit":50}' --output json
forge mcp call forge.events.observability_history --input '{"workflow_id":"<workflow-id>","bucket":"day","group_by":"addon"}' --output json
forge mcp call forge.cost.history --input '{"workflow_id":"<workflow-id>","bucket":"day","group_by":"source_kind"}' --output json
forge mcp call forge.events.improvement_policy --input '{"workflow_id":"<workflow-id>","min_events":3}' --output json
forge mcp call forge.events.ingest --input '{"origin":"telegram","action":"start_workflow","data":{"goal":"Create a partner demo workflow"}}' --output json
forge mcp call forge.events.inbox --input '{"status":"pending","limit":20}' --output json
forge mcp call forge.events.adapters --input '{"transport":"telegram"}' --output json
forge mcp call forge.events.emit --input '{"addon_id":"forge.addon.partner","adapter_id":"partner.webhook_egress","event_type":"partner.notification","action":"notify_partner","origin":"codex","payload":{"id":"demo"}}' --output json
forge mcp call forge.events.worker --input '{"project_root":".","limit":20,"max_cycles":12,"interval_seconds":300,"idle_exit":true,"stop_file":".forge/run/event-worker.stop"}' --output json
forge mcp call forge.events.service_supervise --input '{"kind":"worker","project_root":".","limit":20,"max_cycles":1,"max_runs":12,"backoff_initial_seconds":5,"backoff_max_seconds":300,"stop_file":".forge/run/event-supervisor.stop"}' --output json
forge mcp call forge.events.runtime_reconcile --input '{"project_root":".","limit":20,"service_limit":20,"recover_stale_services":true,"execute":true,"scan_schedules":true,"schedule_executor":"forge-runtime-scheduler","schedule_max_workers":2,"schedule_ttl_seconds":300,"max_cycles":1,"max_runs":1}' --output json
forge mcp call forge.events.runtime_daemon --input '{"project_root":".","limit":20,"service_limit":20,"recover_stale_services":true,"execute":true,"scan_schedules":true,"schedule_executor":"forge-runtime-scheduler","schedule_max_workers":2,"schedule_ttl_seconds":300,"continuous":true,"cycle_retention":100,"interval_seconds":300,"idle_exit":true,"stop_file":".forge/run/event-runtime-daemon.stop"}' --output json
forge mcp call forge.events.services --input '{"project_root":".","kind":"worker","limit":20}' --output json
forge mcp call forge.events.services_recover --input '{"project_root":".","kind":"worker","limit":20,"origin":"mcp"}' --output json
forge mcp call forge.events.route --input '{"event_id":"<event-id>","project_root":"."}' --output json
forge mcp call forge.identity.context --input '{"project_root":"."}' --output json
forge mcp call forge.harness.token_headroom --input '{"content":"error: failed\nwarning: retry\nfinal status ok","content_kind":"log","budget_tokens":120,"source":"demo-log","reversible":true,"persist":true}' --output json
forge mcp call forge.harness.retrieve_headroom --input '{"retrieval_ref":"forge://harness/headroom/<sha256>","include_content":true}' --output json
forge mcp call forge.harness.mode --output json
forge mcp call forge.harness.wrap_plan --input '{"executor":"claude-code","command":["claude","--model","sonnet"],"forge_first":true,"workflow_id":"<workflow-id>","task_id":"<task-id>","run_id":"<run-id>","context_budget":8000,"token_headroom":true}' --output json
forge mcp call forge.harness.install_shims --input '{"shim_dir":".forge/bin","executor":"codex","forge_first":true,"workflow_id":"<workflow-id>","task_id":"<task-id>","run_id":"<run-id>","context_budget":8000}' --output json
forge mcp call forge.harness.shim_status --input '{"shim_dir":".forge/bin","executor":"codex"}' --output json
forge mcp call forge.shell.launch_plan --input '{"executor":"codex","workflow_id":"<workflow-id>","task_id":"<task-id>","run_id":"<run-id>","context_budget":8000,"ttl_seconds":900}' --output json
forge mcp call forge.shell.record_plan --input '{"executor":"codex","workflow_id":"<workflow-id>","task_id":"<task-id>","run_id":"<run-id>","context_budget":8000,"ttl_seconds":900,"origin":"mcp"}' --output json
forge mcp call forge.harness.exec --input '{"executor":"codex","command":["codex","--version"],"forge_first":true,"workflow_id":"<workflow-id>","task_id":"<task-id>","run_id":"<run-id>","dry_run":true}' --output json
forge mcp call forge.cost.maintain --input '{"project_root":".","workflow_id":"<workflow-id>","bucket":"day","group_by":"source_kind","retention_days":31}' --output json
forge mcp call forge.addons.resolve --input '{"goal":"Create a route optimization workflow","registry_source":"./forge-addon-registry.json"}' --output json
forge mcp call forge.addons.validate --input '{"addon_dirs":[".forge/addons"]}' --output json
forge mcp call forge.addons.capabilities --input '{"lifecycle":"enabled"}' --output json
forge mcp call forge.addons.contracts --input '{"contract_type":"validator","capability_id":"route_optimization"}' --output json
forge mcp call forge.addons.contract_policy --input '{"contract_id":"route.validator"}' --output json
forge mcp call forge.addons.dispatch_contract --input '{"contract_id":"route.validator","input":{"route_id":"route-001"},"dry_run":true}' --output json
forge mcp call forge.addons.dispatches --input '{"status":"queued"}' --output json
forge mcp call forge.addons.run_dispatch --input '{"dispatch_id":"<dispatch-id>","worker":"mcp-worker"}' --output json
forge mcp call forge.addons.execute_dispatch --input '{"dispatch_id":"<dispatch-id>","worker_id":"local-worker","lease_seconds":300}' --output json
forge mcp call forge.addons.register_worker --input '{"worker_id":"wasm-worker","runtime":"wasm","trust_level":"signed","data":{"endpoint":"local://wasm-worker","signature_scheme":"ed25519","public_key_hex":"..."}}' --output json
forge mcp call forge.addons.workers --input '{"runtime":"wasm","status":"available"}' --output json
forge mcp call forge.addons.claim_dispatch --input '{"dispatch_id":"<dispatch-id>","worker_id":"wasm-worker","lease_seconds":600}' --output json
forge mcp call forge.addons.complete_dispatch --input '{"dispatch_id":"<dispatch-id>","worker_id":"wasm-worker","result":{"ok":true},"signature":"sig:demo","attestation":{"signer":"demo"}}' --output json
forge mcp call forge.addons.dispatch_planner --input '{"contract_id":"my.addon.planning","goal":"Criar workflow de roteirização","constraints":["preservar aprovação humana"],"context":{"tenant":"demo"}}' --output json
forge mcp call forge.addons.execute_planner --input '{"addon_id":"my.addon","contract_id":"my.addon.planning","worker_id":"route-planner-api-worker","goal":"Criar workflow de roteirização","constraints":["preservar aprovação humana"],"context":{"tenant":"demo"}}' --output json
forge mcp call forge.addons.views --input '{"surface":"ops_console","lifecycle":"enabled"}' --output json
forge mcp call forge.addons.observability --input '{"addon_id":"forge.addon.example","dispatch_limit":1000}' --output json
forge mcp call forge.addons.package --input '{"manifest":"./my-addon.yaml","repository":"registry://forge/my-addon","channel":"stable"}' --output json
forge mcp call forge.addons.trust_key --input '{"repository":"registry://forge/my-addon","channel":"stable","public_key":"<ed25519-public-key-hex>","approved_by":"arthur"}' --output json
forge mcp call forge.addons.publish_package --input '{"package":"./dist/my-addon.package.json"}' --output json
forge mcp call forge.addons.fetch_package --input '{"source":"https://example.com/my-addon.package.json","allow_remote":true,"expected_sha256":"<sha256>","lock":".forge/addon-package-lock.json"}' --output json
forge mcp call forge.addons.sync_registry --input '{"source":"https://example.com/forge-addon-registry.json","allow_remote":true,"lock":".forge/addon-package-lock.json"}' --output json
forge mcp call forge.addons.package_lock --input '{"write":".forge/addon-package-lock.json"}' --output json
forge mcp call forge.addons.marketplace --input '{"repository":"registry://forge/my-addon","channel":"stable"}' --output json
forge mcp call forge.addons.install_package --input '{"package":"./dist/my-addon.package.json","lock":".forge/addon-package-lock.json"}' --output json
forge mcp call forge.addons.migration_workflow --input '{"from_manifest":"./my-addon-v1.yaml","to_manifest":"./my-addon-v2.yaml","action":"upgrade"}' --output json
forge mcp call forge.identity.sync --input '{"project_root":"."}' --output json
forge mcp call forge.identity.registry --input '{"scope":"organization"}' --output json
forge mcp call forge.identity.link --input '{"left_scope":"telegram","left_id":"123","right_scope":"user","right_id":"arthur","reason":"same person across channels"}' --output json
forge mcp call forge.identity.resolve --input '{"scope":"telegram","id":"123"}' --output json
forge mcp call forge.identity.tenant_index --input '{"organization_id":"digital-directive","resource_type":"workflow"}' --output json
forge mcp call forge.identity.tenant_audit --input '{}' --output json
forge mcp call forge.cost.ledger --input '{"organization_id":"digital-directive"}' --output json
forge mcp call forge.cost.materialize --input '{"project_root":".","workflow_id":"<workflow-id>","source_kind":"observed_event"}' --output json
forge mcp call forge.context.request --input '{"workflow_id":"<workflow-id>","task_id":"task-001","budget":1200}' --output json
forge mcp call forge.task.handoff --input '{"workflow_id":"<workflow-id>","task_id":"task-001","executor":"codex","budget":1200}' --output json
forge mcp call forge.schedule.create_daily_goal_research --input '{"goals":["hackathon"],"timezone":"America/Sao_Paulo","cron":"0 8 * * *","origin":"codex"}' --output json
forge mcp call forge.schedule.summary --output json
forge mcp call forge.schedule.loop_summary --output json
forge mcp call forge.loop.inspect --input '{"workflow_id":"<workflow-id>"}' --output json
forge mcp call forge.aws.check --input '{}' --output json
forge mcp call forge.aws.inventory --input '{"regions":"us-east-1,sa-east-1"}' --output json
forge mcp call forge.workflow.attach_artifact --input '{"workflow_id":"<workflow-id>","path":"./report.md","kind":"report","origin":"codex"}' --output json
forge mcp call forge.artifact.fetch --input '{"workflow_id":"<workflow-id>","path":"artifacts/<workflow-id>/attached-report-report.md","max_bytes":4096}' --output json
```

The MCP call surface is a stable local adapter layer over the existing Forge CLI and SQLite state. It does not introduce a second source of truth: mutations still flow through Forge-owned workflow, schedule and artifact APIs, validation remains explicit, and artifact reads are bounded to Forge-owned artifact refs.
`forge identity context --project-root . --output json` loads `.forge/operating-context.yaml`, `.yml` or `.json` when present, otherwise returns default local context. `forge plan` and `forge request start` use that project context and load project Addons from `.forge/addons`, so workflow intents, context packets, persona profiles/contracts and event streams carry organization, brand, product, user, channel, memory scope, personality scope, brand identity, design system, operating policy and `tenant_policy_mode`. The default mode is `audit`; setting `tenant_policy_mode: enforce` blocks `forge plan`, `forge request start`, inbound `start_workflow`, context requests, task handoff/leases, request drive/step/status/heartbeat/final-package/switch/cancel/resume/recover, workflow mutations, checkpoints, interactions, schedule execution, patch artifacts and ops modifier proposals until the context is explicit and the active user has a synced current membership for the organization/brand/product scope with the permission required by the action. Membership data can carry `permission_grants`, `permission_denies`, `expires_at`, `not_before` or `valid_from`; denies take precedence over grants.
`forge identity sync --project-root . --output json` persists the current project organization, brand, product, user and channel into `forge.identity_registry.v1` rows in SQLite and creates an active `operator` membership for that organization/brand/product context. `forge identity registry --scope organization --output json` lists the registry, `forge identity memberships --organization <id> --output json` lists `forge.identity_memberships.v1` with role-derived permissions and environments, and `forge identity membership-update --subject <user> --organization <org> --brand <brand> --product <product> --grant workflow:mutate --deny patch:apply --expires-at <rfc3339> --output json` updates grants, denies, role/status and validity windows without raw SQL or manual `data_json` edits. `forge identity link --left-scope telegram --left-id 123 --right-scope user --right-id arthur --output json` persists `forge.identity_link.v1`, `forge identity resolve --scope telegram --id 123 --output json` returns `forge.identity_resolve.v1`, and `forge identity links --status active --output json` returns `forge.identity_links.v1`; tenant policy resolves active identity links before checking memberships, so a channel identity can authorize through its linked governed user while preserving an unlink audit trail. The same surfaces are available through `forge.identity.sync`, `forge.identity.registry`, `forge.identity.memberships`, `forge.identity.membership_update`, `forge.identity.link`, `forge.identity.unlink`, `forge.identity.links` and `forge.identity.resolve`.
`forge interactive identity --project-root . --output json` and MCP `forge.interactive.identity` expose `forge.interactive.identity.v1`, a read-only identity center for TUI/web/agents. It aggregates the current operating context, identity registry records, active channel aliases, memberships and `tenant-audit` status, and it is also available inside `dashboard.identity_panel` plus the Core `ui_composition_panel`.
`forge identity tenant-index --output json` returns `forge.tenant_index.v1`, a physical resource projection for workflows, runs, artifacts and events. It is updated from workflow operating context when workflows, runs, artifacts and events are persisted, and can be filtered by resource type, organization, brand, product and workflow through CLI or MCP.
`forge identity tenant-audit --output json` returns `forge.tenant_audit.v1`, comparing persisted workflows, runs, artifacts and events against `tenant_index`. It exits non-zero when any resource is missing a tenant row, providing a concrete pre-enforcement gate.
`forge identity tenant-policy --workflow <workflow-id> --mode audit|enforce --action "workflow goal update" --output json` returns `forge.tenant_policy.v1`, checking explicit operating context, active membership, the role-derived permission required by the action and tenant-index coverage. In `enforce` mode the CLI exits non-zero when a gate denies the workflow.
`forge plan --goal "<goal>" --addon-dir .forge/addons --output json` now persists `runtime` on the workflow root as `forge.workflow_runtime.v1`, formalizing `ephemeral_workflow` versus `persistent_workflow`, expected lifetime, persistence flags, persistence upgrade support and scale-to-zero policy outside the nested intent. `forge status --workflow <workflow-id> --output json` returns the same root runtime block. `forge list --output json` projects that root contract into `forge.registry_workflow_runtime.v1`, summarizing persistent/ephemeral workflows, scale-to-zero posture and the current operator action for each workflow. Scheduled workflows are projected as `run_due_schedule` or `sleep_until_schedule` instead of event wakeups, so the runtime can rehydrate cron and one-shot `wait_until` work without starting a webhook/inbox worker unnecessarily. Goals that say `wait until <RFC3339>` create a `wait_until` schedule node with an empty cron field, due `next_run_at`, missed-run history and one-shot completion semantics.
`forge events list --workflow <workflow-id> --output json` returns `forge.event_stream.v1`, projecting the legacy SQLite event log into typed envelopes with tenant context, category, severity, origin and correlation ids. Each envelope now includes `forge.event_observability.v1`, a normalized projection of node/task ref, Addon id, duration, retry, wait, context budget/usage/pressure and memory level/scope evidence when those values already exist in the raw payload. The same projection is available through `forge.events.list`.
`forge events timeline --project-root . --organization <organization-id> --limit 50 --after-sequence <cursor> --output json` returns `forge.event_timeline.v1`, reading the append-only `global_events` projection when available and falling back to legacy workflow-event projection for older stores. It includes workflow events and inbound events that exist before a workflow is created, uses the same typed envelope and observability projection as workflow event streams, supports workflow, organization, brand, product, limit and cursor filters, and returns `page.next_cursor`/`page.has_more` for long-running operators and agents. When the project operating context sets `tenant_policy_mode: enforce`, Forge requires `context:read`, applies organization/brand/product filters from the project context when they are omitted, and rejects explicit tenant filters outside that context before returning global event data. The same report is available through `forge.events.timeline` with optional `project_root`.
`forge events observability --project-root . --organization <organization-id> --node <node-ref> --addon <addon-id> --output json` returns `forge.event_observability_index.v1`, a SQLite-materialized normalized index over the global timeline with a legacy derived fallback. It groups event evidence by tenant, workflow, node and Addon, aggregates duration, retry, wait and context-pressure totals, keeps severity/category counts, projects memory level/scope counts and returns a cursor-compatible page of matching envelopes. When the project operating context sets `tenant_policy_mode: enforce`, Forge requires `context:read`, applies organization/brand/product filters from the project context when they are omitted, and rejects explicit tenant filters outside that context before returning observability records. This is the query surface for operators, dashboards and improvement loops. Historical rollups are available through `forge events observability-history` and `forge.events.observability_history` without reparsing raw event payloads. The same report is available through `forge.events.observability` with optional `project_root`.
`forge events observability-history --project-root . --bucket day --group-by addon --output json` returns `forge.event_observability_history.v1`, deriving time-bucketed rollups from the same materialized event observability index. Buckets can be `hour` or `day`, grouping can be `none`, `tenant`, `workflow`, `node` or `addon`, and every bucket carries the same duration, retry, wait, context-pressure and memory summary used by the live index plus a structured `group` object for tenant/workflow/node/Addon grouped views. When the project operating context sets `tenant_policy_mode: enforce`, Forge requires `context:read`, applies organization/brand/product filters from the project context when they are omitted, and rejects explicit tenant filters outside that context before returning historical rollups. The same report is available through `forge.events.observability_history` with optional `project_root`.
`forge events improvement-policy --project-root . --workflow <workflow-id> --output json` returns `forge.event_improvement_policy.v1`, a read-only policy recommendation layer derived from the same normalized observability records. It identifies repeated expensive node execution, retry hotspots, wait hotspots and context-pressure hotspots, then returns priority, reason, recommended policy and inspection commands without mutating workflows automatically. When the project operating context sets `tenant_policy_mode: enforce`, Forge requires `context:read`, applies organization/brand/product filters from the project context when they are omitted, and rejects explicit tenant filters outside that context before deriving recommendations. `forge improve --workflow <workflow-id>` now imports the top event-policy recommendations into its controlled experiment artifact and changelog while keeping `auto_promoted=false` and `promotion_gate=benchmark_and_validation_required`. `forge improve apply-event-policy --workflow <workflow-id> --policy prefer_deterministic_node --apply --approved-by <operator> --output json` returns `forge.improve.event_policy_application.v1`, applying a governed workflow revision only after approval, recording changed fields, rollback snapshots and keeping promotion blocked behind an equivalence/benchmark gate. Applications can target node, Addon or workflow-scoped recommendations and currently cover `prefer_deterministic_node`, `add_validation_or_rework_gate`, `tighten_context_routing` and `supervise_wait_or_external_dependency`. `forge improve benchmark-event-policy --workflow <workflow-id> --policy prefer_deterministic_node --output json` returns `forge.improve.event_policy_benchmark.v1`, validating the latest applied policy against current workflow state, rollback readiness and `validate_workflow` before reporting `promotion_allowed`; it records benchmark evidence but still does not auto-promote. `forge improve promote-event-policy --workflow <workflow-id> --policy prefer_deterministic_node --approved-by <operator> --output json` returns `forge.improve.event_policy_promotion.v1`, accepting only a validated benchmark through explicit approval, recording an idempotent workflow revision/event and keeping `auto_promoted=false`. Thresholds such as `--min-events`, `--min-duration-ms`, `--min-retries`, `--min-context-pressure-bps` and `--min-wait-seconds` keep the policy domain-agnostic and operator-tunable. The same reports are available through `forge.events.improvement_policy` with optional `project_root`, `forge.improve.apply_event_policy`, `forge.improve.benchmark_event_policy` and `forge.improve.promote_event_policy`.
`forge events ingest --project-root . --origin telegram --action start_workflow --input '{"goal":"..."}' --output json` writes `forge.event_ingest.v1` to the global event inbox without requiring a workflow first, storing the project operating context and tenant columns with the inbound event. `forge events inbox --project-root . --output json` returns `forge.event_inbox.v1`; in `tenant_policy_mode: enforce`, it requires `context:read` and filters organization/brand/product before exposing pending events. `forge events route --event <event-id> --project-root . --output json` supports `start_workflow`, `continue_workflow`, `modify_workflow`/`update_goal`, `pause_workflow`, `resume_workflow` and `complete_workflow`. `forge events scan --project-root . --limit 20 --output json` returns `forge.event_worker.v1` and routes a bounded batch of tenant-filtered pending inbox events in one worker pass, marking failed events with worker error evidence instead of looping silently. `forge events worker --project-root . --limit 20 --max-cycles 12 --interval-seconds 300 --idle-exit --stop-file .forge/run/event-worker.stop --output json` returns `forge.event_worker_loop.v1`, repeatedly running the same scan logic for a bounded number of cycles with optional sleep, idle early-exit and cooperative stop-file shutdown that reports `stop_requested` and `stopped_reason`. `forge events service-plan --kind worker|webhook_ingress --output json` returns `forge.event_service_plan.v1`, a plan-only managed service contract with the re-runnable command, lease TTL, heartbeat interval, backoff, cooperative shutdown and health checks, persisted as `event_service_plan` in the global timeline. `forge events service-run --kind worker --stop-file .forge/run/event-worker.stop --output json` returns `forge.event_service_run.v1`: it acquires a persisted `event_services` lease with the project operating context and tenant columns, refreshes heartbeat and lease expiry after each worker cycle, records live health counters, executes the bounded worker loop and saves final service status for `forge events services --project-root .` / MCP `forge.events.services`, including `stopped` when the stop file is observed. `forge events service-run --kind webhook_ingress --stop-file .forge/run/webhook-ingress.stop --output json` uses the same tenant-aware service registry for a bounded HTTP webhook listener, records `webhook_report`, persists progress heartbeats while listening or waiting for requests, saves final request/ingest/route health counters and can route through declared ingress adapter policy; it also stops safely between requests when the stop file is observed. `forge events service-supervise --kind worker --max-runs 12 --backoff-initial-seconds 5 --backoff-max-seconds 300 --stop-file .forge/run/event-supervisor.stop --output json` returns `forge.event_service_supervisor.v1`, a bounded supervisor loop over `service-run` that restarts managed services, applies executable backoff after failed runs, aggregates health, records stop-file shutdown and writes `event_service_supervisor` audit events without replacing the `event_services` registry. `forge events services --project-root . --output json`, `forge events services-recover --project-root . --kind worker --output json`, `runtime-reconcile` and `runtime-daemon` all apply the same organization/brand/product filters and require `context:read` in enforce mode before exposing, recovering or counting service leases. `forge events services-recover --project-root . --kind worker --output json` returns `forge.event_services_recovery.v1`, marking only tenant-visible `running` services whose leases expired as `stale`, preserving their last health payload and adding recovery evidence before recording a global audit event. `forge events runtime-reconcile --project-root . --recover-stale-services --execute --scan-schedules --output json` returns `forge.event_runtime_reconcile.v1`: it can first apply the same stale-service recovery to worker leases, then reads tenant-filtered `forge.registry_workflow_runtime.v1` rows, tenant-filtered pending inbox events and tenant-filtered active `event_services` leases, recommends or optionally executes a bounded worker supervisor for event wakeups/inbox routing, and when schedule scanning is enabled includes `forge.schedule.worker_status.v1` plus `forge.schedule.scan_due.v1` execution evidence for due cron or one-shot `wait_until` work. `forge events runtime-daemon --project-root . --recover-stale-services --execute --scan-schedules --continuous --cycle-retention 100 --stop-file .forge/run/event-runtime-daemon.stop --output json` returns `forge.event_runtime_daemon.v1`, repeatedly running the same tenant-filtered reconciliation with its own persisted `runtime_reconcile` service lease, heartbeat, cooperative shutdown, per-cycle reports, schedule execution counters and global timeline audit; in continuous mode it ignores `max_cycles`, exits by `idle_exit` or stop-file, retains only the configured number of per-cycle reports while preserving aggregate counters, and can run stale-worker recovery before each cycle's service recommendation. `forge events webhook-ingress --host 127.0.0.1 --port 8787 --path /webhook --origin partner_api --action start_workflow --schema partner.event.v1 --route --hmac-secret-env FORGE_WEBHOOK_SECRET --signature-header X-Forge-Signature --max-requests 100 --stop-file .forge/run/webhook-ingress.stop --output json` returns `forge.event_webhook_ingress.v1`: it runs a bounded local HTTP listener, accepts JSON POST bodies, normalizes them into the same tenant-aware inbox, injects `transport: webhook` and optional schema metadata, and can route immediately through the normal adapter policy when `--route` is set. When `--hmac-secret-env` is set, Forge reads the secret from that environment variable, verifies HMAC-SHA256 over the raw body from the configured signature header (`sha256=<hex>` or raw hex), records `auth_verified` and refuses unsigned or mismatched requests before inbox ingestion. Route reports include `adapter_policy` (`forge.event_adapter_policy.v1`): when a declared ingress Addon adapter matches the event origin/transport, Forge enforces adapter actions, schema compatibility, auth evidence and the Addon permission gate before executing the route. `continue_workflow` can attach artifacts, record checkpoints, answer pending human interactions, complete ready run tasks or drive an existing async run using generic payload fields instead of channel-specific handlers. Completion is validation-gated: Forge refuses to mark a workflow completed unless `validate_workflow` is promotable.

Inbound events can include a generic `identity` or `source_identity` object with `scope`, `id` and optional `label`; ingest, inbox, route and worker reports then expose `forge.event_identity_context.v1` with the source identity, the canonical identity resolved through active identity links, identity/link counts and the normal tenant context.

Declared ingress routes record `inbound_event_routed` with `addon_id`, `adapter_id`, `direction`, `transport`, `event_type` and the matched `adapter_policy` in the workflow/global timeline. `forge addons observability` uses that runtime evidence to count consumed Addon events, complementing emitted egress events.
`forge events adapters --transport telegram --output json` returns `forge.addon_event_adapters.v1`, a catalog view of declarative Addon event adapters. The Core Kernel advertises the generic `forge.core.event_inbox` ingress adapter, and Addons can declare transports, directions, origins, actions, event types, schema names, auth mode and required permissions in YAML/JSON without adding channel-specific routing code to Core. Egress adapters can also declare `endpoint`, `allowed_hosts`, `timeout_seconds` and `max_response_bytes`. Each adapter includes `forge.addon_permission_gate.v1` with declared tools, resources, integrations, actions and tenant scopes. The same surface is available through `forge.events.adapters` and can be filtered by Addon id, transport, direction or Addon manifest directory.
`forge events emit --adapter <adapter-id> --event-type <event-type> --action <action> --payload '<json>' --output json` returns `forge.event_egress_emit.v1`, selecting one declared `egress` or `bidirectional` adapter, enforcing action, event type, origin, permission gate and endpoint host allowlist, then POSTing `forge.event_egress_request.v1` to `http`/`webhook` endpoints or dispatching the built-in `telegram` transport. It supports `--dry-run` to validate and build the request without sending. `http://` endpoints use Forge's bounded local HTTP client; `https://` endpoints use controlled `curl` execution with JSON stdin, response hash reporting and `FORGE_EVENT_EGRESS_HTTPS_MODE=simulate` for non-network validation. Adapters declaring `auth: hmac` with `secret_env`/`hmac_secret_env` or a `credential_vault` block sign the raw JSON body with HMAC-SHA256 and send `sha256=<hex>` in `signature_header` (default `X-Forge-Signature`). Adapters declaring `auth: bearer` use `secret_env` or `credential_vault` as a Bearer token in `Authorization` by default, or in `signature_header` when a custom header name is declared. Telegram adapters use `auth: bot_token`, read only the declared env name or credential-vault record, resolve chat from payload or `TELEGRAM_CHAT_ID`/`TELEGRAM_REPORT_CHAT_ID`, and report `telegram://bot_api/sendMessage|sendDocument` without the token value. Reports expose only auth scheme, secret source, env var name, credential-vault contract metadata and header, never the secret value. Each dry-run or delivery writes an `event_egress` record to `global_events`, returns `global_event_id` and appears in `forge events timeline` with the project operating context loaded from `--project-root` (default `.`). Before non-dry-run transport, Forge enforces action `event egress delivery` (`workflow:deliver`) against the target workflow when the payload includes `workflow_id`; otherwise it enforces the project operating context when `tenant_policy_mode: enforce` is active. Successful workflow-bound deliveries then write `forge.event_egress_delivery_evidence.v1`, attach it back to that workflow as `telegram_delivery_record` or `event_egress_delivery`, and return the `workflow_artifact` attachment report so outcome gates can count delivery evidence. WhatsApp/email adapters and richer transport-specific auth are still Addon/runtime work.
`forge addons validate --addon-dir .forge/addons --output json` returns `forge.addon_validation.v1` and exits non-zero when Addon ids, capability ids, required Addon dependencies, dependency `version_req` clauses, required capabilities or permission references from contracts/adapters/views are invalid.
`forge addons package --manifest ./addon.yaml --repository <repo> --channel stable --package-path ./dist/addon.package.json --output json` returns `forge.addon_package.v1`, a deterministic marketplace/distribution contract. It records raw and canonical manifest SHA-256 hashes, package id, repository/channel, install/upgrade/downgrade commands, capability/dependency/permission/contract/view summaries, validation result and detached Ed25519 signature metadata when supplied. Packaging validates the candidate catalog but does not install, fetch, trust or execute the Addon.
`forge addons trust-key --repository <repo> --channel stable --public-key <ed25519-public-key-hex> --output json` records a trusted package signing key in `forge.addon_trust_store.v1`. `forge addons publish-package --package ./dist/addon.package.json --output json` indexes a package in `forge.addon_marketplace.v1`, projecting current trust-policy evidence and preserving the package path as the installable source when no explicit source is supplied. `forge addons fetch-package --source <path|file://|https://...> --expected-sha256 <sha256> --allow-remote --lock .forge/addon-package-lock.json --output json` returns `forge.addon_package_fetch.v1`, copies the package into Forge's local cache, enforces a size limit and optional SHA-256, optionally enforces a package lock, then indexes it through the same marketplace trust policy; HTTP(S) sources require explicit `--allow-remote`. `forge addons sync-registry --source <index.json|file://|https://...> --allow-remote --lock .forge/addon-package-lock.json --output json` returns `forge.addon_registry_sync.v1`, reads a JSON/YAML registry index of package sources, fetches each bounded package into the same cache, optionally enforces the same lock for every fetched package and indexes trusted results. `forge addons package-lock --write .forge/addon-package-lock.json --output json` returns `forge.addon_package_lock.v1`, a reproducible snapshot of indexed package ids, sources, hashes, channels, capabilities and current policy status. `forge addons marketplace --output json` lists indexed packages with current signature/trust status, and `forge addons install-package --package ./dist/addon.package.json --lock .forge/addon-package-lock.json --output json` installs only when the package schema, embedded manifest identity, canonical manifest hash, detached Ed25519 signature, repository/channel trust key and optional lock entry all verify. Lock matches emit `forge.addon_package_lock_enforcement.v1`; mismatches block fetch/sync/install before package promotion. MCP exposes the same surfaces as `forge.addons.trust_key`, `forge.addons.trust_store`, `forge.addons.publish_package`, `forge.addons.fetch_package`, `forge.addons.sync_registry`, `forge.addons.package_lock`, `forge.addons.marketplace` and `forge.addons.install_package`.
Addon manifests can declare `compatibility.forge_version_req`, `api_versions`, `runtimes`, `features`, `platforms` and `migrations`. `forge addons validate`, install, upgrade, downgrade and install-package reject incompatible Forge versions, unsupported API versions/features/runtimes, unsupported platforms, runtime contracts outside declared compatibility and major version changes without a migration plan that includes rollback evidence. `forge addons migration-workflow --from-manifest ./addon-v1.yaml --to-manifest ./addon-v2.yaml --action upgrade --output json` creates and persists `forge.addon_migration_workflow.v1`, a persistent Forge workflow with backup, migration apply, validation, rollback readiness and audit-package tasks. Major lifecycle operations that cross a declared migration boundary now attach the same migration workflow report to `forge.addon_lifecycle.v1`.
`forge addons install --manifest ./addon.yaml --output json` persists an Addon in SQLite and returns `forge.addon_lifecycle.v1`. `forge addons upgrade --manifest ./addon-v2.yaml --output json` and `forge addons downgrade --manifest ./addon-v1.yaml --output json` replace an installed manifest only when the candidate version moves in the requested direction, validates against the active catalog, preserves the existing lifecycle and rebuilds the materialized capability index. `forge addons enable|disable|uninstall <addon-id> --output json` changes the persisted lifecycle. Disabled Addons remain auditable in the registry but no longer provide planning capabilities.
`forge addons permissions --output json` returns `forge.addon_permission_authorizations.v1`. `forge addons authorize-permission --addon <addon-id> --permission <permission-id> --approved-by <human> --output json` persists human approval for permissions declared with `requires_human_approval`; `revoke-permission` revokes it. Install/enable fail when approval is missing, and store-aware catalog loading marks Addons as lifecycle `unauthorized` when human approval is missing, so capability exposure, contracts, adapters and views report the blocked gate consistently.
`forge addons capabilities --output json` returns `forge.addon_capability_index.v1`, a SQLite-materialized index of installed Addon capabilities. The index is rebuilt from installed manifests during install/upgrade/downgrade/enable/disable/list/permission operations and can be filtered through CLI or MCP by Addon id, capability id and lifecycle. `forge addons contracts --type validator --output json` returns `forge.addon_runtime_contracts.v1`, listing declarative planning, replanning, validator, executor and handoff contracts by Addon, capability and lifecycle with permission gates. `forge addons planners --output json` returns `forge.addon_planner_registry.v1`, filtering `planning_strategy` and `replanning_strategy` contracts by Addon, capability, workflow extension and lifecycle; it marks Core-owned first-party builders as `core_builder_registered` and external strategies as `external_planner_registered` with policy status, commands and MCP tools. `forge addons contract-policy --contract <contract-id> --output json` returns `forge.addon_runtime_contract_policy.v1`, evaluating whether matching contracts are ready for safe runtime dispatch or blocked by lifecycle, permission gate, missing runtime or missing entrypoint. `forge addons dispatch-contract --contract <contract-id> --input '<json>' --output json` returns `forge.addon_runtime_contract_dispatch.v1`, persists a queued or blocked dispatch envelope with input, policy and source, and never executes external code inline. `forge addons dispatch-planner --contract <contract-id> --goal "<goal>" --constraint "<constraint>" --context '<json>' --output json` uses the same dispatch ledger but restricts the contract to `planning_strategy`/`replanning_strategy` and writes a standardized `forge.addon_planner_dispatch_input.v1` payload with goal, constraints, optional workflow/task ids, planner metadata and context. `forge addons run-dispatch --dispatch <dispatch-id> --output json` rechecks the current policy before processing: `forge_core_builtin` contracts run only through allow-listed built-ins such as `builtin:echo`, changed/revoked contracts become `blocked`, and external runtimes such as `wasm` or `external_api` become `needs_external_worker` for specialized workers. `forge addons register-worker --worker <id> --runtime wasm --output json` returns `forge.addon_runtime_workers.v1`, registering external workers with status, trust level and metadata; workers with trust `signed|trusted` can declare `signature_scheme: "ed25519"` and `public_key_hex`, and changing runtime, trust level, signature scheme or public key later requires `--rotation-approved-by` plus an optional `--rotation-reason`, preserving a `forge.addon_runtime_worker_rotation_policy.v1` audit entry in worker data. Local executable workers declare `execution_mode: "local_process"`, and API workers declare `execution_mode: "external_api"` with explicit `http://` or `https://` `endpoint`, optional `allowed_hosts`, `allowed_entrypoints`, `allowed_contracts`, `auth: none|bearer|hmac`, `secret_env`/`hmac_secret_env` or `credential_vault`, bounded timeout and response-size limits. `forge addons workers --runtime wasm --status available --output json` lists the registry, and `run-dispatch` includes eligible workers in the external-runtime evidence. `forge addons execute-dispatch --dispatch <id> --worker <worker-id> --output json` runs registered `local_process` or controlled `external_api` workers: Forge claims the dispatch, sends a typed JSON request, injects configured Bearer/HMAC auth without reporting secret values, reads a JSON completion over bounded TCP for HTTP or controlled `curl` for HTTPS, applies the same completion/signature policy, records attestation and stores the result in the dispatch ledger. `FORGE_EXTERNAL_API_WORKER_HTTPS_MODE=simulate` exercises the HTTPS path without network access. `forge addons claim-dispatch --dispatch <id> --worker <worker-id> --output json` moves an external dispatch to `claimed_external_worker` only when the worker is registered, available, runtime-compatible and current policy still allows the contract; the claim stores a worker identity/key snapshot. `forge addons complete-dispatch --dispatch <id> --worker <worker-id> --result '<json>' --signature <ed25519-hex> --output json` records worker completion or failure with result hash, attestation hash, signature status and worker ownership; signed/trusted workers must provide a valid Ed25519 signature over the canonical completion payload using the claim snapshot, so later approved worker key rotation does not rewrite the dispatch identity, and policy revocation during the claim blocks completion. `forge addons dispatches --status queued --output json` lists the same persistent ledger for runtime workers and operators. `forge addons views --surface ops_console --output json` returns `forge.addon_views.v1`, listing UI/TUI/ops-console views declared by Addons for dynamic interface composition with permission gates. `forge addons observability --output json` returns `forge.addon_observability.v1`, a consolidated operational report over Addons, lifecycle status, capability/dependency/permission counts, permission gates, event ingress/egress declarations, runtime consumed/emitted event counts from `global_events`, views, artifact/event types, integrations and runtime dispatch usage. View manifests can now declare `type`, `component`, `route`, `layout`, `data_bindings`, `actions` and arbitrary `props`, so Addons can provide dashboards, widgets, editors, visualizations and specialized tools as a generic UI contract instead of Core-specific panels. Addon manifests also carry `context_providers`, `memory_providers`, `event_adapters` and `runtime_contracts` declarations; the Core Kernel advertises `forge.core.operating_context`, `forge.core.file_memory` and `forge.core.event_inbox`, while external Addons can declare scoped context sections, memory levels, event ingress/egress contracts, runtime contracts or UI views in YAML/JSON without changing Core planning code.
`forge addons execute-planner --addon <addon-id> --contract <contract-id> --worker <worker-id> --goal "<goal>" --output json` and MCP `forge.addons.execute_planner` now execute `planning_strategy` or `replanning_strategy` contracts through the same registered worker boundary, but with a Core reference plan embedded in the dispatch context. The result must return a task graph with ids, titles, dependencies, validation rules and expected outputs; Forge validates the shape, hashes the external and Core plans, compares task ids/titles/dependencies/validation coverage and reports `planning_strategy_equivalence_validated` only when the worker output is replacement-ready. Non-equivalent but valid results remain review-required instead of silently replacing first-party builders.
`forge addons execute-validator --addon <addon-id> --contract <contract-id> --worker <worker-id> --subject <subject> --input '<json>' --context '<json>' --output json` and MCP `forge.addons.execute_validator` execute `validator` contracts through the same registered worker boundary, but with a standardized `forge.addon_validator_dispatch_input.v1` envelope. Worker results are audited as `forge.addon_validator_execution.v1`: Forge validates the decision/result shape, separates schema issues from reported validation issues, records the dispatch/claim/completion evidence and reports `addon_validator_passed`, `addon_validator_failed` or review/blocking states without embedding domain logic in Core.
`forge addons execute-executor --addon <addon-id> --contract <contract-id> --worker <worker-id> --task <task-ref> --input '<json>' --context '<json>' --output json` and MCP `forge.addons.execute_executor` execute `executor` contracts through the same registered worker boundary, but with a standardized `forge.addon_executor_dispatch_input.v1` envelope. Worker results are audited as `forge.addon_executor_execution.v1`: Forge validates the generic status/result shape, counts returned outputs, artifacts and events, separates schema issues from reported execution issues and reports completed/failed/retry/review states without embedding domain execution logic in Core.
`forge addons execute-handoff --addon <addon-id> --contract <contract-id> --worker <worker-id> --handoff <handoff-ref> --input '<json>' --context '<json>' --output json` and MCP `forge.addons.execute_handoff` execute `handoff` contracts through the same registered worker boundary, but with a standardized `forge.addon_handoff_dispatch_input.v1` envelope. Worker results are audited as `forge.addon_handoff_execution.v1`: Forge validates delivered/accepted/failed/follow-up states, requires target and receipt evidence for delivered or accepted handoffs, counts returned artifacts and events, separates schema issues from reported handoff issues and reports delivery states without embedding channel or partner-specific handoff logic in Core.
`forge harness token-headroom --content <payload> --kind log --budget-tokens <n> --persist --output json` and MCP `forge.harness.token_headroom` provide deterministic local-first compression reports for logs, search results, JSON, code and text, including original/compressed hashes, estimated token savings, budget status and a retrieval ref. With `--persist`/`persist=true`, Forge writes the original and compressed payload to SQLite as a reversible local headroom blob; `forge harness retrieve-headroom --ref <retrieval-ref> --include-content --output json` and MCP `forge.harness.retrieve_headroom` expose `forge.harness.headroom_retrieval.v1` for metadata-only or content retrieval. `forge harness wrap-plan --executor codex|claude|gemini|opencode --cmd <arg> --forge-first --output json` and MCP `forge.harness.wrap_plan` produce a Forge-first CLI environment plan, preserving executor-specific needs such as Claude tool search. The generated `launch_command` now carries the resolved `--token-headroom` or `--no-token-headroom` flag, so a copied wrapper command preserves the same effective headroom policy instead of depending on implicit defaults. Operators can set `FORGE_HARNESS_DEFAULT_MODE=forge_first` or project `.forge/harness.json` with `{"default_mode":"forge_first"}` so CLI `wrap-plan`, `install-shims` and `exec` prefer Forge-first without repeating `--forge-first`; `forge harness mode --output json` and MCP `forge.harness.mode` report the effective default, source, project config path/status and precedence before any shim install or CLI execution, and `--observe-only` or MCP `observe_only=true` overrides those defaults for one command. Precedence is observe-only flag, explicit Forge-first flag, env default, project config, then observation mode. Harness reports expose `forge_first_source` (`explicit_flag`, `env_default`, `project_config`, `observe_only_flag`, `default_observe_only`, `mcp_input` or `mcp_default`) and child env includes `FORGE_HARNESS_MODE_SOURCE`, so operators can audit why a brain CLI launched through Forge-first or observation mode. `forge harness install-shims --shim-dir <dir> --executor codex|claude|gemini|opencode --forge-first --output json` and MCP `forge.harness.install_shims` write a Forge-owned PATH shim that delegates to `forge harness exec --execute --allow-exec` while preserving user argv. When `--real-cmd` / `real_cmd` is omitted, Forge resolves the native CLI from `PATH` while excluding the shim directory, records `real_command_source=path_discovery` and avoids recursion through an existing Forge shim; `--real-cmd` remains available as an explicit override. Existing non-Forge files in the shim directory are blocked unless `--force`/`force=true` is used, and Forge-owned shims are updated in place through the `# forge-harness-shim:v1` marker. `forge harness shim-status --shim-dir <dir> --executor <executor> --output json` and MCP `forge.harness.shim_status` expose `forge.harness.shim_status.v1`, auditing whether the shim exists, is Forge-owned, is executable, has PATH precedence, parses its real CLI/store/Forge binary and would recurse before any execution. `forge sync executors --shim-dir <dir>` projects that audit into each executor as `forge.executor_harness_status.v1`, setting `forge_first_ready` and Forge-first shell entrypoints for `forge brains`, `/brains` and `/shells` when the shim is ready. `forge shells --executor <executor> --workflow <workflow-id> --task <task-id> --run <run-id> --output json` and MCP `forge.shell.launch_plan` expose `forge.shell_launch_plan.v1`, a plan-only launch report that selects the Forge-first or native shell entrypoint, carries harness status, lists preflight commands, emits concrete `forge context`, `forge task handoff` and `forge request heartbeat` commands when workflow/task/run are supplied, and repeats handoff/validation gates without starting a child process. With `--record-session` or MCP `forge.shell.record_plan`, Forge stores the same plan as `forge.shell_session_receipt.v1` and a `shell_launch_planned` global event so session intent appears in `forge events timeline`. `forge harness exec --executor <executor> -- <cmd>` and MCP `forge.harness.exec` return `forge.harness.exec_receipt.v1`, resolving the executable, projecting the Forge env overlay, recording workflow/task/run lineage and staying dry-run by default; real child execution requires both `--execute` and `--allow-exec` or MCP `dry_run=false, allow_exec=true`. When a store plus workflow, task or run lineage is present, the receipt also records `forge.harness.exec_event.v1` in the global timeline and returns `event_recorded` plus `global_event_id`. When token headroom is enabled, executed stdout/stderr streams are summarized with hashes, bounded excerpts and persisted `stdout_headroom` / `stderr_headroom` retrieval refs, so a brain can consume compressed output while the original stream remains recoverable through Forge.

Exec receipts also expose top-level `context_budget`, `context_budget_source`, `token_headroom_source` and `require_token_headroom_for_forge_first` alongside the nested wrapper plan, so TUI, MCP and event consumers can audit the applied runtime policy without parsing wrapper internals.
`forge memory policy --output json` returns `forge.memory_policy.v1`, documenting file-first memory, global/organization/project/processing roots, public/internal/private visibility, shareability levels and the business operating model. `forge memory search --workflow <workflow-id> --query "<query>" --memory-level session|short_term|standard|full|admin|none --scope global --scope organization --scope project --scope processing --organization <organization-id> --organization-root <path> --audience public|internal|manager|private --output json` returns `forge.memory_search.v1`; the memory level reduces effective scopes before file reads, organization memory defaults to the workflow organization when `--workflow` is supplied, and results are snippets with path and line ranges rather than whole files. In `tenant_policy_mode: enforce`, explicit scopes outside the workflow `memory_scope` are rejected before file reads. Context packages and executor handoff packets also include `forge.context.memory_policy.v1`, so a brain receives the tenant-bound memory level, allowed scopes, default audience and exact governed search command instead of reading broad memory directly. `forge memory promote --workflow <workflow-id> --from-scope processing --to-scope project|organization|global --source-path <path> --summary "<curated summary>" --approved-by <operator> --reason "<reason>" --output json` returns `forge.memory_promotion.v1` and writes only a curated Markdown summary with source lineage, classification and approval metadata; raw private source memory is not copied, and workflow-bound promotion rejects targets outside the allowed `memory_scope`. Successful promotions are indexed in SQLite with workflow/organization/brand/product/user/channel columns and listed through `forge memory promotions --workflow <workflow-id> --from-scope <scope> --to-scope <scope> --approved-by <operator> --output json` as `forge.memory_promotion_index.v1`; `--workflow` enforces tenant policy and applies physical workflow/tenant filters. `forge memory retention --workflow <workflow-id> --scope processing --scope project --output json` returns `forge.memory_retention.v1`, classifying keep/promote-or-delete/delete-candidate actions without deleting files and defaulting scopes from the workflow when none are supplied. `forge memory cleanup --workflow <workflow-id> --scope processing --mode archive|delete --approved-by <operator> --reason "<reason>" --confirm --output json` returns `forge.memory_cleanup.v1`; dry-run planning is available without approval, while non-dry-run execution only archives/deletes processing files classified as `delete_after_final_packaging`. The same contracts are available through `forge.memory.policy`, `forge.memory.search`, `forge.memory.promote`, `forge.memory.promotions`, `forge.memory.retention` and `forge.memory.cleanup`.
`forge cost ledger --project-root . --workflow <workflow-id> --output json` returns `forge.cost_ledger.v1`, a read-only cost ledger grouped by workflow, node, tenant and detected Addon source. It combines planned task cost estimates with observed executor/event costs and token counts from workflow events, so operators can inspect cost by organization/brand/product and decide where deterministic nodes or Addon policies should replace repeated AI work. When the project operating context sets `tenant_policy_mode: enforce`, Forge requires `context:read`, applies organization/brand/product filters from the project context when they are omitted, and rejects explicit tenant filters outside that context before returning costs. The same report is available through `forge.cost.ledger` with optional `project_root`.
`forge cost materialize --project-root . --workflow <workflow-id> --output json` returns `forge.cost_ledger_index.v1`, writing a normalized SQLite index with one row per planned task cost and one row per observed event cost. The materialized rows carry workflow, tenant, Addon, executor, model-call flags, estimated cost, observed cost and token totals, so dashboards and improvement policies can query cost history without reparsing whole workflow JSON. When the project operating context sets `tenant_policy_mode: enforce`, Forge requires `context:read`, applies organization/brand/product filters from the project context before writing and returning normalized rows, and rejects explicit tenant filters outside that context. The same operation is available through `forge.cost.materialize` with optional `project_root`.
`forge cost incremental --project-root . --after-sequence <global-event-id> --output json` returns `forge.cost_ledger_incremental.v1`, scanning `global_events` after a cursor, deduplicating affected workflow ids and materializing only those workflows into the normalized cost index. When the project operating context sets `tenant_policy_mode: enforce`, Forge requires `context:read`, applies organization/brand/product filters from the project context before scanning events and materializing affected workflows, and rejects explicit tenant filters outside that context. It returns `next_after_sequence` for the next run and avoids recording a self-feeding event. The same operation is available through `forge.cost.incremental` with optional `project_root`.
`forge cost history --project-root . --bucket day --group-by source_kind --output json` returns `forge.cost_ledger_history.v1`, a read-only hour/day rollup over the materialized `cost_ledger_index`. It can group buckets by none, tenant, workflow, source kind, Addon or executor and reuses the normalized planned/observed row summary for dashboards and Cost OS policy loops. When the project operating context sets `tenant_policy_mode: enforce`, Forge requires `context:read`, applies organization/brand/product filters from the project context when they are omitted, and rejects explicit tenant filters outside that context before returning historical rollups. The same report is available through `forge.cost.history` with optional `project_root`.
`forge cost maintain --project-root . --workflow <workflow-id> --bucket day --group-by source_kind --retention-days 31 --output json` returns `forge.cost_ledger_maintenance.v1`, an idempotent maintenance receipt that materializes the normalized index and immediately returns the requested rollup plus a plan-only retention policy. When the project operating context sets `tenant_policy_mode: enforce`, Forge requires `context:read`, applies organization/brand/product filters from the project context before materialization and rollup, and rejects explicit tenant filters outside that context. It is safe for runtime/schedule workers to call periodically; physical pruning is handled separately by the approval-gated `forge cost retention` surface. The same operation is available through `forge.cost.maintain` with optional `project_root`.
`forge cost daemon --project-root . --workflow <workflow-id> --bucket day --group-by workflow --max-cycles 2 --interval-seconds 300 --retention-days 31 --output json` returns `forge.cost_ledger_daemon.v1`, a bounded dedicated Cost OS loop over the same maintenance unit. When the project operating context sets `tenant_policy_mode: enforce`, Forge requires `context:read`, applies organization/brand/product filters from the project context to every cycle and records the cycle event under that tenant. Each cycle records `cost_ledger_daemon_cycle` in the global event timeline with schema, filters, retention plan and summary, making cost maintenance observable without turning the CLI into an unbounded service supervisor. Use external runtime supervision for long-lived service lifetime. The same operation is available through `forge.cost.daemon` with optional `project_root`.
`forge cost retention --project-root . --organization <organization-id> --retention-days 31 --output json` returns `forge.cost_ledger_retention.v1`, listing stale normalized cost rows older than the retention cutoff without deleting them. When the project operating context sets `tenant_policy_mode: enforce`, Forge requires `context:read`, applies organization/brand/product filters from the project context before listing or deleting candidates, and rejects explicit tenant filters outside that context. Physical deletion requires `--apply --approved-by <operator> --reason "<reason>" --confirm`; otherwise the report stays blocked or plan-only. Successful deletion records an audit event and is also available through `forge.cost.retention` with optional `project_root`.
`forge addons resolve --goal "<goal>" --registry-source <index.json|https://...> --output json` and `forge plan --goal "<goal>" --output json` now include `runtime_contracts` and `capability_suggestions` inside `forge.capability_resolution.v1`. Each activation records the Addon, capability, workflow extension, contract type, runtime, entrypoint, inputs, outputs, permissions, constraints and permission gate that should support the resolved capability. Missing capability dependencies are matched against known inactive/unauthorized Addons and returned with the suggested action, lifecycle status, CLI command and MCP tool, such as enabling a disabled Addon or authorizing a required permission. The store-aware resolver used by `forge addons resolve` and MCP `forge.addons.resolve` checks trusted installable packages already indexed in the local marketplace and, when explicit `registry-source`/`registry_sources` are supplied, first runs bounded `sync-registry` and returns the sync evidence in `registry_syncs`. It suggests `forge addons install-package` when a package path is available, or `forge addons fetch-package --allow-remote --expected-sha256 ...` when the trusted package has an HTTP(S) source. `forge plan` remains catalog-driven and does not fetch or install marketplace packages during planning. First-party workflow-extension planners now prefer resolved capability/extension activations and use hard-coded text triggers only for legacy intents without capability-resolution evidence. Generic manifest-driven extension tasks also carry matching runtime contract references in `context_requirements`, making handoff/audit possible before external runtimes are executable.
The local assisted-operations surface is available through `forge ops snapshot --output json` and `forge ops serve --host 127.0.0.1 --port 8765`. Both commands accept `--addon-dir` and default to `.forge/addons`. They expose workflow/run state plus local POST actions for `drive`, `step`, `complete-task`, live `update-goal` and live task/node update, so a human and a separate AI modifier can operate the same Forge state while execution continues. The snapshot consumes the registry runtime projection (`forge.registry_workflow_runtime.v1`) so the Ops console can show which workflows are persistent, ephemeral, scaled to zero, waiting for events or candidates for promotion to persistent runtime. It also consumes `forge.addon_views.v1` for active `ops_console` views, projects `forge.ops.addon_view_renderers.v1` with safe renderer families, data sources, permission status and action risk, and consumes `forge.addon_observability.v1` for Addon lifecycle, permission gates, event flow and dispatch usage. The HTML console renders both raw view contracts and safe renderer cards without executing external component code, includes interaction/runtime state and generic event forms, then renders a separate Addon observability table with enabled/unauthorized counts, queued/blocked/worker dispatch pressure and consumed/emitted events. Renderer client events can be recorded through `/api/addon-renderer/event`, `forge ops renderer-event` or MCP `forge.ops.addon_renderer_event`; `addon_id` disambiguates repeated `view_id` values, and later snapshots project those events back into `forge.ops.addon_view_runtime_state.v1`. The snapshot also includes `improvement_candidates` (`forge.orchestrator_improvement_candidates.v1`), ranking live/degraded workflows by event logs, heartbeats, outcome gaps, parallel-ready handoffs and repetitive deterministic work still using AI, including estimated average AI cost and avoidable cost. It also includes `modifier_lane` (`forge.ops.modifier_lane.v1`), reconstructed from Forge events, where a strategic AI or operator can create pending goal/node proposals and apply them as ordinary revisioned workflow mutations through `/api/modifier/propose-goal`, `/api/modifier/propose-task` and `/api/modifier/apply`. The same ops snapshot now includes `visual_workflows`, rendering tasks/subtasks and `forge.ops.design_surface.v1` details for Forge-owned whiteboards, screens, wireframes, flows, components, documents, design tokens and collaboration events. The web console can mutate this visual workspace through `/api/visual/create-artifact`, `/api/visual/set-tokens`, `/api/visual/patch-token` and `/api/visual/collaboration-event`, keeping the Figma-like design surface in Forge-owned workflow revisions instead of a separate source of truth.
`forge request status`, `forge request drive` and registry rows expose `outcome_status` (`forge.outcome_status.v1`). It classifies declared deliverables as support or user-facing, counts user-facing artifact evidence, shows the next outcome action and requires a `final_completion_audit` artifact before Forge completes workflows whose intent declares final/user-facing deliverables.
`forge request step` and `forge.run.step` let agents advance one ready deterministic task through the same executor-response validation path used by manual handoff responses. AI tasks and tasks with explicit external validation commands still require an executor handoff; Forge does not fake them.
`forge request complete-task` and `forge.run.complete_task` give executors a direct closeout path for ready AI or mixed handoff work: Forge records a replayable execution trace, builds the executor response, validates passing evidence, promotes the task and immediately drives the next action.
`forge request drive` creates this final delivery package automatically when it closes a run. `forge request final-package` and `forge.run.final_package` can also create or refresh it explicitly, attaching Markdown and JSON artifacts that summarize readiness, declared deliverables, outcome status, task state, evidence and remaining gaps. The package reports `ready_for_user`, `in_progress` or `not_ready_for_user` instead of treating a summary as proof of completion.
AWS operations are exposed through `forge aws check`, `forge aws inventory`, `forge aws raw` and matching MCP tools. They delegate to `~/plugins/aws-ops/scripts/aws-ops`, use the configured AWS credential-vault defaults and keep secret resolution plus mutation gating outside Forge.
The registry-level `execution_policy` summary uses schema `forge.registry_execution_policy.v1` and aggregates AI, mixed, deterministic, no-AI, model-call-required, model-call-avoided, local-code and reusable local-code route counts for both the filtered global summary and every workflow row.
The registry also includes compact `context_handoff`, `context_actions` and `context_quality` projections for every workflow and for the filtered global summary, so operators can see ready tasks, missing-context blockers, dependency blockers, routing quality pressure and the workflow-level `quality_action` recommendation without inspecting each task individually.
`forge plan` and `forge request start` report `reuse_candidates` when the registry already contains a compatible reusable deterministic subflow, and persist the best attachable candidate per requested task as a proposed child subflow before duplicating local Python/Node.js work.
`forge inspect <workflow-id>` renders the current DAG as terminal text and also exposes the same graph as structured JSON when `--output json` is used. `--verbose` includes task goals, expected outputs, validation rules, subtasks and proposed child-subflow links. `--task <task-id>` focuses the terminal and JSON inspection on one node while preserving the full workflow task count. Persona-aware nodes are annotated with their node-scoped persona mode, and every inspected node carries the context handoff status and next operational action derived from the same readiness contract used by `forge context --strict`.
`forge interactive home --output json` and MCP `forge.interactive.home` include a task-board panel under `dashboard.task_board_panel` using schema `forge.interactive.task_board.v1`. The same board is available directly through `forge interactive task-board --output json` and MCP `forge.interactive.task_board`. It aggregates workflow lanes with task totals, operable `task_cards`, ready handoffs, checkpoint resume candidates, pending human waits, attached artifact counts and next commands such as `forge inspect`, `forge task handoff`, `forge context`, `forge interaction list` and `forge artifacts`. Each task card carries the task id, title, status, executor, human-interaction state, handoff readiness, context action, checkpoint id/state, next action and direct commands for the operator or external agent.
`forge workflow validate-subflow` turns a proposed child-subflow binding into a revisioned `validated` binding only when the child workflow/task is present and the child flow is scaled to zero.
`forge validate` blocks promotion when a task declares persona routing that is not node-scoped, auditable, source-model backed and gated by `persona_routing_required`; it also blocks promotion while child-subflow bindings remain proposed, non-promotable or missing validation metadata.

Sync local execution engines before Forge uses external CLIs:

```bash
forge sync executors --home "$HOME" --output json
forge sync executors --home "$HOME" --shim-dir "$HOME/.forge/bin" --allow codex --allow opencode --output json
forge executors --output json
```

Forge detects known CLIs, checks whether they appear configured and asks for human authorization when run interactively. A detected CLI is not usable until the policy is explicitly allowed. On this machine, `codex` and `opencode` can be authorized for Forge self-improvement with the second command above.

Sync async run substrates separately:

```bash
forge sync runtimes --home "$HOME" --output json
forge runtimes --output json
```

Forge can detect Docker, Kubernetes and Knative. If Docker and Kubernetes are available but Knative is missing, Forge reports a Knative install suggestion that requires human approval. Forge does not install or mutate infrastructure by itself.

Register LAN or SSH-reachable cluster nodes before scheduling distributed work:

```bash
forge cluster register \
  --node-id lan-linux-ai \
  --name "LAN Linux AI Worker" \
  --endpoint ssh://forge@lan-linux \
  --os linux \
  --arch x86_64 \
  --cpu-cores 16 \
  --memory-gb 64 \
  --software python3 \
  --capability python \
  --python \
  --network-reachable \
  --status online \
  --trust trusted_lan \
  --sandbox local_process_no_network \
  --output json
forge cluster list --output json
forge cluster place --workflow <workflow-id> --task task-009 --output json
forge cluster handoff --workflow <workflow-id> --task task-001 --budget 1200 --ttl-seconds 900 --output json
forge cluster leases --output json
forge cluster leases --node-id lan-linux-ai --output json
```

The cluster registry records reported CPU, memory, OS, GPUs, installed software,
Python/Node/Docker/GPU availability, network reachability, status,
cost/latency/reliability, trust level and sandbox permissions. `forge cluster list`
also returns `forge.cluster_registry.v2` scheduling posture with one
`forge.cluster_node_scheduling.v1` row per registered node. Those rows expose
whether the node is schedulable from local registry policy, active/expired lease
pressure, blockers and explicit `remote_execution_enabled=false` /
`external_mutation_allowed=false` markers. Placement is a read-only policy
decision: Forge can select a node that satisfies deterministic task requirements,
but it does not connect over SSH, execute remote code or mutate external machines.
Placement candidates also expose active node lease counts and penalize busy
eligible nodes, so a compatible idle node is preferred before handoff.
Each placement report includes a `forge.cluster_placement_policy.v1` receipt
with the authorized scope `placement_metadata_only`, explicit no-remote-execution
and no-external-mutation flags, the required trust policy and deterministic
hashes for the requirements and policy. That receipt is the audit boundary before
Forge creates a node lease or any future remote adapter asks for authorization.
`forge cluster handoff` layers that placement decision over the
normal executor handoff contract: it leases the task to the selected node id and
returns `forge.cluster_task_handoff.v1` with the placement report, executor handoff
packet, node-scoped lease ref and `forge.cluster_sync_manifest.v1`. The sync
manifest carries context, checkpoint, artifact and shard hashes plus a
deterministic `manifest_sha256`, so a future distributed adapter can copy or
verify only content-addressed inputs after an explicit remote-execution policy
exists. `forge cluster leases` provides the
read-only audit surface for those node-scoped leases, including active/expired
state, workflow/task identity, trust level, sandbox permissions and explicit
`remote_execution_enabled=false` / `external_mutation_allowed=false` markers.

Runtime resources are scope-guarded:

```bash
forge runtime guard --substrate knative --resource service/forge-node --namespace forge --action update --owner forge --output json
forge runtime guard --substrate knative --resource service/existing-api --namespace default --action update --owner external --output json
```

Forge may update/delete resources it created. External resources require explicit human authorization, even when the substrate is available.

Workflows can be changed while running:

```bash
forge workflow update-goal --workflow <workflow-id> --goal "new goal" --origin codex --output json
forge workflow attach-artifact --workflow <workflow-id> --path ./report.md --kind report --origin opencode --output json
```

This is how Codex/OpenCode act as the human interface for Forge: the CLI session can update goals, attach artifacts and keep a revision trail without bypassing Forge's persistent runtime state.
Goal updates reprocess the workflow intent while preserving the workflow's operating context, returning the added/removed deliverables and capability ids so operators can verify that outcome gates changed with the goal.

Run Forge self-evolution:

```bash
forge self run \
  --repo /home/arthur/projects/forge-core \
  --until 2026-05-25T10:00:00-03:00 \
  --executor opencode \
  --fallback-executor codex \
  --mode balanced \
  --max-cycles 1 \
  --output json
```

`forge self run` creates a run id and workflow id, writes prompt/report artifacts for every cycle, runs validation before committing, and only pushes when `--push` is passed. `--executor` defines the primary executor schedule; `--fallback-executor` defines the ordered recovery chain for failed executor attempts without stopping the cycle.
Each self-evolution cycle report includes the prompt packet version and SHA-256 checksum so executor runs can be replayed and audited against the exact instructions given to Codex/OpenCode. Prompt packet `forge.self_evolution.prompt.v2` includes the current persisted workflow goal, the initial goal and the workflow revision before the generic strategic backlog, so human goal mutations such as clusterization or n8n node research are carried into subsequent self-evolution runs.
The self-evolution report also includes `forge.self_evolution.overhead_ledger.v1` and `forge.self_evolution.decision_gate.v1`. `--mode lean` rejects governance-heavy cycles when expected value is below orchestration cost; `balanced` is the default; `strict` allows more overhead only for audit, safety or distributed-execution needs. When the persisted terminal goal already has the mode boundary, ledger and decision gate, Forge returns `terminal_goal_reached` and creates no new cycle prompts.

Example autonomous mixed objective:

```bash
forge plan --goal "Execute research now, continue every Friday at 09:00, calculate costs without AI, and email the final workflow cost to finance@example.com" --output json
```

When a goal mentions n8n research, `forge plan` adds a research catalog task and
a separate Forge primitive evaluation task. The atomic graph build depends on
that recommendation, so concepts such as loop-over-items, IF/Switch routing,
Merge, Wait, Code, Execute Sub-workflow, triggers, retries, errors, transforms
and human approval patterns stay outside native Forge semantics until they
improve validated DAG execution, context routing, resumability, observability or
operator clarity.

## Skill Install

Install the Forge Core skill for Codex and OpenCode:

```bash
forge skill install --target codex --target opencode --output json --home "$HOME"
```

The installer writes:

- Codex: `~/.codex/skills/forge-core/SKILL.md`
- OpenCode: `~/.config/opencode/skills/forge-core/SKILL.md`
- Shared agent-compatible path: `~/.agents/skills/forge-core/SKILL.md`

The repository also includes project-local skill definitions:

- `.agents/skills/forge-core/SKILL.md`
- `.opencode/skills/forge-core/SKILL.md`
- `skills/forge-core/SKILL.md`

## Validation

Run the full local gate:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

The current test suite validates:

- planning creates a persistent atomic graph;
- validation blocks promotion until tasks are complete;
- context packages stay task-local, budget-bounded, executor-profiled, versioned and sharded;
- strict context mode blocks executor handoff when required sections are omitted;
- controlled improvement never auto-promotes without validation;
- artifact listing returns SHA-256 hashed outputs;
- workflow registry listing preserves initial requests and lifecycle state;
- workflow inspection renders terminal DAGs with dependency, lifecycle, persona and context next-action annotations;
- context routing carries proposed child-subflow bindings for reusable deterministic nodes;
- simulated execution can complete the graph and unlock validation;
- skill installation works for Codex and OpenCode paths.

## Self-Improvement Model

Forge Core does not perform unrestricted self-modification.

The current loop is:

```text
rank live improvement candidates
→ inspect run/event/outcome/parallelization/cost evidence
→ recover stale or attention-needed runs when needed
→ parallelize ready independent handoffs when useful
→ replace repetitive deterministic AI work with command nodes or reusable subflows
execute workflow
→ collect validation state
→ generate improvement experiment artifact
→ benchmark and validate externally
→ promote only when validation passes
```

`forge improve candidates` returns `forge.orchestrator_improvement_candidates.v1`, ranking workflows by stale/missing heartbeats, failed/blocked tasks, rework events, outcome gaps, final package gaps, parallel-ready task sets and avoidable AI cost. The `cost_efficiency` block reports AI task count, repetitive/deterministic AI task count, estimated total/average AI cost, observed total/average AI cost when events include it and avoidable estimated cost.

`forge improve` generates a controlled experiment artifact and keeps `auto_promoted=false`.
`forge improve apply-event-policy` can convert selected event-policy recommendations into approved workflow revisions with rollback metadata and an equivalence gate. It resolves node-scoped recommendations to one task, Addon-scoped recommendations to all observed workflow tasks for that Addon and workflow-scoped recommendations to all tasks in the workflow. `forge improve benchmark-event-policy` then validates the latest applied policy against the current workflow, rollback readiness and workflow validation, producing promotion evidence without changing revisions or auto-promoting. `forge improve promote-event-policy` accepts only that validated benchmark with explicit `--approved-by`, records a second governed revision/event and is idempotent for repeated calls against the same benchmark. The flow covers deterministic node conversion, validation/rework gates, tighter context routing and supervised waits; promotion remains human-approved and benchmark-gated.

Every improvement can target a version and generates a Markdown changelog:

```bash
forge improve candidates --output json
forge improve --workflow <workflow-id> --target-version 0.3.0 --output json
forge improve apply-event-policy --workflow <workflow-id> --policy prefer_deterministic_node --apply --approved-by <operator> --output json
forge improve apply-event-policy --workflow <workflow-id> --policy tighten_context_routing --apply --approved-by <operator> --output json
forge improve benchmark-event-policy --workflow <workflow-id> --policy prefer_deterministic_node --origin <operator> --output json
forge improve promote-event-policy --workflow <workflow-id> --policy prefer_deterministic_node --approved-by <operator> --origin <operator> --output json
```

Current structural improvement domains:

- task structure: backlog state, subtasks, impediments, owner role and acceptance criteria;
- prompt system: versioned prompt/task packets that can be benchmarked and rolled back;
- process runtime: Scrum/SAFe-style blocked work and promotion readiness;
- validation governance: goals must be definitively ready before promotion;
- executor policy: installed/configured CLIs require saved human authorization;
- runtime substrates: Docker/Kubernetes/Knative require authorization and resource ownership checks;
- runtime mutation: goals/artifacts can change while running with origin trace and revisions.
- async request handoff: skill callers receive a `run_id` and do not need to wait for the full run.
- context routing: proposed child-subflow reuse decisions are included in bounded task context.

## Evolution Direction

Forge should evolve as an operational kernel for agentic systems, not as a subordinate extension of a single agent CLI.

The practical path still includes close CLI coupling where it helps adoption:

- Codex/OpenCode/Gemini invoke `forge plan`, `forge context`, `forge run`, `forge validate` and `forge artifacts` from inside their normal workflows.
- Forge invokes Codex/OpenCode/Gemini/Claude/Ollama adapters for bounded tasks using a strict task packet with allowed context, expected output and validation rules.
- Open-source CLIs can receive deeper native integration over time so their interactive experience can be backed by Forge's persistent workflow runtime.

See [docs/evolution-roadmap.md](docs/evolution-roadmap.md) for the planned integration path.

## Project Scope

This release intentionally does not implement:

- SaaS frontend;
- full provider execution adapters;
- WASM plugin runtime;
- distributed execution;
- unrestricted autonomous code mutation.

The current focus is the portable runtime contract: decomposition, persistence, context minimization, validation, artifacts and controlled improvement.
