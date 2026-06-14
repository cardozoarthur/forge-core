use crate::addon::{
    builtin_addon_catalog, resolve_goal_capabilities, AddonCatalog, CapabilityNeed,
    CapabilityResolutionReport, CAP_ASYNC_RUNTIME, CAP_DAILY_GOAL_RESEARCH, CAP_HACKATHON_FACTORY,
    CAP_TELEGRAM_NOTIFICATION, CAP_VISUAL_WORKSPACE, CAP_WORKFLOW_AUTOMATION_RESEARCH,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentSpec {
    #[serde(default = "intent_schema_version")]
    pub schema_version: String,
    pub goal: String,
    pub constraints: Vec<String>,
    pub deliverables: Vec<String>,
    pub risks: Vec<String>,
    pub unknowns: Vec<String>,
    #[serde(default)]
    pub workflow_mode: WorkflowModeSpec,
    #[serde(default)]
    pub event_policy: EventPolicySpec,
    #[serde(default)]
    pub operating_context: OperatingContextSpec,
    #[serde(default)]
    pub required_capabilities: Vec<CapabilityNeed>,
    #[serde(default)]
    pub active_addons: Vec<String>,
    #[serde(default)]
    pub capability_resolution: CapabilityResolutionReport,
}

impl Default for IntentSpec {
    fn default() -> Self {
        Self {
            schema_version: intent_schema_version(),
            goal: String::new(),
            constraints: Vec::new(),
            deliverables: Vec::new(),
            risks: Vec::new(),
            unknowns: Vec::new(),
            workflow_mode: WorkflowModeSpec::default(),
            event_policy: EventPolicySpec::default(),
            operating_context: OperatingContextSpec::default(),
            required_capabilities: Vec::new(),
            active_addons: Vec::new(),
            capability_resolution: CapabilityResolutionReport::default(),
        }
    }
}

impl IntentSpec {
    pub fn has_capability(&self, capability_id: &str) -> bool {
        self.required_capabilities
            .iter()
            .any(|capability| capability.id == capability_id)
            || self.capability_resolution.has_capability(capability_id)
    }

    pub fn workflow_extension_enabled(&self, extension_id: &str) -> bool {
        self.capability_resolution
            .workflow_extensions()
            .contains(extension_id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowModeSpec {
    #[serde(default = "default_workflow_kind")]
    pub kind: String,
    #[serde(default = "default_workflow_lifetime")]
    pub expected_lifetime: String,
    #[serde(default)]
    pub can_become_persistent: bool,
    #[serde(default = "default_scale_to_zero_policy")]
    pub scale_to_zero_policy: String,
}

impl Default for WorkflowModeSpec {
    fn default() -> Self {
        Self {
            kind: default_workflow_kind(),
            expected_lifetime: default_workflow_lifetime(),
            can_become_persistent: true,
            scale_to_zero_policy: default_scale_to_zero_policy(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPolicySpec {
    #[serde(default = "event_policy_schema_version")]
    pub schema_version: String,
    #[serde(default = "default_event_runtime")]
    pub runtime: String,
    #[serde(default)]
    pub accepted_origins: Vec<String>,
    #[serde(default)]
    pub allowed_actions: Vec<String>,
}

impl Default for EventPolicySpec {
    fn default() -> Self {
        Self {
            schema_version: event_policy_schema_version(),
            runtime: default_event_runtime(),
            accepted_origins: vec![
                "chat".to_string(),
                "api".to_string(),
                "webhook".to_string(),
                "cron".to_string(),
                "file".to_string(),
                "messaging".to_string(),
                "telemetry".to_string(),
            ],
            allowed_actions: vec![
                "start_workflow".to_string(),
                "continue_workflow".to_string(),
                "pause_workflow".to_string(),
                "resume_workflow".to_string(),
                "modify_workflow".to_string(),
                "complete_workflow".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatingContextSpec {
    #[serde(default = "operating_context_schema_version")]
    pub schema_version: String,
    #[serde(default)]
    pub organization: ContextIdentityRef,
    #[serde(default)]
    pub brand: ContextIdentityRef,
    #[serde(default)]
    pub product: ContextIdentityRef,
    #[serde(default)]
    pub user: ContextIdentityRef,
    #[serde(default)]
    pub channel: ContextIdentityRef,
    #[serde(default = "default_memory_scope")]
    pub memory_scope: String,
    #[serde(default = "default_personality_scope")]
    pub personality_scope: String,
    #[serde(default = "default_tenant_policy_mode")]
    pub tenant_policy_mode: String,
    #[serde(default)]
    pub brand_identity: BrandIdentitySpec,
    #[serde(default)]
    pub design_system: DesignSystemSpec,
    #[serde(default)]
    pub operating_policy: OperatingPolicySpec,
}

impl Default for OperatingContextSpec {
    fn default() -> Self {
        Self {
            schema_version: operating_context_schema_version(),
            organization: ContextIdentityRef::organization_default(),
            brand: ContextIdentityRef::brand_default(),
            product: ContextIdentityRef::product_default(),
            user: ContextIdentityRef::user_default(),
            channel: ContextIdentityRef::channel_default(),
            memory_scope: default_memory_scope(),
            personality_scope: default_personality_scope(),
            tenant_policy_mode: default_tenant_policy_mode(),
            brand_identity: BrandIdentitySpec::default(),
            design_system: DesignSystemSpec::default(),
            operating_policy: OperatingPolicySpec::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrandIdentitySpec {
    #[serde(default = "default_brand_voice")]
    pub voice: String,
    #[serde(default = "default_brand_tone")]
    pub tone: String,
    #[serde(default)]
    pub audience: Vec<String>,
    #[serde(default)]
    pub values: Vec<String>,
    #[serde(default)]
    pub terminology: Vec<String>,
}

impl Default for BrandIdentitySpec {
    fn default() -> Self {
        Self {
            voice: default_brand_voice(),
            tone: default_brand_tone(),
            audience: Vec::new(),
            values: Vec::new(),
            terminology: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignSystemSpec {
    #[serde(default = "default_design_token_source")]
    pub token_source: String,
    #[serde(default = "default_component_source")]
    pub component_source: String,
    #[serde(default)]
    pub guidelines: Vec<String>,
}

impl Default for DesignSystemSpec {
    fn default() -> Self {
        Self {
            token_source: default_design_token_source(),
            component_source: default_component_source(),
            guidelines: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatingPolicySpec {
    #[serde(default = "default_data_classification")]
    pub data_classification: String,
    #[serde(default = "default_memory_visibility")]
    pub memory_visibility: String,
    #[serde(default = "default_sharing_policy")]
    pub sharing_policy: String,
    #[serde(default = "default_approval_policy")]
    pub approval_policy: String,
}

impl Default for OperatingPolicySpec {
    fn default() -> Self {
        Self {
            data_classification: default_data_classification(),
            memory_visibility: default_memory_visibility(),
            sharing_policy: default_sharing_policy(),
            approval_policy: default_approval_policy(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextIdentityRef {
    pub scope: String,
    pub id: String,
    pub label: String,
}

impl Default for ContextIdentityRef {
    fn default() -> Self {
        Self {
            scope: "unspecified".to_string(),
            id: "default".to_string(),
            label: "Default".to_string(),
        }
    }
}

impl ContextIdentityRef {
    fn organization_default() -> Self {
        Self {
            scope: "organization".to_string(),
            id: "default-org".to_string(),
            label: "Default Organization".to_string(),
        }
    }

    fn brand_default() -> Self {
        Self {
            scope: "brand".to_string(),
            id: "default-brand".to_string(),
            label: "Default Brand".to_string(),
        }
    }

    fn product_default() -> Self {
        Self {
            scope: "product".to_string(),
            id: "default-product".to_string(),
            label: "Default Product".to_string(),
        }
    }

    fn user_default() -> Self {
        Self {
            scope: "user".to_string(),
            id: "anonymous".to_string(),
            label: "Anonymous User".to_string(),
        }
    }

    fn channel_default() -> Self {
        Self {
            scope: "channel".to_string(),
            id: "local_cli".to_string(),
            label: "Local CLI".to_string(),
        }
    }
}

pub fn parse_intent(goal: &str) -> IntentSpec {
    parse_intent_with_catalog(goal, &builtin_addon_catalog())
}

pub fn parse_intent_with_catalog(goal: &str, catalog: &AddonCatalog) -> IntentSpec {
    parse_intent_with_catalog_and_context(goal, catalog, OperatingContextSpec::default())
}

pub fn parse_intent_with_catalog_and_context(
    goal: &str,
    catalog: &AddonCatalog,
    operating_context: OperatingContextSpec,
) -> IntentSpec {
    let normalized = goal.trim();
    let capability_resolution = resolve_goal_capabilities(normalized, catalog);
    let required_capabilities = capability_resolution.required_capabilities.clone();
    let active_addons = capability_resolution.active_addons.clone();
    let mut deliverables = vec![
        "atomic task graph".to_string(),
        "validation plan".to_string(),
        "artifact manifest".to_string(),
    ];
    let lower = normalized.to_lowercase();

    if lower.contains("api") || lower.contains("platform") {
        deliverables.push("interface contract".to_string());
    }
    if lower.contains("dashboard") || lower.contains("docs") {
        deliverables.push("documentation artifact".to_string());
    }
    if lower.contains("deploy") || lower.contains("deployment") {
        push_deliverable_once(&mut deliverables, "deployment evidence");
    }
    if lower.contains("telegram") {
        push_deliverable_once(&mut deliverables, "Telegram notification evidence");
    }
    if requires_markdown_report(&lower) {
        push_deliverable_once(&mut deliverables, "final Markdown report");
    } else if requires_final_report(&lower) {
        push_deliverable_once(&mut deliverables, "final report artifact");
    }
    if lower.contains("server/client")
        || (lower.contains("server") && lower.contains("client") && lower.contains("evidence"))
    {
        push_deliverable_once(&mut deliverables, "server/client evidence");
    }
    if lower.contains("runtime") || lower.contains("workflow") {
        deliverables.push("persistent runtime state".to_string());
    }
    if lower.contains("n8n") {
        push_deliverable_once(&mut deliverables, "n8n primitive research catalog");
        push_deliverable_once(
            &mut deliverables,
            "Forge primitive promotion recommendation",
        );
    }
    if requires_hackathon_factory(&lower) {
        deliverables.push("hackathon regulation compliance matrix".to_string());
        deliverables.push("idea viability decision".to_string());
        deliverables.push("final idea PDF artifact".to_string());
        deliverables.push("MVP backlog and software factory plan".to_string());
        deliverables.push("pitch package".to_string());
        deliverables.push("buffered deadline improvement loop".to_string());
        deliverables.push("Telegram delivery payload".to_string());
    }
    if requires_daily_goal_research(&lower) {
        deliverables.push("durable daily Goal research schedule".to_string());
        deliverables.push("explicit Goal loop node".to_string());
        deliverables.push("per-Goal research subflow lineage".to_string());
        deliverables.push("Markdown and PDF Goal reports".to_string());
        deliverables.push("Telegram delivery record".to_string());
    }
    if requires_final_user_outcome(&lower) {
        deliverables.push("verified user-facing final outcome".to_string());
    }
    if requires_visual_workspace(&lower) {
        push_deliverable_once(&mut deliverables, "collaborative AI and human whiteboard");
        push_deliverable_once(&mut deliverables, "design token system");
        push_deliverable_once(
            &mut deliverables,
            "component, page, wireframe and flow artifacts",
        );
    }
    for deliverable in explicit_user_facing_deliverables(&lower) {
        push_deliverable_once(&mut deliverables, &deliverable);
    }
    for deliverable in &capability_resolution.intent_overlay.deliverables {
        push_deliverable_once(&mut deliverables, deliverable);
    }

    let mut risks = vec![
        "ambiguous objective can create non-atomic tasks".to_string(),
        "invalid outputs must not be promoted".to_string(),
    ];
    let mut unknowns = vec![
        "provider adapter is selected at execution time".to_string(),
        "human approval rules may vary by workflow".to_string(),
    ];

    if lower.contains("n8n") {
        risks.push("external workflow concepts must not be copied blindly or promoted without Forge validation value".to_string());
        unknowns.push(
            "current n8n source and documentation must be checked during research execution"
                .to_string(),
        );
    }
    if requires_hackathon_factory(&lower) {
        risks.push(
            "user idea may be strategically useful but off-theme unless reframed against the regulation"
                .to_string(),
        );
        risks.push(
            "deadline buffer can be insufficient if the final pitch package is left too late"
                .to_string(),
        );
        risks.push(
            "MVP complexity must not crowd out pitch quality and judging criteria".to_string(),
        );
        unknowns.push(
            "exact final regulation deadline and preferred buffer hours are supplied per run"
                .to_string(),
        );
        unknowns.push("team size, skills and available implementation time must be confirmed before build scope is locked".to_string());
    }
    if requires_daily_goal_research(&lower) {
        risks.push("recurring research must remain Forge-owned instead of becoming an ad hoc terminal loop".to_string());
        risks.push("Telegram delivery records must not expose raw secrets".to_string());
        unknowns.push(
            "live DuckDuckGo and Playwright page availability can vary per daily run".to_string(),
        );
    }
    for risk in &capability_resolution.intent_overlay.risks {
        push_once(&mut risks, risk);
    }
    for unknown in &capability_resolution.intent_overlay.unknowns {
        push_once(&mut unknowns, unknown);
    }

    let mut constraints = vec![
        "context-bounded execution".to_string(),
        "validation before promotion".to_string(),
        "persistent operational state".to_string(),
    ];
    if requires_hackathon_factory(&lower) {
        constraints.push("regulation-first feasibility gate".to_string());
        constraints.push("final package deadline buffer before official submission".to_string());
        constraints.push("PDF and explanation artifact delivered to Telegram".to_string());
    }
    if requires_daily_goal_research(&lower) {
        constraints.push("cron and loop semantics remain native Forge graph state".to_string());
        constraints.push("deterministic code handles stable repeated work".to_string());
        constraints.push("AI is reserved for judgment and summarization".to_string());
    }
    if requires_visual_workspace(&lower) {
        constraints.push(
            "visual artifacts remain Forge-owned workflow state before external export".to_string(),
        );
        constraints
            .push("human and AI collaboration events are auditable in the workflow".to_string());
    }
    for constraint in &capability_resolution.intent_overlay.constraints {
        push_once(&mut constraints, constraint);
    }

    IntentSpec {
        schema_version: intent_schema_version(),
        goal: normalized.to_string(),
        constraints,
        deliverables,
        risks,
        unknowns,
        workflow_mode: workflow_mode_for_capabilities(&required_capabilities, &lower),
        event_policy: EventPolicySpec::default(),
        operating_context,
        required_capabilities,
        active_addons,
        capability_resolution,
    }
}

fn push_deliverable_once(deliverables: &mut Vec<String>, deliverable: &str) {
    if !deliverables
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(deliverable))
    {
        deliverables.push(deliverable.to_string());
    }
}

fn explicit_user_facing_deliverables(normalized_goal: &str) -> Vec<String> {
    let markers = [
        "whose user-facing deliverables are",
        "whose user facing deliverables are",
        "user-facing deliverables are",
        "user facing deliverables are",
        "final user-facing deliverables are",
        "final user facing deliverables are",
        "deliverables are",
        "entregáveis de usuário são",
        "entregaveis de usuario sao",
        "entregáveis finais são",
        "entregaveis finais sao",
    ];
    let Some((marker_index, marker)) = markers
        .iter()
        .filter_map(|marker| normalized_goal.find(marker).map(|index| (index, *marker)))
        .min_by_key(|(index, _)| *index)
    else {
        return Vec::new();
    };

    let tail = &normalized_goal[marker_index + marker.len()..];
    let mut section = tail
        .split(['.', ';', '\n'])
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    for clause_boundary in [
        ", all ",
        ", without ",
        ", using ",
        ", backed by ",
        ", operated ",
        ", powered ",
        ", todos ",
        ", todas ",
        ", sem ",
        ", usando ",
    ] {
        if let Some(index) = section.find(clause_boundary) {
            section.truncate(index);
        }
    }

    section = section
        .replace(" and ", ",")
        .replace(" e ", ",")
        .replace(" & ", ",")
        .replace(" / ", ",");

    section
        .split(',')
        .map(clean_explicit_deliverable)
        .filter(|deliverable| deliverable.chars().count() >= 3)
        .collect()
}

fn clean_explicit_deliverable(raw: &str) -> String {
    raw.trim()
        .trim_matches(|character: char| {
            character == ':'
                || character == '-'
                || character == '–'
                || character == '—'
                || character == '"'
                || character == '\''
                || character.is_whitespace()
        })
        .trim_start_matches("a ")
        .trim_start_matches("an ")
        .trim_start_matches("the ")
        .trim_start_matches("o ")
        .trim_start_matches("a ")
        .trim_start_matches("os ")
        .trim_start_matches("as ")
        .trim()
        .to_string()
}

fn push_once(values: &mut Vec<String>, value: &str) {
    if !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}

fn workflow_mode_for_capabilities(
    capabilities: &[CapabilityNeed],
    lower_goal: &str,
) -> WorkflowModeSpec {
    let persistent = capabilities
        .iter()
        .any(|capability| capability.id == CAP_DAILY_GOAL_RESEARCH)
        || lower_goal.contains("persistente")
        || lower_goal.contains("persistent")
        || lower_goal.contains("indefinidamente")
        || lower_goal.contains("long-running")
        || lower_goal.contains("longa duração")
        || lower_goal.contains("continue every")
        || lower_goal.contains("every ")
        || lower_goal.contains("recorrente")
        || lower_goal.contains("recurring")
        || lower_goal.contains("cron")
        || lower_goal.contains("schedule")
        || lower_goal.contains("daily")
        || lower_goal.contains("weekly")
        || lower_goal.contains("diário")
        || lower_goal.contains("semanal");

    WorkflowModeSpec {
        kind: if persistent {
            "persistent_workflow".to_string()
        } else {
            "ephemeral_workflow".to_string()
        },
        expected_lifetime: if persistent {
            "indefinite_event_reactive".to_string()
        } else {
            "finite_goal_bound".to_string()
        },
        can_become_persistent: true,
        scale_to_zero_policy: if persistent {
            "idle_waiting_for_events".to_string()
        } else {
            "scale_to_zero_after_completion".to_string()
        },
    }
}

fn requires_hackathon_factory(lower_goal: &str) -> bool {
    (lower_goal.contains("hackathon")
        || lower_goal.contains("ideathon")
        || lower_goal.contains("maratona"))
        && (lower_goal.contains("mvp")
            || lower_goal.contains("software factory")
            || lower_goal.contains("fábrica")
            || lower_goal.contains("factory"))
}

pub fn requires_hackathon_factory_capability(intent: &IntentSpec) -> bool {
    intent.has_capability(CAP_HACKATHON_FACTORY)
}

pub fn requires_daily_goal_research_capability(intent: &IntentSpec) -> bool {
    intent.has_capability(CAP_DAILY_GOAL_RESEARCH)
}

pub fn requires_visual_workspace_capability(intent: &IntentSpec) -> bool {
    intent.has_capability(CAP_VISUAL_WORKSPACE)
}

pub fn requires_workflow_automation_research_capability(intent: &IntentSpec) -> bool {
    intent.has_capability(CAP_WORKFLOW_AUTOMATION_RESEARCH)
}

pub fn requires_async_runtime_capability(intent: &IntentSpec) -> bool {
    intent.has_capability(CAP_ASYNC_RUNTIME)
}

pub fn requires_telegram_notification_capability(intent: &IntentSpec) -> bool {
    intent.has_capability(CAP_TELEGRAM_NOTIFICATION)
}

fn requires_daily_goal_research(lower_goal: &str) -> bool {
    (lower_goal.contains("daily goal research")
        || lower_goal.contains("daily goal")
        || lower_goal.contains("goal research workflow"))
        && (lower_goal.contains("goal") || lower_goal.contains("goals"))
}

fn requires_final_user_outcome(lower_goal: &str) -> bool {
    lower_goal.contains("resultado final")
        || lower_goal.contains("resultados finais")
        || lower_goal.contains("final result")
        || lower_goal.contains("final workflow result")
        || lower_goal.contains("relatório final")
        || lower_goal.contains("relatorio final")
        || lower_goal.contains("final report")
        || lower_goal.contains("entrega final")
        || lower_goal.contains("deliver final")
        || lower_goal.contains("final outcome")
}

fn requires_markdown_report(lower_goal: &str) -> bool {
    lower_goal.contains("markdown report")
        || lower_goal.contains("markdown final report")
        || ((lower_goal.contains("markdown") || lower_goal.contains(".md"))
            && (lower_goal.contains("report")
                || lower_goal.contains("relatório")
                || lower_goal.contains("relatorio")))
}

fn requires_final_report(lower_goal: &str) -> bool {
    lower_goal.contains("final report")
        || lower_goal.contains("relatório final")
        || lower_goal.contains("relatorio final")
}

fn requires_visual_workspace(lower_goal: &str) -> bool {
    lower_goal.contains("visual")
        || lower_goal.contains("whiteboard")
        || lower_goal.contains("figma")
        || lower_goal.contains("wireframe")
        || lower_goal.contains("tokens")
        || lower_goal.contains("componentes")
        || lower_goal.contains("components")
        || lower_goal.contains("design system")
        || lower_goal.contains("sistema de design")
}

fn intent_schema_version() -> String {
    "forge.intent.v2".to_string()
}

fn event_policy_schema_version() -> String {
    "forge.event_policy.v1".to_string()
}

fn operating_context_schema_version() -> String {
    "forge.operating_context.v1".to_string()
}

fn default_workflow_kind() -> String {
    "ephemeral_workflow".to_string()
}

fn default_workflow_lifetime() -> String {
    "finite_goal_bound".to_string()
}

fn default_scale_to_zero_policy() -> String {
    "scale_to_zero_after_completion".to_string()
}

fn default_event_runtime() -> String {
    "origin_agnostic_event_engine".to_string()
}

fn default_memory_scope() -> String {
    "organization_project_session".to_string()
}

fn default_personality_scope() -> String {
    "organization_workflow_node".to_string()
}

fn default_tenant_policy_mode() -> String {
    "audit".to_string()
}

fn default_brand_voice() -> String {
    "organization_default".to_string()
}

fn default_brand_tone() -> String {
    "neutral_professional".to_string()
}

fn default_design_token_source() -> String {
    "forge_tokens".to_string()
}

fn default_component_source() -> String {
    "forge_components".to_string()
}

fn default_data_classification() -> String {
    "internal".to_string()
}

fn default_memory_visibility() -> String {
    "organization_project".to_string()
}

fn default_sharing_policy() -> String {
    "private_by_default".to_string()
}

fn default_approval_policy() -> String {
    "risk_based".to_string()
}
