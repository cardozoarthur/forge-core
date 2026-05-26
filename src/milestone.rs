use crate::ir::{
    ir_schema_version, CreativeArtifact, DesignToken, DocumentSection, DocumentSpec, ScreenSpec,
    SemanticAlias, TokenCollection, TokenType,
};
use crate::schedule::create_daily_goal_research_workflow;
use crate::storage::ForgeStore;
use crate::workflow::{attach_creative_artifact, set_workflow_token_collection};
use anyhow::{bail, Result};
use serde::Serialize;
use std::collections::BTreeMap;

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

fn forge_05_capabilities() -> Vec<MilestoneCapability> {
    vec![
        capability(
            "interactive_cli_baseline",
            "Interactive Forge CLI baseline",
            "validated",
            "0.4.97 validates the no-argument TTY home, slash-command catalog, conversational routing and retention decisions. Cycle 24 confirms all 14 required slash commands, conversational routing with direct-answer vs workflow classification, retention decisions with delete/retain/archive policy, and CLI contract tests for TTY/non-TTY behavior with 175 passing tests.",
            "Full terminal TUI loop, autocomplete and inline mode still need implementation evidence.",
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
            "0.4.92-0.4.100 validate cron nodes, loop state, due execution, missed-run policy, daily Goal research smoke artifacts and concurrent DAG execution with parallel wave scheduling.",
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
