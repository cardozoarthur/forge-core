use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use forge_core::artifact::list_workflow_artifacts;
use forge_core::context::build_context_package;
use forge_core::execution::run_simulated;
use forge_core::executor::{load_executors, sync_executors, ExecutorSyncOptions};
use forge_core::graph::create_workflow;
use forge_core::improve::generate_improvement;
use forge_core::intent::parse_intent;
use forge_core::lease::{acquire_task_lease, release_task_lease};
use forge_core::request::{load_request_status, start_async_request};
use forge_core::runtime::{
    guard_runtime_scope, load_runtimes, sync_runtimes, RuntimeGuardRequest, RuntimeSyncOptions,
};
use forge_core::self_evolve::{run_self_evolution, SelfRunOptions};
use forge_core::skill::install_skill;
use forge_core::storage::ForgeStore;
use forge_core::validation::validate_workflow;
use forge_core::workflow::{attach_workflow_artifact, update_workflow_goal};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "forge", version, about = "Forge Core workflow runtime")]
struct Cli {
    #[arg(long, default_value = ".forge/forge.sqlite")]
    store: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Plan {
        #[arg(long)]
        goal: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Status {
        #[arg(long)]
        workflow: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Context {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long, default_value_t = 1200)]
        budget: usize,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Run {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        simulate: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Validate {
        #[arg(long)]
        workflow: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Improve {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        target_version: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Artifacts {
        #[arg(long)]
        workflow: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Skill {
        #[command(subcommand)]
        command: SkillCommands,
    },
    Sync {
        #[command(subcommand)]
        command: SyncCommands,
    },
    Executors {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Runtimes {
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Runtime {
        #[command(subcommand)]
        command: RuntimeCommands,
    },
    Workflow {
        #[command(subcommand)]
        command: WorkflowCommands,
    },
    Task {
        #[command(subcommand)]
        command: TaskCommands,
    },
    Request {
        #[command(subcommand)]
        command: RequestCommands,
    },
    #[command(name = "self")]
    SelfRun {
        #[command(subcommand)]
        command: SelfCommands,
    },
}

#[derive(Debug, Subcommand)]
enum SkillCommands {
    Install {
        #[arg(long, default_value = ".")]
        home: PathBuf,
        #[arg(long)]
        target: Vec<String>,
        #[arg(long = "executor-path")]
        executor_paths: Vec<PathBuf>,
        #[arg(long = "runtime-path")]
        runtime_paths: Vec<PathBuf>,
        #[arg(long)]
        allow: Vec<String>,
        #[arg(long)]
        deny: Vec<String>,
        #[arg(long)]
        no_prompt: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum SyncCommands {
    Executors {
        #[arg(long, default_value = ".")]
        home: PathBuf,
        #[arg(long = "executor-path")]
        executor_paths: Vec<PathBuf>,
        #[arg(long)]
        allow: Vec<String>,
        #[arg(long)]
        deny: Vec<String>,
        #[arg(long)]
        no_prompt: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Runtimes {
        #[arg(long, default_value = ".")]
        home: PathBuf,
        #[arg(long = "runtime-path")]
        runtime_paths: Vec<PathBuf>,
        #[arg(long)]
        allow: Vec<String>,
        #[arg(long)]
        deny: Vec<String>,
        #[arg(long)]
        no_prompt: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    All {
        #[arg(long, default_value = ".")]
        home: PathBuf,
        #[arg(long = "executor-path")]
        executor_paths: Vec<PathBuf>,
        #[arg(long = "runtime-path")]
        runtime_paths: Vec<PathBuf>,
        #[arg(long)]
        allow: Vec<String>,
        #[arg(long)]
        deny: Vec<String>,
        #[arg(long)]
        no_prompt: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum RuntimeCommands {
    Guard {
        #[arg(long)]
        substrate: String,
        #[arg(long)]
        resource: String,
        #[arg(long)]
        namespace: String,
        #[arg(long)]
        action: String,
        #[arg(long)]
        owner: String,
        #[arg(long)]
        allow_external: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum WorkflowCommands {
    UpdateGoal {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        goal: String,
        #[arg(long)]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    AttachArtifact {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        path: PathBuf,
        #[arg(long)]
        kind: String,
        #[arg(long)]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum TaskCommands {
    Acquire {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long)]
        executor: String,
        #[arg(long, default_value_t = 900)]
        ttl_seconds: u64,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Release {
        #[arg(long)]
        workflow: String,
        #[arg(long)]
        task: String,
        #[arg(long)]
        lease: String,
        #[arg(long)]
        executor: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum RequestCommands {
    Start {
        #[arg(long)]
        goal: String,
        #[arg(long, default_value = "forge_cli")]
        origin: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
    Status {
        #[arg(long = "run")]
        run_id: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Subcommand)]
enum SelfCommands {
    Run {
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        #[arg(long)]
        until: String,
        #[arg(long, default_value_t = 1)]
        max_cycles: u32,
        #[arg(long, default_value_t = 1800)]
        sleep_seconds: u64,
        #[arg(long = "executor")]
        executors: Vec<String>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        push: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output: OutputFormat,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Human,
    Json,
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error:?}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<i32> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Plan { goal, output } => {
            let store = ForgeStore::open(cli.store)?;
            let intent = parse_intent(&goal);
            let workflow = create_workflow(intent);
            store.save_workflow(&workflow)?;
            store.record_event(
                &workflow.id,
                "workflow_planned",
                &serde_json::to_value(&workflow)?,
            )?;
            let response = serde_json::json!({
                "status": "planned",
                "workflow_id": workflow.id,
                "goal": workflow.goal,
                "tasks": workflow.tasks,
                "intent": workflow.intent,
            });
            print_response(output, &response)?;
            Ok(0)
        }
        Commands::Status { workflow, output } => {
            let store = ForgeStore::open(cli.store)?;
            let workflow = store.load_workflow(&workflow)?;
            let response = serde_json::json!({
                "workflow_id": workflow.id,
                "status": workflow.status,
                "goal": workflow.goal,
                "tasks": workflow.tasks,
                "artifacts": workflow.artifacts,
                "revisions": workflow.revisions,
            });
            print_response(output, &response)?;
            Ok(0)
        }
        Commands::Context {
            workflow,
            task,
            budget,
            output,
        } => {
            let store = ForgeStore::open(cli.store)?;
            let workflow = store.load_workflow(&workflow)?;
            let context = build_context_package(&workflow, &task, budget)?;
            print_response(output, &context)?;
            Ok(0)
        }
        Commands::Run {
            workflow,
            simulate,
            output,
        } => {
            if !simulate {
                anyhow::bail!("v0 execution requires --simulate; real provider execution is intentionally not enabled");
            }
            let store = ForgeStore::open(cli.store)?;
            let mut workflow = store.load_workflow(&workflow)?;
            let report = run_simulated(&mut workflow);
            store.save_workflow(&workflow)?;
            store.record_event(
                &workflow.id,
                "workflow_simulated",
                &serde_json::to_value(&report)?,
            )?;
            print_response(output, &report)?;
            Ok(0)
        }
        Commands::Validate { workflow, output } => {
            let store = ForgeStore::open(cli.store)?;
            let workflow = store.load_workflow(&workflow)?;
            let report = validate_workflow(&workflow);
            let exit_code = if report.promotable { 0 } else { 1 };
            print_response(output, &report)?;
            Ok(exit_code)
        }
        Commands::Improve {
            workflow,
            target_version,
            output,
        } => {
            let store = ForgeStore::open(cli.store)?;
            let workflow = store.load_workflow(&workflow)?;
            let proposal = generate_improvement(&store, &workflow, target_version)?;
            print_response(output, &proposal)?;
            Ok(0)
        }
        Commands::Artifacts { workflow, output } => {
            let store = ForgeStore::open(cli.store)?;
            let _workflow = store.load_workflow(&workflow)?;
            let artifacts = list_workflow_artifacts(&store.base_dir(), &workflow)?;
            let response = serde_json::json!({
                "workflow_id": workflow,
                "artifacts": artifacts,
            });
            print_response(output, &response)?;
            Ok(0)
        }
        Commands::Skill { command } => match command {
            SkillCommands::Install {
                home,
                target,
                executor_paths,
                runtime_paths,
                allow,
                deny,
                no_prompt,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = install_skill(&home, &target)?;
                let executor_sync = sync_executors(
                    &store,
                    ExecutorSyncOptions {
                        home: home.clone(),
                        executor_paths,
                        allow: allow.clone(),
                        deny: deny.clone(),
                        prompt: !no_prompt,
                    },
                )?;
                let runtime_sync = sync_runtimes(
                    &store,
                    RuntimeSyncOptions {
                        home: home.clone(),
                        runtime_paths,
                        allow: allow.clone(),
                        deny: deny.clone(),
                        prompt: !no_prompt,
                    },
                )?;
                let response = serde_json::json!({
                    "skill": report.skill,
                    "installed": report.installed,
                    "executor_sync": executor_sync,
                    "runtime_sync": runtime_sync,
                });
                print_response(output, &response)?;
                Ok(0)
            }
        },
        Commands::Sync { command } => match command {
            SyncCommands::Executors {
                home,
                executor_paths,
                allow,
                deny,
                no_prompt,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = sync_executors(
                    &store,
                    ExecutorSyncOptions {
                        home,
                        executor_paths,
                        allow,
                        deny,
                        prompt: !no_prompt,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            SyncCommands::Runtimes {
                home,
                runtime_paths,
                allow,
                deny,
                no_prompt,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = sync_runtimes(
                    &store,
                    RuntimeSyncOptions {
                        home,
                        runtime_paths,
                        allow,
                        deny,
                        prompt: !no_prompt,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
            SyncCommands::All {
                home,
                executor_paths,
                runtime_paths,
                allow,
                deny,
                no_prompt,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let executor_sync = sync_executors(
                    &store,
                    ExecutorSyncOptions {
                        home: home.clone(),
                        executor_paths,
                        allow: allow.clone(),
                        deny: deny.clone(),
                        prompt: !no_prompt,
                    },
                )?;
                let runtime_sync = sync_runtimes(
                    &store,
                    RuntimeSyncOptions {
                        home,
                        runtime_paths,
                        allow,
                        deny,
                        prompt: !no_prompt,
                    },
                )?;
                let response = serde_json::json!({
                    "status": "synced",
                    "executor_sync": executor_sync,
                    "runtime_sync": runtime_sync,
                });
                print_response(output, &response)?;
                Ok(0)
            }
        },
        Commands::Executors { output } => {
            let store = ForgeStore::open(cli.store)?;
            let report = load_executors(&store)?;
            print_response(output, &report)?;
            Ok(0)
        }
        Commands::Runtimes { output } => {
            let store = ForgeStore::open(cli.store)?;
            let report = load_runtimes(&store)?;
            print_response(output, &report)?;
            Ok(0)
        }
        Commands::Runtime { command } => match command {
            RuntimeCommands::Guard {
                substrate,
                resource,
                namespace,
                action,
                owner,
                allow_external,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = guard_runtime_scope(
                    &store,
                    RuntimeGuardRequest {
                        substrate,
                        resource,
                        namespace,
                        action,
                        owner,
                        allow_external,
                    },
                )?;
                let exit_code = if report.allowed { 0 } else { 1 };
                print_response(output, &report)?;
                Ok(exit_code)
            }
        },
        Commands::Workflow { command } => match command {
            WorkflowCommands::UpdateGoal {
                workflow,
                goal,
                origin,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = update_workflow_goal(&store, &workflow, &goal, &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
            WorkflowCommands::AttachArtifact {
                workflow,
                path,
                kind,
                origin,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = attach_workflow_artifact(&store, &workflow, &path, &kind, &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
        },
        Commands::Task { command } => match command {
            TaskCommands::Acquire {
                workflow,
                task,
                executor,
                ttl_seconds,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = acquire_task_lease(&store, &workflow, &task, &executor, ttl_seconds)?;
                let exit_code = if report.allowed { 0 } else { 1 };
                print_response(output, &report)?;
                Ok(exit_code)
            }
            TaskCommands::Release {
                workflow,
                task,
                lease,
                executor,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = release_task_lease(&store, &workflow, &task, &lease, &executor)?;
                let exit_code = if report.released { 0 } else { 1 };
                print_response(output, &report)?;
                Ok(exit_code)
            }
        },
        Commands::Request { command } => match command {
            RequestCommands::Start {
                goal,
                origin,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = start_async_request(&store, &goal, &origin)?;
                print_response(output, &report)?;
                Ok(0)
            }
            RequestCommands::Status { run_id, output } => {
                let store = ForgeStore::open(cli.store)?;
                let report = load_request_status(&store, &run_id)?;
                print_response(output, &report)?;
                Ok(0)
            }
        },
        Commands::SelfRun { command } => match command {
            SelfCommands::Run {
                repo,
                until,
                max_cycles,
                sleep_seconds,
                executors,
                dry_run,
                push,
                output,
            } => {
                let store = ForgeStore::open(cli.store)?;
                let report = run_self_evolution(
                    &store,
                    SelfRunOptions {
                        repo,
                        until,
                        max_cycles,
                        sleep_seconds,
                        executors,
                        dry_run,
                        push,
                    },
                )?;
                print_response(output, &report)?;
                Ok(0)
            }
        },
    }
}

fn print_response<T: Serialize>(format: OutputFormat, value: &T) -> Result<()> {
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(value)?),
        OutputFormat::Human => println!("{}", serde_json::to_string_pretty(value)?),
    }
    Ok(())
}
