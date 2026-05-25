use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticAlias {
    pub name: String,
    pub resolves_to: String,
    #[serde(default)]
    pub description: String,
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
