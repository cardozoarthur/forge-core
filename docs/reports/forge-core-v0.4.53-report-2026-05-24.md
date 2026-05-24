# Forge Core v0.4.53 Report - 2026-05-24

## Summary

Forge Core now applies workflow-registry reuse during async request creation. `forge request start` consults existing workflows for compatible deterministic local code-node subflows, attaches the best attachable candidate as a proposed child subflow before saving the new workflow, and returns `reuse_candidates` plus `attached_subflows` in the request-start JSON response.

This closes a planning/runtime gap: direct `forge plan` already avoided duplicate compatible code nodes, but skill-style Codex/OpenCode flows entered through `forge request start` did not.

## Behavior

- `forge request start --output json` still returns `status`, `run_id`, `workflow_id`, `goal`, `origin` and `async`.
- It now also returns the registry-derived `reuse_candidates` considered for the new workflow.
- It returns `attached_subflows`, the number of proposed child-subflow bindings persisted before the async run record is saved.
- The persisted workflow can be inspected immediately with `forge inspect <workflow-id> --output json`, and proposed child subflows appear in `subflows`, node `subflow_refs` and the terminal diagram.

## Validation Notes

- RED: `cargo test request_start_reuses_compatible_subflows_before_persisting_async_workflow` first failed because `attached_subflows` was absent from the request-start response.
- GREEN: the same test passed after routing `start_async_request` through `find_reuse_candidates` and `attach_reuse_candidates_as_child_subflows`.

## Safety

- The change is read/write only against Forge-owned SQLite workflow/run state.
- Candidate selection remains deterministic and uses the existing lifecycle, reuse key and context-lineage checks.
- Forge does not execute local Python/Node.js code during request creation.
- Forge does not promote reused subflows automatically; bindings remain `proposed` and visible to inspection, context routing and validation gates.
- No Docker, Kubernetes or Knative resources are installed or mutated.
