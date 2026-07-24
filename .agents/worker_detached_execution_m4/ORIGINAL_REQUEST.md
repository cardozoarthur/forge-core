## 2026-07-03T21:47:15Z

You are the Worker for Milestone 4: Detached Workflow Execution (R5).
Your working directory is `/home/arthur/projects/forge-core/.agents/worker_detached_execution_m4`.

Please perform the following tasks:
1. Modify `src/main.rs` to support the detached execution option:
   - Add `#[arg(short = 'd', long = "detached")] detached: bool` to the `Plan` variant of `Commands` (around line 231).
   - Add `#[arg(short = 'd', long = "detached")] detached: bool` to the `Start` variant of `RequestCommands` (around line 3014).
   - Add a new variant to `RequestCommands` enum:
     ```rust
     DriveLoop {
         #[arg(long = "run")]
         run_id: String,
         #[arg(long, default_value = "forge_cli")]
         executor: String,
         #[arg(long = "ttl-seconds", default_value_t = 300)]
         ttl_seconds: u64,
         #[arg(long, default_value = "background_driver")]
         origin: String,
     }
     ```
   - Handle the new `RequestCommands::DriveLoop` subcommand in `Commands::Request` match block:
     ```rust
     RequestCommands::DriveLoop {
         run_id,
         executor,
         ttl_seconds,
         origin,
     } => {
         let store = ForgeStore::open(cli.store.clone())?;
         loop {
             let report = step_request(&store, &run_id, &executor, ttl_seconds, &origin)?;
             if report.status == "completed" || report.status == "failed" || report.status == "cancelled" {
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
         Ok(0)
     }
     ```
   - In `Commands::Plan` handler (around line 4189):
     - Extract `detached` from the matched pattern.
     - If `detached` is true, create and save a run record:
       ```rust
       let run = forge_core::request::create_run_record(&workflow, "forge_cli", "accepted");
       forge_core::request::save_run_record(&store, &run)?;
       let run_id = run.run_id.clone();
       ```
     - Include `"run_id"` in the returned JSON response (set to `Some(run.run_id)` if `detached`, otherwise `None`).
     - If `detached` is true, spawn the current binary in the background (detached process) running:
       `request drive-loop --run <run_id> --store <store_path>` (make sure stdout/stderr are redirected to null or a log file so it doesn't block the caller, and use `.spawn()?` instead of waiting).
   - In `RequestCommands::Start` handler (around line 8097):
     - Extract `detached` from the matched pattern.
     - If `detached` is true, spawn the current binary in the background (detached process) running:
       `request drive-loop --run <report.run_id> --store <store_path>`.
2. Run `cargo test` and `cargo clippy --all-targets --all-features -- -D warnings` to verify everything compiles and all tests pass perfectly.
3. Test your implementation by running a smoke test command:
   `./target/release/forge request start --goal "Test detached goal" -d`
   Verify that the spawned background driver process runs successfully (e.g. check the run status in database/logs or query active runs).
4. Write a handoff.md in your working directory and send a message back to the parent (conversation ID: 3e9f825f-a52f-4f9b-8826-e0ccd6f322a6) with the path to your handoff.md.

MANDATORY INTEGRITY WARNING: DO NOT CHEAT. All implementations must be genuine. DO NOT hardcode test results, create dummy/facade implementations, or circumvent the intended task. A Forensic Auditor will independently verify your work. Integrity violations WILL be detected and your work WILL be rejected.
