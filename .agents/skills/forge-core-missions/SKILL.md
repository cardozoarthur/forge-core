---
name: forge-core-missions
description: Operate Forge missions through strict context, receipts, reconciliation, and validation.
license: MIT
compatibility: codex, opencode, agy, claude
metadata:
  runtime: rust
  cli: forge
---

## Mission Contract

Mission/workflow share IDs, revisions, evidence, and gates; Forge is
authoritative. Never edit SQLite. A 40/40 simulation is not production evidence.

## Required Operator Loop

Start on an existing Git worktree; retain mission/workflow IDs and the first
projected `mission.tasks[].id`:

```bash
forge mission start --goal "<objective>" --squad software-factory \
  --worktree <absolute-worktree> --output json
```

Request strict context while the projected task is still pending. Only after it
returns `handoff_ready=true`, `guardrail.status=ready`, call `drive`; require
its assignment task ID to match and retain its agent ID:

```bash
forge context --workflow <workflow-id> --task <task-id> \
  --project-root <absolute-worktree> --budget 4096 \
  --strict --view compact --output json
forge mission drive <mission-id> --output json
forge mission execute <mission-id> --task <task-id> --agent <agent-id> \
  --idempotency-key <unique-execution-key> --purpose test \
  --approved-by <operator> --evidence <required-kind> \
  --command <executable> --command <argument> --output json
forge mission submit <mission-id> --task <task-id> --agent <agent-id> \
  --idempotency-key <unique-submission-key> \
  --receipt-id <execution-receipt-id> \
  --summary "<validated delivery>" --output json
forge mission resume <mission-id> --output json
```

After `resume`, inspect the next pending task, then repeat
`context -> drive -> execute -> submit -> resume`. `resume` returns
`handoff_consumed`, `repair_created`, or `mission_completed`. Then:

```bash
forge mission inspect <mission-id> --output json
forge validate --workflow <workflow-id> --output json
```

Require mission `completed`, validation `passed`, `promotable=true`, no rework.

## Input and Output Contracts

- `start`: non-empty goal, valid squad/version, worktree.
  `forge.mission.start.v1` contains the `forge.mission.v1` record.
- `context --strict`: workflow/task, root, budget. Honor its selected/deferred
  sources, expansion commands, and guardrail. Call it before `drive`.
- `drive`: mission ID. `forge.mission.drive.v1` returns action, revision,
  assignment/handoff, and mission. Use its exact identity.
- `execute`: identity, unique key, purpose, command arguments, typed evidence,
  approval unless `--dry-run`. Require `persisted=true`;
  receipt v3 binds revision, hashes, policy, evidence, sandbox, exit, use.
- `submit`: same identity, completed unused receipt, new key, summary. Submit
  v1 returns queued handoff/inbox, revision, deduplication, acceptance.
  Receipt evidence is authoritative.
- `resume`: consumes inbox and projects workflow state transactionally.
- `inspect`: mission returns `forge.mission.v1`; execution returns ledger state:

```bash
forge mission execution list --mission <mission-id> --output json
forge mission execution inspect <receipt-id> --output json
```

Idempotency replays identical requests only.

## Failure and Recovery

- Context blocked: follow only `guardrail.next_commands`; request it again.
- Failed/timed-out/indeterminate: inspect. Retry only after independent
  no-effect proof and:

```bash
forge mission execution reconcile <receipt-id> \
  --outcome no_effect_retry --approved-by <operator> \
  --reason "<independent no-effect evidence>" \
  --confirm-no-effect-retry --output json
```

  Reconciliation v1 cannot rewrite completed/consumed receipts.
- Stale revision: inspect, request strict context, drive a fresh assignment.
- `repair_created`: inspect gates, risks, inbox/events; repair and resume.
- Lease conflict, dead letter, divergent history: stop, inspect both ledgers,
  preserve evidence, recover only through Forge commands.
