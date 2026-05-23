use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use forge_core::artifact::list_workflow_artifacts;
use forge_core::context::build_context_package;
use forge_core::execution::run_simulated;
use forge_core::graph::create_workflow;
use forge_core::improve::generate_improvement;
use forge_core::intent::parse_intent;
use forge_core::skill::install_skill;
use forge_core::storage::ForgeStore;
use forge_core::validation::validate_workflow;
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
}

#[derive(Debug, Subcommand)]
enum SkillCommands {
    Install {
        #[arg(long, default_value = ".")]
        home: PathBuf,
        #[arg(long)]
        target: Vec<String>,
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
        Commands::Improve { workflow, output } => {
            let store = ForgeStore::open(cli.store)?;
            let workflow = store.load_workflow(&workflow)?;
            let proposal = generate_improvement(&store, &workflow)?;
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
                output,
            } => {
                let report = install_skill(&home, &target)?;
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
