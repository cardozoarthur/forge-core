use crate::graph::{AtomicTask, ExecutorKind, Workflow};
use crate::storage::{CostLedgerIndexWrite, ForgeStore, StoreEvent, StoredCostLedgerIndexRecord};
use anyhow::{bail, Context, Result};
use chrono::{
    DateTime, Datelike, Duration as ChronoDuration, NaiveDate, NaiveDateTime, Timelike, Utc,
};
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeMap;

pub const COST_LEDGER_SCHEMA_VERSION: &str = "forge.cost_ledger.v1";
pub const COST_LEDGER_INDEX_SCHEMA_VERSION: &str = "forge.cost_ledger_index.v1";
pub const COST_LEDGER_HISTORY_SCHEMA_VERSION: &str = "forge.cost_ledger_history.v1";
pub const COST_LEDGER_MAINTENANCE_SCHEMA_VERSION: &str = "forge.cost_ledger_maintenance.v1";
pub const COST_LEDGER_DAEMON_SCHEMA_VERSION: &str = "forge.cost_ledger_daemon.v1";

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

#[derive(Debug, Clone, Serialize)]
pub struct CostLedgerIndexReport {
    pub schema_version: String,
    pub status: String,
    pub filters: CostLedgerIndexFilters,
    pub summary: CostLedgerIndexSummary,
    pub materialized_row_count: usize,
    pub row_count: usize,
    pub rows: Vec<CostLedgerIndexRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CostLedgerIndexFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brand_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addon_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CostLedgerIndexSummary {
    pub total_row_count: usize,
    pub planned_task_row_count: usize,
    pub observed_event_row_count: usize,
    pub workflow_count: usize,
    pub estimated_task_cost_total_usd: f64,
    pub observed_event_cost_total_usd: f64,
    pub observed_tokens_in_total: i64,
    pub observed_tokens_out_total: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CostLedgerIndexRow {
    pub row_key: String,
    pub source_kind: String,
    pub workflow_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<i64>,
    pub organization_id: String,
    pub brand_id: String,
    pub product_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addon_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executor: Option<String>,
    pub model_call_required: bool,
    pub model_call_avoided: bool,
    pub estimated_task_cost_usd: f64,
    pub observed_event_cost_usd: f64,
    pub tokens_in: i64,
    pub tokens_out: i64,
    pub data: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CostLedgerHistoryReport {
    pub schema_version: String,
    pub status: String,
    pub index_source: String,
    pub filters: CostLedgerHistoryFilters,
    pub summary: CostLedgerIndexSummary,
    pub bucket_count: usize,
    pub buckets: Vec<CostLedgerHistoryBucket>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CostLedgerHistoryFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brand_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addon_id: Option<String>,
    pub bucket: String,
    pub group_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CostLedgerHistoryBucket {
    pub bucket: String,
    pub bucket_start: String,
    pub bucket_end: String,
    pub group_by: String,
    pub group_id: String,
    pub summary: CostLedgerIndexSummary,
    pub first_row_key: String,
    pub last_row_key: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CostLedgerMaintenanceReport {
    pub schema_version: String,
    pub status: String,
    pub filters: CostLedgerMaintenanceFilters,
    pub retention: CostLedgerRetentionPlan,
    pub materialized_row_count: usize,
    pub indexed_row_count: usize,
    pub history_bucket_count: usize,
    pub summary: CostLedgerIndexSummary,
    pub materialization: CostLedgerIndexReport,
    pub history: CostLedgerHistoryReport,
    pub maintenance_policy: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CostLedgerMaintenanceFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brand_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub addon_id: Option<String>,
    pub bucket: String,
    pub group_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CostLedgerRetentionPlan {
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retention_days: Option<i64>,
    pub action: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CostLedgerDaemonReport {
    pub schema_version: String,
    pub status: String,
    pub origin: String,
    pub filters: CostLedgerMaintenanceFilters,
    pub max_cycles: usize,
    pub interval_seconds: u64,
    pub idle_exit: bool,
    pub cycle_count: usize,
    pub total_materialized_row_count: usize,
    pub total_indexed_row_count: usize,
    pub total_history_bucket_count: usize,
    pub stop_reason: String,
    pub summary: CostLedgerIndexSummary,
    pub cycles: Vec<CostLedgerDaemonCycle>,
    pub daemon_policy: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CostLedgerDaemonCycle {
    pub cycle: usize,
    pub status: String,
    pub origin: String,
    pub global_event_id: i64,
    pub slept_after_seconds: u64,
    pub maintenance: CostLedgerMaintenanceReport,
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

pub fn materialize_cost_ledger_index(
    store: &ForgeStore,
    workflow_id: Option<&str>,
    organization_id: Option<&str>,
    brand_id: Option<&str>,
    product_id: Option<&str>,
    source_kind: Option<&str>,
    addon_id: Option<&str>,
    limit: Option<usize>,
) -> Result<CostLedgerIndexReport> {
    let ledger = build_cost_ledger(store, workflow_id, organization_id, brand_id, product_id)?;
    let workflow_ids = ledger
        .workflows
        .iter()
        .map(|workflow| workflow.workflow_id.clone())
        .collect::<Vec<_>>();
    let writes = cost_ledger_index_writes(&ledger);
    let materialized_row_count = store.replace_cost_ledger_index_records(&workflow_ids, &writes)?;
    let records = store.load_cost_ledger_index(
        workflow_id,
        organization_id,
        brand_id,
        product_id,
        source_kind,
        addon_id,
        limit,
    )?;
    Ok(cost_ledger_index_report_from_records(
        "cost_ledger_index_materialized",
        workflow_id,
        organization_id,
        brand_id,
        product_id,
        source_kind,
        addon_id,
        limit,
        materialized_row_count,
        records,
    ))
}

pub fn build_cost_ledger_history(
    store: &ForgeStore,
    workflow_id: Option<&str>,
    organization_id: Option<&str>,
    brand_id: Option<&str>,
    product_id: Option<&str>,
    source_kind: Option<&str>,
    addon_id: Option<&str>,
    bucket: Option<&str>,
    group_by: Option<&str>,
    limit: Option<usize>,
) -> Result<CostLedgerHistoryReport> {
    let bucket = normalize_cost_history_bucket(bucket)?;
    let group_by = normalize_cost_history_group_by(group_by)?;
    let mut rows = store
        .load_cost_ledger_index(
            workflow_id,
            organization_id,
            brand_id,
            product_id,
            source_kind,
            addon_id,
            None,
        )?
        .into_iter()
        .map(cost_ledger_index_row)
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        cost_ledger_row_timestamp(left)
            .cmp(&cost_ledger_row_timestamp(right))
            .then_with(|| left.row_key.cmp(&right.row_key))
    });
    let summary = summarize_cost_ledger_index_rows(&rows);
    let buckets = build_cost_ledger_history_buckets(&rows, &bucket, &group_by, limit)?;

    Ok(CostLedgerHistoryReport {
        schema_version: COST_LEDGER_HISTORY_SCHEMA_VERSION.to_string(),
        status: "cost_ledger_history_loaded".to_string(),
        index_source: "sqlite_materialized".to_string(),
        filters: CostLedgerHistoryFilters {
            workflow_id: normalize_filter(workflow_id),
            organization_id: normalize_filter(organization_id),
            brand_id: normalize_filter(brand_id),
            product_id: normalize_filter(product_id),
            source_kind: normalize_filter(source_kind),
            addon_id: normalize_filter(addon_id),
            bucket,
            group_by,
            limit: limit.filter(|limit| *limit > 0),
        },
        summary,
        bucket_count: buckets.len(),
        buckets,
    })
}

pub fn maintain_cost_ledger(
    store: &ForgeStore,
    workflow_id: Option<&str>,
    organization_id: Option<&str>,
    brand_id: Option<&str>,
    product_id: Option<&str>,
    source_kind: Option<&str>,
    addon_id: Option<&str>,
    bucket: Option<&str>,
    group_by: Option<&str>,
    limit: Option<usize>,
    retention_days: Option<i64>,
) -> Result<CostLedgerMaintenanceReport> {
    let bucket = normalize_cost_history_bucket(bucket)?;
    let group_by = normalize_cost_history_group_by(group_by)?;
    let materialization = materialize_cost_ledger_index(
        store,
        workflow_id,
        organization_id,
        brand_id,
        product_id,
        source_kind,
        addon_id,
        limit,
    )?;
    let history = build_cost_ledger_history(
        store,
        workflow_id,
        organization_id,
        brand_id,
        product_id,
        source_kind,
        addon_id,
        Some(&bucket),
        Some(&group_by),
        limit,
    )?;
    let retention = retention_days
        .filter(|days| *days > 0)
        .map(|days| CostLedgerRetentionPlan {
            mode: "plan_only".to_string(),
            retention_days: Some(days),
            action: "retention_not_applied".to_string(),
            reason: "physical cost retention requires a separate approval gate before deleting ledger rows".to_string(),
        })
        .unwrap_or_else(|| CostLedgerRetentionPlan {
            mode: "not_configured".to_string(),
            retention_days: None,
            action: "no_retention_requested".to_string(),
            reason: "maintenance materialized current cost rows and rollups without pruning history".to_string(),
        });
    Ok(CostLedgerMaintenanceReport {
        schema_version: COST_LEDGER_MAINTENANCE_SCHEMA_VERSION.to_string(),
        status: "cost_ledger_maintenance_completed".to_string(),
        filters: CostLedgerMaintenanceFilters {
            workflow_id: normalize_filter(workflow_id),
            organization_id: normalize_filter(organization_id),
            brand_id: normalize_filter(brand_id),
            product_id: normalize_filter(product_id),
            source_kind: normalize_filter(source_kind),
            addon_id: normalize_filter(addon_id),
            bucket,
            group_by,
            limit: limit.filter(|limit| *limit > 0),
        },
        retention,
        materialized_row_count: materialization.materialized_row_count,
        indexed_row_count: materialization.summary.total_row_count,
        history_bucket_count: history.bucket_count,
        summary: history.summary.clone(),
        materialization,
        history,
        maintenance_policy: vec![
            "materialize planned and observed cost rows before reading history".to_string(),
            "derive hour/day rollups from the normalized SQLite index".to_string(),
            "keep the command idempotent so event runtime or schedule workers can run it periodically".to_string(),
            "treat retention as plan-only until an approval-gated delete/archive surface exists".to_string(),
        ],
    })
}

#[allow(clippy::too_many_arguments)]
pub fn run_cost_ledger_daemon(
    store: &ForgeStore,
    workflow_id: Option<&str>,
    organization_id: Option<&str>,
    brand_id: Option<&str>,
    product_id: Option<&str>,
    source_kind: Option<&str>,
    addon_id: Option<&str>,
    bucket: Option<&str>,
    group_by: Option<&str>,
    limit: Option<usize>,
    retention_days: Option<i64>,
    max_cycles: usize,
    interval_seconds: u64,
    idle_exit: bool,
    origin: &str,
) -> Result<CostLedgerDaemonReport> {
    let max_cycles = max_cycles.max(1);
    let bucket = normalize_cost_history_bucket(bucket)?;
    let group_by = normalize_cost_history_group_by(group_by)?;
    let tenant_context =
        cost_daemon_tenant_context(store, workflow_id, organization_id, brand_id, product_id)?;
    let mut cycles = Vec::new();
    let mut total_materialized_row_count = 0;
    let mut total_indexed_row_count = 0;
    let mut total_history_bucket_count = 0;
    let mut summary = CostLedgerIndexSummary::default();
    let mut stop_reason = "max_cycles_reached".to_string();

    for cycle in 1..=max_cycles {
        let maintenance = maintain_cost_ledger(
            store,
            workflow_id,
            organization_id,
            brand_id,
            product_id,
            source_kind,
            addon_id,
            Some(&bucket),
            Some(&group_by),
            limit,
            retention_days,
        )?;
        total_materialized_row_count += maintenance.materialized_row_count;
        total_indexed_row_count += maintenance.indexed_row_count;
        total_history_bucket_count += maintenance.history_bucket_count;
        summary = maintenance.summary.clone();
        let data = json!({
            "schema_version": COST_LEDGER_DAEMON_SCHEMA_VERSION,
            "cycle": cycle,
            "origin": origin,
            "status": "cost_ledger_daemon_cycle_completed",
            "workflow_id": normalize_filter(workflow_id),
            "filters": &maintenance.filters,
            "retention": &maintenance.retention,
            "materialized_row_count": maintenance.materialized_row_count,
            "indexed_row_count": maintenance.indexed_row_count,
            "history_bucket_count": maintenance.history_bucket_count,
            "summary": &maintenance.summary,
        });
        let source_id = format!(
            "cost-ledger-daemon-{cycle}-{}",
            Utc::now().timestamp_millis()
        );
        let global_event_id = store.record_global_event(
            "cost_ledger_daemon",
            &source_id,
            workflow_id,
            "cost_ledger_daemon_cycle",
            origin,
            "recorded",
            &data,
            &tenant_context,
        )?;
        let should_stop_for_idle = idle_exit && maintenance.indexed_row_count == 0;
        let slept_after_seconds = if cycle < max_cycles && !should_stop_for_idle {
            if interval_seconds > 0 {
                std::thread::sleep(std::time::Duration::from_secs(interval_seconds));
            }
            interval_seconds
        } else {
            0
        };
        cycles.push(CostLedgerDaemonCycle {
            cycle,
            status: "cost_ledger_daemon_cycle_completed".to_string(),
            origin: origin.to_string(),
            global_event_id,
            slept_after_seconds,
            maintenance,
        });
        if should_stop_for_idle {
            stop_reason = "idle_exit_no_cost_rows".to_string();
            break;
        }
    }

    Ok(CostLedgerDaemonReport {
        schema_version: COST_LEDGER_DAEMON_SCHEMA_VERSION.to_string(),
        status: "cost_ledger_daemon_completed".to_string(),
        origin: origin.to_string(),
        filters: CostLedgerMaintenanceFilters {
            workflow_id: normalize_filter(workflow_id),
            organization_id: normalize_filter(organization_id),
            brand_id: normalize_filter(brand_id),
            product_id: normalize_filter(product_id),
            source_kind: normalize_filter(source_kind),
            addon_id: normalize_filter(addon_id),
            bucket,
            group_by,
            limit: limit.filter(|limit| *limit > 0),
        },
        max_cycles,
        interval_seconds,
        idle_exit,
        cycle_count: cycles.len(),
        total_materialized_row_count,
        total_indexed_row_count,
        total_history_bucket_count,
        stop_reason,
        summary,
        cycles,
        daemon_policy: vec![
            "run cost ledger maintenance as a bounded dedicated loop".to_string(),
            "record every cycle in the global event timeline for Cost OS observability".to_string(),
            "keep physical retention delegated to the approval-gated maintenance policy"
                .to_string(),
            "use external runtime supervisors for unbounded service lifetime".to_string(),
        ],
    })
}

fn cost_ledger_index_writes(report: &CostLedgerReport) -> Vec<CostLedgerIndexWrite> {
    let mut writes = Vec::new();
    for workflow in &report.workflows {
        for node in &workflow.nodes {
            writes.push(CostLedgerIndexWrite {
                row_key: format!("planned:{}:{}", workflow.workflow_id, node.task_id),
                source_kind: "planned_task".to_string(),
                workflow_id: workflow.workflow_id.clone(),
                task_id: Some(node.task_id.clone()),
                event_id: None,
                organization_id: workflow.organization_id.clone(),
                brand_id: workflow.brand_id.clone(),
                product_id: workflow.product_id.clone(),
                addon_id: node.addon_id.clone(),
                executor: Some(node.executor.clone()),
                model_call_required: node.model_call_required,
                model_call_avoided: node.model_call_avoided,
                estimated_task_cost_usd: node.estimated_task_cost_usd,
                observed_event_cost_usd: 0.0,
                tokens_in: 0,
                tokens_out: 0,
                data: json!({
                    "title": node.title,
                    "status": node.status,
                    "execution_policy_mode": node.execution_policy_mode,
                    "cost_model": node.cost_model,
                }),
            });
        }
        for event in &workflow.observed_events {
            let node = event
                .task_id
                .as_deref()
                .and_then(|task_id| workflow.nodes.iter().find(|node| node.task_id == task_id));
            writes.push(CostLedgerIndexWrite {
                row_key: format!("observed:{}:{}", workflow.workflow_id, event.event_id),
                source_kind: "observed_event".to_string(),
                workflow_id: workflow.workflow_id.clone(),
                task_id: event.task_id.clone(),
                event_id: Some(event.event_id),
                organization_id: workflow.organization_id.clone(),
                brand_id: workflow.brand_id.clone(),
                product_id: workflow.product_id.clone(),
                addon_id: node.and_then(|node| node.addon_id.clone()),
                executor: node.map(|node| node.executor.clone()),
                model_call_required: node
                    .map(|node| node.model_call_required)
                    .unwrap_or_default(),
                model_call_avoided: node.map(|node| node.model_call_avoided).unwrap_or_default(),
                estimated_task_cost_usd: 0.0,
                observed_event_cost_usd: event.estimated_usd,
                tokens_in: event.tokens_in,
                tokens_out: event.tokens_out,
                data: json!({
                    "event_kind": event.event_kind,
                    "event_created_at": event.created_at,
                }),
            });
        }
    }
    writes
}

fn cost_ledger_index_report_from_records(
    status: &str,
    workflow_id: Option<&str>,
    organization_id: Option<&str>,
    brand_id: Option<&str>,
    product_id: Option<&str>,
    source_kind: Option<&str>,
    addon_id: Option<&str>,
    limit: Option<usize>,
    materialized_row_count: usize,
    records: Vec<StoredCostLedgerIndexRecord>,
) -> CostLedgerIndexReport {
    let rows = records
        .into_iter()
        .map(cost_ledger_index_row)
        .collect::<Vec<_>>();
    let summary = summarize_cost_ledger_index_rows(&rows);
    CostLedgerIndexReport {
        schema_version: COST_LEDGER_INDEX_SCHEMA_VERSION.to_string(),
        status: status.to_string(),
        filters: CostLedgerIndexFilters {
            workflow_id: normalize_filter(workflow_id),
            organization_id: normalize_filter(organization_id),
            brand_id: normalize_filter(brand_id),
            product_id: normalize_filter(product_id),
            source_kind: normalize_filter(source_kind),
            addon_id: normalize_filter(addon_id),
            limit: limit.filter(|limit| *limit > 0),
        },
        summary,
        materialized_row_count,
        row_count: rows.len(),
        rows,
    }
}

fn cost_ledger_index_row(record: StoredCostLedgerIndexRecord) -> CostLedgerIndexRow {
    CostLedgerIndexRow {
        row_key: record.row_key,
        source_kind: record.source_kind,
        workflow_id: record.workflow_id,
        task_id: record.task_id,
        event_id: record.event_id,
        organization_id: record.organization_id,
        brand_id: record.brand_id,
        product_id: record.product_id,
        addon_id: record.addon_id,
        executor: record.executor,
        model_call_required: record.model_call_required,
        model_call_avoided: record.model_call_avoided,
        estimated_task_cost_usd: normalize_money(record.estimated_task_cost_usd),
        observed_event_cost_usd: normalize_money(record.observed_event_cost_usd),
        tokens_in: record.tokens_in,
        tokens_out: record.tokens_out,
        data: record.data,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

fn summarize_cost_ledger_index_rows(rows: &[CostLedgerIndexRow]) -> CostLedgerIndexSummary {
    let mut summary = CostLedgerIndexSummary {
        total_row_count: rows.len(),
        ..CostLedgerIndexSummary::default()
    };
    let mut workflow_ids = Vec::new();
    for row in rows {
        if !workflow_ids.contains(&row.workflow_id) {
            workflow_ids.push(row.workflow_id.clone());
        }
        match row.source_kind.as_str() {
            "planned_task" => summary.planned_task_row_count += 1,
            "observed_event" => summary.observed_event_row_count += 1,
            _ => {}
        }
        summary.estimated_task_cost_total_usd += row.estimated_task_cost_usd;
        summary.observed_event_cost_total_usd += row.observed_event_cost_usd;
        summary.observed_tokens_in_total += row.tokens_in;
        summary.observed_tokens_out_total += row.tokens_out;
    }
    summary.workflow_count = workflow_ids.len();
    summary.estimated_task_cost_total_usd = normalize_money(summary.estimated_task_cost_total_usd);
    summary.observed_event_cost_total_usd = normalize_money(summary.observed_event_cost_total_usd);
    summary
}

fn build_cost_ledger_history_buckets(
    rows: &[CostLedgerIndexRow],
    bucket: &str,
    group_by: &str,
    limit: Option<usize>,
) -> Result<Vec<CostLedgerHistoryBucket>> {
    let mut buckets: BTreeMap<String, CostLedgerHistoryAccumulator> = BTreeMap::new();
    for row in rows {
        let occurred_at = parse_cost_history_time(&cost_ledger_row_timestamp(row))?;
        let (bucket_start, bucket_end) = cost_history_bucket_bounds(occurred_at, bucket)?;
        let group_id = cost_history_group_id(row, group_by);
        let key = format!("{bucket_start}|{group_by}|{group_id}");
        let accumulator = buckets
            .entry(key)
            .or_insert_with(|| CostLedgerHistoryAccumulator {
                bucket: bucket.to_string(),
                bucket_start: bucket_start.clone(),
                bucket_end: bucket_end.clone(),
                group_by: group_by.to_string(),
                group_id: group_id.clone(),
                rows: Vec::new(),
            });
        accumulator.rows.push(row.clone());
    }

    let mut history = buckets
        .into_values()
        .map(|mut bucket| {
            bucket.rows.sort_by(|left, right| {
                cost_ledger_row_timestamp(left)
                    .cmp(&cost_ledger_row_timestamp(right))
                    .then_with(|| left.row_key.cmp(&right.row_key))
            });
            let first_row_key = bucket
                .rows
                .first()
                .map(|row| row.row_key.clone())
                .unwrap_or_default();
            let last_row_key = bucket
                .rows
                .last()
                .map(|row| row.row_key.clone())
                .unwrap_or_default();
            CostLedgerHistoryBucket {
                bucket: bucket.bucket,
                bucket_start: bucket.bucket_start,
                bucket_end: bucket.bucket_end,
                group_by: bucket.group_by,
                group_id: bucket.group_id,
                summary: summarize_cost_ledger_index_rows(&bucket.rows),
                first_row_key,
                last_row_key,
            }
        })
        .collect::<Vec<_>>();
    history.sort_by(|left, right| {
        left.bucket_start
            .cmp(&right.bucket_start)
            .then_with(|| left.group_by.cmp(&right.group_by))
            .then_with(|| left.group_id.cmp(&right.group_id))
    });
    if let Some(limit) = limit.filter(|limit| *limit > 0) {
        let start = history.len().saturating_sub(limit);
        history = history.into_iter().skip(start).collect();
    }
    Ok(history)
}

struct CostLedgerHistoryAccumulator {
    bucket: String,
    bucket_start: String,
    bucket_end: String,
    group_by: String,
    group_id: String,
    rows: Vec<CostLedgerIndexRow>,
}

fn normalize_cost_history_bucket(bucket: Option<&str>) -> Result<String> {
    let bucket = normalize_filter(bucket).unwrap_or_else(|| "day".to_string());
    match bucket.as_str() {
        "hour" | "day" => Ok(bucket),
        _ => bail!("cost history bucket must be `hour` or `day`"),
    }
}

fn normalize_cost_history_group_by(group_by: Option<&str>) -> Result<String> {
    let group_by = normalize_filter(group_by).unwrap_or_else(|| "none".to_string());
    match group_by.as_str() {
        "none" | "tenant" | "workflow" | "source_kind" | "addon" | "executor" => Ok(group_by),
        _ => bail!(
            "cost history group_by must be one of `none`, `tenant`, `workflow`, `source_kind`, `addon` or `executor`"
        ),
    }
}

fn cost_ledger_row_timestamp(row: &CostLedgerIndexRow) -> String {
    row.data
        .get("event_created_at")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| row.updated_at.clone())
}

fn parse_cost_history_time(value: &str) -> Result<DateTime<Utc>> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
        return Ok(parsed.with_timezone(&Utc));
    }
    let naive = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").with_context(|| {
        format!("cost ledger timestamp `{value}` must be RFC3339 or SQLite UTC")
    })?;
    Ok(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
}

fn cost_history_bucket_bounds(
    occurred_at: DateTime<Utc>,
    bucket: &str,
) -> Result<(String, String)> {
    let date = NaiveDate::from_ymd_opt(occurred_at.year(), occurred_at.month(), occurred_at.day())
        .with_context(|| "failed to derive cost bucket date")?;
    let start_naive = match bucket {
        "hour" => date
            .and_hms_opt(occurred_at.hour(), 0, 0)
            .with_context(|| "failed to derive hourly cost bucket")?,
        "day" => date
            .and_hms_opt(0, 0, 0)
            .with_context(|| "failed to derive daily cost bucket")?,
        _ => bail!("cost history bucket must be `hour` or `day`"),
    };
    let start = DateTime::<Utc>::from_naive_utc_and_offset(start_naive, Utc);
    let end = match bucket {
        "hour" => start + ChronoDuration::hours(1),
        "day" => start + ChronoDuration::days(1),
        _ => bail!("cost history bucket must be `hour` or `day`"),
    };
    Ok((start.to_rfc3339(), end.to_rfc3339()))
}

fn cost_history_group_id(row: &CostLedgerIndexRow, group_by: &str) -> String {
    match group_by {
        "tenant" => format!(
            "{}|{}|{}",
            row.organization_id, row.brand_id, row.product_id
        ),
        "workflow" => row.workflow_id.clone(),
        "source_kind" => row.source_kind.clone(),
        "addon" => row
            .addon_id
            .clone()
            .unwrap_or_else(|| "_no_addon".to_string()),
        "executor" => row
            .executor
            .clone()
            .unwrap_or_else(|| "_no_executor".to_string()),
        _ => "_all".to_string(),
    }
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

fn cost_daemon_tenant_context(
    store: &ForgeStore,
    workflow_id: Option<&str>,
    organization_id: Option<&str>,
    brand_id: Option<&str>,
    product_id: Option<&str>,
) -> Result<serde_json::Value> {
    if let Some(workflow_id) = workflow_id.and_then(|value| normalize_filter(Some(value))) {
        let workflow = store.load_workflow(&workflow_id)?;
        return serde_json::to_value(&workflow.intent.operating_context)
            .context("failed to serialize workflow operating context for cost daemon");
    }
    Ok(json!({
        "schema_version": "forge.operating_context.v1",
        "organization": cost_daemon_identity(organization_id, "default-org"),
        "brand": cost_daemon_identity(brand_id, "default-brand"),
        "product": cost_daemon_identity(product_id, "default-product"),
        "user": cost_daemon_identity(None, "forge-cost-daemon"),
        "channel": cost_daemon_identity(None, "system"),
        "memory_scope": "project",
        "personality_scope": "default",
        "tenant_policy_mode": "audit"
    }))
}

fn cost_daemon_identity(value: Option<&str>, fallback: &str) -> serde_json::Value {
    let id = normalize_filter(value).unwrap_or_else(|| fallback.to_string());
    json!({ "id": id })
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
