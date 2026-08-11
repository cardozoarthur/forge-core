# Value-aware research contracts

Foundry can persist the measurements needed to study value-aware,
deterministic-first orchestration without claiming that an experimental policy
is already better than the current runtime.

## Goal and boundary

Version 1 is a strictly observational kernel:

- cost and duration use different fields and units;
- a workflow can declare value, delay, opportunity, failure, quality and hard
  constraints;
- G0 through G4 decisions can be recorded with predictions, a
  `decision_point`, policy lineage and evidence;
- observed, simulated and estimated outcomes remain distinguishable;
- an experimental arm can be registered before execution;
- the assignment freezes SHA-256 fingerprints of the workflow protocol, value
  contract and protocol-relevant task definitions while live task status and
  impediments remain mutable;
- an export reports incomplete or unverified evidence instead of inventing
  values or causal claims.

Every v1 gate receipt must have `applied=false`. Recording it never selects an
executor, stops a task, promotes a workflow or mutates another control-plane
field. A future typed control-plane consumer would need its own authorization,
revision and effect receipt before an observed decision could be applied.

Foundry remains the orchestration authority. Addons, model providers, skills
and external services cannot become workflow-state authorities through these
contracts. `research-agent`, arXiv and OpenAlex are future entries for a
governed external-capability catalog; they are not cognitive executors and are
not silently registered as such by this feature.

## Public contracts

| Concern | Contract | Schema |
|---|---|---|
| Value and accounting boundary | `Workflow.value_contract` | `foundry.value_contract.v1` |
| Experimental assignment | `Workflow.experiment` | `foundry.experiment_assignment.v1` |
| Gate prediction and decision | append-only research record | `foundry.value_gate_decision.v1` |
| Process, artifact and in-use outcome | append-only research record | `foundry.outcome_contract.v1` |
| Analysis snapshot | `workflow export-research` | `foundry.research_export.v1` |
| Parallel schedule | simulated run output | `foundry.scheduler.parallel_plan.v2` |

The CLI input definitions are formalized in
[schemas/value-research-v1.schema.json](schemas/value-research-v1.schema.json).
Rust validation is authoritative and adds workflow, ledger and runtime
cross-record invariants that JSON Schema cannot express.

### Assignment and endpoints

`assignment_method` is a closed enum:

- `deterministic`;
- `randomized`;
- `paired`;
- `stratified`.

Randomized, paired and stratified assignments require both `seed` and at least
one `assignment_evidence_refs` entry. These are externally supplied assignment
claims: v1 preserves their lineage but does not independently prove that
randomization or pairing occurred.

Primary and secondary endpoints use the closed `OutcomeMetric` vocabulary:

`process_quality_bps`, `artifact_quality_bps`, `quality_in_use_bps`,
`direct_cost`, `process_cost`, `assurance_cost`, `internal_failure_cost`,
`external_failure_cost`, `delay_cost`, `opportunity_cost`, `realized_value`,
`service_time_ms`, `queue_time_ms`, `wait_time_ms`, `human_time_ms`,
`capacity_units`, `escaped_defect` and `accepted`.

Every registered endpoint must be present in a linked outcome and have a
non-empty `metric_provenance` entry.

### Gate meanings and scope

| Gate | Scope | Question | Core decisions |
|---|---|---|---|
| G0 | workflow/cohort | Is the case worth admitting under value, time, risk and capacity constraints? | `admit`, `defer`, `negotiate`, `reject`, `abstained_missing_contract` |
| G1 | task execution | Does the operation need generative inference? | `deterministic`, `generative`, `mixed`, `abstain` |
| G2 | task execution | Which capable resource should be selected? | `select`, `abstain`, `fallback` |
| G3 | task execution | Which assurance tier is justified by the consequence? | `a0`, `a1`, `a2`, `a3`, `abstain`, `abstained_missing_contract` |
| G4 | task execution | Is more computation worth its marginal cost and delay? | `continue`, `stop`, `escalate`, `abstained_missing_contract` |

G0 must omit `task_id`, `run_id`, `lease_id` and `input_hash`. Experiment-linked
G1 through G4 receipts require all four fields. A linked outcome must use the
same task, run, lease and input SHA-256 as its G1 through G4 receipts.

Each gate input also requires a non-empty `decision_point`. This distinguishes
legitimate iterative decisions, such as multiple G4 evaluations. A declared
trace is not terminal merely because it contains G4 `continue`; G4 contributes
to trace completeness only with `stop`, `escalate` or
`abstained_missing_contract`.

`custom:<policy-decision>` is reserved for versioned Addon policies and requires
`policy.source=addon` plus evidence. All decisions, including custom ones,
remain observational in v1.

## Outcome contract

`measurement_status` is one of `observed`, `simulated` or `estimated`. The
separate outcome `status` is one of `accepted`, `rejected`, `partial`,
`modeled` or `inconclusive`.

An experiment-linked outcome requires its cohort, run, lease, input hash and
full `evaluated_policy` reference. Simulated and estimated outcomes must not
claim an execution receipt or an executed policy.

An observed outcome additionally requires:

- `task_id`, `run_id`, `lease_id`, `input_hash` and `output_hash`;
- `execution_receipt_sha256`;
- at least one evidence reference;
- `reported_executed_policy`.

The reported policy is deliberately named `reported_executed_policy`: v1 can
check the persisted Foundry run, finished runtime claim, sealed execution
receipt, frozen protocol correlation and the append-only canonical dispatch
permit issued by `request-executor-wave`. Generic workflow events are
observability records, not evidence authority, and cannot substitute for that
permit. The observed `input_hash` must match the sealed request and
`output_hash` must equal `receipt.stdout.sha256`; an artifact-only match is not
runtime-verified in v1 because it lacks authoritative run/task/lease output
lineage. Foundry still cannot prove from a caller-supplied policy label which
policy semantics actually ran. For a non-shadow assignment the reported policy
must match the evaluated assignment; for shadow or holdout telemetry it must
identify a different executed policy.

One sealed runtime receipt can back at most one observed outcome contract in
v1. Later quality-in-use or longitudinal measurements need a separate
measurement execution and receipt; they must not overwrite or duplicate the
original observation.

## Required invariants

1. USD is never used as time. `estimated_duration_ms` is optional, and missing
   duration makes latency and `latency_reduction_bps` unavailable.
2. A terminal value that already incorporates delay, failure or opportunity
   cannot also declare or record the same loss separately.
3. Monetary values name their currency. Realized value names its valuation
   method version.
4. Opportunity cost names a counterfactual, method version and evidence.
5. Probabilities and quality scores use basis points in `0..=10000`.
6. G2 `select` names one of its declared candidates. Monetary predictions name
   their currency.
7. Experiment id, arm, cohort, seed and evaluated policy match the frozen
   assignment.
8. Gate inputs are observational: `applied=true` is rejected for every
   assignment mode, not only shadow mode.
9. Idempotency keys return an existing receipt only for an exact replay.
   Reusing a key with different content fails.
10. A different idempotency key cannot duplicate the same gate
    `decision_point` or the same outcome observation.
11. Enrollment occurs before exposure: the workflow and every task remain
    pending, durations are explicit and no task lease or prior executor-runtime
    claim may exist.
12. Value contracts, duration estimates and assignments are frozen after
    enrollment. Later protocol-definition drift makes research readiness fail
    closed without taking authority over live workflow operation.

## CLI workflow

Create the workflow first and retain its control revision:

```bash
foundry plan --goal "Evaluate a value-aware delivery policy" --output json
```

Then apply JSON specs:

```bash
foundry workflow set-value-contract \
  --workflow <workflow-id> \
  --spec value-contract.json \
  --expected-revision <control-revision> \
  --origin codex \
  --output json

foundry workflow set-task-duration \
  --workflow <workflow-id> \
  --task <task-id> \
  --duration-ms <milliseconds> \
  --expected-revision <control-revision> \
  --origin codex \
  --output json

foundry workflow set-experiment \
  --workflow <workflow-id> \
  --spec experiment.json \
  --expected-revision <control-revision> \
  --origin codex \
  --output json

foundry workflow record-gate-decision \
  --workflow <workflow-id> \
  --spec gate-decision.json \
  --expected-revision <control-revision> \
  --origin codex \
  --output json

foundry workflow record-outcome \
  --workflow <workflow-id> \
  --spec outcome.json \
  --expected-revision <control-revision> \
  --origin codex \
  --output json

foundry workflow export-research \
  --workflow <workflow-id> \
  --output json
```

Setting the value contract, task durations and experiment assignment mutates
the operational workflow, advances its control revision and emits the
corresponding workflow event. Gate and outcome writes do not advance that
revision and do not emit generic workflow events. They append Foundry-owned
ids, timestamps, payload hashes and monotonically increasing
`research_revision` values to the dedicated research ledger.

Manual CLI gate recording may be retrospective. It preserves a declared
decision trace but does not prove that the gate was captured before the
corresponding action. This is why it cannot make
`prospective_gate_capture_ready` true.

## SQLite persistence and loading

`workflow_research_records` is the authoritative SQLite store for gate and
outcome telemetry. Database triggers reject updates and deletes, foreign keys
protect workflow lineage, payload SHA-256 is checked on hydration, and
revisions must be contiguous. The ledger is bounded to 1,024 records per
workflow and 64 KiB per serialized payload.

`executor_runtime_dispatch_permits` is the separate append-only authority for
canonical request-wave dispatch lineage. Its immutable permit binds workflow,
run, wave, task, lease, protocol, context, executor authorization and prompt
hashes before the runtime receipt can qualify as observed evidence. The generic
event stream may mirror this activity for operators, but is never consulted as
the authenticity boundary.

Operational workflow JSON intentionally excludes the research arrays:

- `FoundryStore::load_workflow` and normal workflow listings return the
  operational state with `gate_decisions`, `outcomes` and
  `research_revisions` empty;
- `FoundryStore::load_workflow_with_research` hydrates those projections from
  the ledger in a consistent read transaction;
- `workflow export-research` uses the hydrated view and validates it before
  returning data.

This lazy boundary prevents high-churn telemetry from contaminating ordinary
workflow saves. If the research ledger is missing or corrupt, research append,
hydration and export fail closed; normal operational loading remains available.

Attach durable reports or datasets separately with `workflow attach-artifact`
so normal artifact hashes, tags and lineage remain available. Evidence
references in research records point to those artifacts or another explicit
provenance source; they do not embed an unbounded event warehouse.

## Research-readiness interpretation

`export-research` reports five intentionally separate claims:

| Field | Meaning in v1 |
|---|---|
| `declared_trace_complete` | A valid frozen contract and assignment, explicit task durations, all five linked gates including terminal G4, and at least one linked outcome are present. |
| `runtime_evidence_verified` | The declared trace includes an observed outcome whose sealed Foundry runtime evidence passed verification. Simulated or estimated outcomes never satisfy it. |
| `prospective_gate_capture_ready` | Always `false`; manually recorded receipts do not prove pre-action capture. |
| `executed_policy_verified` | Always `false`; `reported_executed_policy` remains a report, not a cryptographic proof of policy semantics. |
| `causal_claim_ready` | Always `false`; the v1 ledger does not establish a causal effect. |

The top-level readiness `status` is `trace_incomplete`,
`declared_trace_complete` or `runtime_evidence_verified`. Missing or
disconnected requirements are listed in `readiness.missing`; missing numeric
measurements are never converted to zero.

Scheduler duration is a dependency- and `max_parallel_tasks`-bounded estimate.
The planner adds no runtime queue, lane, quota or external-service wait unless
the caller included it in `estimated_duration_ms`. It must not be presented as
observed wall-clock time.

Readiness gaps are returned only for structurally valid state. Corrupt ledger
data or a broken cross-record invariant aborts export instead of returning a
seemingly usable snapshot.

Neither declared trace completeness nor runtime evidence verification
establishes causal superiority, economic benefit or production safety. Those
claims still require prospective instrumentation, a frozen protocol,
control/treatment assignment, stable oracles, adequate sample size,
independent validation and a predeclared analysis.
