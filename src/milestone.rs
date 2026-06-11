use crate::artifact::hex_sha256;
use crate::graph::create_workflow;
use crate::harness::{
    build_harness_bootstrap_report, run_cli_harness_exec, CliHarnessExecOptions,
    HarnessBootstrapOptions,
};
use crate::intent::parse_intent;
use crate::interactive::{build_interactive_harness, InteractiveHarnessOptions};
use crate::ir::{
    ir_schema_version, CreativeArtifact, DesignToken, DocumentSection, DocumentSpec, ScreenSpec,
    SemanticAlias, TokenCollection, TokenType,
};
use crate::patch::{
    build_patch_apply, build_patch_diff, build_patch_plan, build_patch_restore, build_patch_revert,
    build_patch_review, PatchApplyArtifactRef, PatchDiffOptions, PatchPlanArtifactRef,
};
use crate::request::{heartbeat_request, start_async_request, RunActivity};
use crate::schedule::{create_daily_goal_research_workflow, run_daily_goal_research_smoke};
use crate::storage::ForgeStore;
use crate::workflow::{
    attach_creative_artifact, attach_workflow_artifact, set_workflow_token_collection,
};
use anyhow::{bail, Result};
use serde::Serialize;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const MILESTONE_STATUS_SCHEMA_VERSION: &str = "forge.milestone.status.v1";
const MILESTONE_MANIFEST_SCHEMA_VERSION: &str = "forge.milestone.manifest.v1";
const SUPPORTED_MILESTONE: &str = "0.5";

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneStatusReport {
    pub schema_version: String,
    pub milestone: String,
    pub release_line_boundary: String,
    pub status_vocabulary: Vec<String>,
    pub summary: MilestoneStatusSummary,
    pub capabilities: Vec<MilestoneCapability>,
    pub promotion_decision: MilestonePromotionDecision,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneStatusSummary {
    pub implemented: usize,
    pub validated: usize,
    pub groundwork: usize,
    pub planned: usize,
    pub blocked: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneCapability {
    pub id: String,
    pub title: String,
    pub status: String,
    pub evidence: String,
    pub gap_before_promotion: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestonePromotionDecision {
    pub decision: String,
    pub promotable: bool,
    pub blocked_by: Vec<String>,
    pub reason: String,
    pub next_action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneManifestReport {
    pub schema_version: String,
    pub milestone: String,
    pub release_line_boundary: String,
    pub requirements: Vec<MilestoneRequirement>,
    pub completed_capabilities: Vec<MilestoneManifestCapability>,
    pub missing_capabilities: Vec<MilestoneManifestCapability>,
    pub validation_evidence: Vec<MilestoneManifestEvidence>,
    pub demos: Vec<MilestoneManifestDemo>,
    pub known_gaps: Vec<MilestoneManifestGap>,
    pub promotion_decision: MilestonePromotionDecision,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneRequirement {
    pub capability_id: String,
    pub title: String,
    pub status: String,
    pub required_evidence: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneManifestCapability {
    pub id: String,
    pub title: String,
    pub status: String,
    pub promotion_ready: bool,
    pub evidence: String,
    pub gap_before_promotion: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneManifestEvidence {
    pub capability_id: String,
    pub status: String,
    pub summary: String,
    pub validation_state: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneManifestDemo {
    pub capability_id: String,
    pub status: String,
    pub summary: String,
    pub required_for_promotion: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneManifestGap {
    pub capability_id: String,
    pub status: String,
    pub gap: String,
    pub next_action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneResearchReport {
    pub schema_version: String,
    pub status: String,
    pub milestone: String,
    pub artifact_path: String,
    pub source_count: usize,
    pub sources: Vec<MilestoneResearchSource>,
    pub local_skill_inputs: Vec<MilestoneResearchSource>,
    pub findings: Vec<MilestoneResearchFinding>,
    pub validation_gates: Vec<MilestoneResearchGate>,
    pub workflow_templates: Vec<MilestoneResearchTemplate>,
    pub lean_governance: Vec<MilestoneLeanDecision>,
    pub promotion_impact: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneResearchSource {
    pub label: String,
    pub url_or_path: String,
    pub evidence: String,
    pub forge_implication: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneResearchFinding {
    pub id: String,
    pub title: String,
    pub source_labels: Vec<String>,
    pub finding: String,
    pub forge_runtime_rule: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneResearchGate {
    pub id: String,
    pub title: String,
    pub validates: String,
    pub failure_condition: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneResearchTemplate {
    pub id: String,
    pub title: String,
    pub stages: Vec<String>,
    pub deterministic_nodes: Vec<String>,
    pub ai_nodes: Vec<String>,
    pub human_gates: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneLeanDecision {
    pub id: String,
    pub decision: String,
    pub accepted_complexity: String,
    pub rejected_complexity: String,
    pub evidence_metric: String,
}

pub fn build_milestone_status(version: &str) -> Result<MilestoneStatusReport> {
    let version = version.trim();
    if version != SUPPORTED_MILESTONE {
        bail!("unsupported milestone {version}; currently supported: {SUPPORTED_MILESTONE}");
    }

    let capabilities = forge_05_capabilities();
    let summary = summarize_capabilities(&capabilities);
    let blocked_by = capabilities
        .iter()
        .filter(|capability| !is_promotion_ready_status(&capability.status))
        .map(|capability| capability.id.clone())
        .collect::<Vec<_>>();
    let promotable = blocked_by.is_empty();

    Ok(MilestoneStatusReport {
        schema_version: MILESTONE_STATUS_SCHEMA_VERSION.to_string(),
        milestone: SUPPORTED_MILESTONE.to_string(),
        release_line_boundary:
            "0.4.x may ship scheduler, lineage, interactive and validation groundwork; 0.5 is the first line allowed to claim the AI-first creative runtime."
                .to_string(),
        status_vocabulary: status_vocabulary(),
        summary,
        capabilities,
        promotion_decision: MilestonePromotionDecision {
            decision: if promotable { "promote" } else { "fail" }.to_string(),
            promotable,
            blocked_by,
            reason: if promotable {
                "All required Forge 0.5 capabilities have implementation and validation evidence."
                    .to_string()
            } else {
                "Forge 0.5 promotion is blocked while any required capability remains planned, blocked or only groundwork."
                    .to_string()
            },
            next_action: if promotable {
                "Run an explicit human-controlled release promotion, version-boundary update and artifact bundle before changing the package line to 0.5."
                    .to_string()
            } else {
                "Implement the next planned creative runtime capability with tests, demos and milestone evidence before reconsidering 0.5 promotion."
                    .to_string()
            },
        },
    })
}

pub fn build_milestone_research(version: &str) -> Result<MilestoneResearchReport> {
    let version = version.trim();
    if version != SUPPORTED_MILESTONE {
        bail!("unsupported milestone {version}; currently supported: {SUPPORTED_MILESTONE}");
    }

    let sources = research_sources();
    let local_skill_inputs = local_research_inputs();

    Ok(MilestoneResearchReport {
        schema_version: "forge.milestone.research.v1".to_string(),
        status: "validated".to_string(),
        milestone: SUPPORTED_MILESTONE.to_string(),
        artifact_path: "docs/research/forge-0.5-creative-runtime-source-research.md".to_string(),
        source_count: sources.len() + local_skill_inputs.len(),
        sources,
        local_skill_inputs,
        findings: research_findings(),
        validation_gates: research_validation_gates(),
        workflow_templates: research_workflow_templates(),
        lean_governance: research_lean_decisions(),
        promotion_impact:
            "The required Forge 0.5 research baseline is now source-grounded and converted into Forge-owned gates and templates; promotion remains controlled by the full milestone manifest rather than by this report alone."
                .to_string(),
    })
}

pub fn build_milestone_manifest(version: &str) -> Result<MilestoneManifestReport> {
    let status = build_milestone_status(version)?;
    let requirements = status
        .capabilities
        .iter()
        .map(|capability| MilestoneRequirement {
            capability_id: capability.id.clone(),
            title: capability.title.clone(),
            status: capability.status.clone(),
            required_evidence: required_evidence_for(&capability.id).to_string(),
        })
        .collect::<Vec<_>>();
    let completed_capabilities = status
        .capabilities
        .iter()
        .filter(|capability| is_promotion_ready_status(&capability.status))
        .map(manifest_capability)
        .collect::<Vec<_>>();
    let missing_capabilities = status
        .capabilities
        .iter()
        .filter(|capability| !is_promotion_ready_status(&capability.status))
        .map(manifest_capability)
        .collect::<Vec<_>>();
    let validation_evidence = status
        .capabilities
        .iter()
        .filter(|capability| capability.status != "planned")
        .map(|capability| MilestoneManifestEvidence {
            capability_id: capability.id.clone(),
            status: capability.status.clone(),
            summary: capability.evidence.clone(),
            validation_state: if is_promotion_ready_status(&capability.status) {
                "promotion_ready"
            } else {
                "groundwork_only"
            }
            .to_string(),
        })
        .collect::<Vec<_>>();
    let demos = status
        .capabilities
        .iter()
        .filter(|capability| is_demo_related(capability))
        .map(|capability| MilestoneManifestDemo {
            capability_id: capability.id.clone(),
            status: capability.status.clone(),
            summary: capability.evidence.clone(),
            required_for_promotion: true,
        })
        .collect::<Vec<_>>();
    let known_gaps = status
        .capabilities
        .iter()
        .filter(|capability| !is_promotion_ready_status(&capability.status))
        .map(|capability| MilestoneManifestGap {
            capability_id: capability.id.clone(),
            status: capability.status.clone(),
            gap: capability.gap_before_promotion.clone(),
            next_action: next_action_for_gap(&capability.id).to_string(),
        })
        .collect::<Vec<_>>();

    Ok(MilestoneManifestReport {
        schema_version: MILESTONE_MANIFEST_SCHEMA_VERSION.to_string(),
        milestone: status.milestone,
        release_line_boundary: status.release_line_boundary,
        requirements,
        completed_capabilities,
        missing_capabilities,
        validation_evidence,
        demos,
        known_gaps,
        promotion_decision: status.promotion_decision,
    })
}

const EXPORT_DEMO_SCHEMA_VERSION: &str = "forge.milestone.export_demo.v1";

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneExportDemoReport {
    pub status: String,
    pub schema_version: String,
    pub workflow_id: String,
    pub goal: String,
    pub screen_artifact_id: String,
    pub document_artifact_id: String,
    pub token_collection_name: String,
    pub creative_artifact_kinds: Vec<String>,
    pub demo_artifacts: Vec<MilestoneDemoArtifact>,
    pub lineage_chain: Vec<String>,
    pub export_evidence: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneDemoArtifact {
    pub kind: String,
    pub goal: String,
    pub status: String,
}

const CLI_DEMO_SCHEMA_VERSION: &str = "forge.milestone.cli_demo.v1";
const PATCH_LIFECYCLE_DEMO_SCHEMA_VERSION: &str = "forge.milestone.patch_lifecycle_demo.v1";
const EXECUTOR_PROJECT_DEMO_SCHEMA_VERSION: &str = "forge.milestone.executor_project_demo.v1";

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneCliDemoReport {
    pub status: String,
    pub schema_version: String,
    pub milestone: String,
    pub capability_id: String,
    pub workflow_id: String,
    pub promotion_ready: bool,
    pub external_resources_mutated: bool,
    pub flows: Vec<ReplacementCliDemoFlow>,
    pub remaining_gaps: Vec<String>,
    pub lean_governance: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReplacementCliDemoFlow {
    pub kind: String,
    pub title: String,
    pub workflow_id: String,
    pub run_id: Option<String>,
    pub run_status: String,
    pub completed_through_forge: bool,
    pub commands: Vec<String>,
    pub artifact_refs: Vec<String>,
    pub validation_evidence: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch_lifecycle: Option<MilestonePatchLifecycleDemo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor_project: Option<MilestoneExecutorProjectDemo>,
    pub activity: Option<RunActivity>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestonePatchLifecycleDemo {
    pub schema_version: String,
    pub status: String,
    pub target_path: String,
    pub repository_path: String,
    pub external_resources_mutated: bool,
    pub restored_to_clean_state: bool,
    pub plan_status: String,
    pub review_status: String,
    pub diff_status: String,
    pub apply_status: String,
    pub revert_status: String,
    pub restore_status: String,
    pub artifact_refs: Vec<MilestonePatchLifecycleArtifact>,
    pub gates: Vec<String>,
    pub commands: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestonePatchLifecycleArtifact {
    pub kind: String,
    pub schema_version: String,
    pub status: String,
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct MilestoneExecutorProjectDemo {
    pub schema_version: String,
    pub status: String,
    pub repository_path: String,
    pub target_path: String,
    pub target_sha256: String,
    pub bootstrap_status: String,
    pub bootstrap_config_status: String,
    pub shim_install_status: String,
    pub exec_status: String,
    pub exec_event_recorded: bool,
    pub exec_global_event_id: Option<i64>,
    pub project_policy_status: String,
    pub stdout_headroom_status: String,
    pub stdout_retrieval_ref: Option<String>,
    pub external_resources_mutated: bool,
    pub lineage: Vec<String>,
    pub summary: String,
}

pub fn build_milestone_export_demo(
    store: &ForgeStore,
    origin: &str,
) -> Result<MilestoneExportDemoReport> {
    let goal = "hackathon".to_string();
    let report = create_daily_goal_research_workflow(
        store,
        vec![goal.clone()],
        "America/Sao_Paulo",
        "0 8 * * *",
        origin,
    )?;
    let workflow_id = report.workflow_id.clone();

    let screen = CreativeArtifact::new_screen(
        "Demo Screen",
        ScreenSpec {
            schema_version: ir_schema_version(),
            width_px: 1440,
            height_px: 900,
            background: "#ffffff".to_string(),
            breakpoints: Vec::new(),
            elements: Vec::new(),
            interactions: Vec::new(),
        },
    );
    let screen_artifact_id = screen.id.clone();
    attach_creative_artifact(store, &workflow_id, screen, origin)?;

    let document = CreativeArtifact::new_document(
        "Demo Document",
        DocumentSpec {
            schema_version: ir_schema_version(),
            title: "Demo Document".to_string(),
            author: origin.to_string(),
            front_matter: BTreeMap::new(),
            sections: vec![DocumentSection {
                id: "sec_intro".to_string(),
                heading: "Introduction".to_string(),
                level: 1,
                content: Vec::new(),
                children: Vec::new(),
            }],
        },
    );
    let document_artifact_id = document.id.clone();
    attach_creative_artifact(store, &workflow_id, document, origin)?;

    let token_collection = TokenCollection {
        name: "export_demo_tokens".to_string(),
        schema_version: ir_schema_version(),
        description: "Export demo design tokens".to_string(),
        tokens: vec![
            DesignToken {
                name: "color.primary".to_string(),
                value: "#3B82F6".to_string(),
                token_type: TokenType::Color,
                description: "Primary brand color".to_string(),
                group: "color".to_string(),
                extensions: BTreeMap::new(),
            },
            DesignToken {
                name: "spacing.md".to_string(),
                value: "16px".to_string(),
                token_type: TokenType::Spacing,
                description: "Medium spacing".to_string(),
                group: "spacing".to_string(),
                extensions: BTreeMap::new(),
            },
        ],
        semantic_aliases: vec![SemanticAlias {
            name: "semantic.export_demo".to_string(),
            resolves_to: "color.primary".to_string(),
            description: "Export demo semantic alias".to_string(),
        }],
        modes: Vec::new(),
    };
    set_workflow_token_collection(store, &workflow_id, token_collection, origin)?;

    let schedule_status = format!(
        "scheduled_nodes={}, cron_nodes={}",
        report.schedule_summary.scheduled_nodes, report.schedule_summary.cron_nodes,
    );

    Ok(MilestoneExportDemoReport {
        status: "export_demo_generated".to_string(),
        schema_version: EXPORT_DEMO_SCHEMA_VERSION.to_string(),
        workflow_id: workflow_id.clone(),
        goal: goal.clone(),
        screen_artifact_id: screen_artifact_id.clone(),
        document_artifact_id: document_artifact_id.clone(),
        token_collection_name: "export_demo_tokens".to_string(),
        creative_artifact_kinds: vec![
            "ScreenSpec".to_string(),
            "DocumentSpec".to_string(),
        ],
        demo_artifacts: vec![
            MilestoneDemoArtifact {
                kind: "scheduled_workflow".to_string(),
                goal: goal.clone(),
                status: schedule_status,
            },
            MilestoneDemoArtifact {
                kind: "creative_screen".to_string(),
                goal: goal.clone(),
                status: "attached".to_string(),
            },
            MilestoneDemoArtifact {
                kind: "creative_document".to_string(),
                goal: goal.clone(),
                status: "attached".to_string(),
            },
            MilestoneDemoArtifact {
                kind: "design_tokens".to_string(),
                goal: goal.clone(),
                status: "set".to_string(),
            },
        ],
        lineage_chain: vec![
            format!("workflow_id:{workflow_id}"),
            format!("screen_artifact_id:{screen_artifact_id}"),
            format!("document_artifact_id:{document_artifact_id}"),
        ],
        export_evidence: "forge.milestone.export_demo.v1 creates a scheduled daily research workflow with creative screen and document artifacts, design token collection, and full lineage chain preservation. The workflow can be inspected via `forge inspect` or `forge schedule list`, creative artifacts via `forge workflow list-creative`, and tokens via `forge workflow get-tokens`. Markdown and PDF artifacts are generated through `forge schedule run-due` per goal.".to_string(),
    })
}

pub fn build_replacement_cli_demo(
    store: &ForgeStore,
    origin: &str,
) -> Result<MilestoneCliDemoReport> {
    let mut coding_workflow = create_workflow(parse_intent(
        "Demonstrate Forge-first coding task with bounded context, file patch, diff review and validation",
    ));
    store.save_workflow(&coding_workflow)?;

    let patch_review_path = store.base_dir().join("tmp").join(format!(
        "{}-replacement-cli-diff-review.md",
        coding_workflow.id
    ));
    if let Some(parent) = patch_review_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &patch_review_path,
        format!(
            "# Replacement-grade CLI coding demo\n\nworkflow_id: `{}`\norigin: `{}`\n\nThis deterministic artifact records the Forge-owned coding flow: context, handoff, patch intent, diff review, validation, artifact attachment and inspectability. It is demo evidence only; it does not edit arbitrary user files.\n",
            coding_workflow.id, origin
        ),
    )?;
    let attached = attach_workflow_artifact(
        store,
        &coding_workflow.id,
        &patch_review_path,
        "cli_demo",
        origin,
    )?;
    coding_workflow = store.load_workflow(&coding_workflow.id)?;
    let patch_lifecycle =
        build_replacement_cli_patch_lifecycle_demo(store, &coding_workflow.id, origin)?;

    let research = create_daily_goal_research_workflow(
        store,
        vec!["hackathon".to_string()],
        "America/Sao_Paulo",
        "0 8 * * *",
        origin,
    )?;
    let mut research_workflow = store.load_workflow(&research.workflow_id)?;
    let smoke = run_daily_goal_research_smoke(store, &mut research_workflow)?
        .expect("daily goal research workflow should contain configured goals");
    store.save_workflow(&research_workflow)?;
    let research_refs = smoke
        .goals
        .iter()
        .flat_map(|goal| {
            vec![
                goal.markdown_path.clone(),
                goal.pdf_path.clone(),
                format!(
                    "artifacts/{}/telegram-delivery-{}.json",
                    smoke.workflow_id, goal.goal
                ),
            ]
        })
        .collect::<Vec<_>>();

    let coding_task_id = coding_workflow
        .tasks
        .first()
        .map(|task| task.id.clone())
        .unwrap_or_else(|| "task-001".to_string());
    let mut harness_options = InteractiveHarnessOptions::default_for_current_dir();
    harness_options.executor = "codex".to_string();
    harness_options.project_root = Some(env::current_dir()?);
    harness_options.workflow_id = Some(coding_workflow.id.clone());
    harness_options.task_id = Some(coding_task_id.clone());
    harness_options.context_budget = Some(1200);
    harness_options.token_headroom = Some(true);
    let harness_panel = build_interactive_harness(store, harness_options)?;
    let executor_project_flow = build_replacement_cli_executor_project_demo(store, origin)?;

    let async_request = start_async_request(
        store,
        "Demonstrate long-running Forge-first async workflow with heartbeat and resume/status visibility",
        origin,
    )?;
    let heartbeat = heartbeat_request(
        store,
        &async_request.run_id,
        "forge_cli_demo",
        "replacement-grade CLI demo run is observable through Forge heartbeat",
        600,
        None,
        origin,
    )?;

    Ok(MilestoneCliDemoReport {
        status: "replacement_cli_demo_generated".to_string(),
        schema_version: CLI_DEMO_SCHEMA_VERSION.to_string(),
        milestone: SUPPORTED_MILESTONE.to_string(),
        capability_id: "replacement_grade_cli".to_string(),
        workflow_id: coding_workflow.id.clone(),
        promotion_ready: false,
        external_resources_mutated: false,
        flows: vec![
            ReplacementCliDemoFlow {
                kind: "coding_task".to_string(),
                title: "Forge-first coding task with bounded patch review".to_string(),
                workflow_id: coding_workflow.id.clone(),
                run_id: None,
                run_status: coding_workflow.status.clone(),
                completed_through_forge: true,
                commands: vec![
                    "forge plan --goal \"Demonstrate coding task\" --output json".to_string(),
                    "forge context --workflow <workflow-id> --task <task-id> --budget 1200 --strict --output json".to_string(),
                    "forge task handoff --workflow <workflow-id> --task <task-id> --executor codex --output json".to_string(),
                    "forge workflow attach-artifact --workflow <workflow-id> --path <diff-review.md> --kind cli_demo --origin forge_cli --output json".to_string(),
                    "forge validate --workflow <workflow-id> --output json".to_string(),
                    "forge inspect <workflow-id> --verbose --output json".to_string(),
                ],
                artifact_refs: vec![attached.artifact.path],
                validation_evidence: vec![
                    "bounded_context_required".to_string(),
                    "diff_review_required".to_string(),
                    "patch_lifecycle_artifacts_recorded".to_string(),
                    "patch_edit_intake_required".to_string(),
                    "approved_restore_returns_fixture_to_clean_state".to_string(),
                    "validation_before_promotion".to_string(),
                    "artifact_lineage_attached".to_string(),
                    "json_stable_commands".to_string(),
                ],
                patch_lifecycle: Some(patch_lifecycle),
                executor_project: None,
                activity: None,
                summary: "The coding demo proves the Forge CLI has a native flow shape for context routing, executor handoff, edit intake, patch plan/review/diff/apply/revert/restore artifact lineage, validation and inspection. It remains groundwork because richer interactive terminal editing still needs broader UX evidence.".to_string(),
            },
            ReplacementCliDemoFlow {
                kind: "harness_control".to_string(),
                title: "Forge-first harness, headroom and session lifecycle control".to_string(),
                workflow_id: coding_workflow.id.clone(),
                run_id: None,
                run_status: harness_panel.status.clone(),
                completed_through_forge: true,
                commands: vec![
                    "forge interactive harness --workflow <workflow-id> --task <task-id> --token-headroom --output json".to_string(),
                    "forge harness headroom-plan --executor codex --project-root <project-root> --context-budget 1200 --token-headroom --output json".to_string(),
                    "forge sessions --provider codex --output json".to_string(),
                    "forge interactive readiness --output json".to_string(),
                ],
                artifact_refs: Vec::new(),
                validation_evidence: vec![
                    "interactive_harness_ready".to_string(),
                    "headroom_plan_ready".to_string(),
                    "session_lifecycle_plan_ready".to_string(),
                    "token_headroom_enabled".to_string(),
                    "json_stable_headroom_commands".to_string(),
                    "no_child_cli_launched".to_string(),
                ],
                patch_lifecycle: None,
                executor_project: None,
                activity: None,
                summary: format!(
                    "The harness demo proves the replacement CLI can expose {} with {}, {} and a ready headroom-plan command without launching a child CLI.",
                    harness_panel.status,
                    harness_panel.headroom_plan.schema_version,
                    harness_panel.session_lifecycle_plan.schema_version
                ),
            },
            executor_project_flow,
            ReplacementCliDemoFlow {
                kind: "research_artifact".to_string(),
                title: "Forge-first research/artifact delivery".to_string(),
                workflow_id: research.workflow_id.clone(),
                run_id: None,
                run_status: smoke.status.clone(),
                completed_through_forge: true,
                commands: vec![
                    "forge schedule create-daily-goal-research --goal hackathon --timezone America/Sao_Paulo --cron \"0 8 * * *\" --origin forge_cli --output json".to_string(),
                    "forge run --workflow <workflow-id> --simulate --output json".to_string(),
                    "forge artifacts --workflow <workflow-id> --output json".to_string(),
                    "forge inspect <workflow-id> --verbose --output json".to_string(),
                ],
                artifact_refs: research_refs,
                validation_evidence: vec![
                    "markdown_report_generated".to_string(),
                    "pdf_report_generated".to_string(),
                    "telegram_delivery_recorded_without_secrets".to_string(),
                    "schedule_loop_lineage_preserved".to_string(),
                ],
                patch_lifecycle: None,
                executor_project: None,
                activity: None,
                summary: "The research demo uses the canonical daily Goal workflow to produce Markdown, PDF and Telegram delivery records through Forge-owned workflow semantics without live external delivery or secrets.".to_string(),
            },
            ReplacementCliDemoFlow {
                kind: "long_running_async".to_string(),
                title: "Forge-first async run handoff with heartbeat".to_string(),
                workflow_id: async_request.workflow_id.clone(),
                run_id: Some(async_request.run_id.clone()),
                run_status: heartbeat.status.clone(),
                completed_through_forge: true,
                commands: vec![
                    "forge request start --goal \"Long-running task\" --origin forge_cli --output json".to_string(),
                    "forge request heartbeat --run <run-id> --executor forge_cli_demo --summary \"executor alive\" --ttl-seconds 600 --origin forge_cli --output json".to_string(),
                    "forge request status --run <run-id> --output json".to_string(),
                    "forge request list --status running --output json".to_string(),
                    "forge inspect <workflow-id> --output json".to_string(),
                ],
                artifact_refs: Vec::new(),
                validation_evidence: vec![
                    "run_id_returned_immediately".to_string(),
                    "fresh_heartbeat_recorded".to_string(),
                    "workflow_lifecycle_marked_running".to_string(),
                    "resume_status_commands_available".to_string(),
                ],
                patch_lifecycle: None,
                executor_project: None,
                activity: Some(heartbeat.activity),
                summary: "The async demo proves Forge can start a durable run, mark it active through heartbeat, expose status/list/inspect visibility and keep orchestration authority during long-running executor work.".to_string(),
            },
        ],
        remaining_gaps: vec![
            "Broader brain-driven real-project coding/research workflows and TUI apply/approval ergonomics remain required before replacement-grade promotion.".to_string(),
            "Deeper provider/session lifecycle controls and richer terminal UX remain required.".to_string(),
            "This demo is deterministic evidence and does not claim Forge 0.5 promotion readiness.".to_string(),
        ],
        lean_governance: vec![
            "The demo reuses existing request, schedule, artifact and validation primitives instead of adding a separate agent shell architecture.".to_string(),
            "No Docker, Kubernetes, Knative, model install, device access, Telegram send or external resource mutation is performed.".to_string(),
        ],
    })
}

fn build_replacement_cli_patch_lifecycle_demo(
    store: &ForgeStore,
    workflow_id: &str,
    origin: &str,
) -> Result<MilestonePatchLifecycleDemo> {
    let artifact_store = open_absolute_store_view(store)?;
    let store = &artifact_store;
    let workflow = store.load_workflow(workflow_id)?;
    let task_id = workflow
        .tasks
        .first()
        .map(|task| task.id.clone())
        .ok_or_else(|| anyhow::anyhow!("replacement CLI demo workflow has no tasks"))?;
    let repository_path = prepare_patch_lifecycle_demo_repository(store, workflow_id)?;
    let target_path = "src/demo.rs".to_string();

    with_current_dir(&repository_path, || {
        let plan = build_patch_plan(
            store,
            workflow_id,
            &task_id,
            vec![target_path.clone()],
            "Update the demo fixture through Forge-owned patch lifecycle evidence.",
            origin,
        )?;
        let plan_artifact_path = patch_plan_artifact_path(store, &plan.artifact, "patch plan")?;

        fs::write(
            repository_path.join(&target_path),
            "pub fn demo_message() -> &'static str {\n    \"updated through forge patch lifecycle\"\n}\n",
        )?;

        let review = build_patch_review(
            store,
            workflow_id,
            &task_id,
            vec![target_path.clone()],
            origin,
            Some(&plan_artifact_path),
        )?;
        let diff = build_patch_diff(
            store,
            workflow_id,
            &task_id,
            vec![target_path.clone()],
            PatchDiffOptions {
                file_index: 0,
                hunk_index: 0,
                context_lines: 3,
                origin,
            },
        )?;
        let validation_commands = vec![format!("git diff --check -- {target_path}")];
        let apply = build_patch_apply(
            store,
            workflow_id,
            &task_id,
            vec![target_path.clone()],
            origin,
            Some(&plan_artifact_path),
            Some(&validation_commands),
        )?;
        let apply_artifact_path = patch_apply_artifact_path(store, &apply.artifact, "patch apply")?;
        let revert = build_patch_revert(
            store,
            workflow_id,
            &task_id,
            &apply_artifact_path,
            origin,
            None,
        )?;
        let revert_artifact_path =
            patch_apply_artifact_path(store, &revert.artifact, "patch revert")?;
        let restore = build_patch_restore(
            store,
            workflow_id,
            &task_id,
            &revert_artifact_path,
            "forge_cli_demo",
            true,
            origin,
        )?;
        let restored_to_clean_state = patch_demo_target_is_clean(&repository_path, &target_path)?;

        Ok(MilestonePatchLifecycleDemo {
            schema_version: PATCH_LIFECYCLE_DEMO_SCHEMA_VERSION.to_string(),
            status: if restored_to_clean_state {
                "patch_lifecycle_demo_ready"
            } else {
                "patch_lifecycle_demo_restore_incomplete"
            }
            .to_string(),
            target_path,
            repository_path: repository_path.display().to_string(),
            external_resources_mutated: false,
            restored_to_clean_state,
            plan_status: plan.status.clone(),
            review_status: review.status.clone(),
            diff_status: diff.status.clone(),
            apply_status: apply.status.clone(),
            revert_status: revert.status.clone(),
            restore_status: restore.status.clone(),
            artifact_refs: vec![
                summarize_plan_artifact("patch_plan", &plan.schema_version, &plan.status, &plan.artifact)?,
                summarize_patch_artifact("patch_review", &review.schema_version, &review.status, &review.artifact)?,
                summarize_patch_artifact("patch_diff", &diff.schema_version, &diff.status, &diff.artifact)?,
                summarize_patch_artifact("patch_apply", &apply.schema_version, &apply.status, &apply.artifact)?,
                summarize_patch_artifact("patch_revert", &revert.schema_version, &revert.status, &revert.artifact)?,
                summarize_patch_artifact("patch_restore", &restore.schema_version, &restore.status, &restore.artifact)?,
            ],
            gates: vec![
                "patch_edit_intake_required".to_string(),
                "plan_before_executor_edit".to_string(),
                "review_before_apply".to_string(),
                "diff_navigation_before_approval".to_string(),
                "validation_before_apply_record".to_string(),
                "rollback_proposal_before_restore".to_string(),
                "human_restore_approval_recorded".to_string(),
            ],
            commands: vec![
                "forge interactive patch-workbench --output json".to_string(),
                format!("forge patch plan --workflow {workflow_id} --task {task_id} --intent <intent> --path src/demo.rs --origin forge_cli --output json"),
                format!("forge patch review --workflow {workflow_id} --task {task_id} --path src/demo.rs --plan-artifact <patch-plan> --origin forge_cli --output json"),
                format!("forge patch diff --workflow {workflow_id} --task {task_id} --path src/demo.rs --file-index 0 --hunk-index 0 --output json"),
                format!("forge patch apply --workflow {workflow_id} --task {task_id} --path src/demo.rs --plan-artifact <patch-plan> --origin forge_cli --output json"),
                format!("forge patch revert --workflow {workflow_id} --task {task_id} --apply-artifact <patch-apply> --origin forge_cli --output json"),
                format!("forge patch restore --workflow {workflow_id} --task {task_id} --revert-artifact <patch-revert> --approved-by forge_cli_demo --confirm-restore --origin forge_cli --output json"),
            ],
            summary: "Deterministic fixture repo executed the full Forge patch lifecycle with plan, review, diff, apply record, revert proposal and approved restore artifacts, then returned the target file to a clean Git state.".to_string(),
        })
    })
}

fn build_replacement_cli_executor_project_demo(
    store: &ForgeStore,
    origin: &str,
) -> Result<ReplacementCliDemoFlow> {
    let request = start_async_request(
        store,
        "Demonstrate executor-driven project editing through Forge harness lineage",
        origin,
    )?;
    let workflow = store.load_workflow(&request.workflow_id)?;
    let task_id = workflow
        .tasks
        .first()
        .map(|task| task.id.clone())
        .ok_or_else(|| anyhow::anyhow!("executor project demo workflow has no tasks"))?;
    let project_root = store
        .base_dir()
        .join("tmp")
        .join(format!("{}-executor-project", workflow.id));
    if project_root.exists() {
        fs::remove_dir_all(&project_root)?;
    }
    fs::create_dir_all(project_root.join("src"))?;
    fs::write(
        project_root.join("README.md"),
        "# Forge executor project demo\n\nThis fixture is mutated only under Forge harness control.\n",
    )?;

    let shim_dir = project_root.join(".forge/shims");
    let bootstrap = build_harness_bootstrap_report(HarnessBootstrapOptions {
        shim_dir: &shim_dir,
        executor: "sh",
        project_root: &project_root,
        store_path: Some(store.path()),
        context_budget: 512,
        context_budget_source: "milestone_cli_demo",
        token_headroom: true,
        token_headroom_source: "milestone_cli_demo",
        apply: true,
        approved_by: Some("forge_cli_demo"),
        force: true,
    })?;

    let edit_script = "mkdir -p src && printf 'workflow=%s\\ntask=%s\\nrun=%s\\nharness=%s\\nmode=%s\\nheadroom=%s\\n' \"$FORGE_WORKFLOW_ID\" \"$FORGE_TASK_ID\" \"$FORGE_RUN_ID\" \"$FORGE_HARNESS\" \"$FORGE_HARNESS_MODE\" \"$FORGE_TOKEN_HEADROOM\" > src/executor-output.txt && cat src/executor-output.txt";
    let command = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        edit_script.to_string(),
    ];
    let receipt = run_cli_harness_exec(CliHarnessExecOptions {
        store: Some(store),
        executor: "sh",
        command: &command,
        forge_first: true,
        forge_first_source: "milestone_cli_demo",
        workflow_id: Some(&workflow.id),
        task_id: Some(&task_id),
        run_id: Some(&request.run_id),
        context_budget: 512,
        context_budget_source: "milestone_cli_demo",
        token_headroom: true,
        token_headroom_source: "milestone_cli_demo",
        require_token_headroom_for_forge_first: true,
        dry_run: false,
        allow_exec: true,
        project_root: Some(&project_root),
        cwd: Some(&project_root),
    })?;
    let target_path = "src/executor-output.txt";
    let target_bytes = fs::read(project_root.join(target_path))?;
    let target_sha256 = hex_sha256(&target_bytes);
    let stdout_headroom_status = receipt
        .stdout_headroom
        .as_ref()
        .map(|headroom| headroom.status.clone())
        .unwrap_or_else(|| "not_recorded".to_string());
    let stdout_retrieval_ref = receipt
        .stdout_headroom
        .as_ref()
        .map(|headroom| headroom.retrieval_ref.clone());
    let shim_install_status = bootstrap
        .shim_install
        .as_ref()
        .map(|report| report.status.clone())
        .unwrap_or_else(|| "not_installed".to_string());

    let executor_project = MilestoneExecutorProjectDemo {
        schema_version: EXECUTOR_PROJECT_DEMO_SCHEMA_VERSION.to_string(),
        status: if receipt.success == Some(true) && receipt.event_recorded {
            "executor_project_demo_completed"
        } else {
            "executor_project_demo_incomplete"
        }
        .to_string(),
        repository_path: project_root.display().to_string(),
        target_path: target_path.to_string(),
        target_sha256,
        bootstrap_status: bootstrap.status.clone(),
        bootstrap_config_status: bootstrap.config_write.status.clone(),
        shim_install_status,
        exec_status: receipt.status.clone(),
        exec_event_recorded: receipt.event_recorded,
        exec_global_event_id: receipt.global_event_id,
        project_policy_status: receipt.project_policy_status.clone(),
        stdout_headroom_status,
        stdout_retrieval_ref,
        external_resources_mutated: false,
        lineage: vec![
            format!("workflow_id:{}", workflow.id),
            format!("task_id:{task_id}"),
            format!("run_id:{}", request.run_id),
            format!("global_event_id:{}", receipt.global_event_id.unwrap_or_default()),
        ],
        summary: "The fixture project is edited by a guarded executor command after Forge writes project harness policy, requires lineage, applies token headroom and records the execution in the global event timeline.".to_string(),
    };

    Ok(ReplacementCliDemoFlow {
        kind: "executor_project".to_string(),
        title: "Executor-driven isolated project edit under Forge harness".to_string(),
        workflow_id: workflow.id,
        run_id: Some(request.run_id),
        run_status: receipt.status,
        completed_through_forge: true,
        commands: vec![
            "forge harness bootstrap --executor sh --shim-dir <project-root>/.forge/shims --project-root <project-root> --apply --approved-by forge_cli_demo --output json".to_string(),
            "forge harness exec --executor sh --project-root <project-root> --workflow <workflow-id> --task <task-id> --run <run-id> --forge-first --execute --allow-exec -- /bin/sh -c <project-edit-script>".to_string(),
            "forge events timeline --workflow <workflow-id> --output json".to_string(),
            "forge harness retrieve-headroom --ref <stdout-retrieval-ref> --output json".to_string(),
        ],
        artifact_refs: vec![project_root.join(target_path).display().to_string()],
        validation_evidence: vec![
            "bootstrap_applied_with_operator_approval".to_string(),
            "project_policy_requires_lineage".to_string(),
            "executor_mutated_isolated_project_under_harness".to_string(),
            "harness_exec_event_recorded".to_string(),
            "stdout_headroom_retrieval_available".to_string(),
            "external_resources_untouched".to_string(),
        ],
        patch_lifecycle: None,
        executor_project: Some(executor_project),
        activity: None,
        summary: "This flow closes part of the replacement-grade CLI gap by proving an executor can modify an isolated project through Forge-owned bootstrap, lineage policy, guarded execution, event recording and reversible stdout headroom.".to_string(),
    })
}

fn open_absolute_store_view(store: &ForgeStore) -> Result<ForgeStore> {
    let path = if store.path().is_absolute() {
        store.path().to_path_buf()
    } else {
        env::current_dir()?.join(store.path())
    };
    ForgeStore::open(path)
}

fn prepare_patch_lifecycle_demo_repository(
    store: &ForgeStore,
    workflow_id: &str,
) -> Result<PathBuf> {
    let repository_path = store
        .base_dir()
        .join("tmp")
        .join(format!("{workflow_id}-patch-lifecycle-repo"));
    if repository_path.exists() {
        fs::remove_dir_all(&repository_path)?;
    }
    fs::create_dir_all(repository_path.join("src"))?;
    fs::write(
        repository_path.join("src/demo.rs"),
        "pub fn demo_message() -> &'static str {\n    \"initial\"\n}\n",
    )?;
    run_demo_git(&repository_path, &["init", "-q"])?;
    run_demo_git(
        &repository_path,
        &["config", "user.email", "forge@example.com"],
    )?;
    run_demo_git(&repository_path, &["config", "user.name", "Forge CLI Demo"])?;
    run_demo_git(&repository_path, &["add", "src/demo.rs"])?;
    run_demo_git(&repository_path, &["commit", "-q", "-m", "initial fixture"])?;
    Ok(repository_path)
}

fn with_current_dir<T>(dir: &Path, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    let previous = env::current_dir()?;
    env::set_current_dir(dir)?;
    let result = operation();
    env::set_current_dir(previous)?;
    result
}

fn run_demo_git(repository_path: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repository_path)
        .output()?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn patch_demo_target_is_clean(repository_path: &Path, target_path: &str) -> Result<bool> {
    let status = Command::new("git")
        .args(["status", "--short", "--", target_path])
        .current_dir(repository_path)
        .output()?;
    if !status.status.success() {
        bail!(
            "git status failed while checking patch lifecycle restore: {}",
            String::from_utf8_lossy(&status.stderr)
        );
    }
    let content = fs::read_to_string(repository_path.join(target_path))?;
    Ok(status.stdout.is_empty() && content.contains("\"initial\""))
}

fn patch_plan_artifact_path(
    store: &ForgeStore,
    artifact: &Option<PatchPlanArtifactRef>,
    label: &str,
) -> Result<String> {
    let Some(artifact) = artifact else {
        bail!("{label} did not produce an artifact");
    };
    Ok(resolve_store_artifact_path(store, &artifact.path)
        .display()
        .to_string())
}

fn patch_apply_artifact_path(
    store: &ForgeStore,
    artifact: &Option<PatchApplyArtifactRef>,
    label: &str,
) -> Result<String> {
    let Some(artifact) = artifact else {
        bail!("{label} did not produce an artifact");
    };
    Ok(resolve_store_artifact_path(store, &artifact.path)
        .display()
        .to_string())
}

fn resolve_store_artifact_path(store: &ForgeStore, artifact_path: &str) -> PathBuf {
    let path = Path::new(artifact_path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        store.base_dir().join(path)
    }
}

fn summarize_plan_artifact(
    kind: &str,
    schema_version: &str,
    status: &str,
    artifact: &Option<PatchPlanArtifactRef>,
) -> Result<MilestonePatchLifecycleArtifact> {
    let Some(artifact) = artifact else {
        bail!("{kind} artifact is missing");
    };
    Ok(MilestonePatchLifecycleArtifact {
        kind: kind.to_string(),
        schema_version: schema_version.to_string(),
        status: status.to_string(),
        path: artifact.path.clone(),
        sha256: artifact.sha256.clone(),
        bytes: artifact.bytes,
    })
}

fn summarize_patch_artifact(
    kind: &str,
    schema_version: &str,
    status: &str,
    artifact: &Option<PatchApplyArtifactRef>,
) -> Result<MilestonePatchLifecycleArtifact> {
    let Some(artifact) = artifact else {
        bail!("{kind} artifact is missing");
    };
    Ok(MilestonePatchLifecycleArtifact {
        kind: kind.to_string(),
        schema_version: schema_version.to_string(),
        status: status.to_string(),
        path: artifact.path.clone(),
        sha256: artifact.sha256.clone(),
        bytes: artifact.bytes,
    })
}

fn forge_05_capabilities() -> Vec<MilestoneCapability> {
    vec![
        capability(
            "interactive_cli_baseline",
            "Interactive Forge CLI baseline",
            "validated",
            "0.4.97 validates the no-argument TTY home, slash-command catalog, conversational routing and retention decisions. Cycle 24 confirms all 14 required slash commands, conversational routing with direct-answer vs workflow classification, retention decisions with delete/retain/archive policy, and CLI contract tests for TTY/non-TTY behavior with 175 passing tests.",
            "Full terminal TUI loop and richer inline mode still need implementation evidence; autocomplete now has read-only CLI, MCP and dashboard evidence.",
        ),
        capability(
            "human_decision_form_nodes",
            "Human decision/form nodes",
            "validated",
            "0.4.98 validates choice prompts, form schemas, durable decisions, timeout state, pause/resume and inspect/list/status visibility. 0.4.104 exposes the same decision bridge through MCP create/list/answer/expire tools. Cycle 24 validates multi-choice, approve/reject/refine/combine, yes/no confirmations, risk acknowledgement, form with review-before-submit and save-as-template through CLI contract tests.",
            "Web UI, repeated-answer default promotion and richer TUI rendering remain planned.",
        ),
        capability(
            "scheduler_loop_subflow_foundation",
            "Scheduler/loop/subflow foundation",
            "validated",
            "0.4.92-0.4.100 validate cron nodes, loop state, due execution, missed-run policy, daily Goal research smoke artifacts and concurrent DAG execution with parallel wave scheduling. Cycle 32 adds node version boundaries: each AtomicTask carries a version field (default 1), `validation::version_boundary`/`version_boundary_changed` for comparison, and validation gates that reject zero-version or dependency-version-mismatch tasks with 5 passing tests.",
            "Production executor adapters for live research/page inspection remain planned.",
        ),
        capability(
            "creative_artifact_ir",
            "Creative artifact IR baseline",
            "validated",
            "0.4.102 validates ScreenSpec, WhiteboardSpec, DocumentSpec, SlideDeckSpec, ComponentSpec as first-class creative artifact types with serde round-trip, CLI attach/list/inspect, and workflow integration. Cycle 26 maintains validated status with passing tests.",
            "Declarative import/export, rendering adapters and full screen/whiteboard/document editing through the runtime remain for 0.5.",
        ),
        capability(
            "design_tokens",
            "Design systems/tokens",
            "validated",
            "0.4.102 validates DesignToken, TokenType, TokenCollection, SemanticAlias as serde-able types with CLI set-tokens/get-tokens and workflow integration. 0.4.125 adds the first token resolution engine for raw tokens, semantic aliases, mode overrides, impact preview, CLI/MCP resolve tools and targeted patch-by-intent without rewriting creative artifacts.",
            "Inheritance across token collections, rendered propagation previews and richer human edit preservation demos remain before 0.5 promotion.",
        ),
        capability(
            "componentization_ai_surfaces",
            "Componentization and AI-first UI surfaces",
            "validated",
            "0.4.102 validates ComponentSpec with props, variants, states, slots, token dependencies and code template as serde-able IR with PatchByIntent schema. 0.4.125 resolves token dependencies in creative artifacts and records targeted token patch diffs as PatchByIntent evidence.",
            "Rendered component preview, action registry generation and AI-driven component generation remain for 0.5.",
        ),
        capability(
            "live_collaboration",
            "Live collaboration",
            "validated",
            "0.4.98-0.4.104 validate human decision audit and MCP human interaction bridges. 0.4.127 adds Forge-owned creative collaboration state on artifacts with presence, cursors/selections, comments, patch streams, conflict records, rollbacks, audit history, CLI event/status commands, MCP collaboration tools and screen/document contract tests.",
            "Full browser live editing transport, multi-user conflict resolution UX and richer rollback visualization remain before a final 0.5 promotion claim.",
        ),
        capability(
            "research_artifact_baseline",
            "Research artifact baseline",
            "validated",
            "0.4.129 adds `forge milestone research` and MCP tool `forge.milestone.research` with a source-grounded comparison across Penpot, Stitch, v0, AG-UI, Impeccable, Figma MCP, Remotion, OBS and local creative/productivity skills. The research is converted into Forge-owned validation gates, creative workflow templates and lean governance decisions in `docs/research/forge-0.5-creative-runtime-source-research.md`.",
            "Keep the research artifact current as external creative/runtime protocols drift; no 0.5 promotion claim should bypass the full milestone manifest.",
        ),
        capability(
            "export_demo_baseline",
            "Export/demo baseline",
            "validated",
            "0.4.130 adds `forge milestone export-demo` as a structured export/demo surface that creates a scheduled daily research workflow with a screen creative artifact, a document creative artifact and a design token collection, proving design/tokens/component export lineage. The demo workflow can be inspected, its creative artifacts listed/inspected and its design tokens resolved/promoted. Daily Goal smoke produces Markdown/PDF artifacts and Telegram delivery records through Forge-owned workflow semantics across all cycles.",
            "Full rendered previews and richer browser-based editing demos remain for a later 0.5 milestone iteration.",
        ),
        capability(
            "replacement_grade_cli",
            "Replacement-grade Forge CLI",
            "groundwork",
            "0.4.x validates the no-argument interactive home, slash commands, conversational routing, human decisions, async run handoff and observability surfaces. 0.4.144 adds `forge milestone cli-demo` and MCP tool `forge.milestone.cli_demo`, which generate deterministic Forge-first demo evidence for coding, harness/headroom/session lifecycle control, research/artifact and long-running async flows, including `forge.milestone.patch_lifecycle_demo.v1` with plan/review/diff/apply/revert/restore artifact lineage in an isolated fixture repo. 0.4.145 adds executor-aware, runtime-aware and cost-sensitive routing classification to the interactive conversational router, plus creative artifact and design token dependency fields to `forge inspect` output. 0.4.146 adds registry-level run health summaries so `forge list` and `forge inspect` expose running, stale and missing-heartbeat runs even when `active_run_count` is zero. 0.4.148 adds process-liveness-aware run activity so a recorded live executor PID keeps long-running handoffs active after heartbeat TTL expiry instead of forcing stale recovery. 0.4.150 adds `forge patch plan` and MCP tool `forge.patch.plan` as a plan-only file-editing contract with repo-relative permission gates, file snapshots, diff-review commands, validation commands and workflow artifact lineage. 0.4.151 adds apply artifacts and guarded revert proposals so rollback intent is recorded without silently executing destructive file restores. 0.4.152 adds in-TUI `/patch plan`, `/patch apply` and `/patch revert` slash commands to the interactive REPL with human approval prompts before execution, plus two-token slash command routing support. 0.4.153 adds in-TUI `/context` and `/handoff` commands so operators can inspect bounded context routes and explicitly approve executor handoff lease acquisition from inside `forge`. 0.4.154 exposes `forge.interactive.home`, `forge.interactive.slash_commands` and `forge.interactive.route` through MCP so agents can inspect and use the same interactive command/chat routing model without taking over orchestration. The patch lifecycle now includes `forge patch review`, MCP `forge.patch.review` and `/patch review`, which persist `forge.patch_review.v1` evidence with Git diff/status/check summaries before apply approval while keeping source files unchanged, `forge patch diff`, MCP `forge.patch.diff` and `/patch diff`, which persist `forge.patch_diff.v1` evidence for read-only multi-file diff navigation, and `forge patch restore`, MCP `forge.patch.restore` and `/patch restore`, which persist `forge.patch_restore.v1` evidence for explicit, approved repo-local file restoration from a revert artifact. The interactive home now carries `forge.interactive.ui_composition.v1` with ordered regions, Core widgets, safe Addon widgets and refresh/inspection commands for TUI/web/agent dashboard composition, plus `forge.interactive.structured_logs.v1` with recent event sequence, workflow, category, severity, origin, correlation, observability and payload preview for timeline drill-downs; the dedicated `forge interactive readiness`/`forge.interactive.readiness` surface exposes executor, runtime, brain, shell, Forge-controlled surface and harness readiness with corrective commands before shell or handoff without loading the full home, the dedicated `forge interactive harness`/`forge.interactive.harness` surface exposes a consolidated harness center with mode, doctor, shim status, wrap-plan, `headroom_plan`, `session_lifecycle_plan` and token-headroom preview without loading the full home or executing child CLIs, the dedicated `forge interactive sessions`/`forge.interactive.sessions` surface exposes provider/session readiness, lifecycle state, per-session `operation_plan`, shell history commands and next lifecycle controls without opening or attaching shells, the dedicated `forge interactive command-palette`/`forge.interactive.command_palette` surface exposes grouped contextual navigation, workflow, patch, permission, harness, session and observability actions with mutation and approval flags without mutating state, `forge interactive action-registry`/`forge.interactive.action_registry` plus `/actions [query]` expose a strict action registry for TUI/web/agent clients, `forge interactive action-invocation`/`forge.interactive.action_invocation` plus `/action <action-id>` resolve one selected action into a non-executing invocation plan, the dedicated `forge interactive autocomplete`/`forge.interactive.autocomplete` surface exposes read-only slash-command, command-palette and `/action <partial>` action-id suggestions for partial operator input with score, source panel, mutation and approval flags, the dedicated `forge interactive patch-workbench`/`forge.interactive.patch_workbench` surface exposes Git status, file lanes, bounded inline `diff_preview`, multi-file `diff_review_queue`, `forge.interactive.patch_edit_intake.v1` required inputs and form readiness, diff stat/check, explicit `approval_flow` review/approval/rollback gates and permission-gated patch lifecycle commands for native file-editing and rich diff-review UI without mutating files, the dedicated `forge interactive permissions`/`forge.interactive.permissions` surface exposes tenant memberships, Addon permission authorizations, pending/timed-out human approvals and granular next-action commands without mutating state, the dedicated `forge interactive workflow-dag`/`forge.interactive.workflow_dag` surface exposes dependency nodes, edges, readiness, human waits and drill-down commands without loading the full home, the dedicated `forge interactive structured-logs`/`forge.interactive.structured_logs` surface exposes the same log contract without loading the full home, and the home plus dedicated `forge interactive task-board`/`forge.interactive.task_board` surface also carry `forge.interactive.task_board.v1`, giving TUI/web/agent dashboards workflow lanes, operable per-task cards, ready handoffs, checkpoint resume candidates, human waits, artifacts and direct next-action commands. The harness also emits guarded CLI execution receipts with Forge-first wrapper env, workflow/task/run lineage, non-destructive PATH shim installation, automatic native CLI discovery that excludes the shim directory, read-only shim status audits for PATH precedence/ownership/recursion, executor-sync projection of Forge-first shim readiness into brain/shell entrypoints, plan-only `forge shells` / MCP `forge.shell.launch_plan` launch reports with readiness/preflight/context/handoff/heartbeat gates, `forge.shell.record_plan` receipts that write `shell_launch_planned` global events, `forge sessions` / MCP `forge.sessions` reports with session lifecycle state, `forge.brain_session_operation_plan.v1` recommendations, `forge sessions lifecycle` / MCP `forge.session.lifecycle` audit-only lifecycle receipts, ordered transition policy with `previous_state`, `lifecycle_sequence`, invalid transition rejection, `lifecycle_policy.allowed_next_states`, next lifecycle commands and provider/state/readiness filters in `forge sessions` plus MCP `forge.sessions`, and `forge sessions history`, MCP `forge.session.history` and `/sessions history` for per-session chronological audit history, `forge.harness.exec_event.v1` global events for guarded CLI receipts with task/node correlation, output hashes/excerpts and reversible stdout/stderr token-headroom reports for authorized real child execution, project `.forge/harness.json` `require_lineage_for_exec` policy that returns `harness_exec_blocked_by_project_policy` when real child execution lacks workflow/task/run lineage, `forge harness doctor` plus MCP `forge.harness.doctor` consolidated readiness audits and the interactive home `harness_doctor_panel`, `forge harness mode --project-root` plus MCP `forge.harness.mode` `project_root` diagnostics for auditing another project before launching a brain CLI, and `forge harness wrap-plan --project-root` plus MCP `forge.harness.wrap_plan` `project_root` support so wrapper planning respects a remote project's Forge-first defaults before shell execution, and `forge harness install-shims --project-root` plus MCP `forge.harness.install_shims` `project_root` support so shim installation uses the same remote project defaults, and `forge harness exec --project-root` plus MCP `forge.harness.exec` `project_root` support so execution uses remote defaults and policy without changing child `cwd`. The `forge milestone cli-demo` output now also includes `forge.milestone.executor_project_demo.v1`, proving a deterministic executor can mutate an isolated project only after governed harness bootstrap, lineage-required execution, event recording and stdout token-headroom retrieval. This is enabling groundwork, not proof that `forge` can replace Codex/OpenCode for daily permission-gated shell work and end-to-end coding/research workflows.",
            "Add broader brain-driven real-project coding/research workflows and continue hardening terminal file editing UX before promoting this beyond groundwork.",
        ),
        capability(
            "experimental_multimodal_runtime",
            "Experimental multimodal runtime",
            "groundwork",
            "0.4.140 adds disabled-by-default multimodal inventory, plan-only install manifests and runtime guards for camera, microphone, screen, input, peripherals, model and filesystem access. 0.4.142 adds plan-only benchmark/report templates and guarded demo plans for local image recognition, audio transcription/synthesis and Blender/avatar preparation through CLI and MCP without installing models or accessing devices. The current line adds approved `.forge/multimodal.json` feature-flag configuration, `--project-root`/MCP project-root inspection, approval-gated `forge multimodal benchmark-result` plus MCP `forge.multimodal.benchmark_result` fixture-only artifacts with explicit no-install, no-model-execution, no-device-access and no-network-access evidence, approval-gated `forge multimodal runtime-benchmark` plus MCP `forge.multimodal.runtime_benchmark` guarded deterministic local runtime execution after opt-in with model guard approval while installs, devices, filesystem and network remain blocked, and approval-gated `forge multimodal demo-receipt` plus MCP `forge.multimodal.demo_receipt` guarded local fixture receipts after opt-in with model guard approval recorded while camera, microphone, screen, input and filesystem access stay blocked unless separately approved. These surfaces prove the safety boundary, guarded runtime execution path and local receipt path, but they do not validate production image/audio/video/3D model quality yet.",
            "Add production model/runtime benchmark evidence with installed or connected models after opt-in; current runtime benchmark proves the guarded execution path but still avoids installs, devices, filesystem and network by default.",
        ),
    ]
}

fn manifest_capability(capability: &MilestoneCapability) -> MilestoneManifestCapability {
    MilestoneManifestCapability {
        id: capability.id.clone(),
        title: capability.title.clone(),
        status: capability.status.clone(),
        promotion_ready: is_promotion_ready_status(&capability.status),
        evidence: capability.evidence.clone(),
        gap_before_promotion: capability.gap_before_promotion.clone(),
    }
}

fn required_evidence_for(capability_id: &str) -> &'static str {
    match capability_id {
        "interactive_cli_baseline" => {
            "TTY and non-TTY CLI contract tests, slash-command surface and routing evidence."
        }
        "human_decision_form_nodes" => {
            "Durable choice/form state, pause/resume, timeout and cross-surface decision evidence."
        }
        "scheduler_loop_subflow_foundation" => {
            "Cron, loop, subflow, lineage, run history and scale-to-zero validation evidence."
        }
        "creative_artifact_ir" => {
            "Serializable, diffable and patchable creative IR tests across required artifact kinds."
        }
        "design_tokens" => {
            "Token schema, semantic resolution, overrides, propagation and human-edit preservation evidence."
        }
        "componentization_ai_surfaces" => {
            "Component manifests, variants/states/actions, token dependencies and patch-by-intent evidence."
        }
        "live_collaboration" => {
            "Presence, patch streams, comments, conflict handling, audit and rollback demo evidence."
        }
        "research_artifact_baseline" => {
            "Source-grounded research comparison and Forge-owned validation/template implications."
        }
        "export_demo_baseline" => {
            "Rendered or exported design/token/component and document/slide/whiteboard workflow demos."
        }
        "replacement_grade_cli" => {
            "Forge-first CLI demo evidence plus native file editing, inline patch workbench previews, multi-file review queues, diff review, permissions, sessions, session operation plans, harness session lifecycle plans and JSON-stable automation evidence."
        }
        "experimental_multimodal_runtime" => {
            "Disabled-by-default multimodal inventory, approved feature-flag config, install-plan, runtime guard, benchmark template, approval-gated fixture-only benchmark-result, guarded local demo-receipt and safe local image/audio/3D demo-plan evidence."
        }
        _ => "Implementation, validation and demo evidence sufficient for 0.5 promotion.",
    }
}

fn is_demo_related(capability: &MilestoneCapability) -> bool {
    capability.id == "export_demo_baseline"
        || capability.gap_before_promotion.contains("demo")
        || capability.evidence.contains("demo")
}

fn next_action_for_gap(capability_id: &str) -> &'static str {
    match capability_id {
        "live_collaboration" => {
            "Extend the validated artifact collaboration baseline into browser transport, richer conflict UX and rendered rollback demos."
        }
        "research_artifact_baseline" => {
            "Keep the source-grounded creative-runtime research report fresh as protocols and local skills change."
        }
        "export_demo_baseline" => {
            "Produce rendered design/tokens/component demo evidence and one structured document/slide/whiteboard workflow demo before 0.5 promotion."
        }
        "replacement_grade_cli" => {
            "Continue from patch workbench review queues, session operation plans and isolated executor project demos into richer file-editing UX and end-to-end brain-driven coding/research workflows."
        }
        "experimental_multimodal_runtime" => {
            "Promote the disabled-by-default multimodal surfaces into production guarded model/runtime benchmarks after local runtime receipts, without performing installs or device access by default."
        }
        _ => "Implement the missing capability with tests, artifacts and milestone evidence.",
    }
}

fn research_sources() -> Vec<MilestoneResearchSource> {
    vec![
        research_source(
            "Penpot data model",
            "https://help.penpot.app/technical-guide/developer/data-model/",
            "Pages and components share a Container abstraction; ShapeTree and Shape carry the editable design model.",
            "Forge creative IR should preserve identity, hierarchy and rendering/export metadata instead of flattening designs into screenshots.",
        ),
        research_source(
            "Penpot data guide",
            "https://help.penpot.app/technical-guide/developer/data-guide/",
            "Penpot treats data evolution, optional attributes and component synchronization as compatibility-sensitive model concerns.",
            "Forge migrations, patch diffs and token/component propagation need backward-compatible defaults plus explicit sync/touched state.",
        ),
        research_source(
            "Penpot design tokens",
            "https://help.penpot.app/user-guide/design-systems/design-tokens/",
            "Penpot aligns tokens with the W3C DTCG format and integrates tokens with components and layout.",
            "Forge tokens should remain source-of-truth artifacts with import/export adapters, semantic aliases and layout/component impact previews.",
        ),
        research_source(
            "Google Stitch real-time design",
            "https://blog.google/innovation-and-ai/models-and-research/google-labs/stitch-updates/",
            "Stitch turns text, voice, codebase and design-file inputs into real-time canvas iterations and production exports.",
            "Forge should model prompt-to-design as staged workflows: brief, variants, critique, patch, validation and export, not one-shot prompting.",
        ),
        research_source(
            "v0 docs",
            "https://v0.app/docs",
            "v0 positions prompt input as a path to high-fidelity UIs, full-stack code, live prototypes, pull requests and deployment.",
            "Forge should route code/product generation through workflow state, validation gates and retention policy before exposing generated products.",
        ),
        research_source(
            "AG-UI protocol",
            "https://github.com/ag-ui-protocol/ag-ui",
            "AG-UI defines event-based agent-user interaction with streaming, shared state, frontend tool calls and human-in-the-loop collaboration.",
            "Forge should own event/audit semantics and expose AGUI-style adapters as transport layers, not as orchestration authority.",
        ),
        research_source(
            "AG-UI overview",
            "https://docs.ag-ui.com/introduction",
            "The protocol highlights typed shared state, streamed event diffs, interrupts, sub-agents, steering and cancellation.",
            "Forge interaction nodes need pause/resume, state diffs, cancellation and durable decision records across CLI, web and MCP surfaces.",
        ),
        research_source(
            "Impeccable design guidance",
            "https://impeccable.style/docs/impeccable/",
            "Impeccable turns design taste into explicit PRODUCT.md/DESIGN.md guidance and anti-pattern checks before code changes.",
            "Forge creative workflows need design-system discovery, anti-generic design gates and explicit persona/taste routing per node.",
        ),
        research_source(
            "Figma MCP developer docs",
            "https://developers.figma.com/docs/figma-mcp-server/",
            "Figma MCP lets agents read design context and write native frames, components, variables and auto-layout using a design system.",
            "Forge MCP tools should exchange structured IR patches and token/component references rather than forcing agents to rewrite whole artifacts.",
        ),
        research_source(
            "Remotion fundamentals",
            "https://www.remotion.dev/docs/the-fundamentals",
            "Remotion models video as React-rendered frames with explicit width, height, duration and fps metadata.",
            "Forge media plans should use deterministic timeline metadata, frame-level validation and bounded renderer adapters without making Remotion a hard dependency.",
        ),
        research_source(
            "Remotion Sequence",
            "https://www.remotion.dev/docs/sequence",
            "Sequences express timed mounting, trimming, nesting and named timeline segments.",
            "Forge animation/video IR should model sequence/timeline nodes, duration constraints and nested composition before choosing an export engine.",
        ),
        research_source(
            "OBS Studio overview",
            "https://obsproject.com/kb/obs-studio-overview",
            "OBS centers composition on scenes, sources, ordering, filters and transitions.",
            "Forge lightweight media composition can reuse scene/source/filter/transition concepts as portable IR while avoiding heavy editor dependencies.",
        ),
    ]
}

fn local_research_inputs() -> Vec<MilestoneResearchSource> {
    vec![
        research_source(
            "Local Superpowers brainstorming skill",
            "/home/arthur/.codex/plugins/cache/openai-curated/superpowers/6188456f/skills/brainstorming/SKILL.md",
            "Requires explicit design exploration, alternatives and approval before implementation.",
            "Forge should convert creative ambiguity into human decision/form nodes with durable approval evidence.",
        ),
        research_source(
            "Local stitch-design skill",
            "/home/arthur/.codex/skills/stitch-design/SKILL.md",
            "Defines prompt enhancement, design-system synthesis and screen generation/editing workflows.",
            "Forge should preserve design-system context and route generation vs edit operations as separate workflow nodes.",
        ),
        research_source(
            "Local imagegen skill",
            "/home/arthur/.codex/skills/.system/imagegen/SKILL.md",
            "Separates generated bitmap assets from repo-native vector/code assets and requires project-bound assets to be persisted.",
            "Forge creative artifacts should distinguish deterministic IR patches from generated bitmap assets with explicit artifact lineage.",
        ),
        research_source(
            "Local Figma generate-design skill",
            "/home/arthur/.codex/plugins/cache/openai-curated/figma/6188456f/skills/figma-generate-design/SKILL.md",
            "Requires component, variable and style discovery before mutating Figma screens.",
            "Forge product workflows should inspect design systems before high-volume generation and reject hardcoded-token drift.",
        ),
        research_source(
            "Local Remotion best-practices skill",
            "/home/arthur/.codex/skills/remotion/SKILL.md",
            "Uses frame/time primitives, sequences and explicit render metadata for code-based video.",
            "Forge can borrow the timeline discipline while keeping video rendering adapters optional.",
        ),
    ]
}

fn research_findings() -> Vec<MilestoneResearchFinding> {
    vec![
        research_finding(
            "editable_ir_identity",
            "Editable creative artifacts need stable identity and hierarchy",
            &["Penpot data model", "Figma MCP developer docs"],
            "Design tools preserve object identity, hierarchy, component context and native editability.",
            "Every Forge creative artifact patch must target stable IDs and preserve token/component references unless the patch explicitly replaces them.",
        ),
        research_finding(
            "tokens_are_runtime_inputs",
            "Tokens are executable creative configuration",
            &["Penpot design tokens", "Local Figma generate-design skill"],
            "Design tokens drive components, layout and cross-tool consistency.",
            "Token changes must run high-impact validation gates and produce impact previews before promotion.",
        ),
        research_finding(
            "prompt_to_ui_is_multi_stage",
            "Prompt-to-UI should become workflow stages",
            &["Google Stitch real-time design", "v0 docs", "Local stitch-design skill"],
            "Modern tools turn prompts into variants, refinements, code and export paths.",
            "Forge must represent brief intake, variant generation, critique, human approval, patching, validation and export as separate nodes.",
        ),
        research_finding(
            "agent_ui_needs_event_state",
            "Agent UI needs durable events and shared state",
            &["AG-UI protocol", "AG-UI overview"],
            "Agent-facing apps need streaming events, shared state, interrupts, frontend tool calls and cancellation.",
            "Forge should expose event streams and MCP tools while keeping authoritative workflow state, audit history and permission policy in Forge.",
        ),
        research_finding(
            "taste_is_a_gate",
            "Design taste is a validation input",
            &["Impeccable design guidance", "Local Superpowers brainstorming skill"],
            "Generic UI failures are predictable enough to become explicit checks.",
            "Forge creative flows should include anti-generic gates, persona/soul routing and human direction choices when taste matters.",
        ),
        research_finding(
            "media_is_timeline_ir",
            "Media output should start from portable timeline IR",
            &["Remotion fundamentals", "Remotion Sequence", "OBS Studio overview"],
            "Video and live composition tools converge on scenes, sources, sequences, timing, filters and transitions.",
            "Forge should model media plans as timeline/scene/source IR first and choose renderer adapters only after validation.",
        ),
    ]
}

fn research_validation_gates() -> Vec<MilestoneResearchGate> {
    vec![
        research_gate(
            "creative_ir_round_trip_fidelity",
            "Creative IR round-trip fidelity",
            "AI and human edits preserve IDs, hierarchy, comments, token references and audit history.",
            "A patch rewrites unrelated artifact content or destroys human-edited fields without explicit approval.",
        ),
        research_gate(
            "design_token_source_of_truth",
            "Design-token source of truth",
            "Raw tokens, semantic aliases, modes and overrides resolve deterministically across artifacts.",
            "A rendered or exported artifact embeds hardcoded values where token references are required.",
        ),
        research_gate(
            "agent_ui_event_audit",
            "Agent UI event audit",
            "Slash commands, web actions and MCP calls produce replayable event records with origin and permission state.",
            "An agent-visible action mutates workflow/artifact state without a durable event.",
        ),
        research_gate(
            "collaboration_conflict_replay",
            "Collaboration conflict replay",
            "Concurrent human/AI patches expose conflict state, chosen resolution and rollback evidence.",
            "A conflict is silently resolved or loses either participant's intent.",
        ),
        research_gate(
            "anti_generic_design_review",
            "Anti-generic design review",
            "Generated creative output is checked for known weak patterns, accessibility and responsive text overflow.",
            "A creative artifact passes while still containing unreviewed generic style, inaccessible contrast or clipped text.",
        ),
        research_gate(
            "media_timeline_determinism",
            "Media timeline determinism",
            "Media/storyboard artifacts declare scenes, sources, timeline, dimensions, fps and duration before rendering.",
            "A video or animation export cannot be reproduced from stored Forge artifact state.",
        ),
        research_gate(
            "export_fidelity_accessibility",
            "Export fidelity and accessibility",
            "Markdown/PDF/slides/web exports preserve source artifact meaning, structure and accessibility metadata.",
            "An export is treated as the source of truth or cannot be traced back to editable IR.",
        ),
    ]
}

fn research_workflow_templates() -> Vec<MilestoneResearchTemplate> {
    vec![
        research_template(
            "prompt_to_screen_with_tokens",
            "Prompt-to-screen with design tokens",
            &[
                "brief intake",
                "design-system discovery",
                "token proposal or reuse",
                "screen variant generation",
                "human direction choice",
                "patch-by-intent",
                "accessibility/export validation",
            ],
            &[
                "token resolution",
                "component dependency scan",
                "text overflow checks",
            ],
            &["variant generation", "design critique"],
            &["approve design-system baseline", "choose visual direction"],
        ),
        research_template(
            "ai_first_whiteboard_brainstorm",
            "AI-first collaborative whiteboard brainstorm",
            &[
                "goal framing",
                "idea generation",
                "duplicate detection",
                "semantic clustering",
                "vote/decision recording",
                "task/subflow conversion",
                "board export",
            ],
            &[
                "duplicate detection",
                "decision trace export",
                "Markdown/PDF export",
            ],
            &["alternative generation", "assumption challenge"],
            &[
                "approve clusters",
                "approve decisions",
                "approve task conversion",
            ],
        ),
        research_template(
            "structured_deck_document_export",
            "Structured document and slide export",
            &[
                "outline",
                "narrative validation",
                "asset selection",
                "slide/document IR assembly",
                "export",
                "fidelity check",
            ],
            &[
                "outline schema validation",
                "link/image checks",
                "PDF/Markdown export",
            ],
            &["narrative synthesis", "visual brief generation"],
            &["approve outline", "approve final delivery constraints"],
        ),
        research_template(
            "long_video_storyboard_plan",
            "Long-form video storyboard plan",
            &[
                "media brief",
                "scene/source/timeline planning",
                "script and beat sheet",
                "asset manifest",
                "render adapter selection",
                "frame/sample validation",
            ],
            &[
                "timeline duration checks",
                "asset hash manifest",
                "sample frame checks",
            ],
            &["script summarization", "scene direction options"],
            &["approve script", "approve render budget"],
        ),
        research_template(
            "agent_visible_component_patch",
            "Agent-visible component patch",
            &[
                "component lookup",
                "intent-to-prop mapping",
                "token dependency impact preview",
                "bounded patch",
                "human review if high impact",
                "status/inspect evidence",
            ],
            &[
                "component manifest parse",
                "action registry validation",
                "token impact preview",
            ],
            &["patch wording normalization"],
            &["approve high-impact component changes"],
        ),
    ]
}

fn research_lean_decisions() -> Vec<MilestoneLeanDecision> {
    vec![
        lean_decision(
            "forge_ir_before_vendor_adapter",
            "Forge-owned IR is the source of truth; vendor tools are import/export or executor adapters.",
            "Compact schemas for screens, whiteboards, documents, slides, media plans, tokens, components and collaboration events.",
            "A hard dependency on Penpot, Figma, Stitch, v0, Remotion or OBS to own workflow state.",
            "Round-trip patch fidelity and fewer whole-artifact rewrites.",
        ),
        lean_decision(
            "deterministic_gates_before_ai_review",
            "Run deterministic validation before spending AI calls on judgment.",
            "Schema checks, token resolution, dependency scans, text overflow checks, artifact hashing and export checks.",
            "Model calls for stable parsing, hashing, listing, PDF generation or Telegram delivery.",
            "Lower cost per recurring workflow and fewer retries after AI review.",
        ),
        lean_decision(
            "event_stream_adapter_not_orchestrator",
            "AGUI-style event streams are transport surfaces; Forge keeps orchestration and audit authority.",
            "Event schema mapping and permission-aware command routing.",
            "Letting frontend event protocols mutate workflow state without Forge revisioning.",
            "Durable replay, pause/resume and cross-surface decision consistency.",
        ),
    ]
}

fn research_source(
    label: &str,
    url_or_path: &str,
    evidence: &str,
    forge_implication: &str,
) -> MilestoneResearchSource {
    MilestoneResearchSource {
        label: label.to_string(),
        url_or_path: url_or_path.to_string(),
        evidence: evidence.to_string(),
        forge_implication: forge_implication.to_string(),
    }
}

fn research_finding(
    id: &str,
    title: &str,
    source_labels: &[&str],
    finding: &str,
    forge_runtime_rule: &str,
) -> MilestoneResearchFinding {
    MilestoneResearchFinding {
        id: id.to_string(),
        title: title.to_string(),
        source_labels: source_labels
            .iter()
            .map(|label| (*label).to_string())
            .collect(),
        finding: finding.to_string(),
        forge_runtime_rule: forge_runtime_rule.to_string(),
    }
}

fn research_gate(
    id: &str,
    title: &str,
    validates: &str,
    failure_condition: &str,
) -> MilestoneResearchGate {
    MilestoneResearchGate {
        id: id.to_string(),
        title: title.to_string(),
        validates: validates.to_string(),
        failure_condition: failure_condition.to_string(),
    }
}

fn research_template(
    id: &str,
    title: &str,
    stages: &[&str],
    deterministic_nodes: &[&str],
    ai_nodes: &[&str],
    human_gates: &[&str],
) -> MilestoneResearchTemplate {
    MilestoneResearchTemplate {
        id: id.to_string(),
        title: title.to_string(),
        stages: stages.iter().map(|stage| (*stage).to_string()).collect(),
        deterministic_nodes: deterministic_nodes
            .iter()
            .map(|node| (*node).to_string())
            .collect(),
        ai_nodes: ai_nodes.iter().map(|node| (*node).to_string()).collect(),
        human_gates: human_gates.iter().map(|gate| (*gate).to_string()).collect(),
    }
}

fn lean_decision(
    id: &str,
    decision: &str,
    accepted_complexity: &str,
    rejected_complexity: &str,
    evidence_metric: &str,
) -> MilestoneLeanDecision {
    MilestoneLeanDecision {
        id: id.to_string(),
        decision: decision.to_string(),
        accepted_complexity: accepted_complexity.to_string(),
        rejected_complexity: rejected_complexity.to_string(),
        evidence_metric: evidence_metric.to_string(),
    }
}

fn capability(
    id: &str,
    title: &str,
    status: &str,
    evidence: &str,
    gap_before_promotion: &str,
) -> MilestoneCapability {
    MilestoneCapability {
        id: id.to_string(),
        title: title.to_string(),
        status: status.to_string(),
        evidence: evidence.to_string(),
        gap_before_promotion: gap_before_promotion.to_string(),
    }
}

fn summarize_capabilities(capabilities: &[MilestoneCapability]) -> MilestoneStatusSummary {
    MilestoneStatusSummary {
        implemented: count_status(capabilities, "implemented"),
        validated: count_status(capabilities, "validated"),
        groundwork: count_status(capabilities, "groundwork"),
        planned: count_status(capabilities, "planned"),
        blocked: count_status(capabilities, "blocked"),
        total: capabilities.len(),
    }
}

fn count_status(capabilities: &[MilestoneCapability], status: &str) -> usize {
    capabilities
        .iter()
        .filter(|capability| capability.status == status)
        .count()
}

fn is_promotion_ready_status(status: &str) -> bool {
    matches!(status, "implemented" | "validated")
}

fn status_vocabulary() -> Vec<String> {
    [
        "implemented",
        "validated",
        "groundwork",
        "planned",
        "blocked",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}
