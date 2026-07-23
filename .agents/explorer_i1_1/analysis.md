# Analysis Report: Forge CLI Parsing & Teamwork Subcommand Design

## Executive Summary
Forge's CLI argument parsing is implemented in `src/main.rs` using `clap` v4.5 with its derive macro features. To add the new `teamwork` subcommand, we need to declare a new variant in the `Commands` enum and add its corresponding routing branch in the `run()` dispatch loop.

---

## 1. Current CLI Parsing Implementation

### Dependency Stack
As defined in `Cargo.toml`:
```toml
clap = { version = "4.5", features = ["derive"] }
```

### Argument Parser Structure
The CLI entry point uses a clean hierarchical clap derive layout in `src/main.rs`:

1. **Top-Level `Cli` Struct**:
   Located around line 219 of `src/main.rs`:
   ```rust
   #[derive(Debug, Parser)]
   #[command(name = "forge", version, about = "Forge Core workflow runtime")]
   struct Cli {
       #[arg(long, default_value = ".forge/forge.sqlite")]
       store: PathBuf,

       #[command(subcommand)]
       command: Option<Commands>,
   }
   ```

2. **Subcommand Routing Enum (`Commands`)**:
   Located between lines 229 and 473 of `src/main.rs`. It contains variants representing each CLI command (such as `Plan`, `List`, `Inspect`, etc.) with their respective arguments.

3. **Output format Enum (`OutputFormat`)**:
   Defined as a `clap::ValueEnum` around line 4165 of `src/main.rs`:
   ```rust
   #[derive(Debug, Clone, Copy, ValueEnum)]
   enum OutputFormat {
       Human,
       Json,
   }
   ```

4. **Execution Dispatch loop (`run`)**:
   Defined in `fn run() -> Result<i32>` (lines 4197-4202). Subcommands are resolved via a `match` expression on `cli.command`:
   ```rust
   let cli = Cli::parse();
   let Some(command) = cli.command else {
       return run_forge_tui(&cli.store, Some(std::env::current_dir()?));
   };
   match command {
       Commands::Plan { ... } => { ... }
       ...
   }
   ```

---

## 2. Recommended Design / Fix Strategy

To introduce the `teamwork` subcommand satisfying all requested specifications, the following design strategy is recommended.

### Step A: Subcommand Structure Update
Add the `Teamwork` variant to the `Commands` enum in `src/main.rs` (before the closing bracket of the enum at line 473):

```rust
    /// Orchestrate a multi-agent teamwork pipeline for a specified goal
    Teamwork {
        /// The objective or goal of the teamwork orchestration
        #[arg(long)]
        goal: String,

        /// Run the orchestration execution detached in the background
        #[arg(short = 'd', long = "detached")]
        detached: bool,

        /// Format for output printing
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
```

### Step B: Dispatch Routing Update
In `src/main.rs` within the `match command` statement (around line 4202), add a branch to handle `Commands::Teamwork`:

```rust
        Commands::Teamwork {
            goal,
            detached,
            output,
        } => {
            let store_path = cli.store.clone();
            let store = ForgeStore::open(store_path.clone())?;
            let project_root = std::env::current_dir()?;

            // 1. Core Teamwork Planning & Brain Allocation logic (referencing R2/R3)
            // (e.g. Decompose goal -> roster allocation mapping -> execution driver)
            let result = forge_core::runtime::run_teamwork_orchestration(
                &store,
                &project_root,
                &goal,
                detached,
            )?;

            // 2. Output Formatting & Standard Serialization
            print_response(output, &result)?;

            Ok(0)
        }
```

### Step C: Module Integration Pattern
To avoid cluttering `src/main.rs`, the core teamwork orchestration logic should be packaged as a function `run_teamwork_orchestration(...)` in the runtime module (e.g., `src/runtime.rs`), matching the product structure guidelines in `AGENTS.md`.

---

## 3. Recommended Test & Verification Strategy

To guarantee that the implementation parses correctly, meets requirements, and performs dynamic brain allocations based on task types, a series of integration tests should be added to `tests/forge_cli_contract.rs`.

### Proposed Integration Test Design
Add the following test case inside `tests/forge_cli_contract.rs`:

```rust
#[test]
fn test_teamwork_subcommand_contract_and_brain_allocation() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    // Invoke the teamwork subcommand targeting a coding and auditing goal
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "teamwork",
            "--goal",
            "Write a python parser for AST extraction and review the generated schema.",
            "--output",
            "json",
        ])
        .assert()
        .success() // Verifies R1: CLI returns successful exit code
        .get_output()
        .stdout
        .clone();

    let response: Value = serde_json::from_slice(&output).unwrap();

    // Verify response structure and schema attributes
    assert!(response.get("roster").is_some());
    assert!(response.get("tasks").is_some());

    // Verify R2: Task allocation heuristics correctly map tasks to specialist brains
    let roster = response["roster"].as_array().expect("Roster must be present as array");

    // Assert that coding tasks mapped to 'codex' or 'opencode'
    let coding_task_brain = roster.iter()
        .find(|agent| agent["role"] == "Worker" || agent["task_type"] == "Coding")
        .map(|agent| agent["allocated_brain"].as_str().unwrap());
    assert!(
        coding_task_brain == Some("codex") || coding_task_brain == Some("opencode"),
        "Coding tasks must map to a code-specialized brain (codex/opencode)"
    );

    // Assert that audit/routing/coordination tasks mapped to 'antigravity' or 'gemini'
    let audit_task_brain = roster.iter()
        .find(|agent| agent["role"] == "Auditor" || agent["task_type"] == "Audit")
        .map(|agent| agent["allocated_brain"].as_str().unwrap());
    assert!(
        audit_task_brain == Some("antigravity") || audit_task_brain == Some("agy") || audit_task_brain == Some("gemini"),
        "Auditing tasks must map to an coordination brain (agy/gemini)"
    );
}
```
This test cleanly asserts subcommand parsing (R1), dynamic role roster generation & brain allocation rules (R2), and output structure alignment.
