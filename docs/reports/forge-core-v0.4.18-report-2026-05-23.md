# Forge Core v0.4.18 Report - 2026-05-23

## Objective

Advance workflow composition and deterministic execution policy with a small structural increment: expose reusable local code-node subflows in the Forge registry and report compatible candidates during planning before creating duplicate deterministic work.

## Change

`forge list` now projects reusable deterministic subflows from persisted workflows. A task becomes reusable when its execution policy is a repeated/frequent `local_code_node` with no AI allowance. Each registry row includes:

- task id and title;
- executor and policy mode;
- reuse hint;
- compatibility key based on policy mode, language, entrypoint and validation gate;
- context lineage SHA-256 derived from task-local context requirements and validation rules;
- language, entrypoint, validation gate and lifecycle state.

`forge plan` now checks the existing registry before saving the new workflow. When a new deterministic code node has the same compatibility key and context lineage hash as an existing reusable subflow, the plan response includes a `reuse_candidates` entry with the requested task, candidate workflow/task and whether it is attachable as a child subflow.

## Safety

This is registry and planning metadata only. Forge does not execute local Python or Node.js code during planning, does not authorize external CLIs, does not mutate Docker/Kubernetes/Knative resources and does not automatically attach child subflows. Candidate attachment is explicitly reserved for a later validated cycle.

Candidates are marked attachable only when the existing workflow lifecycle is `idle`, `completed` or `scaled_to_zero`.

## Validation

Fresh validation run in this cycle:

- `cargo fmt --check`: passed;
- `cargo clippy --all-targets --all-features -- -D warnings`: passed;
- `cargo test`: passed, 43 CLI contract tests;
- `cargo build --release`: passed;
- CLI smoke `./target/release/forge --store /tmp/forge-core-v0418-plan-smoke.sqlite plan --goal "Create a delivery platform" --output json`: passed;
- CLI smoke `./target/release/forge --store /tmp/forge-core-v0418-skill-smoke.sqlite skill install --target codex --target opencode --output json --home /tmp/forge-skill-smoke-v0418`: passed.

New focused tests added before implementation:

- `list_surfaces_reusable_code_node_subflows_with_compatibility_keys`
- `plan_reports_compatible_reuse_candidates_before_creating_duplicate_code_nodes`

## Installation And Publication

- `cargo install --path . --force`: blocked because `/home/arthur/.cargo/.crates.toml` is on a read-only filesystem in this execution environment.
- `cargo install --path . --force --root .forge/local-install --offline`: passed.
- `.forge/local-install/bin/forge --version`: `forge 0.4.18`.
- `git add ...`: blocked because Git could not create `.git/index.lock` on a read-only filesystem.
- `gh auth token`: passed locally; token value was not recorded.
- `git remote get-url origin`: `https://github.com/cardozoarthur/forge-core.git`.
- `git push`: blocked because this environment could not resolve `github.com`.

## Next Recommended Cycle

Use the validated reuse candidates to persist explicit child-subflow references on the requested task, then render those recursive links in `forge inspect --verbose` without automatically promoting or executing reused subflows.
