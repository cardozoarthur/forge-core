# CLI Parsing Investigation & Teamwork Subcommand Design

This report details how the `forge` CLI parsing is implemented and proposes a design/fix strategy to add the `teamwork` subcommand.

---

## 1. Current CLI Parsing Implementation

The `forge` CLI uses the **Clap** library (version 4.x with the `derive` feature) to parse command-line arguments. The implementation is located primarily in `src/main.rs`.

### Key Components

1. **`Cli` Struct (`src/main.rs:219-227`)**:
   Acts as the main entry point for Clap parsing.
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
   - `--store`: Configures the SQLite database path (defaults to `.forge/forge.sqlite`).
   - `command`: An optional subcommand mapped to the `Commands` enum.

2. **`Commands` Enum (`src/main.rs:230-473`)**:
   Contains all top-level subcommands. Each variant is a subcommand with its own fields representing its arguments and flags. For example, `Commands::Plan` is declared as:
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

3. **`OutputFormat` Enum (`src/main.rs:4164-4168`)**:
   Defines the available output formats used by commands:
   ```rust
   #[derive(Debug, Clone, Copy, ValueEnum)]
   enum OutputFormat {
       Human,
       Json,
   }
   ```

4. **`run()` and `main()` Dispatch Loop (`src/main.rs:4187-4800+`)**:
   The entrypoint `main()` invokes `run()`, which parses the arguments using `Cli::parse()` and matches `cli.command` against `Commands`:
   ```rust
   fn run() -> Result<i32> {
       let cli = Cli::parse();
       let Some(command) = cli.command else {
           return run_forge_tui(&cli.store, Some(std::env::current_dir()?));
       };
       match command {
           Commands::Plan { ... } => { ... }
           // Match arms for all other commands
       }
   }
   ```

---

## 2. Design Strategy for the `teamwork` Subcommand (R1)

To add the `teamwork` subcommand supporting `--goal`, `--detached`, and `--output` options, the following design strategy is recommended.

### Step A: Declare `Commands::Teamwork` Subcommand

Add the `Teamwork` variant to the `Commands` enum in `src/main.rs`.

```rust
    Teamwork {
        #[arg(long)]
        goal: String,
        #[arg(short = 'd', long = "detached")]
        detached: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
```
- **`goal`**: A required string (`String` without `Option<String>`), parsed via `--goal`.
- **`detached`**: A boolean flag, parsed via `-d` or `--detached`. If present, it evaluates to `true`; otherwise `false`.
- **`output`**: An enum argument mapped to the existing `OutputFormat` enum. It defaults to `OutputFormat::Human`.

### Step B: Declare matching/dispatch logic in `run()`

Add a new match arm in `fn run() -> Result<i32>` in `src/main.rs` to process `Commands::Teamwork`.

```rust
        Commands::Teamwork {
            goal,
            detached,
            output,
        } => {
            let store_path = cli.store.clone();
            let store = ForgeStore::open(store_path.clone())?;

            // 1. Plan the teamwork workflow (Decompose goal, determine team roster & roles, allocate brains)
            // Note: The concrete planning implementation will be done in Milestone I2.
            // For now, this calls into the future teamwork planning API.
            let plan = forge_core::runtime::plan_teamwork_workflow(&store, &goal)?;

            // 2. Output the planning metadata / roster
            match output {
                OutputFormat::Json => {
                    print_response(output, &serde_json::to_value(&plan)?)?;
                }
                OutputFormat::Human => {
                    println!("Teamwork Workflow Planned Successfully!");
                    println!("Workflow ID: {}", plan.workflow_id);
                    println!("Goal: {}", plan.goal);
                    println!("--- Roster & Brain Allocation ---");
                    for slot in &plan.roster.agent_slots {
                        let brain = slot.brain_id.as_deref().unwrap_or("unassigned");
                        println!("Role: {:<15} | Allocated Brain: {:<12} | Slot: {}", slot.role, brain, slot.slot_id);
                    }
                }
            }

            // 3. Drive the workflow execution
            if detached {
                // Background execution (Detached): spawn background process to run request drive-loop
                let current_exe = std::env::current_exe()?;
                std::process::Command::new(current_exe)
                    .arg("--store")
                    .arg(&store_path)
                    .arg("request")
                    .arg("drive-loop")
                    .arg("--run")
                    .arg(&plan.run_id)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()?;

                if let OutputFormat::Human = output {
                    println!("Workflow execution started in background. Run ID: {}", plan.run_id);
                }
            } else {
                // Synchronous execution: step the request to completion
                if let OutputFormat::Human = output {
                    println!("Driving workflow execution synchronously...");
                }
                loop {
                    // Drive execution loop step-by-step using standard request driver
                    let report = forge_core::request::step_request(&store, &plan.run_id, "forge_cli", 300, "teamwork_driver")?;
                    if report.status == "completed"
                        || report.status == "failed"
                        || report.status == "cancelled"
                    {
                        break;
                    }
                    if report.status == "skipped" && report.reason.contains("no ready handoff task") {
                        break;
                    }
                    if report.status == "handoff_required" || report.status == "validation_failed" {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }

                if let OutputFormat::Human = output {
                    println!("Workflow driving finished.");
                }
            }

            Ok(0)
        }
```

### Step C: Introduce Stubs to allow compiling for Milestone I1

To ensure that compiling works correctly during Milestone I1 before Milestone I2 and I3 are implemented, a temporary stub structure can be introduced in `src/runtime.rs`:

```rust
use crate::graph::{NodeBrainRoutingSpec, NodeBrainAgentSlotSpec};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamworkPlanReport {
    pub workflow_id: String,
    pub run_id: String,
    pub goal: String,
    pub roster: NodeBrainRoutingSpec,
}

pub fn plan_teamwork_workflow(
    store: &crate::storage::ForgeStore,
    goal: &str,
) -> anyhow::Result<TeamworkPlanReport> {
    // Basic Stub / Mock structure for compilation and baseline verification.
    // Full heuristics logic will overwrite this in Milestone I2.
    let workflow_id = format!("wf_teamwork_stub_{}", crate::artifact::hex_sha256(goal.as_bytes())[..8].to_string());
    let run_id = format!("run_teamwork_stub_{}", crate::artifact::hex_sha256(goal.as_bytes())[..8].to_string());

    let mut roster = NodeBrainRoutingSpec::default();
    roster.agent_slots = vec![
        NodeBrainAgentSlotSpec {
            slot_id: "slot_orch".to_string(),
            brain_id: Some("agy".to_string()),
            role: "Orchestrator".to_string(),
            parallel_group: "0".to_string(),
            state_owner: "forge".to_string(),
        },
        NodeBrainAgentSlotSpec {
            slot_id: "slot_coder".to_string(),
            brain_id: Some("codex".to_string()),
            role: "Coder".to_string(),
            parallel_group: "1".to_string(),
            state_owner: "forge".to_string(),
        }
    ];

    Ok(TeamworkPlanReport {
        workflow_id,
        run_id,
        goal: goal.to_string(),
        roster,
    })
}
```

---

## 3. Integration with Downstream Milestones

- **Milestone I2 (Roster Heuristics & Benchmark Consolidation)**: Overwrite the stub `plan_teamwork_workflow` function with a complete task graph decomposition, mapping logic to assign agent roles, and consolidated web benchmark fetching.
- **Milestone I3 (Multi-Agent Execution & Lineage)**: Enhance the driving mechanism (synchronous/detached) to support multi-agent orchestrator driver tracking and execution lineage caching.
