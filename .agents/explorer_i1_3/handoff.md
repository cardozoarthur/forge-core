# Handoff Report: CLI Parsing & Teamwork Subcommand Design

## 1. Observation

- **CLI Parser Location**: The parser is defined in `src/main.rs:219-227` as `struct Cli` using Clap derive macros:
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
- **Subcommand Enum Location**: The subcommands are located in `enum Commands` in `src/main.rs:230-473`.
- **Match Dispatch Loop**: The commands are matched and dispatched in `fn run() -> Result<i32>` starting at line 4202:
  ```rust
  fn run() -> Result<i32> {
      let cli = Cli::parse();
      let Some(command) = cli.command else {
          return run_forge_tui(&cli.store, Some(std::env::current_dir()?));
      };
      match command {
          Commands::Plan { ... } => { ... }
          // Match arms...
      }
  }
  ```
- **Existing `OutputFormat` Enum**: Already defined in `src/main.rs:4164-4168`:
  ```rust
  #[derive(Debug, Clone, Copy, ValueEnum)]
  enum OutputFormat {
      Human,
      Json,
  }
  ```
- **Background Execution Spawning pattern**: Spawning background/detached drivers in `src/main.rs` (e.g., `RequestCommands::Start` at line 8146) is done via spawning self in a subprocess pointing to `drive-loop`:
  ```rust
  std::process::Command::new(current_exe)
      .arg("--store")
      .arg(&store_path)
      .arg("request")
      .arg("drive-loop")
      .arg("--run")
      .arg(&report.run_id)
      .stdout(std::process::Stdio::null())
      .stderr(std::process::Stdio::null())
      .spawn()?;
  ```
- **Roster & Routing Structures**: Located in `src/graph.rs:166-173` (`NodeBrainAgentSlotSpec`) and `src/graph.rs:176-200` (`NodeBrainRoutingSpec`).
- **Tests Execution**: `cargo test` successfully compiled and ran 443 tests with 0 failures:
  ```
  test result: ok. 443 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 38.77s
  ```

## 2. Logic Chain

1. **Subcommand Registration**: Since `forge` defines its subcommands as variants on the `enum Commands` in `src/main.rs`, adding the `teamwork` subcommand requires adding a new `Teamwork` variant to `enum Commands`.
2. **Subcommand Options**: Based on user requirements, `teamwork` requires `--goal`, `--detached`, and `--output`. These maps to fields inside the new `Teamwork` variant:
   - `goal: String` (required, mapping to `--goal`),
   - `detached: bool` with `#[arg(short = 'd', long = "detached")]` (optional flag, mapping to `-d` or `--detached`),
   - `output: OutputFormat` with `#[arg(long, value_enum, default_value_t = OutputFormat::Human)]` (optional enum, mapping to `--output`).
3. **Dispatch & Execution Loop**:
   - A new match arm `Commands::Teamwork` must be added to `run()` in `src/main.rs`.
   - The arm needs to plan the workflow (calling an upcoming planning function, e.g., `plan_teamwork_workflow` inside `src/runtime.rs` or `src/execution.rs`).
   - For Milestone I1 compilation, a basic stub for `plan_teamwork_workflow` can be introduced to return a mock roster and workflow ID.
   - If `--detached` is set, it will spawn a background subprocess executing `request drive-loop --run <run_id>` matching existing detached execution patterns.
   - If not detached, it will synchronously step the request to completion by looping over `step_request`.
   - The plan details/roster will be printed in the requested format (human-readable or JSON).

## 3. Caveats

- The specific teamwork planning heuristics (decomposing goals, dynamic brain mapping, consolidated web benchmark retrieval) are not part of Milestone I1 and will be designed/implemented in Milestone I2.
- The multi-agent orchestrator driver tracking and metrics lineage collection will be implemented in Milestone I3.
- The design assumes a stub structure is acceptable for I1 to maintain codebase compilation and pass cargo checks.

## 4. Conclusion

Adding the `teamwork` subcommand can be cleanly achieved without code breakage by:
1. Defining the `Teamwork` variant in `Commands` in `src/main.rs`.
2. Mapping the subcommand option fields directly to Clap derive arguments.
3. Adding the match arm in `run()` to invoke the teamwork workflow planning and handle detached/synchronous driving.
4. Implementing a temporary compilation stub for teamwork planning in `src/runtime.rs` or `src/execution.rs`.

## 5. Verification Method

To verify the implementation once coded:
1. Run `cargo clippy --all-targets --all-features -- -D warnings` and `cargo test` to ensure there are no compilation errors or test failures.
2. Execute:
   ```bash
   cargo run -- teamwork --goal "Implement new feature" --output json
   ```
   Verify that it correctly outputs the JSON structure of the planned workflow and roster.
3. Execute:
   ```bash
   cargo run -- teamwork --goal "Implement new feature" --detached
   ```
   Verify that the command finishes quickly and starts a background `request drive-loop` subprocess.
