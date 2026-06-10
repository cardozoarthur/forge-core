use crate::addon::{
    addon_observability_report, default_addon_dirs, list_addon_views,
    load_addon_catalog_from_store, AddonObservabilityReport, AddonViewReport,
};
use crate::identity::ensure_workflow_policy;
use crate::improve::{rank_improvement_candidates, OrchestratorImprovementCandidatesReport};
use crate::registry::{
    list_workflows_with_filters, WorkflowLifecycleFilter, WorkflowRegistryFilters,
    WorkflowRegistryReport,
};
use crate::request::{
    complete_ready_task, drive_request, step_request, RequestTaskCompletionInput,
};
use crate::storage::ForgeStore;
use crate::{
    graph::TaskStatus,
    ir::{
        ir_schema_version, ComponentSpec, CreativeArtifact, CreativeArtifactKind, DesignToken,
        DocumentSpec, ScreenSpec, SemanticAlias, SlideDeckSpec, TokenCollection, TokenType,
        WhiteboardSpec,
    },
    workflow::{
        attach_creative_artifact, patch_workflow_token, record_creative_collaboration_event,
        set_workflow_token_collection, update_workflow_goal, update_workflow_task,
        CreativeCollaborationEventRequest, WorkflowTaskUpdateInput,
    },
};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use uuid::Uuid;

const OPS_SNAPSHOT_SCHEMA_VERSION: &str = "forge.ops.snapshot.v1";
const OPS_ACTION_SCHEMA_VERSION: &str = "forge.ops.action.v1";
const OPS_MODIFIER_LANE_SCHEMA_VERSION: &str = "forge.ops.modifier_lane.v1";
const OPS_MODIFIER_PROPOSAL_SCHEMA_VERSION: &str = "forge.ops.modifier_proposal.v1";
const OPS_MODIFIER_PROPOSAL_CREATED_EVENT: &str = "ops_modifier_proposal_created";
const OPS_MODIFIER_PROPOSAL_APPLIED_EVENT: &str = "ops_modifier_proposal_applied";
const MAX_HTTP_REQUEST_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct OpsSnapshot {
    pub status: String,
    pub schema_version: String,
    pub generated_at: DateTime<Utc>,
    pub mode: OpsMode,
    pub registry: WorkflowRegistryReport,
    pub improvement_candidates: OrchestratorImprovementCandidatesReport,
    pub modifier_lane: OpsModifierLane,
    pub addon_observability: AddonObservabilityReport,
    pub addon_views: AddonViewReport,
    pub visual_workflows: Vec<OpsWorkflowVisual>,
    pub actions: Vec<OpsActionSpec>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpsMode {
    pub operational: bool,
    pub strategic: bool,
    pub realtime_mutation: bool,
    pub assisted_operations: bool,
    pub local_only_by_default: bool,
    pub ai_modifier_lane: String,
    pub human_access: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpsActionSpec {
    pub id: String,
    pub method: String,
    pub path: String,
    pub description: String,
    pub mutates_workflow: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpsWorkflowVisual {
    pub workflow_id: String,
    pub goal: String,
    pub status: String,
    pub design_surface: OpsDesignSurface,
    pub tasks: Vec<OpsTaskVisual>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpsTaskVisual {
    pub task_id: String,
    pub title: String,
    pub status: String,
    pub executor: String,
    pub dependencies: Vec<String>,
    pub expected_output: String,
    pub subtasks: Vec<OpsSubtaskVisual>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpsSubtaskVisual {
    pub subtask_id: String,
    pub title: String,
    pub status: String,
    pub definition_of_done: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpsDesignSurface {
    pub schema_version: String,
    pub creative_artifact_count: usize,
    pub whiteboard_count: usize,
    pub screen_count: usize,
    pub component_count: usize,
    pub document_count: usize,
    pub slide_deck_count: usize,
    pub token_collection_present: bool,
    pub token_count: usize,
    pub token_mode_count: usize,
    pub active_presence_count: usize,
    pub comment_count: usize,
    pub patch_event_count: usize,
    pub artifacts: Vec<OpsCreativeArtifactVisual>,
    pub tokens: Vec<OpsDesignTokenVisual>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpsCreativeArtifactVisual {
    pub artifact_id: String,
    pub title: String,
    pub kind: String,
    pub updated_at: DateTime<Utc>,
    pub active_presence_count: usize,
    pub comment_count: usize,
    pub patch_event_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpsDesignTokenVisual {
    pub name: String,
    pub value: String,
    pub token_type: String,
    pub group: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpsModifierLane {
    pub schema_version: String,
    pub purpose: String,
    pub pending_count: usize,
    pub applied_count: usize,
    pub proposals: Vec<OpsModifierProposal>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsModifierProposal {
    pub schema_version: String,
    pub proposal_id: String,
    pub workflow_id: String,
    pub target_kind: String,
    pub task_id: Option<String>,
    pub title: String,
    pub summary: String,
    pub rationale: String,
    pub proposed_goal: Option<String>,
    pub proposed_title: Option<String>,
    pub proposed_expected_output: Option<String>,
    pub author: String,
    pub status: String,
    pub created_at: String,
    pub applied_at: Option<String>,
    pub applied_revision: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct OpsModifierProposalInput<'a> {
    pub workflow_id: &'a str,
    pub target_kind: &'a str,
    pub task_id: Option<&'a str>,
    pub title: &'a str,
    pub summary: &'a str,
    pub rationale: &'a str,
    pub proposed_goal: Option<&'a str>,
    pub proposed_title: Option<&'a str>,
    pub proposed_expected_output: Option<&'a str>,
    pub author: &'a str,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpsModifierProposalReport {
    pub status: String,
    pub proposal: OpsModifierProposal,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpsModifierApplyReport {
    pub status: String,
    pub proposal_id: String,
    pub workflow_id: String,
    pub target_kind: String,
    pub task_id: Option<String>,
    pub origin: String,
    pub applied_at: String,
    pub revision: u64,
    pub mutation: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpsServeReport {
    pub status: String,
    pub schema_version: String,
    pub bind_addr: String,
    pub url: String,
    pub local_only: bool,
    pub routes: Vec<OpsActionSpec>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpsActionReport {
    pub status: String,
    pub schema_version: String,
    pub action: String,
    pub result: Value,
}

#[derive(Debug)]
pub struct OpsHttpResponse {
    pub status_code: u16,
    pub reason: String,
    pub content_type: String,
    pub body: Vec<u8>,
}

pub fn build_ops_snapshot(store: &ForgeStore) -> Result<OpsSnapshot> {
    let addon_dirs = default_addon_dirs();
    build_ops_snapshot_with_addon_dirs(store, &addon_dirs)
}

pub fn build_ops_snapshot_with_addon_dirs(
    store: &ForgeStore,
    addon_dirs: &[PathBuf],
) -> Result<OpsSnapshot> {
    let registry = list_workflows_with_filters(
        store,
        WorkflowRegistryFilters::new(WorkflowLifecycleFilter::All),
    )?;
    let improvement_candidates = rank_improvement_candidates(store, 10)?;
    let modifier_lane = load_modifier_lane(store)?;
    let addon_catalog = load_addon_catalog_from_store(store, addon_dirs)?;
    let addon_observability = addon_observability_report(store, &addon_catalog, None, None, 1000)?;
    let addon_views = list_addon_views(&addon_catalog, None, Some("ops_console"), Some("enabled"));
    let visual_workflows = build_visual_workflows(store)?;
    Ok(OpsSnapshot {
        status: "ok".to_string(),
        schema_version: OPS_SNAPSHOT_SCHEMA_VERSION.to_string(),
        generated_at: Utc::now(),
        mode: OpsMode {
            operational: true,
            strategic: true,
            realtime_mutation: true,
            assisted_operations: true,
            local_only_by_default: true,
            ai_modifier_lane:
                "separate_orchestrator_can_update_goals_nodes_and_subflows_via_forge_apis"
                    .to_string(),
            human_access: "full_local_workflow_visibility_and_runtime_mutation_controls"
                .to_string(),
        },
        registry,
        improvement_candidates,
        modifier_lane,
        addon_observability,
        addon_views,
        visual_workflows,
        actions: ops_actions(),
    })
}

fn build_visual_workflows(store: &ForgeStore) -> Result<Vec<OpsWorkflowVisual>> {
    let mut workflows = store
        .load_workflows()?
        .into_iter()
        .map(|workflow| {
            let design_surface = summarize_design_surface(&workflow);
            let tasks = workflow
                .tasks
                .iter()
                .map(|task| OpsTaskVisual {
                    task_id: task.id.clone(),
                    title: task.title.clone(),
                    status: task_status(&task.status),
                    executor: format!("{:?}", task.executor).to_lowercase(),
                    dependencies: task.dependencies.clone(),
                    expected_output: task.expected_output.clone(),
                    subtasks: task
                        .work_item
                        .subtasks
                        .iter()
                        .map(|subtask| OpsSubtaskVisual {
                            subtask_id: subtask.id.clone(),
                            title: subtask.title.clone(),
                            status: task_status(&subtask.status),
                            definition_of_done: subtask.definition_of_done.clone(),
                        })
                        .collect(),
                })
                .collect();
            OpsWorkflowVisual {
                workflow_id: workflow.id,
                goal: workflow.goal,
                status: workflow.status,
                design_surface,
                tasks,
            }
        })
        .collect::<Vec<_>>();
    workflows.sort_by(|left, right| left.workflow_id.cmp(&right.workflow_id));
    Ok(workflows)
}

fn summarize_design_surface(workflow: &crate::graph::Workflow) -> OpsDesignSurface {
    let mut whiteboard_count = 0;
    let mut screen_count = 0;
    let mut component_count = 0;
    let mut document_count = 0;
    let mut slide_deck_count = 0;
    let mut active_presence_count = 0;
    let mut comment_count = 0;
    let mut patch_event_count = 0;
    let mut artifacts = Vec::new();

    for artifact in &workflow.creative_artifacts {
        match &artifact.kind {
            CreativeArtifactKind::Whiteboard => whiteboard_count += 1,
            CreativeArtifactKind::Screen => screen_count += 1,
            CreativeArtifactKind::Component => component_count += 1,
            CreativeArtifactKind::Document => document_count += 1,
            CreativeArtifactKind::SlideDeck => slide_deck_count += 1,
        }
        let collaboration = artifact.collaboration.summary();
        active_presence_count += collaboration.active_presence_count;
        comment_count += collaboration.comment_count;
        patch_event_count += collaboration.patch_event_count;
        artifacts.push(OpsCreativeArtifactVisual {
            artifact_id: artifact.id.clone(),
            title: artifact.title.clone(),
            kind: creative_kind_label(&artifact.kind).to_string(),
            updated_at: artifact.updated_at,
            active_presence_count: collaboration.active_presence_count,
            comment_count: collaboration.comment_count,
            patch_event_count: collaboration.patch_event_count,
        });
    }
    artifacts.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then(left.title.cmp(&right.title))
            .then(left.artifact_id.cmp(&right.artifact_id))
    });

    let tokens = workflow
        .token_collection
        .as_ref()
        .map(|collection| {
            let mut tokens = collection
                .tokens
                .iter()
                .map(|token| OpsDesignTokenVisual {
                    name: token.name.clone(),
                    value: token.value.clone(),
                    token_type: token.token_type.as_str().to_string(),
                    group: token.group.clone(),
                })
                .collect::<Vec<_>>();
            tokens.sort_by(|left, right| left.name.cmp(&right.name));
            tokens
        })
        .unwrap_or_default();

    OpsDesignSurface {
        schema_version: "forge.ops.design_surface.v1".to_string(),
        creative_artifact_count: workflow.creative_artifacts.len(),
        whiteboard_count,
        screen_count,
        component_count,
        document_count,
        slide_deck_count,
        token_collection_present: workflow.token_collection.is_some(),
        token_count: workflow
            .token_collection
            .as_ref()
            .map(|tokens| tokens.tokens.len())
            .unwrap_or(0),
        token_mode_count: workflow
            .token_collection
            .as_ref()
            .map(|tokens| tokens.modes.len())
            .unwrap_or(0),
        active_presence_count,
        comment_count,
        patch_event_count,
        artifacts,
        tokens,
    }
}

fn creative_kind_label(kind: &CreativeArtifactKind) -> &'static str {
    match kind {
        CreativeArtifactKind::Screen => "screen",
        CreativeArtifactKind::Whiteboard => "whiteboard",
        CreativeArtifactKind::Document => "document",
        CreativeArtifactKind::SlideDeck => "slide_deck",
        CreativeArtifactKind::Component => "component",
    }
}

fn task_status(status: &TaskStatus) -> String {
    match status {
        TaskStatus::Pending => "pending",
        TaskStatus::Running => "running",
        TaskStatus::Completed => "completed",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Failed => "failed",
    }
    .to_string()
}

pub fn load_modifier_lane(store: &ForgeStore) -> Result<OpsModifierLane> {
    let mut proposals = Vec::new();
    let mut applied = BTreeMap::new();

    for workflow in store.load_workflows()? {
        for event in store.load_workflow_events(&workflow.id)? {
            match event.kind.as_str() {
                OPS_MODIFIER_PROPOSAL_CREATED_EVENT => {
                    let mut proposal: OpsModifierProposal =
                        serde_json::from_value(event.data.clone()).with_context(|| {
                            format!("invalid modifier proposal event for {}", workflow.id)
                        })?;
                    if proposal.created_at.trim().is_empty() {
                        proposal.created_at = event.created_at;
                    }
                    proposals.push(proposal);
                }
                OPS_MODIFIER_PROPOSAL_APPLIED_EVENT => {
                    if let Some(proposal_id) = event.data.get("proposal_id").and_then(Value::as_str)
                    {
                        let applied_at = event
                            .data
                            .get("applied_at")
                            .and_then(Value::as_str)
                            .unwrap_or(&event.created_at)
                            .to_string();
                        let revision = event.data.get("revision").and_then(Value::as_u64);
                        applied.insert(proposal_id.to_string(), (applied_at, revision));
                    }
                }
                _ => {}
            }
        }
    }

    for proposal in &mut proposals {
        if let Some((applied_at, revision)) = applied.get(&proposal.proposal_id) {
            proposal.status = "applied".to_string();
            proposal.applied_at = Some(applied_at.clone());
            proposal.applied_revision = *revision;
        }
    }

    proposals.sort_by(|left, right| {
        left.status
            .cmp(&right.status)
            .then(left.created_at.cmp(&right.created_at))
            .then(left.proposal_id.cmp(&right.proposal_id))
    });
    let pending_count = proposals
        .iter()
        .filter(|proposal| proposal.status == "pending")
        .count();
    let applied_count = proposals
        .iter()
        .filter(|proposal| proposal.status == "applied")
        .count();

    Ok(OpsModifierLane {
        schema_version: OPS_MODIFIER_LANE_SCHEMA_VERSION.to_string(),
        purpose: "separate_ai_or_human_modifier_lane_for_live_strategy_and_node_mutation"
            .to_string(),
        pending_count,
        applied_count,
        proposals,
    })
}

pub fn create_modifier_proposal(
    store: &ForgeStore,
    input: OpsModifierProposalInput<'_>,
) -> Result<OpsModifierProposalReport> {
    ensure_workflow_policy(store, input.workflow_id, "ops modifier proposal")?;
    let workflow = store.load_workflow(input.workflow_id)?;
    let task_id = clean_optional(input.task_id);

    match input.target_kind {
        "workflow_goal" => {
            if clean_optional(input.proposed_goal).is_none() {
                bail!("workflow goal modifier proposals require proposed_goal");
            }
        }
        "task_node" => {
            let Some(task_id) = task_id.as_deref() else {
                bail!("task node modifier proposals require task_id");
            };
            if !workflow.tasks.iter().any(|task| task.id == task_id) {
                bail!("task {task_id} not found in workflow {}", input.workflow_id);
            }
            if clean_optional(input.proposed_title).is_none()
                && clean_optional(input.proposed_goal).is_none()
                && clean_optional(input.proposed_expected_output).is_none()
            {
                bail!(
                    "task node modifier proposals require title, goal or expected_output mutation"
                );
            }
        }
        other => bail!("unsupported modifier target_kind `{other}`"),
    }

    let proposal = OpsModifierProposal {
        schema_version: OPS_MODIFIER_PROPOSAL_SCHEMA_VERSION.to_string(),
        proposal_id: format!("ops_prop_{}", Uuid::new_v4().to_string().replace('-', "")),
        workflow_id: input.workflow_id.to_string(),
        target_kind: input.target_kind.to_string(),
        task_id,
        title: clean_required(input.title, "title")?,
        summary: clean_required(input.summary, "summary")?,
        rationale: clean_required(input.rationale, "rationale")?,
        proposed_goal: clean_optional(input.proposed_goal),
        proposed_title: clean_optional(input.proposed_title),
        proposed_expected_output: clean_optional(input.proposed_expected_output),
        author: clean_optional(Some(input.author)).unwrap_or_else(|| "ops-web".to_string()),
        status: "pending".to_string(),
        created_at: Utc::now().to_rfc3339(),
        applied_at: None,
        applied_revision: None,
    };
    store.record_event(
        &proposal.workflow_id,
        OPS_MODIFIER_PROPOSAL_CREATED_EVENT,
        &serde_json::to_value(&proposal)?,
    )?;

    Ok(OpsModifierProposalReport {
        status: "modifier_proposal_created".to_string(),
        proposal,
    })
}

pub fn apply_modifier_proposal(
    store: &ForgeStore,
    proposal_id: &str,
    origin: &str,
) -> Result<OpsModifierApplyReport> {
    let lane = load_modifier_lane(store)?;
    let proposal = lane
        .proposals
        .into_iter()
        .find(|proposal| proposal.proposal_id == proposal_id)
        .with_context(|| format!("modifier proposal not found: {proposal_id}"))?;
    if proposal.status != "pending" {
        bail!(
            "modifier proposal {} is not pending; current status is {}",
            proposal.proposal_id,
            proposal.status
        );
    }
    ensure_workflow_policy(store, &proposal.workflow_id, "ops modifier apply")?;

    let applied_at = Utc::now().to_rfc3339();
    let (revision, mutation) = match proposal.target_kind.as_str() {
        "workflow_goal" => {
            let goal = proposal
                .proposed_goal
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .context("workflow goal proposal has no proposed_goal")?;
            let report = update_workflow_goal(store, &proposal.workflow_id, goal, origin)?;
            (report.revision, serde_json::to_value(report)?)
        }
        "task_node" => {
            let task_id = proposal
                .task_id
                .as_deref()
                .context("task node proposal has no task_id")?;
            let report = update_workflow_task(
                store,
                &proposal.workflow_id,
                WorkflowTaskUpdateInput {
                    task_id,
                    title: proposal.proposed_title.as_deref(),
                    goal: proposal.proposed_goal.as_deref(),
                    expected_output: proposal.proposed_expected_output.as_deref(),
                    origin,
                },
            )?;
            (report.revision, serde_json::to_value(report)?)
        }
        other => bail!("unsupported modifier target_kind `{other}`"),
    };

    store.record_event(
        &proposal.workflow_id,
        OPS_MODIFIER_PROPOSAL_APPLIED_EVENT,
        &serde_json::json!({
            "proposal_id": proposal.proposal_id,
            "workflow_id": proposal.workflow_id,
            "target_kind": proposal.target_kind,
            "task_id": proposal.task_id,
            "origin": origin,
            "applied_at": applied_at,
            "revision": revision,
            "mutation": mutation
        }),
    )?;

    Ok(OpsModifierApplyReport {
        status: "modifier_proposal_applied".to_string(),
        proposal_id: proposal.proposal_id,
        workflow_id: proposal.workflow_id,
        target_kind: proposal.target_kind,
        task_id: proposal.task_id,
        origin: origin.to_string(),
        applied_at,
        revision,
        mutation,
    })
}

fn build_ops_creative_artifact(kind: &str, title: &str, origin: &str) -> Result<CreativeArtifact> {
    let title = clean_required(title, "title")?;
    let normalized = kind.trim().to_ascii_lowercase().replace('-', "_");
    match normalized.as_str() {
        "screen" | "page" | "wireframe" | "flow" => Ok(CreativeArtifact::new_screen(
            &title,
            ScreenSpec {
                schema_version: ir_schema_version(),
                width_px: 1440,
                height_px: 900,
                background: "#ffffff".to_string(),
                breakpoints: Vec::new(),
                elements: Vec::new(),
                interactions: Vec::new(),
            },
        )),
        "whiteboard" | "board" | "canvas" => Ok(CreativeArtifact::new_whiteboard(
            &title,
            WhiteboardSpec {
                schema_version: ir_schema_version(),
                width_px: 1920,
                height_px: 1080,
                background: "#f8fafc".to_string(),
                layers: Vec::new(),
                sticky_notes: Vec::new(),
                drawings: Vec::new(),
                text_blocks: Vec::new(),
                images: Vec::new(),
            },
        )),
        "document" | "doc" | "page_doc" => Ok(CreativeArtifact::new_document(
            &title,
            DocumentSpec {
                schema_version: ir_schema_version(),
                title: title.clone(),
                author: origin.to_string(),
                front_matter: BTreeMap::new(),
                sections: Vec::new(),
            },
        )),
        "slide_deck" | "slides" | "deck" => Ok(CreativeArtifact::new_slide_deck(
            &title,
            SlideDeckSpec {
                schema_version: ir_schema_version(),
                title: title.clone(),
                theme: "forge-ops".to_string(),
                slides: Vec::new(),
            },
        )),
        "component" | "component_manifest" => Ok(CreativeArtifact::new_component(
            &title,
            ComponentSpec {
                schema_version: ir_schema_version(),
                name: title.clone(),
                description: "Componente do sistema visual Forge Ops".to_string(),
                props: Vec::new(),
                variants: Vec::new(),
                states: Vec::new(),
                slots: Vec::new(),
                token_dependencies: Vec::new(),
                code_template: None,
            },
        )),
        other => bail!(
            "unknown visual artifact kind: {other}; expected one of: screen, page, wireframe, flow, whiteboard, document, slide_deck, component"
        ),
    }
}

fn build_ops_token_collection(name: &str) -> TokenCollection {
    let collection_name =
        clean_optional(Some(name)).unwrap_or_else(|| "Forge Ops Design System".to_string());
    TokenCollection {
        schema_version: ir_schema_version(),
        name: collection_name.clone(),
        description: format!("Design tokens for {collection_name}"),
        tokens: vec![
            ops_design_token(
                "color.primary",
                "#1f6feb",
                TokenType::Color,
                "Cor principal da interface operacional",
                "color",
            ),
            ops_design_token(
                "color.surface",
                "#ffffff",
                TokenType::Color,
                "Superfície base de painéis e cards",
                "color",
            ),
            ops_design_token(
                "color.text",
                "#18212f",
                TokenType::Color,
                "Texto primário de leitura",
                "color",
            ),
            ops_design_token(
                "spacing.md",
                "16px",
                TokenType::Spacing,
                "Espaçamento médio de layout",
                "spacing",
            ),
            ops_design_token(
                "radius.card",
                "8px",
                TokenType::BorderRadius,
                "Raio padrão de cards e painéis",
                "radius",
            ),
            ops_design_token(
                "typography.body",
                "system-ui",
                TokenType::FontFamily,
                "Fonte principal de produto",
                "typography",
            ),
        ],
        semantic_aliases: vec![
            SemanticAlias {
                name: "semantic.brand".to_string(),
                resolves_to: "color.primary".to_string(),
                description: format!("Marca operacional de {collection_name}"),
            },
            SemanticAlias {
                name: "semantic.surface".to_string(),
                resolves_to: "color.surface".to_string(),
                description: "Superfície padrão da UI".to_string(),
            },
            SemanticAlias {
                name: "semantic.body".to_string(),
                resolves_to: "color.text".to_string(),
                description: "Texto padrão da UI".to_string(),
            },
        ],
        modes: Vec::new(),
    }
}

fn ops_design_token(
    name: &str,
    value: &str,
    token_type: TokenType,
    description: &str,
    group: &str,
) -> DesignToken {
    DesignToken {
        name: name.to_string(),
        value: value.to_string(),
        token_type,
        description: description.to_string(),
        group: group.to_string(),
        extensions: BTreeMap::new(),
    }
}

pub fn serve_ops_console(store_path: PathBuf, host: &str, port: u16) -> Result<OpsServeReport> {
    let addon_dirs = default_addon_dirs();
    serve_ops_console_with_addon_dirs(store_path, host, port, &addon_dirs)
}

pub fn serve_ops_console_with_addon_dirs(
    store_path: PathBuf,
    host: &str,
    port: u16,
    addon_dirs: &[PathBuf],
) -> Result<OpsServeReport> {
    let listener = TcpListener::bind((host, port))
        .with_context(|| format!("failed to bind Forge ops server on {host}:{port}"))?;
    let addr = listener.local_addr()?;
    let report = OpsServeReport {
        status: "listening".to_string(),
        schema_version: "forge.ops.serve.v1".to_string(),
        bind_addr: addr.to_string(),
        url: format!("http://{addr}/"),
        local_only: addr.ip().is_loopback(),
        routes: ops_actions(),
    };
    println!("{}", serde_json::to_string_pretty(&report)?);

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                if let Err(error) = handle_stream(&store_path, addon_dirs, &mut stream) {
                    let response = error_response(500, "Internal Server Error", &error.to_string());
                    let _ = stream.write_all(&response.to_http_bytes());
                }
            }
            Err(error) => eprintln!("forge ops server connection error: {error}"),
        }
    }

    Ok(report)
}

pub fn handle_ops_http_request(store: &ForgeStore, request: &str) -> OpsHttpResponse {
    let addon_dirs = default_addon_dirs();
    handle_ops_http_request_with_addon_dirs(store, request, &addon_dirs)
}

pub fn handle_ops_http_request_with_addon_dirs(
    store: &ForgeStore,
    request: &str,
    addon_dirs: &[PathBuf],
) -> OpsHttpResponse {
    match route_ops_http_request(store, request, addon_dirs) {
        Ok(response) => response,
        Err(error) => error_response(400, "Bad Request", &error.to_string()),
    }
}

fn handle_stream(
    store_path: &PathBuf,
    addon_dirs: &[PathBuf],
    stream: &mut TcpStream,
) -> Result<()> {
    let mut buffer = vec![0; MAX_HTTP_REQUEST_BYTES];
    let bytes_read = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
    let store = ForgeStore::open(store_path)?;
    let response = handle_ops_http_request_with_addon_dirs(&store, &request, addon_dirs);
    stream.write_all(&response.to_http_bytes())?;
    Ok(())
}

fn route_ops_http_request(
    store: &ForgeStore,
    request: &str,
    addon_dirs: &[PathBuf],
) -> Result<OpsHttpResponse> {
    let parsed = ParsedRequest::parse(request)?;
    match (parsed.method.as_str(), parsed.path.as_str()) {
        ("GET", "/") => {
            let snapshot = build_ops_snapshot_with_addon_dirs(store, addon_dirs)?;
            Ok(html_response(render_ops_html(&snapshot)))
        }
        ("GET", "/api/snapshot") => {
            json_response(&build_ops_snapshot_with_addon_dirs(store, addon_dirs)?)
        }
        ("POST", "/api/run/drive") => {
            let run_id = parsed.required("run_id")?;
            let executor = parsed
                .params
                .get("executor")
                .map(String::as_str)
                .unwrap_or("ops-web");
            let report = drive_request(store, run_id, executor, 300, "ops-web")?;
            action_response("drive_run", &report)
        }
        ("POST", "/api/run/step") => {
            let run_id = parsed.required("run_id")?;
            let executor = parsed
                .params
                .get("executor")
                .map(String::as_str)
                .unwrap_or("ops-web");
            let report = step_request(store, run_id, executor, 300, "ops-web")?;
            action_response("step_run", &report)
        }
        ("POST", "/api/run/complete-task") => {
            let run_id = parsed.required("run_id")?;
            let task_id = parsed.required("task_id")?;
            let summary = parsed.required("summary")?;
            let executor = parsed
                .params
                .get("executor")
                .map(String::as_str)
                .unwrap_or("ops-web");
            let evidence_command = parsed.params.get("evidence_command").map(String::as_str);
            let report = complete_ready_task(
                store,
                run_id,
                RequestTaskCompletionInput {
                    task_id,
                    executor,
                    summary,
                    artifact_paths: &[],
                    evidence_command,
                    evidence_summary: Some(summary),
                    estimated_usd: 0.0,
                    tokens_in: 0,
                    tokens_out: 0,
                    ttl_seconds: 300,
                    origin: "ops-web",
                },
            )?;
            action_response("complete_task", &report)
        }
        ("POST", "/api/workflow/update-goal") => {
            let workflow_id = parsed.required("workflow_id")?;
            let goal = parsed.required("goal")?;
            let report = update_workflow_goal(store, workflow_id, goal, "ops-web")?;
            action_response("update_goal", &report)
        }
        ("POST", "/api/workflow/update-task") => {
            let workflow_id = parsed.required("workflow_id")?;
            let task_id = parsed.required("task_id")?;
            let title = parsed.params.get("title").map(String::as_str);
            let goal = parsed.params.get("goal").map(String::as_str);
            let expected_output = parsed.params.get("expected_output").map(String::as_str);
            let report = update_workflow_task(
                store,
                workflow_id,
                WorkflowTaskUpdateInput {
                    task_id,
                    title,
                    goal,
                    expected_output,
                    origin: "ops-web",
                },
            )?;
            action_response("update_task", &report)
        }
        ("POST", "/api/visual/create-artifact") => {
            let workflow_id = parsed.required("workflow_id")?;
            let kind = parsed.required("kind")?;
            let title = parsed.required("title")?;
            let origin = parsed
                .params
                .get("origin")
                .map(String::as_str)
                .unwrap_or("ops-web");
            let artifact = build_ops_creative_artifact(kind, title, origin)?;
            let report = attach_creative_artifact(store, workflow_id, artifact, origin)?;
            action_response("visual_create_artifact", &report)
        }
        ("POST", "/api/visual/set-tokens") => {
            let workflow_id = parsed.required("workflow_id")?;
            let name = parsed
                .params
                .get("name")
                .map(String::as_str)
                .unwrap_or("Forge Ops Design System");
            let origin = parsed
                .params
                .get("origin")
                .map(String::as_str)
                .unwrap_or("ops-web");
            let report = set_workflow_token_collection(
                store,
                workflow_id,
                build_ops_token_collection(name),
                origin,
            )?;
            action_response("visual_set_tokens", &report)
        }
        ("POST", "/api/visual/patch-token") => {
            let workflow_id = parsed.required("workflow_id")?;
            let token_name = parsed.required("token_name")?;
            let value = parsed.required("value")?;
            let origin = parsed
                .params
                .get("origin")
                .map(String::as_str)
                .unwrap_or("ops-web");
            let report = patch_workflow_token(store, workflow_id, token_name, value, origin)?;
            action_response("visual_patch_token", &report)
        }
        ("POST", "/api/visual/collaboration-event") => {
            let workflow_id = parsed.required("workflow_id")?;
            let artifact_id = parsed.required("artifact_id")?;
            let event_kind = parsed
                .params
                .get("event_kind")
                .or_else(|| parsed.params.get("kind"))
                .map(String::as_str)
                .unwrap_or("comment");
            let actor = parsed
                .params
                .get("actor")
                .map(String::as_str)
                .unwrap_or("human");
            let summary = parsed.required("summary")?;
            let target = parsed
                .params
                .get("target")
                .map(String::as_str)
                .unwrap_or("canvas");
            let selections = parsed
                .params
                .get("selection")
                .map(|selection| {
                    selection
                        .split(',')
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let origin = parsed
                .params
                .get("origin")
                .map(String::as_str)
                .unwrap_or("ops-web");
            let report = record_creative_collaboration_event(
                store,
                CreativeCollaborationEventRequest {
                    workflow_id: workflow_id.to_string(),
                    artifact_id: artifact_id.to_string(),
                    event_kind: event_kind.to_string(),
                    actor: actor.to_string(),
                    summary: summary.to_string(),
                    target: target.to_string(),
                    selections,
                    origin: origin.to_string(),
                },
            )?;
            action_response("visual_collaboration_event", &report)
        }
        ("POST", "/api/modifier/propose-goal") => {
            let workflow_id = parsed.required("workflow_id")?;
            let goal = parsed.required("goal")?;
            let title = parsed
                .params
                .get("title")
                .map(String::as_str)
                .unwrap_or("Proposta de objetivo");
            let summary = parsed
                .params
                .get("summary")
                .map(String::as_str)
                .unwrap_or(goal);
            let rationale = parsed
                .params
                .get("rationale")
                .map(String::as_str)
                .unwrap_or("Proposta criada pela lane modificadora");
            let author = parsed
                .params
                .get("author")
                .map(String::as_str)
                .unwrap_or("ops-web");
            let report = create_modifier_proposal(
                store,
                OpsModifierProposalInput {
                    workflow_id,
                    target_kind: "workflow_goal",
                    task_id: None,
                    title,
                    summary,
                    rationale,
                    proposed_goal: Some(goal),
                    proposed_title: None,
                    proposed_expected_output: None,
                    author,
                },
            )?;
            action_response("modifier_propose_goal", &report)
        }
        ("POST", "/api/modifier/propose-task") => {
            let workflow_id = parsed.required("workflow_id")?;
            let task_id = parsed.required("task_id")?;
            let title = parsed
                .params
                .get("proposal_title")
                .map(String::as_str)
                .unwrap_or("Proposta de atualização de node");
            let summary = parsed
                .params
                .get("summary")
                .map(String::as_str)
                .unwrap_or("Atualizar node durante a operação");
            let rationale = parsed
                .params
                .get("rationale")
                .map(String::as_str)
                .unwrap_or("Proposta criada pela lane modificadora");
            let author = parsed
                .params
                .get("author")
                .map(String::as_str)
                .unwrap_or("ops-web");
            let report = create_modifier_proposal(
                store,
                OpsModifierProposalInput {
                    workflow_id,
                    target_kind: "task_node",
                    task_id: Some(task_id),
                    title,
                    summary,
                    rationale,
                    proposed_goal: parsed.params.get("goal").map(String::as_str),
                    proposed_title: parsed.params.get("node_title").map(String::as_str),
                    proposed_expected_output: parsed
                        .params
                        .get("expected_output")
                        .map(String::as_str),
                    author,
                },
            )?;
            action_response("modifier_propose_task", &report)
        }
        ("POST", "/api/modifier/apply") => {
            let proposal_id = parsed.required("proposal_id")?;
            let origin = parsed
                .params
                .get("origin")
                .map(String::as_str)
                .unwrap_or("ops-web");
            let report = apply_modifier_proposal(store, proposal_id, origin)?;
            action_response("modifier_apply", &report)
        }
        _ => Ok(error_response(404, "Not Found", "unknown Forge ops route")),
    }
}

fn action_response<T: Serialize>(action: &str, result: &T) -> Result<OpsHttpResponse> {
    json_response(&OpsActionReport {
        status: "ok".to_string(),
        schema_version: OPS_ACTION_SCHEMA_VERSION.to_string(),
        action: action.to_string(),
        result: serde_json::to_value(result)?,
    })
}

pub fn render_ops_html(snapshot: &OpsSnapshot) -> String {
    let mut rows = String::new();
    for workflow in &snapshot.registry.workflows {
        let run_ids = if workflow.run_ids.is_empty() {
            "none".to_string()
        } else {
            workflow.run_ids.join(", ")
        };
        rows.push_str(&format!(
            "<tr><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}/{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(&workflow.workflow_id),
            escape_html(&workflow.workflow_status),
            escape_html(&workflow.lifecycle_state),
            escape_html(&workflow.runtime.lifecycle_kind),
            escape_html(&workflow.runtime.operator_action),
            workflow.task_summary.completed,
            workflow.task_summary.total,
            workflow.active_run_count,
            escape_html(&run_ids),
            escape_html(&workflow.outcome_status.status),
            escape_html(&truncate(&workflow.current_goal, 120)),
        ));
    }
    let mut visual_sections = String::new();
    for workflow in &snapshot.visual_workflows {
        let design = &workflow.design_surface;
        let mut artifact_items = String::new();
        for artifact in &design.artifacts {
            artifact_items.push_str(&format!(
                "<li><code>{}</code> <strong>{}</strong> <span class=\"badge\">{}</span> <small>{} presença, {} comentários, {} patches</small></li>",
                escape_html(&artifact.artifact_id),
                escape_html(&artifact.title),
                escape_html(&artifact.kind),
                artifact.active_presence_count,
                artifact.comment_count,
                artifact.patch_event_count,
            ));
        }
        if artifact_items.is_empty() {
            artifact_items.push_str("<li><small>Nenhum artefato visual criado.</small></li>");
        }
        let mut token_items = String::new();
        for token in &design.tokens {
            token_items.push_str(&format!(
                "<li><code>{}</code> = <strong>{}</strong> <span class=\"badge\">{}</span></li>",
                escape_html(&token.name),
                escape_html(&token.value),
                escape_html(&token.token_type),
            ));
        }
        if token_items.is_empty() {
            token_items.push_str("<li><small>Nenhuma coleção de tokens criada.</small></li>");
        }
        let mut task_cards = String::new();
        for task in &workflow.tasks {
            let dependency_text = if task.dependencies.is_empty() {
                "sem dependências".to_string()
            } else {
                task.dependencies.join(", ")
            };
            let mut subtasks = String::new();
            for subtask in &task.subtasks {
                subtasks.push_str(&format!(
                    "<li><span class=\"badge\">{}</span> {} <small>{}</small></li>",
                    escape_html(&subtask.status),
                    escape_html(&subtask.title),
                    escape_html(&truncate(&subtask.definition_of_done.join("; "), 120)),
                ));
            }
            if subtasks.is_empty() {
                subtasks.push_str("<li><small>Sem subtarefas registradas.</small></li>");
            }
            task_cards.push_str(&format!(
                "<article class=\"task-card\"><div class=\"task-head\"><strong>{}</strong><span class=\"badge status-{}\">{}</span></div><p>{}</p><div class=\"task-meta\"><span>{}</span><span>{}</span></div><ul>{}</ul></article>",
                escape_html(&task.title),
                escape_html(&task.status),
                escape_html(&task.status),
                escape_html(&truncate(&task.expected_output, 160)),
                escape_html(&task.executor),
                escape_html(&dependency_text),
                subtasks,
            ));
        }
        visual_sections.push_str(&format!(
            r##"<section class="workflow-visual"><h3><code>{}</code></h3><p>{}</p><div class="design-strip"><span>whiteboards: {}</span><span>telas/wireframes/fluxos: {}</span><span>componentes: {}</span><span>docs: {}</span><span>tokens: {}</span><span>colaboração: {} comentários</span></div><div class="visual-panels"><div><h4>Artefatos visuais</h4><ul class="compact-list">{}</ul></div><div><h4>Tokens</h4><ul class="compact-list">{}</ul></div></div><div class="visual-actions"><form method="post" action="/api/visual/create-artifact"><input type="hidden" name="workflow_id" value="{}"><label>Criar artefato visual</label><select name="kind"><option value="whiteboard">Whiteboard</option><option value="screen">Tela</option><option value="wireframe">Wireframe</option><option value="flow">Fluxo</option><option value="component">Componente</option><option value="document">Documento</option><option value="slide_deck">Slides</option></select><input name="title" placeholder="Título do artefato"><input name="origin" value="ops-web"><button type="submit">Criar artefato visual</button></form><form method="post" action="/api/visual/set-tokens"><input type="hidden" name="workflow_id" value="{}"><label>Sistema de design</label><input name="name" value="Forge Ops Design System"><input name="origin" value="ops-web"><button type="submit">Criar tokens base</button></form><form method="post" action="/api/visual/patch-token"><input type="hidden" name="workflow_id" value="{}"><label>Atualizar token</label><input name="token_name" placeholder="color.primary"><input name="value" placeholder="#1f6feb"><input name="origin" value="ops-web"><button type="submit">Atualizar token</button></form><form method="post" action="/api/visual/collaboration-event"><input type="hidden" name="workflow_id" value="{}"><label>Registrar colaboração</label><input name="artifact_id" placeholder="artifact_id"><select name="event_kind"><option value="comment">Comentário</option><option value="presence">Presença</option><option value="patch">Patch</option><option value="conflict">Conflito</option><option value="rollback">Rollback</option></select><input name="actor" value="human"><input name="target" value="canvas"><input name="selection" placeholder="seleção opcional"><textarea name="summary" placeholder="Comentário, instrução de patch ou observação"></textarea><input name="origin" value="ops-web"><button type="submit">Registrar colaboração</button></form></div><div class="task-board">{}</div></section>"##,
            escape_html(&workflow.workflow_id),
            escape_html(&truncate(&workflow.goal, 180)),
            design.whiteboard_count,
            design.screen_count,
            design.component_count,
            design.document_count + design.slide_deck_count,
            design.token_count,
            design.comment_count,
            artifact_items,
            token_items,
            escape_html(&workflow.workflow_id),
            escape_html(&workflow.workflow_id),
            escape_html(&workflow.workflow_id),
            escape_html(&workflow.workflow_id),
            task_cards,
        ));
    }
    if visual_sections.is_empty() {
        visual_sections.push_str("<p>Nenhum workflow visual disponível.</p>");
    }
    let mut addon_view_cards = String::new();
    let mut addon_view_rows = String::new();
    for entry in &snapshot.addon_views.views {
        let layout = &entry.view.layout;
        let zone = if layout.zone.trim().is_empty() {
            "main"
        } else {
            layout.zone.as_str()
        };
        let width = if layout.width.trim().is_empty() {
            "auto"
        } else {
            layout.width.as_str()
        };
        let density = if layout.density.trim().is_empty() {
            "standard"
        } else {
            layout.density.as_str()
        };
        let mut bindings = String::new();
        for binding in &entry.view.data_bindings {
            bindings.push_str(&format!(
                "<li><code>{}</code> via <code>{}</code> <span>{}</span></li>",
                escape_html(&binding.id),
                escape_html(&binding.source),
                escape_html(&binding.scope),
            ));
        }
        if bindings.is_empty() {
            bindings.push_str("<li>Nenhum binding declarado.</li>");
        }
        let mut actions = String::new();
        for action in &entry.view.actions {
            actions.push_str(&format!(
                "<li><code>{}</code> {} <span>{} {}</span></li>",
                escape_html(&action.id),
                escape_html(&action.label),
                escape_html(&action.method),
                escape_html(&action.target),
            ));
        }
        if actions.is_empty() {
            actions.push_str("<li>Nenhuma ação declarada.</li>");
        }
        addon_view_cards.push_str(&format!(
            "<section class=\"addon-view-card\"><div class=\"addon-view-head\"><div><h3>{}</h3><code>{}</code></div><span>{}</span></div><div class=\"design-strip\"><span>addon: {}</span><span>tipo: {}</span><span>zona: {}</span><span>ordem: {}</span><span>largura: {}</span><span>densidade: {}</span></div><p>{}</p><div class=\"visual-panels\"><div><h4>Data bindings</h4><ul class=\"compact-list\">{}</ul></div><div><h4>Ações</h4><ul class=\"compact-list\">{}</ul></div></div></section>",
            escape_html(&entry.view.title),
            escape_html(&entry.view.id),
            escape_html(&entry.view.surface),
            escape_html(&entry.addon_id),
            escape_html(&entry.view.view_type),
            escape_html(zone),
            entry.view.layout.order,
            escape_html(width),
            escape_html(density),
            escape_html(&entry.view.component),
            bindings,
            actions,
        ));
        addon_view_rows.push_str(&format!(
            "<tr><td><code>{}</code></td><td>{}</td><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(&entry.addon_id),
            escape_html(&entry.addon_name),
            escape_html(&entry.view.id),
            escape_html(&entry.view.title),
            escape_html(&entry.view.surface),
            escape_html(&entry.view.view_type),
            escape_html(zone),
        ));
    }
    if addon_view_cards.is_empty() {
        addon_view_cards.push_str("<p>Nenhuma view ativa de Addon para o console ops.</p>");
    }
    if addon_view_rows.is_empty() {
        addon_view_rows.push_str(
            "<tr><td colspan=\"7\">Nenhuma view ativa de Addon para o console ops.</td></tr>",
        );
    }
    let mut addon_observability_rows = String::new();
    for entry in &snapshot.addon_observability.addons {
        let consumed = if entry.event_flow.consumed_event_types.is_empty() {
            "none".to_string()
        } else {
            entry.event_flow.consumed_event_types.join(", ")
        };
        let emitted = if entry.event_flow.emitted_event_types.is_empty() {
            "none".to_string()
        } else {
            entry.event_flow.emitted_event_types.join(", ")
        };
        addon_observability_rows.push_str(&format!(
            "<tr><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}/{}/{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(&entry.addon_id),
            escape_html(&entry.addon_lifecycle),
            entry.capability_count,
            entry.permission_count,
            entry.runtime_contract_count,
            entry.event_adapter_count,
            entry.dispatches.queued_count,
            entry.dispatches.blocked_count,
            entry.dispatches.needs_external_worker_count,
            escape_html(&entry.permission_gate.status),
            escape_html(&truncate(&consumed, 120)),
            escape_html(&truncate(&emitted, 120)),
        ));
    }
    if addon_observability_rows.is_empty() {
        addon_observability_rows.push_str(
            "<tr><td colspan=\"10\">Nenhum Addon encontrado no catálogo ativo.</td></tr>",
        );
    }
    let mut proposal_rows = String::new();
    for proposal in &snapshot.modifier_lane.proposals {
        let target = proposal
            .task_id
            .as_ref()
            .map(|task_id| format!("{} / {}", proposal.target_kind, task_id))
            .unwrap_or_else(|| proposal.target_kind.clone());
        proposal_rows.push_str(&format!(
            "<tr><td><code>{}</code></td><td>{}</td><td><code>{}</code></td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(&proposal.proposal_id),
            escape_html(&proposal.status),
            escape_html(&proposal.workflow_id),
            escape_html(&target),
            escape_html(&proposal.title),
            escape_html(&truncate(&proposal.summary, 120)),
        ));
    }
    if proposal_rows.is_empty() {
        proposal_rows.push_str(
            "<tr><td colspan=\"6\">Nenhuma proposta da lane modificadora registrada.</td></tr>",
        );
    }

    format!(
        r#"<!doctype html>
<html lang="pt-BR">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Forge Ops</title>
  <style>
    body {{ font-family: system-ui, -apple-system, Segoe UI, sans-serif; margin: 24px; color: #18212f; background: #f7f8fb; }}
    h1 {{ margin: 0 0 8px; font-size: 28px; }}
    h2 {{ margin-top: 28px; font-size: 18px; }}
    table {{ width: 100%; border-collapse: collapse; background: white; border: 1px solid #d9deea; }}
    th, td {{ padding: 10px 12px; border-bottom: 1px solid #e8ecf4; text-align: left; vertical-align: top; font-size: 14px; }}
    th {{ background: #eef2f7; }}
    code {{ font-size: 12px; }}
    form {{ display: grid; gap: 8px; max-width: 760px; margin: 12px 0; }}
    input, textarea, select, button {{ font: inherit; padding: 8px 10px; border: 1px solid #cbd3df; border-radius: 6px; }}
    button {{ width: fit-content; background: #1f6feb; color: white; border-color: #1f6feb; cursor: pointer; }}
    label {{ font-size: 12px; font-weight: 650; color: #344054; }}
    .summary {{ display: flex; gap: 12px; flex-wrap: wrap; margin: 16px 0; }}
    .pill {{ background: white; border: 1px solid #d9deea; border-radius: 999px; padding: 8px 12px; }}
    .section-note {{ max-width: 900px; color: #4b5563; }}
    .workflow-visual {{ margin: 16px 0 24px; padding: 16px; background: white; border: 1px solid #d9deea; border-radius: 8px; }}
    .workflow-visual h3 {{ margin: 0 0 8px; font-size: 15px; }}
    .workflow-visual h4 {{ margin: 0 0 8px; font-size: 13px; }}
    .visual-panels, .visual-actions {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 12px; margin-top: 12px; }}
    .visual-panels > div, .visual-actions form {{ border: 1px solid #e0e5ef; border-radius: 8px; padding: 12px; background: #fbfcfe; }}
    .compact-list {{ margin: 0; padding-left: 18px; }}
    .compact-list li {{ margin: 4px 0; }}
    .task-board {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(240px, 1fr)); gap: 12px; margin-top: 12px; }}
    .task-card {{ border: 1px solid #d9deea; border-radius: 8px; padding: 12px; background: #fbfcfe; }}
    .task-card p {{ margin: 8px 0; color: #4b5563; }}
    .task-card ul {{ margin: 8px 0 0; padding-left: 18px; }}
    .addon-view-card {{ margin: 16px 0; padding: 16px; background: white; border: 1px solid #d9deea; border-radius: 8px; }}
    .addon-view-head {{ display: flex; justify-content: space-between; gap: 12px; align-items: start; }}
    .addon-view-head h3 {{ margin: 0 0 4px; font-size: 15px; }}
    .task-head, .task-meta, .design-strip {{ display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }}
    .task-head {{ justify-content: space-between; }}
    .task-meta, .design-strip {{ color: #5f6b7a; font-size: 12px; }}
    .badge {{ display: inline-block; border: 1px solid #cbd3df; border-radius: 999px; padding: 2px 7px; font-size: 12px; background: #fff; }}
    .status-completed {{ background: #e8f5ee; border-color: #a9dbc0; }}
    .status-running {{ background: #fff7df; border-color: #ead37e; }}
    .status-blocked, .status-failed {{ background: #ffeceb; border-color: #f0b2ad; }}
  </style>
</head>
<body>
  <h1>Forge Ops</h1>
  <p>Operação assistida local: humano e IA podem observar workflows, dirigir runs e alterar objetivos em tempo real.</p>
  <div class="summary">
    <span class="pill">workflows: {}</span>
    <span class="pill">running: {}</span>
    <span class="pill">persistentes: {}</span>
    <span class="pill">efêmeros: {}</span>
    <span class="pill">generated: {}</span>
    <span class="pill">local-only: {}</span>
    <span class="pill">modifier pending: {}</span>
    <span class="pill">addons: {}</span>
    <span class="pill">addons enabled: {}</span>
    <span class="pill">addons unauthorized: {}</span>
    <span class="pill">addon views: {}</span>
  </div>
  <h2>Workflows</h2>
  <table>
    <thead><tr><th>Workflow</th><th>Status</th><th>Lifecycle</th><th>Runtime</th><th>Ação runtime</th><th>Tasks</th><th>Active runs</th><th>Runs</th><th>Outcome</th><th>Goal</th></tr></thead>
    <tbody>{}</tbody>
  </table>
  <h2>Visualização operacional</h2>
  <p class="section-note">Tarefas e subtarefas em formato visual, com resumo do workspace criativo para whiteboard, telas, componentes, páginas, tokens e colaboração humano+IA.</p>
  {}
  <h2>Views de Addons</h2>
  <p class="section-note">Composição dinâmica de UI/TUI/Ops declarada por Addons ativos para esta superfície.</p>
  {}
  <table>
    <thead><tr><th>Addon</th><th>Nome</th><th>View</th><th>Título</th><th>Surface</th><th>Tipo</th><th>Zona</th></tr></thead>
    <tbody>{}</tbody>
  </table>
  <h2>Observabilidade de Addons</h2>
  <p class="section-note">Visão consolidada de lifecycle, capabilities, permissões, contratos runtime, adapters de evento e uso do dispatch ledger por Addon.</p>
  <table>
    <thead><tr><th>Addon</th><th>Lifecycle</th><th>Capabilities</th><th>Permissões</th><th>Contratos runtime</th><th>Event adapters</th><th>Dispatch queued/blocked/worker</th><th>Permission gate</th><th>Eventos consumidos</th><th>Eventos emitidos</th></tr></thead>
    <tbody>{}</tbody>
  </table>
  <h2>Lane modificadora</h2>
  <p class="section-note">Trilha separada para uma IA estratégica ou operador humano propor mudanças de objetivo e nodes sem interromper a operação.</p>
  <table>
    <thead><tr><th>Proposta</th><th>Status</th><th>Workflow</th><th>Alvo</th><th>Título</th><th>Resumo</th></tr></thead>
    <tbody>{}</tbody>
  </table>
  <form method="post" action="/api/modifier/propose-goal">
    <input name="workflow_id" placeholder="workflow_id">
    <input name="title" value="Ajuste estratégico de objetivo">
    <textarea name="goal" placeholder="Objetivo proposto"></textarea>
    <textarea name="summary" placeholder="Resumo da proposta"></textarea>
    <textarea name="rationale" placeholder="Racional estratégico"></textarea>
    <input name="author" value="ops-web">
    <button type="submit">Propor objetivo</button>
  </form>
  <form method="post" action="/api/modifier/propose-task">
    <input name="workflow_id" placeholder="workflow_id">
    <input name="task_id" placeholder="task_id">
    <input name="proposal_title" value="Ajuste estratégico de node">
    <input name="node_title" placeholder="Novo título opcional">
    <textarea name="goal" placeholder="Novo objetivo do node opcional"></textarea>
    <input name="expected_output" placeholder="Novo output esperado opcional">
    <textarea name="summary" placeholder="Resumo da proposta"></textarea>
    <textarea name="rationale" placeholder="Racional estratégico"></textarea>
    <input name="author" value="ops-web">
    <button type="submit">Propor node</button>
  </form>
  <form method="post" action="/api/modifier/apply">
    <input name="proposal_id" placeholder="proposal_id">
    <input name="origin" value="ops-web">
    <button type="submit">Aplicar proposta</button>
  </form>
  <h2>Operar run</h2>
  <form method="post" action="/api/run/drive">
    <input name="run_id" placeholder="run_id">
    <input name="executor" value="ops-web">
    <button type="submit">Drive</button>
  </form>
  <form method="post" action="/api/run/step">
    <input name="run_id" placeholder="run_id">
    <input name="executor" value="ops-web">
    <button type="submit">Step determinístico</button>
  </form>
  <form method="post" action="/api/run/complete-task">
    <input name="run_id" placeholder="run_id">
    <input name="task_id" placeholder="task_id">
    <input name="executor" value="ops-web">
    <textarea name="summary" placeholder="Resumo/evidência do executor"></textarea>
    <input name="evidence_command" placeholder="comando ou gate de evidência">
    <button type="submit">Completar task com evidência</button>
  </form>
  <h2>Atualizar objetivo em tempo real</h2>
  <form method="post" action="/api/workflow/update-goal">
    <input name="workflow_id" placeholder="workflow_id">
    <textarea name="goal" placeholder="Novo objetivo"></textarea>
    <button type="submit">Atualizar objetivo</button>
  </form>
  <h2>Atualizar node em tempo real</h2>
  <form method="post" action="/api/workflow/update-task">
    <input name="workflow_id" placeholder="workflow_id">
    <input name="task_id" placeholder="task_id">
    <input name="title" placeholder="Novo título opcional">
    <textarea name="goal" placeholder="Novo objetivo do node opcional"></textarea>
    <input name="expected_output" placeholder="Novo output esperado opcional">
    <button type="submit">Atualizar node</button>
  </form>
</body>
</html>"#,
        snapshot.registry.summary.total,
        snapshot.registry.summary.running,
        snapshot.registry.summary.runtime.persistent_workflows,
        snapshot.registry.summary.runtime.ephemeral_workflows,
        snapshot.generated_at,
        snapshot.mode.local_only_by_default,
        snapshot.modifier_lane.pending_count,
        snapshot.addon_observability.addon_count,
        snapshot.addon_observability.enabled_count,
        snapshot.addon_observability.unauthorized_count,
        snapshot.addon_views.view_count,
        rows,
        visual_sections,
        addon_view_cards,
        addon_view_rows,
        addon_observability_rows,
        proposal_rows
    )
}

fn json_response<T: Serialize>(value: &T) -> Result<OpsHttpResponse> {
    Ok(OpsHttpResponse {
        status_code: 200,
        reason: "OK".to_string(),
        content_type: "application/json; charset=utf-8".to_string(),
        body: serde_json::to_vec_pretty(value)?,
    })
}

fn html_response(html: String) -> OpsHttpResponse {
    OpsHttpResponse {
        status_code: 200,
        reason: "OK".to_string(),
        content_type: "text/html; charset=utf-8".to_string(),
        body: html.into_bytes(),
    }
}

fn error_response(status_code: u16, reason: &str, message: &str) -> OpsHttpResponse {
    OpsHttpResponse {
        status_code,
        reason: reason.to_string(),
        content_type: "application/json; charset=utf-8".to_string(),
        body: serde_json::json!({
            "status": "error",
            "schema_version": "forge.ops.error.v1",
            "message": message
        })
        .to_string()
        .into_bytes(),
    }
}

impl OpsHttpResponse {
    fn to_http_bytes(&self) -> Vec<u8> {
        let header = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.status_code,
            self.reason,
            self.content_type,
            self.body.len()
        );
        let mut bytes = header.into_bytes();
        bytes.extend_from_slice(&self.body);
        bytes
    }
}

#[derive(Debug)]
struct ParsedRequest {
    method: String,
    path: String,
    params: BTreeMap<String, String>,
}

impl ParsedRequest {
    fn parse(request: &str) -> Result<Self> {
        let (head, body) = request.split_once("\r\n\r\n").unwrap_or((request, ""));
        let request_line = head.lines().next().context("missing HTTP request line")?;
        let mut parts = request_line.split_whitespace();
        let method = parts.next().context("missing HTTP method")?.to_string();
        let raw_target = parts.next().context("missing HTTP target")?;
        let (path, query) = raw_target.split_once('?').unwrap_or((raw_target, ""));
        let mut params = parse_form(query);
        if method == "POST" {
            params.extend(parse_form(body));
        }
        Ok(Self {
            method,
            path: path.to_string(),
            params,
        })
    }

    fn required(&self, key: &str) -> Result<&str> {
        self.params
            .get(key)
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
            .with_context(|| format!("missing required parameter `{key}`"))
    }
}

fn parse_form(input: &str) -> BTreeMap<String, String> {
    let mut params = BTreeMap::new();
    for pair in input.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        params.insert(percent_decode(key), percent_decode(value));
    }
    params
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                output.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                if let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                    if let Ok(value) = u8::from_str_radix(hex, 16) {
                        output.push(value);
                        i += 3;
                        continue;
                    }
                }
                output.push(bytes[i]);
                i += 1;
            }
            byte => {
                output.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&output).to_string()
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn truncate(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    let mut value = input.chars().take(max_chars).collect::<String>();
    value.push_str("...");
    value
}

fn clean_required(input: &str, field: &str) -> Result<String> {
    let value = input.trim();
    if value.is_empty() {
        bail!("missing required field `{field}`");
    }
    Ok(value.to_string())
}

fn clean_optional(input: Option<&str>) -> Option<String> {
    input
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn ops_actions() -> Vec<OpsActionSpec> {
    vec![
        action(
            "snapshot",
            "GET",
            "/api/snapshot",
            "Read the current operational snapshot.",
            false,
        ),
        action(
            "drive_run",
            "POST",
            "/api/run/drive",
            "Drive a run to its next safe action.",
            true,
        ),
        action(
            "step_run",
            "POST",
            "/api/run/step",
            "Auto-promote one ready deterministic task when safe.",
            true,
        ),
        action(
            "complete_task",
            "POST",
            "/api/run/complete-task",
            "Complete a ready task with executor evidence.",
            true,
        ),
        action(
            "update_goal",
            "POST",
            "/api/workflow/update-goal",
            "Mutate a workflow objective while processing is live.",
            true,
        ),
        action(
            "update_task",
            "POST",
            "/api/workflow/update-task",
            "Mutate a workflow task/node title, goal or expected output while processing is live.",
            true,
        ),
        action(
            "visual_create_artifact",
            "POST",
            "/api/visual/create-artifact",
            "Create a Forge-owned visual artifact such as a whiteboard, screen, wireframe, flow, component, document or slide deck.",
            true,
        ),
        action(
            "visual_set_tokens",
            "POST",
            "/api/visual/set-tokens",
            "Create or replace the workflow design-token collection from the visual ops console.",
            true,
        ),
        action(
            "visual_patch_token",
            "POST",
            "/api/visual/patch-token",
            "Patch one workflow design token and persist a bounded token change revision.",
            true,
        ),
        action(
            "visual_collaboration_event",
            "POST",
            "/api/visual/collaboration-event",
            "Record human or AI presence, comments, patches, conflicts or rollbacks on a visual artifact.",
            true,
        ),
        action(
            "modifier_propose_goal",
            "POST",
            "/api/modifier/propose-goal",
            "Create a pending strategic modifier proposal for a workflow objective.",
            true,
        ),
        action(
            "modifier_propose_task",
            "POST",
            "/api/modifier/propose-task",
            "Create a pending strategic modifier proposal for a workflow task/node.",
            true,
        ),
        action(
            "modifier_apply",
            "POST",
            "/api/modifier/apply",
            "Apply a pending modifier proposal as a live workflow mutation.",
            true,
        ),
    ]
}

fn action(
    id: &str,
    method: &str,
    path: &str,
    description: &str,
    mutates_workflow: bool,
) -> OpsActionSpec {
    OpsActionSpec {
        id: id.to_string(),
        method: method.to_string(),
        path: path.to_string(),
        description: description.to_string(),
        mutates_workflow,
    }
}
