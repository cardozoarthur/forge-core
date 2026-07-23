# Handoff Report: Teamwork Subcommand CLI Parsing Design

## 1. Observation
From the investigation of the `forge-core` repository, the following configurations and source code patterns were observed:

- **Clap dependency**: `Cargo.toml` line 16 contains:
  ```toml
  clap = { version = "4.5", features = ["derive"] }
  ```
- **CLI parser definition**: `src/main.rs` lines 219–227 contains:
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
- **Subcommand definition**: `src/main.rs` lines 229–230 contains:
  ```rust
  #[derive(Debug, Subcommand)]
  enum Commands {
  ```
- **Command Dispatch**: `src/main.rs` lines 4197–4202 contains:
  ```rust
  fn run() -> Result<i32> {
      let cli = Cli::parse();
      let Some(command) = cli.command else {
          return run_forge_tui(&cli.store, Some(std::env::current_dir()?));
      };
      match command {
  ```
- **Test environment**: `tests/forge_cli_contract.rs` uses `assert_cmd::Command` (line 21) to spawn the `forge` binary and assert command outcomes.

---

## 2. Logic Chain
1. *CLI entry point*: Since argument parsing is driven by `clap` via the `Cli` and `Commands` types in `src/main.rs`, adding a new command option must be done within these structures.
2. *Argument Mapping*: The requested subcommand `teamwork` needs to accept:
   - `--goal "<goal>"` (required string)
   - `-d` / `--detached` (optional boolean flag)
   - `--output` (standard `human`/`json` enum)
   These match `clap` attributes `#[arg(long)]`, `#[arg(short = 'd', long = "detached")]`, and `#[arg(long, value_enum, default_value_t = OutputFormat::Human)]`.
3. *Execution Routing*: The `run()` function matches on `cli.command` and routes subcommands. Adding `Commands::Teamwork` requires a corresponding match arm inside the `match command` block.
4. *Test Verification*: Because integration tests are structured using `assert_cmd` in `tests/forge_cli_contract.rs`, the new subcommand's behavior and brain allocations can be asserted by spawning the forge binary under a temporary SQLite test database.

---

## 3. Caveats
- The core teamwork multi-agent orchestrator logic itself is not being implemented as part of this read-only investigation.
- We assume that the teamwork orchestration module will expose an API function (e.g. `run_teamwork_orchestration(...)`) under `src/runtime.rs` or a separate `src/teamwork.rs` file.

---

## 4. Conclusion
The parsing implementation is fully localized in `src/main.rs`. Adding the `teamwork` subcommand is straightforward:
1. Define the `Teamwork` variant in `enum Commands` in `src/main.rs`.
2. Insert a match arm for `Commands::Teamwork` inside the `run()` function in `src/main.rs`.
3. Add integration tests inside `tests/forge_cli_contract.rs` to verify correct argument parsing and dynamic brain allocations.

---

## 5. Verification Method
After applying the proposed changes (when permitted):
1. Run `cargo clippy --all-targets --all-features -- -D warnings` to verify compiling syntax.
2. Run `cargo test` to verify integration tests.
3. Manually execute:
   ```bash
   cargo run -- teamwork --goal "Design a new database adapter and audit the transaction logs." --output json
   ```
   Inspect the JSON output to check that `roster`, `tasks`, and the brain selections are present.
