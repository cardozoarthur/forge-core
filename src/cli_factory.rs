use crate::graph::{create_workflow, Workflow};
use crate::intent::{IntentSpec, WorkflowModeSpec};
use crate::storage::ForgeStore;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

const CLI_FACTORY_CREATION_SCHEMA_VERSION: &str = "forge.cli_factory.creation_plan.v1";

#[derive(Debug, Clone)]
pub struct CliFactoryCreateInput {
    pub name: String,
    pub goal: String,
    pub source: Option<String>,
    pub commands: Vec<String>,
    pub compound_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliFactoryCreationPlan {
    pub schema_version: String,
    pub status: String,
    pub state_owner: String,
    pub workflow_id: String,
    pub workflow_created: bool,
    pub files_written: bool,
    pub cli: CliFactoryCliSpec,
    pub workflow_contract: CliFactoryWorkflowContract,
    pub addon_manifest: CliFactoryAddonManifest,
    pub local_first: CliFactoryLocalFirstSpec,
    pub agent_native: CliFactoryAgentNativeSpec,
    pub surfaces: CliFactorySurfaces,
    pub verification_pipeline: CliFactoryVerificationPipeline,
    pub benchmark_inspiration: CliFactoryBenchmarkInspiration,
    pub next_commands: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliFactoryCliSpec {
    pub name: String,
    pub binary_name: String,
    pub mcp_server_name: String,
    pub skill_name: String,
    pub goal: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliFactoryWorkflowContract {
    pub schema_version: String,
    pub engine: String,
    pub workflow_kind: String,
    pub state_owner: String,
    pub runtime_boundary: String,
    pub not_executed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliFactoryAddonManifest {
    pub addon_id: String,
    pub capability_id: String,
    pub runtime_contracts: Vec<CliFactoryRuntimeContract>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliFactoryRuntimeContract {
    pub contract_id: String,
    pub contract_type: String,
    pub executor: String,
    pub permission_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliFactoryLocalFirstSpec {
    pub persistence: String,
    pub sync_model: String,
    pub search_index: String,
    pub data_source_modes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliFactoryAgentNativeSpec {
    pub default_commands: Vec<String>,
    pub requested_commands: Vec<String>,
    pub compound_commands: Vec<String>,
    pub output_modes: Vec<String>,
    pub exit_code_policy: String,
    pub dry_run_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliFactorySurfaces {
    pub cli: bool,
    pub mcp: bool,
    pub skill: bool,
    pub addon: bool,
    pub workflow: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliFactoryVerificationPipeline {
    pub checks: Vec<String>,
    pub promotion_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliFactoryBenchmarkInspiration {
    pub source: String,
    pub principles: Vec<String>,
}

pub fn create_cli_factory_plan(
    store: &ForgeStore,
    input: CliFactoryCreateInput,
) -> Result<CliFactoryCreationPlan> {
    let name = normalize_cli_name(&input.name)?;
    let goal = normalize_goal(&input.goal, &name);
    let source = input
        .source
        .map(|source| source.trim().to_string())
        .filter(|source| !source.is_empty())
        .unwrap_or_else(|| "unspecified".to_string());
    let requested_commands = normalize_commands(input.commands);
    let compound_commands = normalize_commands(input.compound_commands);
    let workflow = create_workflow(cli_factory_intent(&name, &goal, &source));
    store.save_workflow(&workflow)?;

    let plan = cli_factory_creation_plan(
        workflow,
        name,
        goal,
        source,
        requested_commands,
        compound_commands,
    );
    store.record_event(
        &plan.workflow_id,
        "cli_factory_creation_planned",
        &serde_json::to_value(&plan)?,
    )?;
    Ok(plan)
}

fn cli_factory_intent(name: &str, goal: &str, source: &str) -> IntentSpec {
    IntentSpec {
        goal: format!("Create workflow-backed CLI {name}: {goal}"),
        constraints: vec![
            "Forge workflow runtime owns state, checkpoints, approvals and validation".to_string(),
            "Generated CLI code must be an Addon/runtime contract, not an external orchestration authority".to_string(),
            "CLI and MCP surfaces must be generated from the same workflow-backed contract".to_string(),
            "Local-first persistence must use SQLite where practical".to_string(),
        ],
        deliverables: vec![
            "Workflow-backed CLI manifest".to_string(),
            "Addon runtime contract for generated CLI execution".to_string(),
            "MCP surface plan sharing the same command model".to_string(),
            "Agent-native command model with JSON, compact and dry-run modes".to_string(),
            "Verification pipeline with scorecard, dogfood, proof and smoke checks".to_string(),
        ],
        risks: vec![
            "generated_cli_bypasses_forge_runtime".to_string(),
            "api_auth_or_schema_discovery_incomplete".to_string(),
            "compound_commands_without_local_store".to_string(),
        ],
        unknowns: vec![format!("source_api_or_site_contract:{source}")],
        workflow_mode: WorkflowModeSpec {
            kind: "cli_factory_workflow".to_string(),
            expected_lifetime: "long_running".to_string(),
            can_become_persistent: true,
            scale_to_zero_policy: "scale_to_zero_when_idle".to_string(),
        },
        ..IntentSpec::default()
    }
}

fn cli_factory_creation_plan(
    workflow: Workflow,
    name: String,
    goal: String,
    source: String,
    requested_commands: Vec<String>,
    compound_commands: Vec<String>,
) -> CliFactoryCreationPlan {
    let binary_name = format!("{name}-forge-cli");
    let mcp_server_name = format!("{name}-forge-mcp");
    let skill_name = format!("{name}-forge-skill");
    let addon_id = format!("forge.addon.generated_cli.{name}");
    let capability_id = format!("{name}_workflow_backed_cli");
    let contract_id = format!("{name}.cli.workflow_executor");
    let permission_id = format!("{name}.cli.execute");
    CliFactoryCreationPlan {
        schema_version: CLI_FACTORY_CREATION_SCHEMA_VERSION.to_string(),
        status: "cli_creation_workflow_created".to_string(),
        state_owner: "forge_workflow_runtime".to_string(),
        workflow_id: workflow.id.clone(),
        workflow_created: true,
        files_written: false,
        cli: CliFactoryCliSpec {
            name: name.clone(),
            binary_name,
            mcp_server_name,
            skill_name,
            goal,
            source,
        },
        workflow_contract: CliFactoryWorkflowContract {
            schema_version: "forge.cli_factory.workflow_contract.v1".to_string(),
            engine: "forge_workflow_runtime".to_string(),
            workflow_kind: "cli_factory_workflow".to_string(),
            state_owner: "forge_workflow_runtime".to_string(),
            runtime_boundary: "generated_cli_is_addon_runtime_contract".to_string(),
            not_executed: true,
        },
        addon_manifest: CliFactoryAddonManifest {
            addon_id,
            capability_id,
            runtime_contracts: vec![CliFactoryRuntimeContract {
                contract_id,
                contract_type: "cli_workflow_executor".to_string(),
                executor: "forge_workflow_runtime".to_string(),
                permission_id,
            }],
        },
        local_first: CliFactoryLocalFirstSpec {
            persistence: "sqlite".to_string(),
            sync_model: "incremental_cursor_sync".to_string(),
            search_index: "sqlite_fts5".to_string(),
            data_source_modes: vec!["auto".to_string(), "local".to_string(), "live".to_string()],
        },
        agent_native: CliFactoryAgentNativeSpec {
            default_commands: default_agent_native_commands(),
            requested_commands,
            compound_commands,
            output_modes: vec![
                "json".to_string(),
                "compact".to_string(),
                "human_table".to_string(),
            ],
            exit_code_policy: "typed_exit_codes_for_agent_recovery".to_string(),
            dry_run_default: true,
        },
        surfaces: CliFactorySurfaces {
            cli: true,
            mcp: true,
            skill: true,
            addon: true,
            workflow: true,
        },
        verification_pipeline: CliFactoryVerificationPipeline {
            checks: vec![
                "scorecard".to_string(),
                "dogfood".to_string(),
                "proof_of_behavior".to_string(),
                "live_api_smoke".to_string(),
            ],
            promotion_policy: "validate_before_publish_or_install".to_string(),
        },
        benchmark_inspiration: CliFactoryBenchmarkInspiration {
            source: "cli-printing-press".to_string(),
            principles: vec![
                "agent_native_cli".to_string(),
                "local_first_sqlite".to_string(),
                "compound_commands".to_string(),
                "dual_cli_mcp_surface".to_string(),
                "mechanical_verification".to_string(),
            ],
        },
        next_commands: vec![
            vec![
                "workflow".to_string(),
                "attach-artifact".to_string(),
                "--workflow".to_string(),
                workflow.id.clone(),
                "--artifact".to_string(),
                "<generated-cli-manifest>".to_string(),
                "--output".to_string(),
                "json".to_string(),
            ],
            vec![
                "addons".to_string(),
                "dispatch-planner".to_string(),
                "--contract".to_string(),
                "cli_factory.codegen".to_string(),
                "--goal".to_string(),
                "Generate workflow-backed CLI implementation".to_string(),
                "--workflow".to_string(),
                workflow.id,
                "--output".to_string(),
                "json".to_string(),
            ],
        ],
    }
}

fn default_agent_native_commands() -> Vec<String> {
    ["sync", "search", "sql", "insight", "status"]
        .iter()
        .map(|command| (*command).to_string())
        .collect()
}

fn normalize_commands(commands: Vec<String>) -> Vec<String> {
    commands
        .into_iter()
        .map(|command| normalize_token(&command))
        .filter(|command| !command.is_empty())
        .collect()
}

fn normalize_goal(goal: &str, name: &str) -> String {
    let goal = goal.trim();
    if goal.is_empty() {
        format!("Create an agent-native workflow-backed CLI named {name}")
    } else {
        goal.to_string()
    }
}

fn normalize_cli_name(name: &str) -> Result<String> {
    let normalized = normalize_token(name);
    if normalized.is_empty() {
        bail!("cli name must contain at least one ascii alphanumeric character");
    }
    Ok(normalized)
}

fn normalize_token(value: &str) -> String {
    let mut normalized = String::new();
    let mut previous_dash = false;
    for ch in value.trim().chars().flat_map(|ch| ch.to_lowercase()) {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch);
            previous_dash = false;
        } else if !previous_dash && !normalized.is_empty() {
            normalized.push('-');
            previous_dash = true;
        }
    }
    while normalized.ends_with('-') {
        normalized.pop();
    }
    normalized
}
