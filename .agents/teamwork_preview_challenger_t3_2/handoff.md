# Challenge & Handoff Report — Teamwork Heuristics, Roster Planning, and Caching

## 1. Observation

During our empirical stress-testing and code auditing of the teamwork subcommand heuristics, dynamic roster planning, and caching (`src/teamwork.rs`, `src/request.rs`, `tests/forge_teamwork_heuristics_stress.rs`), we observed the following:

### A. Non-Functional Database Cache in Production (Critical Defect)
In `src/teamwork.rs` (lines 125-131), the database cache check reads:
```rust
    let cache_table_exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='web_benchmark_cache')",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);
```
However, a code search for `web_benchmark_cache` across the rest of the codebase (`src/storage.rs` where all other database tables are initialized) reveals that the table is never created in production.
- **Impact**: `cache_table_exists` will always evaluate to `false` in production, meaning that benchmark caching is completely non-functional.
- **Verbatim command and result**:
  Running `cargo test --test forge_teamwork_heuristics_stress` requires manual setup of the `web_benchmark_cache` table using `CREATE TABLE` inside tests (e.g., `tests/forge_teamwork_heuristics_stress.rs` line 269). Without manual creation, SQLite queries fail or are silently ignored, forcing dynamic benchmark fetches on every run (if `FORGE_BENCHMARK_URL` is set) or fallback to static heuristics (if unset).

### B. Security/Policy Violation on Disallowing All Brains (Robustness Gap)
In `src/teamwork.rs` (lines 261-267 and 288), if all brains are marked `allowed: false` in the database under `executor_policy`, the fallback logic defaults to assigning disallowed brains:
```rust
    let mut selected_worker_brain = None;
    for brain in &preferred_list {
        if !disallowed_brains.contains(*brain) {
            selected_worker_brain = Some((*brain).to_string());
            break;
        }
    }
    ...
    let worker_brain = selected_worker_brain.unwrap_or_else(|| "gemini".to_string());
```
Similarly, for Orchestrator and Auditor roles:
```rust
    roles.push(TeamworkRole {
        role: "Orchestrator".to_string(),
        brain: if disallowed_brains.contains("gemini") {
            "opencode".to_string()
        } else {
            "gemini".to_string()
        },
    });
    ...
    roles.push(TeamworkRole {
        role: "Auditor".to_string(),
        brain: if disallowed_brains.contains("opencode") {
            "gemini".to_string()
        } else {
            "opencode".to_string()
        },
    });
```
- **Impact**: If `gemini` and `opencode` are disallowed in the policy, they are still assigned to the roles. Roster generation succeeds and the command exits with exit code `0`, silently violating the user's explicit policy.
- **Verbatim result from test**:
  Our new test `test_stress_executor_policy_fallback` Case D explicitly verifies that when all brains are disallowed, the system assigns disallowed brains anyway:
  `Deny-All Roster: Worker=gemini, Orchestrator=opencode, Auditor=gemini`

### C. Fragile and Non-Granular Cache Invalidation
In `src/teamwork.rs` (lines 154-166), if even one cached brain has an expired or malformed timestamp, the `all_unexpired` boolean is set to `false`, discarding the entire cached list.
- **Impact**: If the benchmark server is offline, the system throws away otherwise valid cached scores due to a single malformed entry, defaulting directly to static heuristics instead of using valid cached entries.

---

## 2. Logic Chain

1. **Caching Failure**: Because the `web_benchmark_cache` table is omitted from the table creation list in `src/storage.rs` (lines 640-1080), any fresh database initialized by Forge will lack this table. Thus, caching is bypassed on every execution in production.
2. **Policy Bypass**: The unwrap defaults (`unwrap_or_else(|| "gemini".to_string())`) and role assignment fallback logic in `src/teamwork.rs` (lines 295-314) do not validate if the fallback brain is allowed under the policy. Consequently, a user who blocks "gemini" and "opencode" will still have these brains assigned to roles in the planning output.
3. **Cognitive Task Halting**: In `src/request.rs`, `is_auto_steppable_task` correctly evaluates task executor types. Tasks requiring `ExecutorKind::Ai` or `ExecutorKind::Mixed` correctly fail the auto-step check and trigger a halt to the step loop, returning `status: "handoff_required"`.

---

## 3. Caveats

- **Mock Server Limitations**: Tests use local MockServers that bind to random ports on localhost. We assume no firewall rules prevent localhost sockets.
- **Policy Enforcement Scope**: The `executor_policy` check only disallows brains during the roster planning phase. We did not investigate whether other validation gates prevent disallowed brains during actual task step execution.

---

## 4. Conclusion

**Overall risk assessment**: HIGH

While cognitive task halting operates perfectly and correctly triggers `handoff_required`, the teamwork heuristics and dynamic roster planning suffer from two high-impact correctness and robustness gaps:
1. **Caching is disabled in production** because the SQLite cache table is never created by default.
2. **Security/Policy is bypassed** if all preferred brains are disallowed, as the system silently falls back to disallowed default brains without erroring.

### Actionable Mitigations
1. **Initialize Caching Table**: Add the `web_benchmark_cache` schema definition to the connection migration/creation sequence in `src/storage.rs`.
2. **Enforce Policy strictly**: If `selected_worker_brain` is `None` or if fallback brains are disallowed under policy, `plan_teamwork_workflow` should return an error indicating that no allowed executors are available, rather than silently violating the policy.
3. **Refine Cache Expiration**: Only discard expired entries rather than invalidating the entire cache list, and fallback to using expired cached entries if the benchmark server is unreachable.

---

## 5. Verification Method

To independently verify this:

### Run Stress/Adversarial Integration Tests
Execute the dedicated stress test suite:
```bash
cargo test --test forge_teamwork_heuristics_stress
```

### Inspect Codebase
1. Observe the absence of `web_benchmark_cache` table creation in `src/storage.rs`.
2. Observe the unwrap default `unwrap_or_else(|| "gemini".to_string())` in `src/teamwork.rs` at line 288.

---

# Adversarial Review Challenge Report

## Challenge Summary

**Overall risk assessment**: HIGH

## Challenges

### [High] Challenge 1: Silent Executor Policy Bypass
- **Assumption challenged**: The system assumes that fallback options (`unwrap_or_else(|| "gemini".to_string())` and Orchestrator/Auditor defaults) are safe.
- **Attack scenario**: A user disallows `gemini` and `opencode` due to strict privacy or compliance rules. The teamwork subcommand silently overrides this policy, scheduling tasks to runs using `gemini` and `opencode`.
- **Blast radius**: Unauthorized data exposure to external model APIs and compliance violations.
- **Mitigation**: Return a validation error if no allowed brains match the roles, or fallback to human-in-the-loop validation.

### [Medium] Challenge 2: Bypassed Database Cache in Production
- **Assumption challenged**: The system assumes the benchmark ranking cache is active and functional.
- **Attack scenario**: The cache table is never created in production database schema initialization. The system performs redundant network calls on every run, introducing latency and rate limit risks.
- **Blast radius**: Performance degradation and potential connection failures on offline environments.
- **Mitigation**: Add the table creation statement to `src/storage.rs`.

## Stress Test Results

- **Handoff Halting Scenario** → Step cognitive tasks repeatedly → Halts with `handoff_required` → **PASS**
- **Disallowed Fallback Scenario** → Disallow preferred brains via policy -> Verify fallback selection → Falls back correctly / defaults to gemini on deny-all → **PASS** (Asserted fallback output)
- **Cache Hit / Bypass Scenario** → Pre-populate SQLite cache -> Query teamwork with and without bypass -> Uses cached vs fetched brain → **PASS**
- **Cache Expiration Scenario** → Set cache timestamp > 24 hours ago -> Query teamwork -> Bypasses cache and fetches from server → **PASS**
- **Unreachable Server Scenario** → Point URL to dead port -> Query teamwork -> Falls back to static heuristics gracefully → **PASS**
- **Malformed JSON Scenario** → Serve invalid JSON payload -> Query teamwork -> Defaults to static heuristics without panicking → **PASS**

## Unchallenged Areas

- **Network timeouts under heavy network congestion** — Not challenged as the TCP stream uses a standard 5-second timeout, but behavior under packet-loss scenarios was not verified.
