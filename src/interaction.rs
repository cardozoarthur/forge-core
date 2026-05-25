use crate::graph::{
    AtomicTask, HumanChoiceOption, HumanDecisionRecord, HumanFormField, HumanFormSchema,
    HumanInteractionSpec, TaskStatus, Workflow, WorkflowRevision,
};
use crate::storage::ForgeStore;
use anyhow::{bail, Context, Result};
use chrono::{Duration, Utc};
use serde::Serialize;
use std::collections::BTreeMap;
use uuid::Uuid;

const HUMAN_INTERACTION_SUMMARY_SCHEMA_VERSION: &str = "forge.human_interaction.summary.v1";

#[derive(Debug, Clone, Default, Serialize)]
pub struct HumanInteractionSummary {
    pub schema_version: String,
    pub total: usize,
    pub required: usize,
    pub pending: usize,
    pub answered: usize,
    pub timed_out: usize,
    pub pending_required: usize,
    pub timed_out_required: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct HumanInteractionReport {
    pub status: String,
    pub workflow_id: String,
    pub task_id: String,
    pub task_status: String,
    pub origin: String,
    pub workflow_revision: u64,
    pub interaction: HumanInteractionSpec,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<HumanDecisionRecord>,
    pub summary: HumanInteractionSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct HumanInteractionListReport {
    pub status: String,
    pub summary: HumanInteractionSummary,
    pub interactions: Vec<ListedHumanInteraction>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListedHumanInteraction {
    pub workflow_id: String,
    pub task_id: String,
    pub task_title: String,
    pub interaction: HumanInteractionSpec,
}

#[derive(Debug, Clone, Serialize)]
pub struct HumanInteractionBlocker {
    pub schema_version: String,
    pub workflow_id: String,
    pub task_id: String,
    pub task_title: String,
    pub interaction_id: String,
    pub kind: String,
    pub state: String,
    pub prompt: String,
    pub required: bool,
}

pub struct CreateChoiceInteractionRequest<'a> {
    pub workflow_id: &'a str,
    pub task_id: &'a str,
    pub kind: &'a str,
    pub prompt: &'a str,
    pub choices: &'a [String],
    pub timeout_seconds: Option<u64>,
    pub origin: &'a str,
}

struct CreateInteractionRequest<'a> {
    workflow_id: &'a str,
    task_id: &'a str,
    kind: &'a str,
    prompt: &'a str,
    choices: Vec<HumanChoiceOption>,
    form: Option<HumanFormSchema>,
    timeout_seconds: Option<u64>,
    origin: &'a str,
}

pub fn summarize_human_interactions(tasks: &[AtomicTask]) -> HumanInteractionSummary {
    let mut summary = HumanInteractionSummary {
        schema_version: HUMAN_INTERACTION_SUMMARY_SCHEMA_VERSION.to_string(),
        ..HumanInteractionSummary::default()
    };

    for interaction in tasks
        .iter()
        .filter_map(|task| task.human_interaction.as_ref())
    {
        summary.total += 1;
        if interaction.required {
            summary.required += 1;
        }
        match interaction.state.as_str() {
            "pending" => {
                summary.pending += 1;
                if interaction.required {
                    summary.pending_required += 1;
                }
            }
            "answered" => summary.answered += 1,
            "timed_out" => {
                summary.timed_out += 1;
                if interaction.required {
                    summary.timed_out_required += 1;
                }
            }
            _ => {}
        }
    }

    summary
}

pub fn blocking_human_interaction(workflow: &Workflow) -> Option<HumanInteractionBlocker> {
    workflow.tasks.iter().find_map(|task| {
        let interaction = task.human_interaction.as_ref()?;
        if interaction.required && matches!(interaction.state.as_str(), "pending" | "timed_out") {
            Some(HumanInteractionBlocker {
                schema_version: "forge.human_interaction.blocker.v1".to_string(),
                workflow_id: workflow.id.clone(),
                task_id: task.id.clone(),
                task_title: task.title.clone(),
                interaction_id: interaction.interaction_id.clone(),
                kind: interaction.kind.clone(),
                state: interaction.state.clone(),
                prompt: interaction.prompt.clone(),
                required: interaction.required,
            })
        } else {
            None
        }
    })
}

pub fn create_choice_interaction(
    store: &ForgeStore,
    request: CreateChoiceInteractionRequest<'_>,
) -> Result<HumanInteractionReport> {
    let kind = normalize_choice_kind(request.kind)?;
    let parsed_choices = parse_choices(request.choices)?;
    if parsed_choices.is_empty() {
        bail!("at least one --choice is required for a human choice interaction");
    }
    create_interaction(
        store,
        CreateInteractionRequest {
            workflow_id: request.workflow_id,
            task_id: request.task_id,
            kind,
            prompt: request.prompt,
            choices: parsed_choices,
            form: None,
            timeout_seconds: request.timeout_seconds,
            origin: request.origin,
        },
    )
}

pub fn create_form_interaction(
    store: &ForgeStore,
    workflow_id: &str,
    task_id: &str,
    prompt: &str,
    fields: &[String],
    timeout_seconds: Option<u64>,
    origin: &str,
) -> Result<HumanInteractionReport> {
    let fields = parse_form_fields(fields)?;
    if fields.is_empty() {
        bail!("at least one --field is required for a human form interaction");
    }
    let form = HumanFormSchema {
        schema_version: "forge.human_form.v1".to_string(),
        title: prompt.to_string(),
        review_before_submit: true,
        save_as_template_available: true,
        fields,
    };
    create_interaction(
        store,
        CreateInteractionRequest {
            workflow_id,
            task_id,
            kind: "form",
            prompt,
            choices: Vec::new(),
            form: Some(form),
            timeout_seconds,
            origin,
        },
    )
}

pub fn answer_human_interaction(
    store: &ForgeStore,
    workflow_id: &str,
    task_id: &str,
    selected_options: &[String],
    field_values: &[String],
    rationale: Option<&str>,
    origin: &str,
) -> Result<HumanInteractionReport> {
    let mut workflow = store.load_workflow(workflow_id)?;
    let parsed_fields = parse_field_values(field_values)?;
    let workflow_goal = workflow.goal.clone();
    let (interaction, decision, task_status) = {
        let task = workflow_task_mut(&mut workflow, task_id)?;
        let interaction = task
            .human_interaction
            .as_mut()
            .with_context(|| format!("task {task_id} has no human interaction"))?;
        if interaction.state != "pending" {
            bail!(
                "human interaction {} is not pending: {}",
                interaction.interaction_id,
                interaction.state
            );
        }
        validate_answer(interaction, selected_options, &parsed_fields)?;
        let decision = HumanDecisionRecord {
            schema_version: "forge.human_decision.v1".to_string(),
            decision_id: format!("decision_{}", compact_uuid()),
            workflow_id: workflow_id.to_string(),
            task_id: task_id.to_string(),
            interaction_id: interaction.interaction_id.clone(),
            kind: interaction.kind.clone(),
            status: "answered".to_string(),
            origin: origin.to_string(),
            selected_options: selected_options.to_vec(),
            field_values: parsed_fields,
            rationale: rationale.map(str::to_string),
            affected_tasks: vec![task_id.to_string()],
            affected_goals: vec![workflow_goal],
            affected_artifacts: Vec::new(),
            decided_at: Utc::now(),
            audit_event: "human_decision_recorded".to_string(),
        };
        interaction.state = "answered".to_string();
        interaction.decisions.push(decision.clone());
        task.human_required = false;
        task.status = TaskStatus::Pending;
        task.work_item.backlog_state = "ready_after_human_decision".to_string();
        (
            interaction.clone(),
            decision,
            task_status_label(&task.status),
        )
    };

    if !workflow_has_blocking_human_interaction(&workflow) {
        workflow.status = "pending".to_string();
    }
    let revision = push_interaction_revision(
        &mut workflow,
        origin,
        "human_interaction_answer",
        &format!("human interaction answered for task {task_id}"),
    );
    let summary = summarize_human_interactions(&workflow.tasks);
    store.save_workflow(&workflow)?;
    store.record_event(
        workflow_id,
        "human_interaction_answered",
        &serde_json::json!({
            "origin": origin,
            "task_id": task_id,
            "revision": revision,
            "decision": decision
        }),
    )?;

    Ok(HumanInteractionReport {
        status: "human_interaction_answered".to_string(),
        workflow_id: workflow_id.to_string(),
        task_id: task_id.to_string(),
        task_status,
        origin: origin.to_string(),
        workflow_revision: revision,
        interaction,
        decision: Some(decision),
        summary,
    })
}

pub fn expire_human_interaction(
    store: &ForgeStore,
    workflow_id: &str,
    task_id: &str,
    origin: &str,
) -> Result<HumanInteractionReport> {
    let mut workflow = store.load_workflow(workflow_id)?;
    let (interaction, task_status) = {
        let task = workflow_task_mut(&mut workflow, task_id)?;
        let interaction = task
            .human_interaction
            .as_mut()
            .with_context(|| format!("task {task_id} has no human interaction"))?;
        if interaction.state != "pending" {
            bail!(
                "human interaction {} is not pending: {}",
                interaction.interaction_id,
                interaction.state
            );
        }
        let timeout_at = interaction.timeout_at.with_context(|| {
            format!(
                "human interaction {} has no timeout",
                interaction.interaction_id
            )
        })?;
        if timeout_at > Utc::now() {
            bail!(
                "human interaction {} has not timed out yet",
                interaction.interaction_id
            );
        }
        interaction.state = "timed_out".to_string();
        task.human_required = true;
        task.status = TaskStatus::Blocked;
        task.work_item.backlog_state = "blocked_on_human_interaction_timeout".to_string();
        (interaction.clone(), task_status_label(&task.status))
    };
    workflow.status = "blocked".to_string();
    let revision = push_interaction_revision(
        &mut workflow,
        origin,
        "human_interaction_timeout",
        &format!("human interaction timed out for task {task_id}"),
    );
    let summary = summarize_human_interactions(&workflow.tasks);
    store.save_workflow(&workflow)?;
    store.record_event(
        workflow_id,
        "human_interaction_timed_out",
        &serde_json::json!({
            "origin": origin,
            "task_id": task_id,
            "revision": revision,
            "interaction_id": interaction.interaction_id
        }),
    )?;

    Ok(HumanInteractionReport {
        status: "human_interaction_timed_out".to_string(),
        workflow_id: workflow_id.to_string(),
        task_id: task_id.to_string(),
        task_status,
        origin: origin.to_string(),
        workflow_revision: revision,
        interaction,
        decision: None,
        summary,
    })
}

pub fn list_human_interactions(store: &ForgeStore) -> Result<HumanInteractionListReport> {
    let workflows = store.load_workflows()?;
    let mut interactions = Vec::new();
    let mut all_tasks = Vec::new();
    for workflow in workflows {
        for task in workflow.tasks {
            all_tasks.push(task.clone());
            if let Some(interaction) = task.human_interaction {
                interactions.push(ListedHumanInteraction {
                    workflow_id: workflow.id.clone(),
                    task_id: task.id,
                    task_title: task.title,
                    interaction,
                });
            }
        }
    }

    Ok(HumanInteractionListReport {
        status: "human_interactions_loaded".to_string(),
        summary: summarize_human_interactions(&all_tasks),
        interactions,
    })
}

fn create_interaction(
    store: &ForgeStore,
    request: CreateInteractionRequest<'_>,
) -> Result<HumanInteractionReport> {
    let mut workflow = store.load_workflow(request.workflow_id)?;
    let interaction = HumanInteractionSpec {
        schema_version: "forge.human_interaction.v1".to_string(),
        interaction_id: format!("hi_{}", compact_uuid()),
        kind: request.kind.to_string(),
        prompt: request.prompt.to_string(),
        required: true,
        state: "pending".to_string(),
        explanation:
            "Forge paused this task because human judgment is required before execution continues."
                .to_string(),
        choices: request.choices,
        form: request.form,
        timeout_at: request
            .timeout_seconds
            .map(|seconds| Utc::now() + Duration::seconds(seconds.min(i64::MAX as u64) as i64)),
        on_timeout: "keep_blocked_and_notify".to_string(),
        created_at: Utc::now(),
        origin: request.origin.to_string(),
        pending_decision_id: format!("decision_{}", compact_uuid()),
        decisions: Vec::new(),
    };
    let task_status = {
        let task = workflow_task_mut(&mut workflow, request.task_id)?;
        task.human_required = true;
        task.status = TaskStatus::Blocked;
        task.work_item.backlog_state = "blocked_on_human_interaction".to_string();
        task.human_interaction = Some(interaction.clone());
        task_status_label(&task.status)
    };
    workflow.status = "blocked".to_string();
    let revision = push_interaction_revision(
        &mut workflow,
        request.origin,
        "human_interaction_create",
        &format!("human interaction created for task {}", request.task_id),
    );
    let summary = summarize_human_interactions(&workflow.tasks);
    store.save_workflow(&workflow)?;
    store.record_event(
        request.workflow_id,
        "human_interaction_created",
        &serde_json::json!({
            "origin": request.origin,
            "task_id": request.task_id,
            "revision": revision,
            "interaction": interaction
        }),
    )?;

    Ok(HumanInteractionReport {
        status: "human_interaction_created".to_string(),
        workflow_id: request.workflow_id.to_string(),
        task_id: request.task_id.to_string(),
        task_status,
        origin: request.origin.to_string(),
        workflow_revision: revision,
        interaction,
        decision: None,
        summary,
    })
}

fn workflow_task_mut<'a>(workflow: &'a mut Workflow, task_id: &str) -> Result<&'a mut AtomicTask> {
    workflow
        .tasks
        .iter_mut()
        .find(|task| task.id == task_id)
        .with_context(|| format!("task not found in workflow {}: {task_id}", workflow.id))
}

fn workflow_has_blocking_human_interaction(workflow: &Workflow) -> bool {
    blocking_human_interaction(workflow).is_some()
}

fn validate_answer(
    interaction: &HumanInteractionSpec,
    selected_options: &[String],
    field_values: &BTreeMap<String, String>,
) -> Result<()> {
    if interaction.kind == "form" {
        let form = interaction
            .form
            .as_ref()
            .context("form interaction is missing form schema")?;
        for field in &form.fields {
            if field.required && !field_values.contains_key(&field.id) {
                bail!("missing required form field: {}", field.id);
            }
        }
        return Ok(());
    }

    if selected_options.is_empty() {
        bail!("at least one --selected option is required for this interaction");
    }
    let allowed = interaction
        .choices
        .iter()
        .map(|choice| choice.id.as_str())
        .collect::<Vec<_>>();
    for selected in selected_options {
        if !allowed.contains(&selected.as_str()) {
            bail!("unknown selected option: {selected}");
        }
    }
    if matches!(
        interaction.kind.as_str(),
        "single_choice" | "yes_no" | "risk_acknowledgement"
    ) && selected_options.len() != 1
    {
        bail!(
            "interaction kind {} requires exactly one selection",
            interaction.kind
        );
    }
    Ok(())
}

fn parse_choices(values: &[String]) -> Result<Vec<HumanChoiceOption>> {
    values.iter().map(|value| parse_choice(value)).collect()
}

fn parse_choice(value: &str) -> Result<HumanChoiceOption> {
    let (id, rest) = value
        .split_once('=')
        .with_context(|| format!("invalid choice spec `{value}`; expected id=Label"))?;
    let mut parts = rest.split('|');
    let label = parts.next().unwrap_or(rest).trim();
    let description = parts.next().unwrap_or(label).trim();
    let effect = parts
        .next()
        .unwrap_or("resume workflow with the selected human direction")
        .trim();
    let id = id.trim();
    if id.is_empty() || label.is_empty() {
        bail!("invalid choice spec `{value}`; id and label are required");
    }
    Ok(HumanChoiceOption {
        id: id.to_string(),
        label: label.to_string(),
        description: description.to_string(),
        effect: effect.to_string(),
    })
}

fn parse_form_fields(values: &[String]) -> Result<Vec<HumanFormField>> {
    values.iter().map(|value| parse_form_field(value)).collect()
}

fn parse_form_field(value: &str) -> Result<HumanFormField> {
    let parts = value.split(':').collect::<Vec<_>>();
    if parts.len() < 3 {
        bail!("invalid field spec `{value}`; expected id:type:required|optional[:default]");
    }
    let id = parts[0].trim();
    let field_type = parts[1].trim();
    let required = match parts[2].trim() {
        "required" => true,
        "optional" => false,
        other => bail!("invalid field requirement `{other}`; expected required or optional"),
    };
    if id.is_empty() || field_type.is_empty() {
        bail!("invalid field spec `{value}`; id and type are required");
    }
    Ok(HumanFormField {
        id: id.to_string(),
        label: id.replace('_', " "),
        field_type: field_type.to_string(),
        required,
        default_value: parts.get(3).map(|value| (*value).to_string()),
        help_text: format!("Provide {id} before the workflow resumes"),
    })
}

fn parse_field_values(values: &[String]) -> Result<BTreeMap<String, String>> {
    let mut parsed = BTreeMap::new();
    for value in values {
        let (key, field_value) = value
            .split_once('=')
            .with_context(|| format!("invalid field value `{value}`; expected id=value"))?;
        let key = key.trim();
        if key.is_empty() {
            bail!("invalid field value `{value}`; field id is required");
        }
        parsed.insert(key.to_string(), field_value.trim().to_string());
    }
    Ok(parsed)
}

fn normalize_choice_kind(kind: &str) -> Result<&'static str> {
    match kind.trim() {
        "single_choice" => Ok("single_choice"),
        "multi_choice" => Ok("multi_choice"),
        "ranked_choice" => Ok("ranked_choice"),
        "approve_reject_refine_combine" => Ok("approve_reject_refine_combine"),
        "yes_no" => Ok("yes_no"),
        "risk_acknowledgement" => Ok("risk_acknowledgement"),
        other => bail!("unsupported human interaction kind: {other}"),
    }
}

fn push_interaction_revision(
    workflow: &mut Workflow,
    origin: &str,
    change_type: &str,
    summary: &str,
) -> u64 {
    let revision = workflow
        .revisions
        .last()
        .map(|revision| revision.revision + 1)
        .unwrap_or(1);
    workflow.revisions.push(WorkflowRevision {
        revision,
        origin: origin.to_string(),
        change_type: change_type.to_string(),
        summary: summary.to_string(),
        created_at: Utc::now(),
    });
    revision
}

fn task_status_label(status: &TaskStatus) -> String {
    serde_json::to_value(status)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

fn compact_uuid() -> String {
    Uuid::new_v4().to_string().replace('-', "")
}
