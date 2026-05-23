# Forge Core v0.4.16 Report - 2026-05-23

## Objective

Advance the Context Routing Engine with executor-aware context profiles so deterministic no-AI nodes receive smaller, validation-first context envelopes while AI nodes keep richer reasoning context.

## Change

`forge context` now emits schema `forge.context.v5` with routing policy `task_local_revisioned_persona_compressed_executor_profile_budget_v5`.

Each context packet now includes:

- `executor_profile`: executor kind, profile id, deterministic flag, reasoning allowance, max profile budget and allowed sections;
- `requested_budget` and `effective_budget`: the caller's budget and the profile-capped budget actually used for routing;
- `profile_omitted_sections`: sections intentionally excluded by executor profile rather than by byte pressure;
- `shards[].profile_excluded`: per-shard audit signal for profile-based omissions.

Deterministic `command` and `wait` nodes use the `no_ai_deterministic` profile. That profile prioritizes local objective, validation rules, task context requirements and dependencies before lower-priority narrative context. Notification nodes use a smaller deterministic profile that still allows persona routing. AI and mixed nodes keep reasoning-oriented profiles.

## Safety

This change is local to context packet construction. It does not mutate workflow state, authorize CLIs, change executor policy, alter validation gates or touch Docker, Kubernetes, Knative or other external resources. Profile decisions are visible in the packet and shard manifest so omitted context can be audited.

## Validation

Fresh validation run in this cycle:

- `cargo fmt --check`: passed;
- `cargo clippy --all-targets --all-features -- -D warnings`: passed;
- `cargo test`: passed, 40 CLI contract tests;
- `cargo build --release`: passed;
- CLI smoke `forge plan --goal "Create a delivery platform" --output json`: passed with the release binary on `PATH`;
- CLI smoke `forge skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke`: passed with the release binary on `PATH`.

Post-validation local installation:

- `cargo install --path . --force`: blocked because `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem in this execution environment;
- `cargo install --path . --force --root .forge/local-install --offline`: passed;
- `.forge/local-install/bin/forge --version`: `forge 0.4.16`.

Publication attempt:

- `git commit -m "feat: add executor-aware context profiles"`: blocked because Git could not create `.git/index.lock` on the read-only Git metadata path;
- `gh auth token`: passed locally, token value not logged;
- `git remote get-url origin`: `https://github.com/cardozoarthur/forge-core.git`;
- `git push`: blocked because this environment could not resolve `github.com`.

## Next Recommended Cycle

Add deterministic Python/Node.js code-node contracts and execution-policy selection so Forge can explicitly choose reusable local code nodes for frequent work instead of routing those nodes through model executors.
