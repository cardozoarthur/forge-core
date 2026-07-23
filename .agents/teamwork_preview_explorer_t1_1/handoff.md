# Teamwork Preview Feature 1 - Analysis & Test Plan

## 1. Observation

Direct observations from the `forge-core` workspace:

### A. E2E & Integration Test Environment (`tests/forge_cli_contract.rs`)
The integration tests utilize the `assert_cmd` crate to execute the compiled binary.
- Helper function definition (lines 21-23):
```rust
fn forge() -> Command {
    Command::cargo_bin("forge").expect("forge binary should build")
}
```
- Typical CLI execution and success assertion (lines 139-158):
```rust
    let output = forge()
        .args([
            "harness",
            "token-headroom",
            "--content",
            &content,
            "--kind",
            "log",
            "--budget-tokens",
            "120",
            "--source",
            "test",
            "--output",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
```
- Typical failure assertion and stderr validation (lines 6170-6172):
```rust
        .assert()
        .failure()
        .stderr(predicates::str::contains("refusing to overwrite"));
```

### B. Subcommand Parsing (`src/main.rs`)
Subcommand parsing is handled via `clap` (v4) derive macros.
- The `Plan` subcommand definition (lines 231-240):
```rust
    Plan {
        #[arg(long)]
        goal: String,
        #[arg(long = "addon-dir")]
        addon_dirs: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
        #[arg(short = 'd', long = "detached")]
        detached: bool,
    },
```
- The output format enum option (lines 4164-4168):
```rust
#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Human,
    Json,
}
```
- Print response helper (lines 9946-9952):
```rust
fn print_response<T: Serialize>(format: OutputFormat, value: &T) -> Result<()> {
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(value)?),
        OutputFormat::Human => println!("{}", serde_json::to_string_pretty(value)?),
    }
    Ok(())
}
```
- Some commands print custom formats for Human vs Json explicitly, such as `Inspect` (lines 4306-4309):
```rust
            match output {
                OutputFormat::Json => print_response(output, &report)?,
                OutputFormat::Human => println!("{}", report.diagram),
            }
```

### C. Background Execution Dispatch (`src/main.rs`)
Detached execution spawns a background command executing the `drive-loop` action (lines 4247-4261):
```rust
            if detached {
                if let Some(ref r_id) = run_id {
                    let current_exe = std::env::current_exe()?;
                    std::process::Command::new(current_exe)
                        .arg("--store")
                        .arg(&store_path)
                        .arg("request")
                        .arg("drive-loop")
                        .arg("--run")
                        .arg(r_id)
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn()?;
                }
            }
```

---

## 2. Logic Chain

1. **Subcommand Registration**:
   - Given that `clap` parses subcommands via the `Commands` enum in `src/main.rs` (Obs B), adding `teamwork` as a subcommand requires adding a `Teamwork` variant to `enum Commands` containing fields `goal: String`, `detached: bool`, and `output: OutputFormat`.
   - The arguments should follow identical conventions to `Plan` (Obs B): `--goal` is a required string, `-d` / `--detached` is an optional boolean flag, and `--output` is an enum defaulting to `OutputFormat::Human`.

2. **Goal Validation**:
   - Because commands validate inputs and fail with a clear exit code and error message on invalid input, checking if the `goal` is empty or only whitespace should trigger an exit with code 1. Using `anyhow::bail!` in the `run()` match arm achieves this cleanly, printing the error via `eprintln!("{error:?}")` in `main()` and exiting.

3. **Background/Detached Control Flow**:
   - Based on how `Plan` handles `--detached` by spawning `drive-loop` (Obs C), the `teamwork` command should follow the same pattern:
     - Plan the multi-agent roster and create the workflow, task, and run records in the sqlite database.
     - If `--detached` is true: spawn a background process executing the current binary with arguments `--store <path> request drive-loop --run <run_id>`, then return the plan metadata immediately.
     - If `--detached` is false: run the execution loop synchronously in the foreground until completion.

4. **Output Formatting Design**:
   - To support both `json` and `human` outputs:
     - **JSON Output**: The stdout must serialize a structured JSON payload representing the plan (e.g. matching schema version `"forge.teamwork.plan.v1"`).
     - **Human Output**: The stdout must format the plan in a clear, readable text format detailing the goal, executing mode, roster (with roles and brains), and the task graph.

5. **E2E Integration Testing**:
   - Using the existing test infrastructure helper `forge()` (Obs A), contract tests can be added to verify Feature 1 behavior. Success matches `.success()` and parses json/human outputs, while invalid inputs (like empty goals or invalid outputs) are verified to return exit code >0 and print appropriate errors to stderr via `.failure()`.

---

## 3. Caveats

- **Mocking/Integration with Feature 2 & 3**: This design focuses specifically on Feature 1 (CLI parsing and formatting). The actual brain allocation heuristics (Feature 2) and E2E multi-agent execution loop (Feature 3) are assumed to have placeholder structures in the first iteration of the subcommand.
- **SQLite Database Path**: The test suite uses `tempfile::tempdir()` to isolate database states between tests, preventing write conflict errors. This practice must be strictly followed in the proposed test cases.
- **Detached Process Spawning**: Spawning a detached command assumes that the compiled binary is available at `std::env::current_exe()`, which is always true during cargo-driven E2E tests and production execution.

---

## 4. Conclusion

Adding `forge teamwork` as a first-class command requires extending the Clap parser in `src/main.rs`, adding input validation, integrating background execution logic via subprocess spawning, and providing rich JSON/Human format options. Implementing E2E tests using `assert_cmd` validates the contract for both positive and negative cases.

### Proposed CLI Interface Addition
In `src/main.rs`:
```rust
#[derive(Debug, Subcommand)]
enum Commands {
    // ... other subcommands ...

    /// Multi-agent teamwork orchestration subcommand
    Teamwork {
        /// The objective/goal to plan and execute
        #[arg(long)]
        goal: String,

        /// Run the multi-agent execution pipeline in the background
        #[arg(short = 'd', long = "detached")]
        detached: bool,

        /// Standard output format
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}
```

### Proposed JSON Output Format
```json
{
  "schema_version": "forge.teamwork.plan.v1",
  "workflow_id": "wf_1234567890",
  "goal": "Implement high-performance logging subsystem",
  "detached": true,
  "roster": {
    "schema_version": "forge.teamwork.roster.v1",
    "roles": [
      {
        "role": "Orchestrator",
        "brain": "agy",
        "reason": "Planning and context routing tasks require high-level reasoning and coordination."
      },
      {
        "role": "Worker",
        "brain": "codex",
        "reason": "Coding and implementation tasks map to code-specialized models."
      },
      {
        "role": "Auditor",
        "brain": "agy",
        "reason": "Verification and review tasks require independent verification capability."
      }
    ]
  },
  "tasks": [
    {
      "task_id": "task_01",
      "name": "Design logging layout",
      "role": "Orchestrator",
      "brain": "agy",
      "dependencies": []
    },
    {
      "task_id": "task_02",
      "name": "Write implementation",
      "role": "Worker",
      "brain": "codex",
      "dependencies": ["task_01"]
    },
    {
      "task_id": "task_03",
      "name": "Audit source code",
      "role": "Auditor",
      "brain": "agy",
      "dependencies": ["task_02"]
    }
  ],
  "benchmarks": {
    "retrieved_at": "2026-07-04T10:39:00Z",
    "scores": [
      {
        "brain": "agy",
        "mismatch_penalty": 0.0,
        "evals": {
          "lmsys_chatbot_arena": 1345,
          "mmlu": 0.88,
          "human_eval": 0.82
        }
      },
      {
        "brain": "codex",
        "mismatch_penalty": 0.1,
        "evals": {
          "lmsys_chatbot_arena": 1210,
          "mmlu": 0.79,
          "human_eval": 0.92
        }
      }
    ]
  },
  "run_id": "run_9876543210"
}
```

### Proposed Human Output Format
```
================================================================================
FORGE TEAMWORK EXECUTION PLAN
================================================================================
Goal: Implement high-performance logging subsystem
Workflow ID: wf_1234567890
Execution Mode: Detached (Background Run)

--- TEAM ROSTER ---
* Orchestrator (Brain: agy)
  Reason: Planning and context routing tasks require high-level reasoning and coordination.
* Worker (Brain: codex)
  Reason: Coding and implementation tasks map to code-specialized models.
* Auditor (Brain: agy)
  Reason: Verification and review tasks require independent verification capability.

--- TASK GRAPH ---
[task_01] Design logging layout (Orchestrator / agy)
  └─ [task_02] Write implementation (Worker / codex) (depends on: task_01)
       └─ [task_03] Audit source code (Auditor / agy) (depends on: task_02)

--- BENCHMARKS CONSOLIDATION ---
- Brain 'agy': {"lmsys_chatbot_arena":1345,"mmlu":0.88,"human_eval":0.82}
- Brain 'codex': {"lmsys_chatbot_arena":1210,"mmlu":0.79,"human_eval":0.92}

--- EXECUTION STATUS ---
Launched detached run run_9876543210 in background.
Use `forge status --workflow wf_1234567890` to track progress.
================================================================================
```

---

## 5. Verification Method

To independently verify CLI parsing and output formatting:

### Specific Commands
Run the CLI binary compilation and execute unit/integration test commands:
```bash
cargo build --bin forge
cargo test --test forge_cli_contract teamwork
```

### Proposed Integration Test Cases (in `tests/forge_cli_contract.rs`)
```rust
#[test]
fn teamwork_cli_empty_goal_fails_with_proper_error() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    // Test empty goal error handling
    forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["teamwork", "--goal", "", "--output", "json"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("Goal cannot be empty"));

    // Test whitespace-only goal error handling
    forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["teamwork", "--goal", "    ", "--output", "human"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("Goal cannot be empty"));
}

#[test]
fn teamwork_cli_missing_goal_fails_via_clap() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["teamwork", "--output", "json"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("required arguments were not provided")
            .or(predicates::str::contains("error: the following required arguments were not provided")));
}

#[test]
fn teamwork_cli_invalid_output_fails_via_clap() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args(["teamwork", "--goal", "Build a site", "--output", "yaml"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("invalid value 'yaml'"));
}

#[test]
fn teamwork_cli_detached_json_happy_path() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Create a clean rust server",
            "--detached",
            "--output",
            "json"
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output)
        .expect("Stdout should be valid JSON");

    assert_eq!(json["schema_version"], "forge.teamwork.plan.v1");
    assert_eq!(json["goal"], "Create a clean rust server");
    assert_eq!(json["detached"], true);
    assert!(json["workflow_id"].is_string());
    assert!(json["run_id"].is_string());

    let roster = &json["roster"];
    assert_eq!(roster["schema_version"], "forge.teamwork.roster.v1");
    assert!(roster["roles"].is_array());
    assert!(json["tasks"].is_array());
}

#[test]
fn teamwork_cli_detached_human_happy_path() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    let output = forge()
        .arg("--store")
        .arg(store.to_str().unwrap())
        .args([
            "teamwork",
            "--goal",
            "Create a clean rust server",
            "--detached",
            "--output",
            "human"
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let stdout_str = String::from_utf8(output).unwrap();
    assert!(stdout_str.contains("FORGE TEAMWORK EXECUTION PLAN"));
    assert!(stdout_str.contains("Goal: Create a clean rust server"));
    assert!(stdout_str.contains("Detached (Background Run)"));
    assert!(stdout_str.contains("TEAM ROSTER"));
    assert!(stdout_str.contains("TASK GRAPH"));
    assert!(stdout_str.contains("EXECUTION STATUS"));
    assert!(stdout_str.contains("Launched detached run"));
}
```

### Invalidation Conditions
- Test validation fails if cargo cannot locate the `forge` binary or if `clap` versions mismatch.
- Human output assertions fail if capitalization or formatting symbols change, so strict checks should target key sections rather than matching the exact full-string output verbatim.
