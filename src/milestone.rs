use anyhow::{bail, Result};
use serde::Serialize;

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
            next_action:
                "Implement the next planned creative runtime capability with tests, demos and milestone evidence before reconsidering 0.5 promotion."
                    .to_string(),
        },
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
            "0.4.102 validates DesignToken, TokenType, TokenCollection, SemanticAlias as serde-able types with CLI set-tokens/get-tokens and workflow integration. Cycle 26 confirms validated status.",
            "Token resolution engine, inheritance, propagation and human edit preservation demos remain for 0.5.",
        ),
        capability(
            "componentization_ai_surfaces",
            "Componentization and AI-first UI surfaces",
            "validated",
            "0.4.102 validates ComponentSpec with props, variants, states, slots, token dependencies and code template as serde-able IR with PatchByIntent schema. Cycle 26 confirms validated status.",
            "Rendered component preview, token dependency resolution, patch-by-intent execution engine and AI-driven component generation remain for 0.5.",
        ),
        capability(
            "live_collaboration",
            "Live collaboration",
            "groundwork",
            "Human decision audit, durable interaction state, MCP human interaction bridge (create/list/answer/expire) validated in 0.4.98-0.4.104. Cycle 26 adds aggregate schedule/loop CLI commands and interactive dashboard enhancements. Cycle 28 creative MCP tools enable agent-driven creative artifact collaboration.",
            "Need presence, cursors/selections, patch streams, comments, conflict handling and rollback demo before 0.5 promotion.",
        ),
        capability(
            "research_artifact_baseline",
            "Research artifact baseline",
            "planned",
            "Research topics are listed in prompt v2, the milestone document and the scheduler/loop/subflow validation report.",
            "Need source-grounded comparison of Penpot, Stitch, v0, Impeccable/AGUI-style protocols, Superpowers, Remotion/Figma capabilities and OBS/media composition lessons before 0.5 promotion.",
        ),
        capability(
            "export_demo_baseline",
            "Export/demo baseline",
            "validated",
            "Cycle 28 validates MCP creative artifact list/inspect/attach and token get/set tools, exposing the full creative IR, design token and componentization surface through agent-facing MCP tools. Cycle 29 validates native scheduler worker-status surface with due/idle scheduling posture, scale-to-zero eligibility and bounded worker-pool capacity. Daily Goal smoke produces Markdown/PDF artifacts and Telegram delivery records through Forge-owned workflow semantics across all cycles.",
            "Need one design/tokens/component workflow demo generating actual rendered artifacts and one structured document/slide/whiteboard workflow demo before 0.5 promotion.",
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
            "Build the smallest structured collaboration demo with presence, patch history and rollback."
        }
        "research_artifact_baseline" => {
            "Produce the source-grounded creative-runtime research report before promotion."
        }
        _ => "Implement the missing capability with tests, artifacts and milestone evidence.",
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
