use anyhow::{bail, Result};
use serde::Serialize;

const MILESTONE_STATUS_SCHEMA_VERSION: &str = "forge.milestone.status.v1";
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
            "groundwork",
            "0.4.102 validates ScreenSpec, WhiteboardSpec, DocumentSpec, SlideDeckSpec, ComponentSpec as first-class creative artifact types with serde round-trip, CLI attach/list/inspect, and workflow integration.",
            "Need declarative import/export, rendering adapters and full screen/whiteboard/document editing through the runtime.",
        ),
        capability(
            "design_tokens",
            "Design systems/tokens",
            "groundwork",
            "0.4.102 validates DesignToken, TokenType, TokenCollection, SemanticAlias as serde-able types with CLI set-tokens/get-tokens and workflow integration.",
            "Need token resolution engine, inheritance, propagation and human edit preservation demos.",
        ),
        capability(
            "componentization_ai_surfaces",
            "Componentization and AI-first UI surfaces",
            "groundwork",
            "0.4.102 validates ComponentSpec with props, variants, states, slots, token dependencies and code template as serde-able IR with PatchByIntent schema.",
            "Need rendered component preview, token dependency resolution, patch-by-intent execution engine and AI-driven component generation.",
        ),
        capability(
            "live_collaboration",
            "Live collaboration",
            "planned",
            "Human decision audit groundwork exists in 0.4.98.",
            "Need presence, cursors/selections, patch streams, comments, conflict handling and rollback demo.",
        ),
        capability(
            "research_artifact_baseline",
            "Research artifact baseline",
            "planned",
            "Research topics are listed in prompt v2 and the milestone document.",
            "Need source-grounded comparison of Penpot, Stitch, v0, Impeccable/AGUI-style protocols, Superpowers, Remotion/Figma capabilities and OBS/media composition lessons.",
        ),
        capability(
            "export_demo_baseline",
            "Export/demo baseline",
            "planned",
            "Daily Goal smoke produces Markdown/PDF as scheduler validation, not creative runtime proof.",
            "Need one design/tokens/component workflow demo and one structured document/slide/whiteboard workflow demo.",
        ),
    ]
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
