# Analysis Report: Forge CLI Parsing & Teamwork Subcommand Design

## Executive Summary
Forge's CLI is built on Rust using `clap` v4.5 with derive macro support, with its CLI structure and subcommand dispatch routing situated in `src/main.rs`. This investigation covers the current command-line parsing setup and details the design and routing strategy needed to add the new `teamwork` subcommand. We provide a modular implementation approach that adheres to the product rules outlined in `AGENTS.md` (e.g., maintaining small, single-purpose modules and avoiding bloated logic in `src/main.rs`).

---

## 1. Current CLI Parsing Implementation

### 1.1 Command-Line Parsing Framework
The `forge` CLI parses input args via `clap` (v4.5), configured with the `derive` feature in `Cargo.toml`:
```toml
clap = { version = "4.5", features = ["derive"] }
```

In `src/main.rs`, the parsing structures are defined as follows:
- **`Cli` Struct** (lines 219–227): Represents the main entry-point configuration. It defines the SQLite database path `--store` (default: `.forge/forge.sqlite`) and parses the subcommand:
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

- **`Commands` Enum** (lines 229–473): An enum annotated with `#[derive(Debug, Subcommand)]` that represents the CLI commands (e.g., `Plan`, `List`, `Inspect`, `Run`, `Validate`, etc.).

- **`OutputFormat` Enum** (lines 4164–4168): A value enum defining output formats (`Human` and `Json`):
  ```rust
  #[derive(Debug, Clone, Copy, ValueEnum)]
  enum OutputFormat {
      Human,
      Json,
  }
  ```

### 1.2 Subcommand Routing & Dispatch
In `src/main.rs` (lines 4197–4202), `Cli::parse()` processes arguments. If no subcommand is provided, `run_forge_tui` starts. If a subcommand is present, it matches on the parsed `Commands` variant:
```rust
fn run() -> Result<i32> {
    let cli = Cli::parse();
    let Some(command) = cli.command else {
        return run_forge_tui(&cli.store, Some(std::env::current_dir()?));
    };
    match command {
        Commands::Plan {
            goal,
            addon_dirs,
            output,
            detached,
        } => {
            // Planning implementation logic...
        }
        // Other command arms...
    }
}
```

---

## 2. Design Strategy: The `teamwork` Subcommand

To introduce the `teamwork` subcommand under `forge teamwork`, the following design and routing pattern is recommended:

### 2.1 Extending `enum Commands`
Add the `Teamwork` variant to the `Commands` enum in `src/main.rs` before line 473:
```rust
    /// Orchestrate a multi-agent teamwork pipeline for a specified goal
    Teamwork {
        /// The goal or objective of the teamwork orchestration
        #[arg(long)]
        goal: String,

        /// Run the execution detached in the background
        #[arg(short = 'd', long = "detached")]
        detached: bool,

        /// Format for output printing
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
```

### 2.2 Wire the Dispatch Match in `src/main.rs`
Add a match arm to `match command` in `fn run()` to route `Commands::Teamwork`:
```rust
        Commands::Teamwork {
            goal,
            detached,
            output,
        } => {
            let store_path = cli.store.clone();
            let store = ForgeStore::open(store_path.clone())?;
            let project_root = std::env::current_dir()?;

            // Call modular teamwork runner in runtime module
            let result = forge_core::runtime::run_teamwork_orchestration(
                &store,
                &project_root,
                &goal,
                detached,
            )?;

            // Print output using output format (human, json)
            print_response(output, &result)?;

            Ok(0)
        }
```

### 2.3 Modular Architecture (Avoiding Main Bloat)
In accordance with `AGENTS.md` product guidelines, the core teamwork orchestration logic must be separated into the relevant module (e.g. `src/runtime.rs` or a new `src/teamwork.rs`), rather than placed directly in `src/main.rs`.

The function signature in `src/runtime.rs` or `src/teamwork.rs` would be:
```rust
use crate::storage::ForgeStore;
use anyhow::Result;
use std::path::Path;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct TeamworkOrchestrationResult {
    pub status: String,
    pub goal: String,
    pub roster: Vec<RosterMember>,
    pub workflow_id: String,
    pub run_id: Option<String>,
}

pub fn run_teamwork_orchestration(
    store: &ForgeStore,
    project_root: &Path,
    goal: &str,
    detached: bool,
) -> Result<TeamworkOrchestrationResult> {
    // 1. Parse intent and decompose the goal into a task dependency graph.
    // 2. Map tasks to agent roles (Orchestrator, Worker, Auditor) using heuristics.
    // 3. Query/reference LLM benchmarks (LMSYS, MMLU, HumanEval) to dynamically rank & allocate executors.
    // 4. Create and save the workflow/run records in SQLite.
    // 5. If detached, launch the drive-loop subcommand in the background; else run synchronous execution.
    // 6. Return structured result metadata.
    todo!()
}
```

---

## 3. Integration & Testing Strategy

To verify the `teamwork` subcommand behaves according to requirements (R1, R2, and R3), integration tests should be added to `tests/forge_cli_contract.rs`.

### 3.1 Proposed Integration Test Design
```rust
#[test]
fn test_forge_teamwork_subcommand_contract() {
    let temp = tempdir().unwrap();
    let store = temp.path().join("forge.sqlite");

    // Invoke teamwork command with goal and json output
    let output = forge()
        .args([
            "--store",
            store.to_str().unwrap(),
            "teamwork",
            "--goal",
            "Build an AST parser in Python and audit the schema mapping.",
            "--output",
            "json",
        ])
        .assert()
        .success() // Verifies R1: exit status code is 0
        .get_output()
        .stdout
        .clone();

    let json_val: serde_json::Value = serde_json::from_slice(&output).unwrap();

    // Verify R2: roster allocations are returned and correct
    assert!(json_val.get("roster").is_some(), "Roster must be populated");
    let roster = json_val["roster"].as_array().expect("Roster must be an array");

    // Verify task-specific brain mapping heuristics (R2)
    let coding_task = roster.iter()
        .find(|m| m["role"] == "Worker" || m["task_type"] == "Coding");
    assert!(coding_task.is_some());
    let code_brain = coding_task.unwrap()["allocated_brain"].as_str().unwrap();
    assert!(code_brain == "codex" || code_brain == "opencode", "Coding tasks should route to codex or opencode");

    let audit_task = roster.iter()
        .find(|m| m["role"] == "Auditor" || m["task_type"] == "Audit");
    assert!(audit_task.is_some());
    let audit_brain = audit_task.unwrap()["allocated_brain"].as_str().unwrap();
    assert!(audit_brain == "antigravity" || audit_brain == "agy" || audit_brain == "gemini", "Audit/verification tasks should route to agy or gemini");
}
```

### 3.2 Required Pre-Promotion Validations
Before finalizing any future code changes to implement this design:
```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```
