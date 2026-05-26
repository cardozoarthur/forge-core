use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

pub fn ir_schema_version() -> String {
    "forge.ir.v1".to_string()
}

fn default_updated_at() -> DateTime<Utc> {
    Utc::now()
}

fn empty_tags() -> Vec<String> {
    Vec::new()
}

fn empty_patches() -> Vec<PatchRecord> {
    Vec::new()
}

fn default_collaboration() -> CreativeCollaborationState {
    CreativeCollaborationState::default()
}

fn empty_extensions() -> BTreeMap<String, String> {
    BTreeMap::new()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreativeArtifact {
    #[serde(default = "ir_schema_version")]
    pub schema_version: String,
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub kind: CreativeArtifactKind,
    pub content: CreativeContent,
    pub created_at: DateTime<Utc>,
    #[serde(default = "default_updated_at")]
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub workflow_id: String,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default = "empty_tags")]
    pub tags: Vec<String>,
    #[serde(default = "empty_patches")]
    pub patches: Vec<PatchRecord>,
    #[serde(default = "default_collaboration")]
    pub collaboration: CreativeCollaborationState,
}

impl CreativeArtifact {
    pub fn new_screen(title: &str, spec: ScreenSpec) -> Self {
        Self::new(
            title,
            CreativeArtifactKind::Screen,
            CreativeContent::Screen(spec),
        )
    }

    pub fn new_whiteboard(title: &str, spec: WhiteboardSpec) -> Self {
        Self::new(
            title,
            CreativeArtifactKind::Whiteboard,
            CreativeContent::Whiteboard(spec),
        )
    }

    pub fn new_document(title: &str, spec: DocumentSpec) -> Self {
        Self::new(
            title,
            CreativeArtifactKind::Document,
            CreativeContent::Document(spec),
        )
    }

    pub fn new_slide_deck(title: &str, spec: SlideDeckSpec) -> Self {
        Self::new(
            title,
            CreativeArtifactKind::SlideDeck,
            CreativeContent::SlideDeck(spec),
        )
    }

    pub fn new_component(title: &str, spec: ComponentSpec) -> Self {
        Self::new(
            title,
            CreativeArtifactKind::Component,
            CreativeContent::Component(spec),
        )
    }

    fn new(title: &str, kind: CreativeArtifactKind, content: CreativeContent) -> Self {
        Self {
            schema_version: ir_schema_version(),
            id: format!("ca_{}", Uuid::new_v4().to_string().replace('-', "")),
            title: title.to_string(),
            description: String::new(),
            kind,
            content,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            workflow_id: String::new(),
            task_id: None,
            tags: Vec::new(),
            patches: Vec::new(),
            collaboration: CreativeCollaborationState::default(),
        }
    }

    pub fn summary(&self) -> CreativeArtifactSummary {
        CreativeArtifactSummary {
            id: self.id.clone(),
            title: self.title.clone(),
            description: self.description.clone(),
            kind: self.kind.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            tag_count: self.tags.len(),
            patch_count: self.patches.len(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreativeArtifactSummary {
    pub id: String,
    pub title: String,
    pub description: String,
    pub kind: CreativeArtifactKind,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub tag_count: usize,
    pub patch_count: usize,
}

// -- Live collaboration --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreativeCollaborationState {
    #[serde(default = "creative_collaboration_schema_version")]
    pub schema_version: String,
    #[serde(default)]
    pub presences: Vec<CollaborationPresence>,
    #[serde(default)]
    pub comments: Vec<CollaborationComment>,
    #[serde(default)]
    pub patch_stream: Vec<CollaborationPatchEvent>,
    #[serde(default)]
    pub conflicts: Vec<CollaborationConflictEvent>,
    #[serde(default)]
    pub rollbacks: Vec<CollaborationRollbackEvent>,
    #[serde(default)]
    pub audit_history: Vec<CollaborationAuditEvent>,
}

impl Default for CreativeCollaborationState {
    fn default() -> Self {
        Self {
            schema_version: creative_collaboration_schema_version(),
            presences: Vec::new(),
            comments: Vec::new(),
            patch_stream: Vec::new(),
            conflicts: Vec::new(),
            rollbacks: Vec::new(),
            audit_history: Vec::new(),
        }
    }
}

impl CreativeCollaborationState {
    pub fn summary(&self) -> CreativeCollaborationSummary {
        CreativeCollaborationSummary {
            schema_version: "forge.creative_collaboration.summary.v1".to_string(),
            active_presence_count: self
                .presences
                .iter()
                .filter(|presence| presence.status == "active")
                .count(),
            comment_count: self.comments.len(),
            patch_event_count: self.patch_stream.len(),
            conflict_count: self.conflicts.len(),
            rollback_count: self.rollbacks.len(),
            audit_event_count: self.audit_history.len(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationPresence {
    pub event_id: String,
    pub actor: String,
    pub cursor: Option<String>,
    #[serde(default)]
    pub selections: Vec<String>,
    pub status: String,
    pub origin: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationComment {
    pub event_id: String,
    pub actor: String,
    pub target: String,
    pub body: String,
    pub status: String,
    pub origin: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationPatchEvent {
    pub event_id: String,
    pub actor: String,
    pub target: String,
    pub instruction: String,
    pub status: String,
    pub origin: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationConflictEvent {
    pub event_id: String,
    pub actor: String,
    pub target: String,
    pub summary: String,
    pub resolution_status: String,
    pub origin: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationRollbackEvent {
    pub event_id: String,
    pub actor: String,
    pub target_event_id: String,
    pub reason: String,
    pub origin: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationAuditEvent {
    pub event_id: String,
    pub kind: String,
    pub actor: String,
    pub summary: String,
    pub origin: String,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreativeCollaborationSummary {
    pub schema_version: String,
    pub active_presence_count: usize,
    pub comment_count: usize,
    pub patch_event_count: usize,
    pub conflict_count: usize,
    pub rollback_count: usize,
    pub audit_event_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CreativeArtifactKind {
    Screen,
    Whiteboard,
    Document,
    SlideDeck,
    Component,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CreativeContent {
    Screen(ScreenSpec),
    Whiteboard(WhiteboardSpec),
    Document(DocumentSpec),
    SlideDeck(SlideDeckSpec),
    Component(ComponentSpec),
}

// -- Screen --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenSpec {
    #[serde(default = "ir_schema_version")]
    pub schema_version: String,
    pub width_px: u32,
    pub height_px: u32,
    #[serde(default)]
    pub background: String,
    #[serde(default)]
    pub breakpoints: Vec<Breakpoint>,
    #[serde(default)]
    pub elements: Vec<ScreenElement>,
    #[serde(default)]
    pub interactions: Vec<InteractionFlow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Breakpoint {
    pub name: String,
    pub max_width_px: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenElement {
    pub id: String,
    pub component_ref: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub props: BTreeMap<String, String>,
    #[serde(default = "return_true")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    #[serde(default)]
    pub layer: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionFlow {
    pub trigger: String,
    pub action: String,
    pub target_id: String,
}

// -- Whiteboard --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhiteboardSpec {
    #[serde(default = "ir_schema_version")]
    pub schema_version: String,
    pub width_px: u32,
    pub height_px: u32,
    #[serde(default)]
    pub background: String,
    #[serde(default)]
    pub layers: Vec<WhiteboardLayer>,
    #[serde(default)]
    pub sticky_notes: Vec<StickyNote>,
    #[serde(default)]
    pub drawings: Vec<DrawingElement>,
    #[serde(default)]
    pub text_blocks: Vec<TextBlock>,
    #[serde(default)]
    pub images: Vec<ImageElement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhiteboardLayer {
    pub id: String,
    pub name: String,
    #[serde(default = "return_true")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    pub order: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StickyNote {
    pub id: String,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub width: f64,
    #[serde(default)]
    pub height: f64,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub text: String,
    pub layer_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawingElement {
    pub id: String,
    #[serde(default)]
    pub path: Vec<Point>,
    #[serde(default)]
    pub stroke_color: String,
    #[serde(default)]
    pub stroke_width: f64,
    pub layer_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextBlock {
    pub id: String,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub width: f64,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub font_family: String,
    #[serde(default)]
    pub font_size: f64,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub color: String,
    pub layer_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageElement {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub src: String,
    #[serde(default)]
    pub alt_text: String,
    pub layer_id: String,
}

// -- Document --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSpec {
    #[serde(default = "ir_schema_version")]
    pub schema_version: String,
    pub title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default = "empty_extensions")]
    pub front_matter: BTreeMap<String, String>,
    #[serde(default)]
    pub sections: Vec<DocumentSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSection {
    pub id: String,
    pub heading: String,
    #[serde(default)]
    pub level: u32,
    #[serde(default)]
    pub content: Vec<DocumentContent>,
    #[serde(default)]
    pub children: Vec<DocumentSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DocumentContent {
    Text {
        value: String,
    },
    Image {
        src: String,
        alt: String,
    },
    Table {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Code {
        language: String,
        code: String,
    },
    List {
        items: Vec<String>,
        ordered: bool,
    },
    Divider,
}

// -- Slide Deck --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlideDeckSpec {
    #[serde(default = "ir_schema_version")]
    pub schema_version: String,
    pub title: String,
    #[serde(default)]
    pub theme: String,
    #[serde(default)]
    pub slides: Vec<SlideSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlideSpec {
    pub id: String,
    #[serde(default)]
    pub layout: String,
    pub title: String,
    #[serde(default)]
    pub content: Vec<SlideContent>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub transition: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlideContent {
    pub kind: String,
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    #[serde(default)]
    pub width: f64,
    #[serde(default)]
    pub height: f64,
    #[serde(default)]
    pub value: String,
    #[serde(default = "empty_extensions")]
    pub styling: BTreeMap<String, String>,
}

// -- Component --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentSpec {
    #[serde(default = "ir_schema_version")]
    pub schema_version: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub props: Vec<ComponentProp>,
    #[serde(default)]
    pub variants: Vec<ComponentVariant>,
    #[serde(default)]
    pub states: Vec<ComponentState>,
    #[serde(default)]
    pub slots: Vec<ComponentSlot>,
    #[serde(default)]
    pub token_dependencies: Vec<String>,
    #[serde(default)]
    pub code_template: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentProp {
    pub name: String,
    pub prop_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default_value: Option<String>,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentVariant {
    pub name: String,
    #[serde(default = "empty_extensions")]
    pub props_override: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentState {
    pub name: String,
    #[serde(default = "empty_extensions")]
    pub styling: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentSlot {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required: bool,
}

// -- Design Tokens --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignToken {
    pub name: String,
    pub value: String,
    pub token_type: TokenType,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub group: String,
    #[serde(default = "empty_extensions")]
    pub extensions: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TokenType {
    Color,
    Spacing,
    Typography,
    Shadow,
    BorderRadius,
    Opacity,
    ZIndex,
    FontFamily,
    FontSize,
    FontWeight,
    LineHeight,
    Duration,
    Easing,
    Custom(String),
}

impl TokenType {
    pub fn as_str(&self) -> &str {
        match self {
            TokenType::Color => "color",
            TokenType::Spacing => "spacing",
            TokenType::Typography => "typography",
            TokenType::Shadow => "shadow",
            TokenType::BorderRadius => "border_radius",
            TokenType::Opacity => "opacity",
            TokenType::ZIndex => "z_index",
            TokenType::FontFamily => "font_family",
            TokenType::FontSize => "font_size",
            TokenType::FontWeight => "font_weight",
            TokenType::LineHeight => "line_height",
            TokenType::Duration => "duration",
            TokenType::Easing => "easing",
            TokenType::Custom(kind) => kind,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenCollection {
    #[serde(default = "ir_schema_version")]
    pub schema_version: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tokens: Vec<DesignToken>,
    #[serde(default)]
    pub semantic_aliases: Vec<SemanticAlias>,
    #[serde(default)]
    pub modes: Vec<TokenMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticAlias {
    pub name: String,
    pub resolves_to: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMode {
    pub name: String,
    #[serde(default)]
    pub overrides: Vec<TokenOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenOverride {
    pub token_name: String,
    pub value: String,
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenResolutionReport {
    pub schema_version: String,
    pub collection_name: String,
    pub mode: Option<String>,
    pub resolved_tokens: Vec<ResolvedToken>,
    pub unresolved_aliases: Vec<UnresolvedTokenAlias>,
    pub impact_preview: TokenImpactPreview,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolvedToken {
    pub name: String,
    pub value: String,
    pub token_type: String,
    pub source: String,
    pub resolves_to: Option<String>,
    pub applied_overrides: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnresolvedTokenAlias {
    pub name: String,
    pub resolves_to: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenImpactPreview {
    pub schema_version: String,
    pub affected_token_count: usize,
    pub affected_artifact_count: usize,
    pub references: Vec<TokenReference>,
    pub possible_regressions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenReference {
    pub artifact_id: String,
    pub artifact_title: String,
    pub artifact_kind: String,
    pub path: String,
    pub token_name: String,
    pub reference_kind: String,
}

pub fn resolve_token_collection(
    collection: &TokenCollection,
    mode: Option<&str>,
    artifacts: &[CreativeArtifact],
) -> TokenResolutionReport {
    let mode = mode
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let token_map = collection
        .tokens
        .iter()
        .map(|token| (token.name.as_str(), token))
        .collect::<BTreeMap<_, _>>();
    let alias_map = collection
        .semantic_aliases
        .iter()
        .map(|alias| (alias.name.as_str(), alias))
        .collect::<BTreeMap<_, _>>();
    let selected_mode = mode
        .as_deref()
        .and_then(|mode_name| collection.modes.iter().find(|mode| mode.name == mode_name));
    let overrides = selected_mode
        .map(|mode| {
            mode.overrides
                .iter()
                .map(|item| (item.token_name.as_str(), item))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();

    let mut resolved_tokens = collection
        .tokens
        .iter()
        .map(|token| {
            let (value, applied_overrides) =
                token_value_with_override(token, &overrides, mode.as_deref());
            ResolvedToken {
                name: token.name.clone(),
                value,
                token_type: token.token_type.as_str().to_string(),
                source: "raw_token".to_string(),
                resolves_to: Some(token.name.clone()),
                applied_overrides,
            }
        })
        .collect::<Vec<_>>();
    let mut unresolved_aliases = Vec::new();

    for alias in &collection.semantic_aliases {
        let mut seen = BTreeSet::new();
        match resolve_token_name(
            &alias.resolves_to,
            &token_map,
            &alias_map,
            &overrides,
            mode.as_deref(),
            &mut seen,
        ) {
            Ok(resolved) => resolved_tokens.push(ResolvedToken {
                name: alias.name.clone(),
                value: resolved.value,
                token_type: resolved.token_type,
                source: "semantic_alias".to_string(),
                resolves_to: Some(resolved.raw_name),
                applied_overrides: resolved.applied_overrides,
            }),
            Err(reason) => unresolved_aliases.push(UnresolvedTokenAlias {
                name: alias.name.clone(),
                resolves_to: alias.resolves_to.clone(),
                reason,
            }),
        }
    }

    resolved_tokens.sort_by(|left, right| left.name.cmp(&right.name));
    let known_names = known_token_names(collection);
    let impact_preview = preview_token_impact(artifacts, &known_names);

    TokenResolutionReport {
        schema_version: "forge.tokens.resolution.v1".to_string(),
        collection_name: collection.name.clone(),
        mode,
        resolved_tokens,
        unresolved_aliases,
        impact_preview,
    }
}

pub fn preview_token_change_impact(
    collection: &TokenCollection,
    artifacts: &[CreativeArtifact],
    token_name: &str,
) -> TokenImpactPreview {
    let mut names = BTreeSet::from([token_name.to_string()]);
    let token_map = collection
        .tokens
        .iter()
        .map(|token| (token.name.as_str(), token))
        .collect::<BTreeMap<_, _>>();
    let alias_map = collection
        .semantic_aliases
        .iter()
        .map(|alias| (alias.name.as_str(), alias))
        .collect::<BTreeMap<_, _>>();

    for alias in &collection.semantic_aliases {
        let mut seen = BTreeSet::new();
        if let Ok(resolved) = resolve_token_name(
            &alias.resolves_to,
            &token_map,
            &alias_map,
            &BTreeMap::new(),
            None,
            &mut seen,
        ) {
            if resolved.raw_name == token_name {
                names.insert(alias.name.clone());
            }
        }
    }

    preview_token_impact(artifacts, &names)
}

fn known_token_names(collection: &TokenCollection) -> BTreeSet<String> {
    collection
        .tokens
        .iter()
        .map(|token| token.name.clone())
        .chain(
            collection
                .semantic_aliases
                .iter()
                .map(|alias| alias.name.clone()),
        )
        .collect()
}

struct ResolvedTokenValue {
    raw_name: String,
    value: String,
    token_type: String,
    applied_overrides: Vec<String>,
}

fn resolve_token_name(
    name: &str,
    token_map: &BTreeMap<&str, &DesignToken>,
    alias_map: &BTreeMap<&str, &SemanticAlias>,
    overrides: &BTreeMap<&str, &TokenOverride>,
    mode: Option<&str>,
    seen: &mut BTreeSet<String>,
) -> std::result::Result<ResolvedTokenValue, String> {
    if let Some(token) = token_map.get(name) {
        let (value, applied_overrides) = token_value_with_override(token, overrides, mode);
        return Ok(ResolvedTokenValue {
            raw_name: token.name.clone(),
            value,
            token_type: token.token_type.as_str().to_string(),
            applied_overrides,
        });
    }

    let Some(alias) = alias_map.get(name) else {
        return Err(format!("token or alias not found: {name}"));
    };
    if !seen.insert(alias.name.clone()) {
        return Err(format!("semantic alias cycle detected at {}", alias.name));
    }
    resolve_token_name(
        &alias.resolves_to,
        token_map,
        alias_map,
        overrides,
        mode,
        seen,
    )
}

fn token_value_with_override(
    token: &DesignToken,
    overrides: &BTreeMap<&str, &TokenOverride>,
    mode: Option<&str>,
) -> (String, Vec<String>) {
    overrides
        .get(token.name.as_str())
        .map(|override_value| {
            (
                override_value.value.clone(),
                mode.map(|name| vec![format!("mode:{name}")])
                    .unwrap_or_default(),
            )
        })
        .unwrap_or_else(|| (token.value.clone(), Vec::new()))
}

fn preview_token_impact(
    artifacts: &[CreativeArtifact],
    token_names: &BTreeSet<String>,
) -> TokenImpactPreview {
    let mut references = Vec::new();
    for artifact in artifacts {
        collect_artifact_token_references(artifact, token_names, &mut references);
    }
    references.sort_by(|left, right| {
        left.artifact_id
            .cmp(&right.artifact_id)
            .then(left.path.cmp(&right.path))
            .then(left.token_name.cmp(&right.token_name))
    });
    let affected_tokens = references
        .iter()
        .map(|reference| reference.token_name.clone())
        .collect::<BTreeSet<_>>();
    let affected_artifacts = references
        .iter()
        .map(|reference| reference.artifact_id.clone())
        .collect::<BTreeSet<_>>();
    let possible_regressions = if references.is_empty() {
        Vec::new()
    } else {
        vec![
            "Run design quality, accessibility and export-fidelity gates before applying broad token changes.".to_string(),
        ]
    };

    TokenImpactPreview {
        schema_version: "forge.tokens.impact_preview.v1".to_string(),
        affected_token_count: affected_tokens.len(),
        affected_artifact_count: affected_artifacts.len(),
        references,
        possible_regressions,
    }
}

fn collect_artifact_token_references(
    artifact: &CreativeArtifact,
    token_names: &BTreeSet<String>,
    references: &mut Vec<TokenReference>,
) {
    match &artifact.content {
        CreativeContent::Screen(screen) => {
            push_if_token(
                artifact,
                "content.background",
                &screen.background,
                "screen_background",
                token_names,
                references,
            );
            for (index, element) in screen.elements.iter().enumerate() {
                for (key, value) in &element.props {
                    push_if_token(
                        artifact,
                        &format!("content.elements[{index}].props.{key}"),
                        value,
                        "screen_element_prop",
                        token_names,
                        references,
                    );
                }
            }
        }
        CreativeContent::Whiteboard(board) => {
            push_if_token(
                artifact,
                "content.background",
                &board.background,
                "whiteboard_background",
                token_names,
                references,
            );
            for (index, note) in board.sticky_notes.iter().enumerate() {
                push_if_token(
                    artifact,
                    &format!("content.sticky_notes[{index}].color"),
                    &note.color,
                    "sticky_note_color",
                    token_names,
                    references,
                );
            }
            for (index, drawing) in board.drawings.iter().enumerate() {
                push_if_token(
                    artifact,
                    &format!("content.drawings[{index}].stroke_color"),
                    &drawing.stroke_color,
                    "drawing_stroke",
                    token_names,
                    references,
                );
            }
        }
        CreativeContent::Document(document) => {
            for (key, value) in &document.front_matter {
                push_if_token(
                    artifact,
                    &format!("content.front_matter.{key}"),
                    value,
                    "document_front_matter",
                    token_names,
                    references,
                );
            }
        }
        CreativeContent::SlideDeck(deck) => {
            for (slide_index, slide) in deck.slides.iter().enumerate() {
                for (content_index, content) in slide.content.iter().enumerate() {
                    for (key, value) in &content.styling {
                        push_if_token(
                            artifact,
                            &format!(
                                "content.slides[{slide_index}].content[{content_index}].styling.{key}"
                            ),
                            value,
                            "slide_styling",
                            token_names,
                            references,
                        );
                    }
                }
            }
        }
        CreativeContent::Component(component) => {
            for (index, token_name) in component.token_dependencies.iter().enumerate() {
                push_reference(
                    artifact,
                    &format!("content.token_dependencies[{index}]"),
                    token_name,
                    "component_dependency",
                    token_names,
                    references,
                );
            }
            for (state_index, state) in component.states.iter().enumerate() {
                for (key, value) in &state.styling {
                    push_if_token(
                        artifact,
                        &format!("content.states[{state_index}].styling.{key}"),
                        value,
                        "component_state_style",
                        token_names,
                        references,
                    );
                }
            }
        }
    }
}

fn push_if_token(
    artifact: &CreativeArtifact,
    path: &str,
    value: &str,
    reference_kind: &str,
    token_names: &BTreeSet<String>,
    references: &mut Vec<TokenReference>,
) {
    if let Some(token_name) = extract_token_ref(value, token_names) {
        push_reference(
            artifact,
            path,
            &token_name,
            reference_kind,
            token_names,
            references,
        );
    }
}

fn push_reference(
    artifact: &CreativeArtifact,
    path: &str,
    token_name: &str,
    reference_kind: &str,
    token_names: &BTreeSet<String>,
    references: &mut Vec<TokenReference>,
) {
    if !token_names.contains(token_name) {
        return;
    }
    references.push(TokenReference {
        artifact_id: artifact.id.clone(),
        artifact_title: artifact.title.clone(),
        artifact_kind: format!("{:?}", artifact.kind),
        path: path.to_string(),
        token_name: token_name.to_string(),
        reference_kind: reference_kind.to_string(),
    });
}

fn extract_token_ref(value: &str, token_names: &BTreeSet<String>) -> Option<String> {
    let trimmed = value.trim();
    if token_names.contains(trimmed) {
        return Some(trimmed.to_string());
    }
    trimmed
        .strip_prefix("{token:")
        .and_then(|rest| rest.strip_suffix('}'))
        .map(str::trim)
        .filter(|token| token_names.contains(*token))
        .map(str::to_string)
}

fn creative_collaboration_schema_version() -> String {
    "forge.creative_collaboration.v1".to_string()
}

// -- Patch-by-Intent --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchByIntent {
    pub id: String,
    pub instruction: String,
    pub target_artifact_id: String,
    #[serde(default)]
    pub scope: String,
    pub applied_at: DateTime<Utc>,
    #[serde(default)]
    pub applied_by: String,
    #[serde(default)]
    pub changes: Vec<ConcreteChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcreteChange {
    pub path: String,
    #[serde(default)]
    pub old_value: Option<String>,
    pub new_value: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchRecord {
    pub patch_id: String,
    pub instruction: String,
    pub applied_at: DateTime<Utc>,
    pub change_count: u32,
}

fn return_true() -> bool {
    true
}
