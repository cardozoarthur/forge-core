use crate::graph::{AtomicTask, ExecutorKind, Workflow};
use crate::storage::{ForgeStore, StoreEvent};
use anyhow::Result;
use serde::Serialize;
use std::collections::BTreeMap;

pub const COST_LEDGER_SCHEMA_VERSION: &str = "forge.cost_ledger.v1";

#[derive(Debug, Clone, Serialize)]
pub struct CostLedgerReport {
    pub schema_version: String,
    pub status: String,
    pub filters: CostLedgerFilters,
    pub summary: CostLedgerSummary,
    pub tenants: Vec<CostLedgerTenantSummary>,
    pub addons: Vec<CostLedgerAddonSummary>,
    pub workflows: Vec<CostLedgerWorkflow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CostLedgerFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brand_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CostLedgerSummary {
    pub workflow_count: usize,
    pub node_count: usize,
    pub ai_node_count: usize,
    pub deterministic_node_count: usize,
    pub model_call_required_node_count: usize,
    pub model_call_avoided_node_count: usize,
    pub estimated_task_cost_total_usd: f64,
    pub observed_event_cost_total_usd: f64,
    pub observed_tokens_in_total: i64,
    pub observed_tokens_out_total: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CostLedgerTenantSummary {
    pub organization_id: String,
    pub brand_id: String,
    pub product_id: String,
    pub workflow_count: usize,
    pub node_count: usize,
    pub estimated_task_cost_total_usd: f64,
    pub observed_event_cost_total_usd: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CostLedgerAddonSummary {
    pub addon_id: String,
    pub workflow_count: usize,
    pub node_count: usize,
    pub estimated_task_cost_total_usd: f64,
    pub observed_event_cost_total_usd: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CostLedgerWorkflow {
    pub workflow_id: String,
    pub goal: String,
    pub status: String,
    pub organization_id: String,
    pub brand_id: String,
    pub product_id: String,
    pub user_id: String,
    pub channel_id: String,
    pub node_count: usize,
    pub ai_node_count: usize,
    pub deterministic_node_count: usize,
    pub model_call_required_node_count: usize,
    pub model_call_avoided_node_count: usize,
    pub estimated_task_cost_total_usd: f64,
    pub observed_event_cost_total_usd: f64,
    pub observed_tokens_in_total: i64,
    pub observed_tokens_out_total: i64,
    pub nodes: Vec<CostLedgerNode>,
    pub observed_events: Vec<CostLedgerObservedEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CostLedgerNode {
    pub task_id: String,
    pub title: String,
    pub executor: String,
    pub status: String,
    pub addon_id: Option<String>,
    pub execution_policy_mode: String,
    pub model_call_required: bool,
    pub model_call_avoided: bool,
    pub estimated_task_cost_usd: f64,
    pub observed_event_cost_usd: f64,
    pub observed_tokens_in: i64,
    pub observed_tokens_out: i64,
    pub cost_model: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CostLedgerObservedEvent {
    pub event_id: i64,
    pub event_kind: String,
    pub task_id: Option<String>,
    pub created_at: String,
    pub estimated_usd: f64,
    pub tokens_in: i64,
    pub tokens_out: i64,
}

#[derive(Default)]
struct SummaryBucket {
    workflow_ids: Vec<String>,
    node_count: usize,
    estimated_task_cost_total_usd: f64,
    observed_event_cost_total_usd: f64,
}

pub fn build_cost_ledger(
    store: &ForgeStore,
    workflow_id: Option<&str>,
    organization_id: Option<&str>,
    brand_id: Option<&str>,
    product_id: Option<&str>,
) -> Result<CostLedgerReport> {
    let workflows = if let Some(workflow_id) = workflow_id.filter(|value| !value.trim().is_empty())
    {
        vec![store.load_workflow(workflow_id)?]
    } else {
        store.load_workflows()?
    };
    let mut ledger_workflows = Vec::new();
    for workflow in workflows {
        if !tenant_filter_matches(&workflow, organization_id, brand_id, product_id) {
            continue;
        }
        ledger_workflows.push(build_workflow_cost_ledger(store, &workflow)?);
    }
    let summary = summarize_workflows(&ledger_workflows);
    let tenants = summarize_tenants(&ledger_workflows);
    let addons = summarize_addons(&ledger_workflows);
    Ok(CostLedgerReport {
        schema_version: COST_LEDGER_SCHEMA_VERSION.to_string(),
        status: "cost_ledger_loaded".to_string(),
        filters: CostLedgerFilters {
            workflow_id: normalize_filter(workflow_id),
            organization_id: normalize_filter(organization_id),
            brand_id: normalize_filter(brand_id),
            product_id: normalize_filter(product_id),
        },
        summary,
        tenants,
        addons,
        workflows: ledger_workflows,
    })
}

fn build_workflow_cost_ledger(
    store: &ForgeStore,
    workflow: &Workflow,
) -> Result<CostLedgerWorkflow> {
    let events = store.load_workflow_events(&workflow.id)?;
    let observed_events = events
        .iter()
        .filter_map(cost_event_from_store_event)
        .collect::<Vec<_>>();
    let nodes = workflow
        .tasks
        .iter()
        .map(|task| node_cost_ledger(task, &observed_events))
        .collect::<Vec<_>>();
    let summary = summarize_nodes(&nodes);
    let context = &workflow.intent.operating_context;
    Ok(CostLedgerWorkflow {
        workflow_id: workflow.id.clone(),
        goal: workflow.goal.clone(),
        status: workflow.status.clone(),
        organization_id: context.organization.id.clone(),
        brand_id: context.brand.id.clone(),
        product_id: context.product.id.clone(),
        user_id: context.user.id.clone(),
        channel_id: context.channel.id.clone(),
        node_count: summary.node_count,
        ai_node_count: summary.ai_node_count,
        deterministic_node_count: summary.deterministic_node_count,
        model_call_required_node_count: summary.model_call_required_node_count,
        model_call_avoided_node_count: summary.model_call_avoided_node_count,
        estimated_task_cost_total_usd: summary.estimated_task_cost_total_usd,
        observed_event_cost_total_usd: summary.observed_event_cost_total_usd,
        observed_tokens_in_total: summary.observed_tokens_in_total,
        observed_tokens_out_total: summary.observed_tokens_out_total,
        nodes,
        observed_events,
    })
}

fn node_cost_ledger(
    task: &AtomicTask,
    observed_events: &[CostLedgerObservedEvent],
) -> CostLedgerNode {
    let task_events = observed_events
        .iter()
        .filter(|event| event.task_id.as_deref() == Some(task.id.as_str()))
        .collect::<Vec<_>>();
    let observed_event_cost_usd = normalize_money(
        task_events
            .iter()
            .map(|event| event.estimated_usd)
            .sum::<f64>(),
    );
    let observed_tokens_in = task_events.iter().map(|event| event.tokens_in).sum::<i64>();
    let observed_tokens_out = task_events
        .iter()
        .map(|event| event.tokens_out)
        .sum::<i64>();
    let model_call_required =
        task.execution_policy.ai_allowed && !task.execution_policy.deterministic;
    let model_call_avoided = !model_call_required && !task.execution_policy.ai_allowed;
    CostLedgerNode {
        task_id: task.id.clone(),
        title: task.title.clone(),
        executor: executor_label(&task.executor).to_string(),
        status: format!("{:?}", task.status).to_lowercase(),
        addon_id: source_addon_from_task(task),
        execution_policy_mode: task.execution_policy.mode.clone(),
        model_call_required,
        model_call_avoided,
        estimated_task_cost_usd: normalize_money(task.cost.estimated_cost_usd),
        observed_event_cost_usd,
        observed_tokens_in,
        observed_tokens_out,
        cost_model: task.cost.cost_model.clone(),
    }
}

fn summarize_workflows(workflows: &[CostLedgerWorkflow]) -> CostLedgerSummary {
    let mut summary = CostLedgerSummary {
        workflow_count: workflows.len(),
        ..CostLedgerSummary::default()
    };
    for workflow in workflows {
        summary.node_count += workflow.node_count;
        summary.ai_node_count += workflow.ai_node_count;
        summary.deterministic_node_count += workflow.deterministic_node_count;
        summary.model_call_required_node_count += workflow.model_call_required_node_count;
        summary.model_call_avoided_node_count += workflow.model_call_avoided_node_count;
        summary.estimated_task_cost_total_usd += workflow.estimated_task_cost_total_usd;
        summary.observed_event_cost_total_usd += workflow.observed_event_cost_total_usd;
        summary.observed_tokens_in_total += workflow.observed_tokens_in_total;
        summary.observed_tokens_out_total += workflow.observed_tokens_out_total;
    }
    summary.estimated_task_cost_total_usd = normalize_money(summary.estimated_task_cost_total_usd);
    summary.observed_event_cost_total_usd = normalize_money(summary.observed_event_cost_total_usd);
    summary
}

fn summarize_nodes(nodes: &[CostLedgerNode]) -> CostLedgerSummary {
    let mut summary = CostLedgerSummary {
        node_count: nodes.len(),
        ..CostLedgerSummary::default()
    };
    for node in nodes {
        if matches!(node.executor.as_str(), "ai" | "mixed") {
            summary.ai_node_count += 1;
        }
        if !node.model_call_required {
            summary.deterministic_node_count += 1;
        }
        if node.model_call_required {
            summary.model_call_required_node_count += 1;
        }
        if node.model_call_avoided {
            summary.model_call_avoided_node_count += 1;
        }
        summary.estimated_task_cost_total_usd += node.estimated_task_cost_usd;
        summary.observed_event_cost_total_usd += node.observed_event_cost_usd;
        summary.observed_tokens_in_total += node.observed_tokens_in;
        summary.observed_tokens_out_total += node.observed_tokens_out;
    }
    summary.estimated_task_cost_total_usd = normalize_money(summary.estimated_task_cost_total_usd);
    summary.observed_event_cost_total_usd = normalize_money(summary.observed_event_cost_total_usd);
    summary
}

fn summarize_tenants(workflows: &[CostLedgerWorkflow]) -> Vec<CostLedgerTenantSummary> {
    let mut buckets: BTreeMap<(String, String, String), SummaryBucket> = BTreeMap::new();
    for workflow in workflows {
        let key = (
            workflow.organization_id.clone(),
            workflow.brand_id.clone(),
            workflow.product_id.clone(),
        );
        let bucket = buckets.entry(key).or_default();
        add_workflow_to_bucket(bucket, workflow);
    }
    buckets
        .into_iter()
        .map(
            |((organization_id, brand_id, product_id), bucket)| CostLedgerTenantSummary {
                organization_id,
                brand_id,
                product_id,
                workflow_count: unique_count(&bucket.workflow_ids),
                node_count: bucket.node_count,
                estimated_task_cost_total_usd: normalize_money(
                    bucket.estimated_task_cost_total_usd,
                ),
                observed_event_cost_total_usd: normalize_money(
                    bucket.observed_event_cost_total_usd,
                ),
            },
        )
        .collect()
}

fn summarize_addons(workflows: &[CostLedgerWorkflow]) -> Vec<CostLedgerAddonSummary> {
    let mut buckets: BTreeMap<String, SummaryBucket> = BTreeMap::new();
    for workflow in workflows {
        for node in &workflow.nodes {
            let addon_id = node
                .addon_id
                .clone()
                .unwrap_or_else(|| "core_or_unassigned".to_string());
            let bucket = buckets.entry(addon_id).or_default();
            if !bucket.workflow_ids.contains(&workflow.workflow_id) {
                bucket.workflow_ids.push(workflow.workflow_id.clone());
            }
            bucket.node_count += 1;
            bucket.estimated_task_cost_total_usd += node.estimated_task_cost_usd;
            bucket.observed_event_cost_total_usd += node.observed_event_cost_usd;
        }
    }
    buckets
        .into_iter()
        .map(|(addon_id, bucket)| CostLedgerAddonSummary {
            addon_id,
            workflow_count: unique_count(&bucket.workflow_ids),
            node_count: bucket.node_count,
            estimated_task_cost_total_usd: normalize_money(bucket.estimated_task_cost_total_usd),
            observed_event_cost_total_usd: normalize_money(bucket.observed_event_cost_total_usd),
        })
        .collect()
}

fn add_workflow_to_bucket(bucket: &mut SummaryBucket, workflow: &CostLedgerWorkflow) {
    if !bucket.workflow_ids.contains(&workflow.workflow_id) {
        bucket.workflow_ids.push(workflow.workflow_id.clone());
    }
    bucket.node_count += workflow.node_count;
    bucket.estimated_task_cost_total_usd += workflow.estimated_task_cost_total_usd;
    bucket.observed_event_cost_total_usd += workflow.observed_event_cost_total_usd;
}

fn cost_event_from_store_event(event: &StoreEvent) -> Option<CostLedgerObservedEvent> {
    let cost = event
        .data
        .get("cost")
        .or_else(|| event.data.get("executor_cost"))?;
    let estimated_usd = cost
        .get("estimated_usd")
        .and_then(|value| value.as_f64())
        .filter(|value| value.is_finite() && *value >= 0.0)?;
    Some(CostLedgerObservedEvent {
        event_id: event.id,
        event_kind: event.kind.clone(),
        task_id: event_task_id(event),
        created_at: event.created_at.clone(),
        estimated_usd: normalize_money(estimated_usd),
        tokens_in: cost_i64(cost, "tokens_in"),
        tokens_out: cost_i64(cost, "tokens_out"),
    })
}

fn tenant_filter_matches(
    workflow: &Workflow,
    organization_id: Option<&str>,
    brand_id: Option<&str>,
    product_id: Option<&str>,
) -> bool {
    let context = &workflow.intent.operating_context;
    filter_eq(organization_id, &context.organization.id)
        && filter_eq(brand_id, &context.brand.id)
        && filter_eq(product_id, &context.product.id)
}

fn source_addon_from_task(task: &AtomicTask) -> Option<String> {
    task.context_requirements
        .iter()
        .find_map(|requirement| requirement.strip_prefix("source Addon "))
        .map(|addon| addon.trim().to_string())
        .filter(|addon| !addon.is_empty())
}

fn event_task_id(event: &StoreEvent) -> Option<String> {
    event
        .data
        .get("task_id")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

fn cost_i64(cost: &serde_json::Value, key: &str) -> i64 {
    cost.get(key).and_then(|value| value.as_i64()).unwrap_or(0)
}

fn filter_eq(filter: Option<&str>, value: &str) -> bool {
    filter
        .map(|filter| filter.trim())
        .filter(|filter| !filter.is_empty())
        .map(|filter| filter == value)
        .unwrap_or(true)
}

fn normalize_filter(value: Option<&str>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalize_money(value: f64) -> f64 {
    if !value.is_finite() || value <= 0.0 {
        0.0
    } else {
        (value * 1_000_000.0).round() / 1_000_000.0
    }
}

fn unique_count(values: &[String]) -> usize {
    values
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len()
}

fn executor_label(executor: &ExecutorKind) -> &'static str {
    match executor {
        ExecutorKind::Ai => "ai",
        ExecutorKind::Command => "command",
        ExecutorKind::Wait => "wait",
        ExecutorKind::Notification => "notification",
        ExecutorKind::Mixed => "mixed",
    }
}
