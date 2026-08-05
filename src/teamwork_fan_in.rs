use crate::artifact::hex_sha256;
use crate::graph::TaskStatus;
use crate::storage::FoundryStore;
use crate::worktree::{
    bind_worktree, bound_worktree_mutation_claim, inspect_registered_worktree,
    list_registered_worktrees, WorktreeBinding, WorktreeRecord,
};
use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output, Stdio};

pub const TEAMWORK_GIT_FAN_IN_VALIDATION_RULE: &str = "git_dependency_fan_in";
pub const TEAMWORK_FAN_IN_SCHEMA_VERSION: &str =
    "foundry.worktree.dependency_integration_receipt.v1";
pub const TEAMWORK_FAN_IN_STATUS_SCHEMA_VERSION: &str =
    "foundry.worktree.dependency_integration_status.v1";

const INTEGRATED_EVENT_KIND: &str = "worktree_dependencies_integrated";
const CONFLICT_EVENT_KIND: &str = "worktree_dependency_integration_conflict";
const MAX_GIT_MESSAGE_BYTES: usize = 4_096;

#[derive(Debug, Clone)]
pub struct IntegrateDependenciesOptions<'a> {
    pub workflow_id: &'a str,
    pub task_id: &'a str,
    pub allow_repository_mutation: bool,
    pub approved_by: &'a str,
    pub reason: &'a str,
    pub origin: &'a str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamworkFanInAuthorizationReceipt {
    pub repository_mutation_allowed: bool,
    pub approved_by: String,
    pub reason: String,
    pub origin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamworkFanInWorktreeRef {
    pub task_id: String,
    pub worktree_id: String,
    pub worktree_root: String,
    pub repository_root: String,
    pub branch: String,
    pub binding_head: String,
    #[serde(default)]
    pub binding_workflow_revision: u64,
    #[serde(default)]
    pub worktree_identity_sha256: String,
    #[serde(default)]
    pub binding_config_sha256: String,
    pub head: String,
    #[serde(alias = "created_by_forge")] // foundry-brand-allow: legacy-compat
    pub created_by_foundry: bool,
    pub clean: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamworkFanInSourceRef {
    #[serde(flatten)]
    pub worktree: TeamworkFanInWorktreeRef,
    pub already_ancestor: bool,
    pub changed_paths: Vec<String>,
    pub changed_paths_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamworkFanInAtomicityReceipt {
    pub mode: String,
    pub temporary_worktree_used: bool,
    pub destination_pre_head: String,
    pub destination_post_head: String,
    pub rollback_required: bool,
    pub rollback_verified: bool,
    pub limitation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamworkFanInReport {
    pub schema_version: String,
    pub validation_rule_kind: String,
    pub status: String,
    pub success: bool,
    pub dry_run: bool,
    pub replay: bool,
    pub workflow_id: String,
    pub task_id: String,
    pub ordered_source_task_ids: Vec<String>,
    pub destination: TeamworkFanInWorktreeRef,
    pub sources: Vec<TeamworkFanInSourceRef>,
    pub common_base_head: String,
    pub pre_head: String,
    pub result_head: String,
    pub commit_created: bool,
    pub repository_mutation_attempted: bool,
    pub repository_mutated: bool,
    #[serde(default)]
    pub destination_rebound: bool,
    pub integrated_paths: Vec<String>,
    pub conflict_paths: Vec<String>,
    pub plan_sha256: String,
    pub receipt_sha256: String,
    pub authorization: TeamworkFanInAuthorizationReceipt,
    pub atomicity: TeamworkFanInAtomicityReceipt,
    pub reason: String,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CurrentTeamworkFanInStatus {
    pub schema_version: String,
    pub workflow_id: String,
    pub task_id: String,
    pub status: String,
    pub current: bool,
    pub receipt_successful: bool,
    pub receipt_sha256: Option<String>,
    pub latest_event_id: Option<i64>,
    pub destination_worktree_id: Option<String>,
    pub expected_result_head: Option<String>,
    pub current_destination_head: Option<String>,
    pub destination_clean: Option<bool>,
    pub destination_binding_current: bool,
    pub source_heads_current: bool,
    pub reason: String,
}

#[derive(Debug, Clone)]
struct PreparedTaskWorktree {
    record: WorktreeRecord,
    binding: WorktreeBinding,
}

#[derive(Debug, Clone)]
struct PreparedFanIn {
    workflow_id: String,
    task_id: String,
    destination: PreparedTaskWorktree,
    sources: Vec<PreparedTaskWorktree>,
    source_refs: Vec<TeamworkFanInSourceRef>,
    common_base_head: String,
    pre_head: String,
    plan_sha256: String,
}

struct DestinationFanInLock {
    _file: File,
}

struct DestinationRollbackGuard {
    destination: PathBuf,
    pre_head: String,
    armed: bool,
}

impl DestinationRollbackGuard {
    fn new(destination: &Path, pre_head: &str) -> Self {
        Self {
            destination: destination.to_path_buf(),
            pre_head: pre_head.to_string(),
            armed: true,
        }
    }

    fn rollback(&mut self) -> Result<bool> {
        let restored = restore_destination(&self.destination, &self.pre_head)?;
        self.armed = false;
        Ok(restored)
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for DestinationRollbackGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = restore_destination(&self.destination, &self.pre_head);
        }
    }
}

enum FanInGitOutcome {
    Conflict {
        conflict_paths: Vec<String>,
        reason: String,
    },
    Integrated {
        result_head: String,
        integrated_paths: Vec<String>,
    },
}

pub fn integrate_worktree_dependencies(
    store: &FoundryStore,
    options: &IntegrateDependenciesOptions<'_>,
) -> Result<TeamworkFanInReport> {
    let mut prepared = prepare_fan_in(store, options.workflow_id, options.task_id)?;
    let authorization = authorization_receipt(options);
    if !options.allow_repository_mutation {
        return seal_report(planned_report(prepared, authorization));
    }
    validate_apply_authorization(options)?;

    let destination_root = PathBuf::from(&prepared.destination.record.worktree_root);
    let _destination_lock = acquire_destination_fan_in_lock(&destination_root)?;
    revalidate_prepared_state(store, &prepared)?;

    if prepared
        .source_refs
        .iter()
        .all(|source| source.already_ancestor)
    {
        return persist_replay_report(store, &mut prepared, authorization, options);
    }

    ensure_safe_integration_attributes(&destination_root, &prepared.source_refs)?;
    let mut rollback_guard = DestinationRollbackGuard::new(&destination_root, &prepared.pre_head);
    let git_outcome = match perform_git_integration(store, &prepared, &destination_root) {
        Ok(outcome) => outcome,
        Err(error) => {
            return Err(error_after_rollback(&mut rollback_guard, error));
        }
    };
    match git_outcome {
        FanInGitOutcome::Conflict {
            conflict_paths,
            reason,
        } => {
            let rollback_verified = rollback_guard.rollback()?;
            revalidate_sources_unchanged(store, &prepared.sources)?;
            let report = seal_report(conflict_report(
                prepared,
                authorization,
                conflict_paths,
                rollback_verified,
                reason,
            ))?;
            store.record_event(
                options.workflow_id,
                CONFLICT_EVENT_KIND,
                &serde_json::to_value(&report)?,
            )?;
            Ok(report)
        }
        FanInGitOutcome::Integrated {
            result_head,
            integrated_paths,
        } => {
            let pre_head = prepared.pre_head.clone();
            match persist_success_report(
                store,
                &mut prepared,
                authorization,
                options,
                result_head,
                integrated_paths,
            ) {
                Ok(report) => {
                    rollback_guard.disarm();
                    Ok(report)
                }
                Err(error) => {
                    debug_assert_eq!(rollback_guard.pre_head, pre_head);
                    Err(error_after_rollback(&mut rollback_guard, error))
                }
            }
        }
    }
}

fn acquire_destination_fan_in_lock(destination: &Path) -> Result<DestinationFanInLock> {
    let git_dir = git_text(destination, &["rev-parse", "--absolute-git-dir"])?;
    let git_dir = canonical_path(Path::new(&git_dir))?;
    let lock_path = git_dir.join("foundry-dependency-fan-in.lock");
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .with_context(|| {
            format!(
                "failed to open dependency fan-in lock {}",
                lock_path.display()
            )
        })?;
    match file.try_lock() {
        Ok(()) => Ok(DestinationFanInLock { _file: file }),
        Err(TryLockError::WouldBlock) => bail!(
            "dependency fan-in is already running for destination {}",
            destination.display()
        ),
        Err(TryLockError::Error(error)) => Err(error).with_context(|| {
            format!(
                "failed to acquire dependency fan-in lock {}",
                lock_path.display()
            )
        }),
    }
}

fn perform_git_integration(
    store: &FoundryStore,
    prepared: &PreparedFanIn,
    destination_root: &Path,
) -> Result<FanInGitOutcome> {
    let missing_heads = prepared
        .source_refs
        .iter()
        .filter(|source| !source.already_ancestor)
        .map(|source| source.worktree.head.clone())
        .collect::<Vec<_>>();
    let merge_output = run_no_commit_merge(destination_root, &missing_heads)?;
    if !merge_output.status.success() {
        return Ok(FanInGitOutcome::Conflict {
            conflict_paths: unmerged_paths(destination_root)?,
            reason: bounded_git_message(&merge_output),
        });
    }

    if git_head(destination_root)? != prepared.pre_head {
        bail!(
            "dependency merge moved destination HEAD before the Foundry integration commit; expected {}",
            prepared.pre_head
        );
    }
    revalidate_sources_unchanged(store, &prepared.sources)?;

    let commit_message = integration_commit_message(prepared);
    let commit_output = run_foundry_commit(destination_root, &commit_message)?;
    if !commit_output.status.success() {
        bail!(
            "failed to create Foundry dependency integration commit: {}",
            bounded_git_message(&commit_output)
        );
    }

    let result_head = git_head(destination_root)?;
    let destination_clean = git_is_clean(destination_root)?;
    let all_sources_integrated = prepared
        .source_refs
        .iter()
        .map(|source| git_is_ancestor(destination_root, &source.worktree.head, &result_head))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .all(|integrated| integrated);
    if result_head == prepared.pre_head || !destination_clean || !all_sources_integrated {
        bail!("Foundry integration commit failed post-commit verification");
    }
    revalidate_worktree_unchanged(store, &prepared.destination, &result_head)?;
    revalidate_sources_unchanged(store, &prepared.sources)?;
    let integrated_paths = changed_paths(destination_root, &prepared.pre_head, &result_head)?;
    Ok(FanInGitOutcome::Integrated {
        result_head,
        integrated_paths,
    })
}

fn persist_replay_report(
    store: &FoundryStore,
    prepared: &mut PreparedFanIn,
    authorization: TeamworkFanInAuthorizationReceipt,
    options: &IntegrateDependenciesOptions<'_>,
) -> Result<TeamworkFanInReport> {
    store.with_transaction(|| {
        revalidate_prepared_state(store, prepared)?;
        let destination_rebound =
            if destination_binding_freezes_head(&prepared.destination, &prepared.pre_head) {
                false
            } else {
                rebind_destination(store, prepared, options, &prepared.pre_head.clone())?;
                true
            };
        revalidate_prepared_state(store, prepared)?;
        let report = seal_report(replay_report(prepared, authorization, destination_rebound))?;
        store.record_event(
            options.workflow_id,
            INTEGRATED_EVENT_KIND,
            &serde_json::to_value(&report)?,
        )?;
        Ok(report)
    })
}

fn persist_success_report(
    store: &FoundryStore,
    prepared: &mut PreparedFanIn,
    authorization: TeamworkFanInAuthorizationReceipt,
    options: &IntegrateDependenciesOptions<'_>,
    result_head: String,
    integrated_paths: Vec<String>,
) -> Result<TeamworkFanInReport> {
    store.with_transaction(|| {
        revalidate_worktree_unchanged(store, &prepared.destination, &result_head)?;
        revalidate_sources_unchanged(store, &prepared.sources)?;
        rebind_destination(store, prepared, options, &result_head)?;
        revalidate_worktree_unchanged(store, &prepared.destination, &result_head)?;
        revalidate_sources_unchanged(store, &prepared.sources)?;
        let report = seal_report(success_report(
            prepared,
            authorization,
            result_head,
            integrated_paths,
        ))?;
        store.record_event(
            options.workflow_id,
            INTEGRATED_EVENT_KIND,
            &serde_json::to_value(&report)?,
        )?;
        Ok(report)
    })
}

fn rebind_destination(
    store: &FoundryStore,
    prepared: &mut PreparedFanIn,
    options: &IntegrateDependenciesOptions<'_>,
    expected_head: &str,
) -> Result<()> {
    let rebind = bind_worktree(
        store,
        &prepared.destination.record.id,
        options.workflow_id,
        Some(options.task_id),
        options.origin,
    )?;
    let rebound_binding = rebind
        .binding
        .with_context(|| "destination rebind returned no task-scoped binding")?;
    if rebind.worktree.id != prepared.destination.record.id
        || rebind.worktree.head != expected_head
        || rebound_binding.head_at_binding != expected_head
        || rebound_binding.workflow_id != options.workflow_id
        || rebound_binding.task_id.as_deref() != Some(options.task_id)
    {
        bail!(
            "destination rebind did not freeze the verified integration HEAD {}; refusing to record a successful fan-in receipt",
            expected_head
        );
    }
    prepared.destination = PreparedTaskWorktree {
        record: rebind.worktree,
        binding: rebound_binding,
    };
    Ok(())
}

fn destination_binding_freezes_head(destination: &PreparedTaskWorktree, head: &str) -> bool {
    destination.binding.head_at_binding == head
        && destination.binding.worktree_identity_sha256 == destination.record.identity_sha256
        && destination.binding.config_sha256_at_binding == destination.record.config.sha256
}

fn error_after_rollback(
    rollback_guard: &mut DestinationRollbackGuard,
    error: anyhow::Error,
) -> anyhow::Error {
    let pre_head = rollback_guard.pre_head.clone();
    match rollback_guard.rollback() {
        Ok(true) => error.context(format!(
            "dependency fan-in failed; destination was restored to clean HEAD {pre_head}"
        )),
        Ok(false) => error.context(format!(
            "dependency fan-in failed and destination rollback did not verify HEAD {pre_head}"
        )),
        Err(rollback_error) => anyhow!(
            "dependency fan-in failed: {error:#}; destination rollback to {pre_head} also failed: {rollback_error:#}"
        ),
    }
}

pub fn current_teamwork_fan_in_status(
    store: &FoundryStore,
    workflow_id: &str,
    task_id: &str,
) -> Result<CurrentTeamworkFanInStatus> {
    let mut latest = None;
    for event in store.load_workflow_events(workflow_id)?.into_iter().rev() {
        if !matches!(
            event.kind.as_str(),
            INTEGRATED_EVENT_KIND | CONFLICT_EVENT_KIND
        ) {
            continue;
        }
        let report = match serde_json::from_value::<TeamworkFanInReport>(event.data.clone()) {
            Ok(report) => report,
            Err(error) => {
                return Ok(invalid_fan_in_status(
                    workflow_id,
                    task_id,
                    event.id,
                    None,
                    format!("fan-in event {} is structurally invalid: {error}", event.id),
                ));
            }
        };
        if let Err(error) =
            validate_recorded_fan_in_report(&report, &event.data, &event.kind, workflow_id)
        {
            return Ok(invalid_fan_in_status(
                workflow_id,
                task_id,
                event.id,
                Some(&report),
                format!("fan-in event {} failed closed: {error:#}", event.id),
            ));
        }
        if report.task_id == task_id {
            latest = Some((event.id, report));
            break;
        }
    }
    let Some((event_id, report)) = latest else {
        return Ok(missing_fan_in_status(workflow_id, task_id));
    };

    let destination = inspect_registered_worktree(store, &report.destination.worktree_id)?;
    let destination_head_matches = destination.head == report.result_head;
    let destination_binding_current =
        recorded_worktree_ref_current(store, workflow_id, task_id, &report.destination);
    let sources_current = report.sources.iter().all(|source| {
        recorded_worktree_ref_current(
            store,
            workflow_id,
            &source.worktree.task_id,
            &source.worktree,
        )
    });
    let source_lineage_current = report.sources.iter().all(|source| {
        git_is_ancestor(
            Path::new(&destination.worktree_root),
            &source.worktree.head,
            &report.result_head,
        )
        .unwrap_or(false)
    });
    let receipt_successful = report.success && !report.dry_run;
    let current = receipt_successful
        && destination_head_matches
        && !destination.dirty
        && destination_binding_current
        && sources_current
        && source_lineage_current;
    let reason = if !receipt_successful {
        format!("latest fan-in receipt has status {}", report.status)
    } else if !destination_head_matches {
        format!(
            "destination HEAD drifted: expected {}, current {}",
            report.result_head, destination.head
        )
    } else if destination.dirty {
        "destination worktree is dirty after fan-in".to_string()
    } else if !destination_binding_current {
        "destination task binding no longer freezes the recorded integration HEAD and workflow revision"
            .to_string()
    } else if !sources_current {
        "one or more dependency source bindings, identities, roots or heads drifted after fan-in"
            .to_string()
    } else if !source_lineage_current {
        "one or more recorded dependency HEADs are no longer ancestors of the destination result"
            .to_string()
    } else {
        "successful receipt, destination HEAD and source heads are current".to_string()
    };
    Ok(CurrentTeamworkFanInStatus {
        schema_version: TEAMWORK_FAN_IN_STATUS_SCHEMA_VERSION.to_string(),
        workflow_id: workflow_id.to_string(),
        task_id: task_id.to_string(),
        status: if current { "current" } else { "stale" }.to_string(),
        current,
        receipt_successful,
        receipt_sha256: Some(report.receipt_sha256),
        latest_event_id: Some(event_id),
        destination_worktree_id: Some(destination.id),
        expected_result_head: Some(report.result_head),
        current_destination_head: Some(destination.head),
        destination_clean: Some(!destination.dirty),
        destination_binding_current,
        source_heads_current: sources_current && source_lineage_current,
        reason,
    })
}

fn missing_fan_in_status(workflow_id: &str, task_id: &str) -> CurrentTeamworkFanInStatus {
    CurrentTeamworkFanInStatus {
        schema_version: TEAMWORK_FAN_IN_STATUS_SCHEMA_VERSION.to_string(),
        workflow_id: workflow_id.to_string(),
        task_id: task_id.to_string(),
        status: "missing".to_string(),
        current: false,
        receipt_successful: false,
        receipt_sha256: None,
        latest_event_id: None,
        destination_worktree_id: None,
        expected_result_head: None,
        current_destination_head: None,
        destination_clean: None,
        destination_binding_current: false,
        source_heads_current: false,
        reason: "no dependency fan-in receipt is recorded for this task".to_string(),
    }
}

fn invalid_fan_in_status(
    workflow_id: &str,
    task_id: &str,
    event_id: i64,
    report: Option<&TeamworkFanInReport>,
    reason: String,
) -> CurrentTeamworkFanInStatus {
    CurrentTeamworkFanInStatus {
        schema_version: TEAMWORK_FAN_IN_STATUS_SCHEMA_VERSION.to_string(),
        workflow_id: workflow_id.to_string(),
        task_id: task_id.to_string(),
        status: "invalid_receipt".to_string(),
        current: false,
        receipt_successful: false,
        receipt_sha256: report.map(|report| report.receipt_sha256.clone()),
        latest_event_id: Some(event_id),
        destination_worktree_id: report.map(|report| report.destination.worktree_id.clone()),
        expected_result_head: report.map(|report| report.result_head.clone()),
        current_destination_head: None,
        destination_clean: None,
        destination_binding_current: false,
        source_heads_current: false,
        reason,
    }
}

fn validate_recorded_fan_in_report(
    report: &TeamworkFanInReport,
    raw: &serde_json::Value,
    event_kind: &str,
    workflow_id: &str,
) -> Result<()> {
    if report.schema_version != TEAMWORK_FAN_IN_SCHEMA_VERSION
        || report.validation_rule_kind != TEAMWORK_GIT_FAN_IN_VALIDATION_RULE
        || report.workflow_id != workflow_id
        || report.task_id.trim().is_empty()
    {
        bail!("receipt schema, validation rule, workflow or task identity is invalid");
    }
    if serde_json::to_value(report)? != *raw {
        bail!("receipt payload contains unknown, omitted or non-canonical fields");
    }
    let expected_receipt_sha256 = receipt_sha256(report)?;
    if report.receipt_sha256 != expected_receipt_sha256 {
        bail!("receipt SHA-256 does not match its canonical payload");
    }
    let expected_plan_sha256 = hash_plan(
        &report.workflow_id,
        &report.task_id,
        &report.destination.worktree_id,
        &report.pre_head,
        &report.sources,
    )?;
    if report.plan_sha256 != expected_plan_sha256 {
        bail!("fan-in plan SHA-256 does not match the recorded source plan");
    }
    let ordered_source_task_ids = report
        .sources
        .iter()
        .map(|source| source.worktree.task_id.clone())
        .collect::<Vec<_>>();
    let mut sorted_source_task_ids = ordered_source_task_ids.clone();
    sorted_source_task_ids.sort();
    sorted_source_task_ids.dedup();
    if report.ordered_source_task_ids != ordered_source_task_ids
        || sorted_source_task_ids != ordered_source_task_ids
        || ordered_source_task_ids.is_empty()
    {
        bail!("receipt source task order is empty, duplicated or non-deterministic");
    }
    if report.sources.iter().any(|source| {
        source.changed_paths_sha256 != hash_string_list(&source.changed_paths)
            || !paths_are_sorted_unique(&source.changed_paths)
    }) {
        bail!("one or more source changed-path manifests are invalid");
    }
    if report.destination.task_id != report.task_id
        || report.destination.head != report.result_head
        || (event_kind == INTEGRATED_EVENT_KIND
            && report.destination.binding_head != report.result_head)
        || !report.destination.created_by_foundry
        || !report.destination.clean
        || report.sources.iter().any(|source| {
            source.worktree.task_id.is_empty()
                || !source.worktree.created_by_foundry
                || !source.worktree.clean
                || source.worktree.worktree_identity_sha256.is_empty()
                || source.worktree.binding_config_sha256.is_empty()
        })
        || report.destination.worktree_identity_sha256.is_empty()
        || report.destination.binding_config_sha256.is_empty()
    {
        bail!("receipt destination/source ownership or binding invariants are invalid");
    }
    if !report.authorization.repository_mutation_allowed
        || report.authorization.approved_by.trim().is_empty()
        || report.authorization.reason.trim().is_empty()
        || report.authorization.origin.trim().is_empty()
    {
        bail!("receipt repository mutation authorization is incomplete");
    }
    let semantic_status_valid = match (event_kind, report.status.as_str()) {
        (INTEGRATED_EVENT_KIND, "dependencies_integrated") => {
            report.success
                && !report.dry_run
                && !report.replay
                && report.commit_created
                && report.destination_rebound
                && report.repository_mutation_attempted
                && report.repository_mutated
                && report.result_head != report.pre_head
        }
        (INTEGRATED_EVENT_KIND, "already_integrated") => {
            report.success
                && !report.dry_run
                && report.replay
                && !report.commit_created
                && !report.repository_mutation_attempted
                && !report.repository_mutated
                && report.result_head == report.pre_head
        }
        (CONFLICT_EVENT_KIND, "integration_conflict") => {
            !report.success
                && !report.dry_run
                && !report.replay
                && !report.commit_created
                && !report.destination_rebound
                && report.repository_mutation_attempted
                && !report.repository_mutated
                && report.result_head == report.pre_head
                && report.atomicity.rollback_required
                && report.atomicity.rollback_verified
        }
        _ => false,
    };
    if !semantic_status_valid {
        bail!("receipt event kind, status and mutation flags are inconsistent");
    }
    Ok(())
}

fn paths_are_sorted_unique(paths: &[String]) -> bool {
    paths.windows(2).all(|pair| pair[0] < pair[1])
        && paths
            .iter()
            .all(|path| validate_git_relative_path(path).is_ok())
}

fn recorded_worktree_ref_current(
    store: &FoundryStore,
    workflow_id: &str,
    task_id: &str,
    expected: &TeamworkFanInWorktreeRef,
) -> bool {
    let owners = match task_binding_owner_ids(store, workflow_id, task_id) {
        Ok(owners) => owners,
        Err(_) => return false,
    };
    if owners.len() != 1 || owners[0] != expected.worktree_id {
        return false;
    }
    let claim = match bound_worktree_mutation_claim(store, workflow_id, task_id) {
        Ok(Some(claim)) => claim,
        _ => return false,
    };
    let record = match inspect_registered_worktree(store, &expected.worktree_id) {
        Ok(record) => record,
        Err(_) => return false,
    };
    let matching_bindings = record
        .bindings
        .iter()
        .filter(|binding| {
            binding.workflow_id == workflow_id
                && binding.task_id.as_deref() == Some(task_id)
                && binding.head_at_binding == expected.binding_head
                && binding.workflow_revision == expected.binding_workflow_revision
                && binding.worktree_identity_sha256 == expected.worktree_identity_sha256
                && binding.config_sha256_at_binding == expected.binding_config_sha256
        })
        .count();
    claim.binding_scope == "task"
        && claim.worktree_id == expected.worktree_id
        && claim.worktree_identity_sha256 == expected.worktree_identity_sha256
        && claim.binding_workflow_revision == expected.binding_workflow_revision
        && claim.head == expected.head
        && claim.config_sha256 == expected.binding_config_sha256
        && record.id == expected.worktree_id
        && record.head == expected.head
        && !record.dirty
        && !record.detached
        && record.created_by_foundry
        && record.identity_sha256 == expected.worktree_identity_sha256
        && record.config.sha256 == expected.binding_config_sha256
        && record.branch.as_deref() == Some(expected.branch.as_str())
        && matching_bindings == 1
        && canonical_paths_equal(&claim.repository_root, &expected.repository_root)
        && canonical_paths_equal(&claim.worktree_root, &expected.worktree_root)
        && canonical_paths_equal(&record.repository_root, &expected.repository_root)
        && canonical_paths_equal(&record.worktree_root, &expected.worktree_root)
}

fn canonical_paths_equal(left: &str, right: &str) -> bool {
    canonical_path(Path::new(left))
        .and_then(|left| canonical_path(Path::new(right)).map(|right| left == right))
        .unwrap_or(false)
}

fn prepare_fan_in(store: &FoundryStore, workflow_id: &str, task_id: &str) -> Result<PreparedFanIn> {
    require_text(workflow_id, "workflow id")?;
    require_text(task_id, "task id")?;
    let workflow = store.load_workflow(workflow_id)?;
    if workflow_is_terminal(&workflow.status) {
        bail!(
            "cannot integrate dependencies for terminal workflow {} with status {}",
            workflow.id,
            workflow.status
        );
    }
    let task = workflow
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .with_context(|| format!("task {task_id} not found in workflow {workflow_id}"))?;
    if matches!(task.status, TaskStatus::Completed | TaskStatus::Failed) {
        bail!(
            "cannot integrate dependencies for terminal task {} with status {:?}",
            task.id,
            task.status
        );
    }
    if !task
        .validation_rules
        .iter()
        .any(|rule| rule.kind == TEAMWORK_GIT_FAN_IN_VALIDATION_RULE)
    {
        bail!(
            "task {} is not explicitly scoped for dependency fan-in; missing validation rule {}",
            task.id,
            TEAMWORK_GIT_FAN_IN_VALIDATION_RULE
        );
    }
    if task.dependencies.is_empty() {
        bail!("task {} has no dependencies to integrate", task.id);
    }
    let mut dependency_ids = task.dependencies.clone();
    dependency_ids.sort();
    let original_count = dependency_ids.len();
    dependency_ids.dedup();
    if dependency_ids.len() != original_count {
        bail!("task {} contains duplicate dependency ids", task.id);
    }
    for dependency_id in &dependency_ids {
        let dependency = workflow
            .tasks
            .iter()
            .find(|candidate| candidate.id == *dependency_id)
            .with_context(|| {
                format!(
                    "task {} references missing dependency {}",
                    task.id, dependency_id
                )
            })?;
        if dependency.status != TaskStatus::Completed {
            bail!(
                "dependency {} must be Completed before fan-in; found {:?}",
                dependency.id,
                dependency.status
            );
        }
    }

    let destination = load_task_worktree(store, workflow_id, task_id)?;
    let common_base_head = require_binding_head(&destination.binding, task_id)?.to_string();
    ensure_commit_exists(
        Path::new(&destination.record.worktree_root),
        &common_base_head,
    )?;
    if !git_is_ancestor(
        Path::new(&destination.record.worktree_root),
        &common_base_head,
        &destination.record.head,
    )? {
        bail!(
            "destination HEAD {} is not descended from its frozen binding head {}",
            destination.record.head,
            common_base_head
        );
    }

    let repository_root = canonical_path(Path::new(&destination.record.repository_root))?;
    let mut worktree_ids = BTreeSet::from([destination.record.id.clone()]);
    let mut sources = Vec::with_capacity(dependency_ids.len());
    let mut source_refs = Vec::with_capacity(dependency_ids.len());
    for dependency_id in &dependency_ids {
        let source = load_task_worktree(store, workflow_id, dependency_id)?;
        if !worktree_ids.insert(source.record.id.clone()) {
            bail!(
                "dependency {} reuses worktree {}; every source and destination must be distinct",
                dependency_id,
                source.record.id
            );
        }
        let source_repository = canonical_path(Path::new(&source.record.repository_root))?;
        if source_repository != repository_root {
            bail!(
                "dependency {} belongs to repository {}, expected {}",
                dependency_id,
                source_repository.display(),
                repository_root.display()
            );
        }
        let source_binding_head = require_binding_head(&source.binding, dependency_id)?;
        let source_root = Path::new(&source.record.worktree_root);
        ensure_commit_exists(source_root, source_binding_head)?;
        if !git_is_ancestor(source_root, source_binding_head, &source.record.head)? {
            bail!(
                "dependency {} HEAD {} is not descended from its frozen binding head {}",
                dependency_id,
                source.record.head,
                source_binding_head
            );
        }
        ensure_commit_exists(source_root, &source.record.head)?;
        let paths = changed_paths(source_root, source_binding_head, &source.record.head)?;
        let already_ancestor = git_is_ancestor(
            Path::new(&destination.record.worktree_root),
            &source.record.head,
            &destination.record.head,
        )?;
        source_refs.push(TeamworkFanInSourceRef {
            worktree: worktree_ref(dependency_id, &source),
            already_ancestor,
            changed_paths_sha256: hash_string_list(&paths),
            changed_paths: paths,
        });
        sources.push(source);
    }
    let replay = source_refs.iter().all(|source| source.already_ancestor);
    if !replay {
        for (source_ref, source) in source_refs.iter_mut().zip(&sources) {
            let source_root = Path::new(&source.record.worktree_root);
            ensure_commit_exists(source_root, &common_base_head)?;
            if !git_is_ancestor(source_root, &common_base_head, &source_ref.worktree.head)? {
                bail!(
                    "dependency {} HEAD {} is not descended from destination frozen fan-in base {}",
                    source_ref.worktree.task_id,
                    source_ref.worktree.head,
                    common_base_head
                );
            }
            let paths = changed_paths(source_root, &common_base_head, &source_ref.worktree.head)?;
            source_ref.changed_paths_sha256 = hash_string_list(&paths);
            source_ref.changed_paths = paths;
        }
    }

    let pre_head = destination.record.head.clone();
    let plan_sha256 = hash_plan(
        workflow_id,
        task_id,
        &destination.record.id,
        &pre_head,
        &source_refs,
    )?;
    Ok(PreparedFanIn {
        workflow_id: workflow_id.to_string(),
        task_id: task_id.to_string(),
        destination,
        sources,
        source_refs,
        common_base_head,
        pre_head,
        plan_sha256,
    })
}

fn load_task_worktree(
    store: &FoundryStore,
    workflow_id: &str,
    task_id: &str,
) -> Result<PreparedTaskWorktree> {
    let claim = bound_worktree_mutation_claim(store, workflow_id, task_id)?
        .with_context(|| format!("task {task_id} has no bound worktree"))?;
    let binding_owners = task_binding_owner_ids(store, workflow_id, task_id)?;
    if binding_owners.len() != 1 || binding_owners[0] != claim.worktree_id {
        bail!(
            "task {} must have exactly one global task-scoped worktree owner, found {:?}",
            task_id,
            binding_owners
        );
    }
    if claim.binding_scope != "task" {
        bail!(
            "task {} worktree {} uses binding scope {}; task scope is required",
            task_id,
            claim.worktree_id,
            claim.binding_scope
        );
    }
    let record = inspect_registered_worktree(store, &claim.worktree_id)?;
    if !record.created_by_foundry {
        bail!(
            "task {} worktree {} is not Foundry-owned",
            task_id,
            record.id
        );
    }
    if record.dirty {
        let status = git_text(
            Path::new(&record.worktree_root),
            &["status", "--porcelain=v1", "--untracked-files=normal"],
        )
        .unwrap_or_else(|error| format!("status unavailable: {error:#}"));
        bail!(
            "task {} worktree {} must be clean before dependency fan-in: {}",
            task_id,
            record.id,
            status
        );
    }
    if record.detached {
        bail!(
            "task {} worktree {} uses detached HEAD; a Foundry-owned branch is required",
            task_id,
            record.id
        );
    }
    let branch = record
        .branch
        .as_deref()
        .filter(|branch| !branch.trim().is_empty())
        .with_context(|| {
            format!(
                "task {} worktree {} has no current branch",
                task_id, record.id
            )
        })?;
    if claim.worktree_id != record.id
        || claim.head != record.head
        || claim.worktree_identity_sha256 != record.identity_sha256
        || claim.config_sha256 != record.config.sha256
        || canonical_path(Path::new(&claim.repository_root))?
            != canonical_path(Path::new(&record.repository_root))?
        || canonical_path(Path::new(&claim.worktree_root))?
            != canonical_path(Path::new(&record.worktree_root))?
    {
        bail!(
            "task {} worktree mutation claim drifted before fan-in",
            task_id
        );
    }
    let bindings = record
        .bindings
        .iter()
        .filter(|binding| {
            binding.workflow_id == workflow_id && binding.task_id.as_deref() == Some(task_id)
        })
        .cloned()
        .collect::<Vec<_>>();
    if bindings.len() != 1 {
        bail!(
            "task {} worktree {} must have exactly one matching task-scoped binding, found {}",
            task_id,
            record.id,
            bindings.len()
        );
    }
    let binding = &bindings[0];
    if claim.binding_workflow_revision != binding.workflow_revision
        || binding.worktree_identity_sha256 != record.identity_sha256
        || binding.config_sha256_at_binding != record.config.sha256
    {
        bail!(
            "task {} worktree {} binding identity, configuration or revision drifted before fan-in",
            task_id,
            record.id
        );
    }
    if branch.starts_with('-') {
        bail!("task {} worktree branch is unsafe: {}", task_id, branch);
    }
    Ok(PreparedTaskWorktree {
        record,
        binding: binding.clone(),
    })
}

fn task_binding_owner_ids(
    store: &FoundryStore,
    workflow_id: &str,
    task_id: &str,
) -> Result<Vec<String>> {
    let mut owners = list_registered_worktrees(store, None, Some(workflow_id))?
        .worktrees
        .into_iter()
        .flat_map(|record| {
            let record_id = record.id;
            record
                .bindings
                .into_iter()
                .filter(move |binding| {
                    binding.workflow_id == workflow_id
                        && binding.task_id.as_deref() == Some(task_id)
                })
                .map(move |_| record_id.clone())
        })
        .collect::<Vec<_>>();
    owners.sort();
    Ok(owners)
}

fn revalidate_prepared_state(store: &FoundryStore, prepared: &PreparedFanIn) -> Result<()> {
    revalidate_worktree_unchanged(store, &prepared.destination, &prepared.pre_head)?;
    revalidate_sources_unchanged(store, &prepared.sources)
}

fn revalidate_sources_unchanged(
    store: &FoundryStore,
    sources: &[PreparedTaskWorktree],
) -> Result<()> {
    for source in sources {
        revalidate_worktree_unchanged(store, source, &source.record.head)?;
    }
    Ok(())
}

fn revalidate_worktree_unchanged(
    store: &FoundryStore,
    expected: &PreparedTaskWorktree,
    expected_head: &str,
) -> Result<()> {
    let task_id = expected
        .binding
        .task_id
        .as_deref()
        .with_context(|| "dependency fan-in requires a task-scoped binding")?;
    let current = load_task_worktree(store, &expected.binding.workflow_id, task_id)?;
    if current.record.id != expected.record.id
        || current.record.head != expected_head
        || current.record.dirty
        || current.record.branch != expected.record.branch
        || !current.record.created_by_foundry
        || current.record.identity_sha256 != expected.record.identity_sha256
        || current.record.config.sha256 != expected.record.config.sha256
        || canonical_path(Path::new(&current.record.repository_root))?
            != canonical_path(Path::new(&expected.record.repository_root))?
        || canonical_path(Path::new(&current.record.worktree_root))?
            != canonical_path(Path::new(&expected.record.worktree_root))?
        || current.binding.workflow_revision != expected.binding.workflow_revision
        || current.binding.head_at_binding != expected.binding.head_at_binding
        || current.binding.worktree_identity_sha256 != expected.binding.worktree_identity_sha256
        || current.binding.config_sha256_at_binding != expected.binding.config_sha256_at_binding
    {
        bail!(
            "worktree {} or its task-scoped binding drifted during dependency fan-in: expected head={} branch={:?} clean Foundry-owned source",
            expected.record.id,
            expected_head,
            expected.record.branch
        );
    }
    Ok(())
}

fn planned_report(
    prepared: PreparedFanIn,
    authorization: TeamworkFanInAuthorizationReceipt,
) -> TeamworkFanInReport {
    let replay = prepared
        .source_refs
        .iter()
        .all(|source| source.already_ancestor);
    let reason = if replay {
        "dry-run: every frozen dependency HEAD is already an ancestor of the destination HEAD"
    } else {
        "dry-run: validated an ordered dependency fan-in plan; repository mutation was not authorized"
    };
    build_report(
        &prepared,
        authorization,
        if replay {
            "already_integrated"
        } else {
            "integration_planned"
        },
        true,
        true,
        replay,
        prepared.pre_head.clone(),
        false,
        false,
        false,
        false,
        Vec::new(),
        Vec::new(),
        false,
        false,
        reason.to_string(),
    )
}

fn replay_report(
    prepared: &PreparedFanIn,
    authorization: TeamworkFanInAuthorizationReceipt,
    destination_rebound: bool,
) -> TeamworkFanInReport {
    build_report(
        prepared,
        authorization,
        "already_integrated",
        true,
        false,
        true,
        prepared.pre_head.clone(),
        false,
        destination_rebound,
        false,
        false,
        Vec::new(),
        Vec::new(),
        false,
        false,
        "idempotent replay: every frozen dependency HEAD is already an ancestor of the destination HEAD"
            .to_string(),
    )
}

fn conflict_report(
    prepared: PreparedFanIn,
    authorization: TeamworkFanInAuthorizationReceipt,
    conflict_paths: Vec<String>,
    rollback_verified: bool,
    git_reason: String,
) -> TeamworkFanInReport {
    build_report(
        &prepared,
        authorization,
        "integration_conflict",
        false,
        false,
        false,
        prepared.pre_head.clone(),
        false,
        false,
        true,
        false,
        Vec::new(),
        conflict_paths,
        true,
        rollback_verified,
        format!(
            "dependency fan-in conflicted and destination rollback was verified: {}",
            git_reason
        ),
    )
}

fn success_report(
    prepared: &PreparedFanIn,
    authorization: TeamworkFanInAuthorizationReceipt,
    result_head: String,
    integrated_paths: Vec<String>,
) -> TeamworkFanInReport {
    build_report(
        prepared,
        authorization,
        "dependencies_integrated",
        true,
        false,
        false,
        result_head,
        true,
        true,
        true,
        true,
        integrated_paths,
        Vec::new(),
        false,
        false,
        "Foundry created one deterministic-order integration commit from the frozen dependency HEADs"
            .to_string(),
    )
}

#[allow(clippy::too_many_arguments)]
fn build_report(
    prepared: &PreparedFanIn,
    authorization: TeamworkFanInAuthorizationReceipt,
    status: &str,
    success: bool,
    dry_run: bool,
    replay: bool,
    result_head: String,
    commit_created: bool,
    destination_rebound: bool,
    repository_mutation_attempted: bool,
    repository_mutated: bool,
    integrated_paths: Vec<String>,
    conflict_paths: Vec<String>,
    rollback_required: bool,
    rollback_verified: bool,
    reason: String,
) -> TeamworkFanInReport {
    TeamworkFanInReport {
        schema_version: TEAMWORK_FAN_IN_SCHEMA_VERSION.to_string(),
        validation_rule_kind: TEAMWORK_GIT_FAN_IN_VALIDATION_RULE.to_string(),
        status: status.to_string(),
        success,
        dry_run,
        replay,
        workflow_id: prepared.workflow_id.clone(),
        task_id: prepared.task_id.clone(),
        ordered_source_task_ids: prepared
            .source_refs
            .iter()
            .map(|source| source.worktree.task_id.clone())
            .collect(),
        destination: worktree_ref(&prepared.task_id, &prepared.destination),
        sources: prepared.source_refs.clone(),
        common_base_head: prepared.common_base_head.clone(),
        pre_head: prepared.pre_head.clone(),
        result_head: result_head.clone(),
        commit_created,
        repository_mutation_attempted,
        repository_mutated,
        destination_rebound,
        integrated_paths,
        conflict_paths,
        plan_sha256: prepared.plan_sha256.clone(),
        receipt_sha256: String::new(),
        authorization,
        atomicity: TeamworkFanInAtomicityReceipt {
            mode: if repository_mutation_attempted {
                "destination_no_commit_octopus_merge_with_verified_abort"
            } else {
                "no_repository_mutation"
            }
            .to_string(),
            temporary_worktree_used: false,
            destination_pre_head: prepared.pre_head.clone(),
            destination_post_head: result_head,
            rollback_required,
            rollback_verified,
            limitation: if repository_mutation_attempted {
                "Git temporarily updates the destination index/worktree before the Foundry commit; on any merge failure Foundry aborts or hard-resets only this clean Foundry-owned destination and verifies exact pre_head restoration."
            } else {
                "No Git repository mutation was attempted."
            }
            .to_string(),
        },
        reason,
        generated_at: Utc::now(),
    }
}

fn seal_report(mut report: TeamworkFanInReport) -> Result<TeamworkFanInReport> {
    report.receipt_sha256.clear();
    report.receipt_sha256 = receipt_sha256(&report)?;
    Ok(report)
}

fn receipt_sha256(report: &TeamworkFanInReport) -> Result<String> {
    let mut unsigned = report.clone();
    unsigned.receipt_sha256.clear();
    Ok(hex_sha256(&serde_json::to_vec(&unsigned)?))
}

fn worktree_ref(task_id: &str, prepared: &PreparedTaskWorktree) -> TeamworkFanInWorktreeRef {
    TeamworkFanInWorktreeRef {
        task_id: task_id.to_string(),
        worktree_id: prepared.record.id.clone(),
        worktree_root: prepared.record.worktree_root.clone(),
        repository_root: prepared.record.repository_root.clone(),
        branch: prepared.record.branch.clone().unwrap_or_default(),
        binding_head: prepared.binding.head_at_binding.clone(),
        binding_workflow_revision: prepared.binding.workflow_revision,
        worktree_identity_sha256: prepared.record.identity_sha256.clone(),
        binding_config_sha256: prepared.binding.config_sha256_at_binding.clone(),
        head: prepared.record.head.clone(),
        created_by_foundry: prepared.record.created_by_foundry,
        clean: !prepared.record.dirty,
    }
}

fn authorization_receipt(
    options: &IntegrateDependenciesOptions<'_>,
) -> TeamworkFanInAuthorizationReceipt {
    TeamworkFanInAuthorizationReceipt {
        repository_mutation_allowed: options.allow_repository_mutation,
        approved_by: options.approved_by.trim().to_string(),
        reason: options.reason.trim().to_string(),
        origin: options.origin.trim().to_string(),
    }
}

fn validate_apply_authorization(options: &IntegrateDependenciesOptions<'_>) -> Result<()> {
    if !options.allow_repository_mutation {
        bail!("dependency fan-in requires explicit repository mutation authorization");
    }
    require_text(options.approved_by, "dependency fan-in approved_by")?;
    require_text(options.reason, "dependency fan-in reason")?;
    require_text(options.origin, "dependency fan-in origin")?;
    Ok(())
}

fn require_text<'a>(value: &'a str, label: &str) -> Result<&'a str> {
    let value = value.trim();
    if value.is_empty() {
        bail!("{label} cannot be empty");
    }
    Ok(value)
}

fn require_binding_head<'a>(binding: &'a WorktreeBinding, task_id: &str) -> Result<&'a str> {
    require_text(
        &binding.head_at_binding,
        &format!("task {task_id} frozen binding head"),
    )
}

fn workflow_is_terminal(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "completed" | "complete" | "cancelled" | "canceled" | "failed"
    )
}

fn hash_plan(
    workflow_id: &str,
    task_id: &str,
    destination_worktree_id: &str,
    pre_head: &str,
    sources: &[TeamworkFanInSourceRef],
) -> Result<String> {
    Ok(hex_sha256(&serde_json::to_vec(&serde_json::json!({
        "schema_version": "foundry.worktree.dependency_integration_plan.v1",
        "workflow_id": workflow_id,
        "task_id": task_id,
        "destination_worktree_id": destination_worktree_id,
        "pre_head": pre_head,
        "sources": sources,
    }))?))
}

fn hash_string_list(values: &[String]) -> String {
    hex_sha256(values.join("\0").as_bytes())
}

fn integration_commit_message(prepared: &PreparedFanIn) -> String {
    let sources = prepared
        .source_refs
        .iter()
        .filter(|source| !source.already_ancestor)
        .map(|source| {
            format!(
                "{} {}",
                source.worktree.task_id,
                &source.worktree.head[..source.worktree.head.len().min(12)]
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "foundry: integrate dependencies for {}\n\nWorkflow: {}\nSources: {}",
        prepared.task_id, prepared.workflow_id, sources
    )
}

fn ensure_safe_integration_attributes(
    destination: &Path,
    sources: &[TeamworkFanInSourceRef],
) -> Result<()> {
    ensure_no_external_git_drivers(destination)?;
    let paths = sources
        .iter()
        .flat_map(|source| source.changed_paths.iter().cloned())
        .collect::<BTreeSet<_>>();
    for path in &paths {
        validate_git_relative_path(path)?;
        if Path::new(path)
            .file_name()
            .is_some_and(|name| name == ".gitattributes")
        {
            bail!(
                "dependency fan-in refuses source changes to {}; Git attribute policy must be reviewed separately",
                path
            );
        }
    }
    for chunk in paths.iter().collect::<Vec<_>>().chunks(256) {
        let mut command = git_command(destination);
        command.args(["check-attr", "-z", "merge", "filter", "--"]);
        command.args(chunk.iter().map(|path| path.as_str()));
        let output = command
            .output()
            .context("failed to inspect Git merge and filter attributes")?;
        if !output.status.success() {
            bail!(
                "failed to inspect Git merge and filter attributes: {}",
                bounded_git_message(&output)
            );
        }
        validate_safe_attribute_output(&output.stdout)?;
    }
    Ok(())
}

fn ensure_no_external_git_drivers(destination: &Path) -> Result<()> {
    for pattern in [
        "^merge\\..*\\.driver$",
        "^filter\\..*\\.(process|clean|smudge)$",
    ] {
        let output = git_output(destination, &["config", "--get-regexp", pattern])?;
        match output.status.code() {
            Some(1) => {}
            Some(0) => {
                bail!(
                    "dependency fan-in refuses repository-configured external Git drivers: {}",
                    bounded_git_message(&output)
                );
            }
            _ => {
                bail!(
                    "failed to inspect repository Git drivers: {}",
                    bounded_git_message(&output)
                );
            }
        }
    }
    Ok(())
}

fn validate_safe_attribute_output(bytes: &[u8]) -> Result<()> {
    let fields = bytes
        .split(|byte| *byte == 0)
        .filter(|field| !field.is_empty())
        .collect::<Vec<_>>();
    if fields.len() % 3 != 0 {
        bail!("Git check-attr returned a malformed NUL-delimited response");
    }
    for fields in fields.chunks_exact(3) {
        let path =
            std::str::from_utf8(fields[0]).context("Git attribute path is not valid UTF-8")?;
        let attribute =
            std::str::from_utf8(fields[1]).context("Git attribute name is not valid UTF-8")?;
        let value =
            std::str::from_utf8(fields[2]).context("Git attribute value is not valid UTF-8")?;
        validate_git_relative_path(path)?;
        let safe = match attribute {
            "merge" => matches!(
                value,
                "unspecified" | "set" | "unset" | "text" | "binary" | "union"
            ),
            "filter" => matches!(value, "unspecified" | "unset"),
            _ => false,
        };
        if !safe {
            bail!(
                "dependency fan-in refuses Git attribute {}={} for {}; external merge/filter execution requires a separately authorized sandbox",
                attribute,
                value,
                path
            );
        }
    }
    Ok(())
}

fn run_no_commit_merge(destination: &Path, source_heads: &[String]) -> Result<Output> {
    if source_heads.is_empty() {
        bail!("dependency fan-in has no missing source HEADs to merge");
    }
    let mut command = git_command(destination);
    command
        .args([
            "-c",
            "merge.autoStash=false",
            "merge",
            "--no-commit",
            "--no-ff",
            "--no-edit",
            "--no-verify",
            "--no-autostash",
        ])
        .args(source_heads)
        .env("GIT_MERGE_AUTOEDIT", "no");
    command
        .output()
        .context("failed to invoke Git dependency merge")
}

fn run_foundry_commit(destination: &Path, message: &str) -> Result<Output> {
    let committed_at = Utc::now().to_rfc3339();
    git_command(destination)
        .args([
            "-c",
            "user.name=Foundry Core",
            "-c",
            "user.email=foundry-core@localhost.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "--no-verify",
            "--no-gpg-sign",
            "-m",
            message,
        ])
        .env("GIT_AUTHOR_DATE", &committed_at)
        .env("GIT_COMMITTER_DATE", &committed_at)
        .output()
        .context("failed to invoke Git integration commit")
}

fn restore_destination(destination: &Path, pre_head: &str) -> Result<bool> {
    let _ = git_command(destination).args(["merge", "--abort"]).output();
    let restored_without_reset = git_head(destination)? == pre_head
        && git_is_clean(destination)?
        && !git_merge_in_progress(destination)?;
    if !restored_without_reset {
        let reset = git_output(destination, &["reset", "--hard", pre_head])?;
        if !reset.status.success() {
            bail!(
                "failed to restore Foundry-owned destination to {}: {}",
                pre_head,
                bounded_git_message(&reset)
            );
        }
    }
    let restored = git_head(destination)? == pre_head
        && git_is_clean(destination)?
        && !git_merge_in_progress(destination)?;
    if !restored {
        bail!(
            "destination rollback verification failed; expected clean HEAD {}",
            pre_head
        );
    }
    Ok(true)
}

fn git_head(path: &Path) -> Result<String> {
    git_text(path, &["rev-parse", "HEAD"])
}

fn git_is_clean(path: &Path) -> Result<bool> {
    Ok(git_text(
        path,
        &["status", "--porcelain=v1", "--untracked-files=normal"],
    )?
    .is_empty())
}

fn git_merge_in_progress(path: &Path) -> Result<bool> {
    let output = git_output(path, &["rev-parse", "--verify", "-q", "MERGE_HEAD"])?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(anyhow!(
            "failed to inspect Git merge state: {}",
            bounded_git_message(&output)
        )),
    }
}

fn ensure_commit_exists(path: &Path, commit: &str) -> Result<()> {
    let revision = format!("{commit}^{{commit}}");
    let output = git_output(path, &["rev-parse", "--verify", &revision])?;
    if !output.status.success() {
        bail!(
            "frozen Git commit {} is unavailable: {}",
            commit,
            bounded_git_message(&output)
        );
    }
    Ok(())
}

fn git_is_ancestor(path: &Path, ancestor: &str, descendant: &str) -> Result<bool> {
    let output = git_output(path, &["merge-base", "--is-ancestor", ancestor, descendant])?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(anyhow!(
            "failed to compare Git ancestry {}..{}: {}",
            ancestor,
            descendant,
            bounded_git_message(&output)
        )),
    }
}

fn changed_paths(path: &Path, base: &str, head: &str) -> Result<Vec<String>> {
    let range = format!("{base}..{head}");
    let output = git_output(path, &["diff", "--name-only", "-z", &range, "--"])?;
    if !output.status.success() {
        bail!(
            "failed to inspect changed paths for {}: {}",
            range,
            bounded_git_message(&output)
        );
    }
    parse_nul_paths(&output.stdout)
}

fn unmerged_paths(path: &Path) -> Result<Vec<String>> {
    let output = git_output(
        path,
        &["diff", "--name-only", "--diff-filter=U", "-z", "--"],
    )?;
    if !output.status.success() {
        bail!(
            "failed to inspect dependency merge conflicts: {}",
            bounded_git_message(&output)
        );
    }
    parse_nul_paths(&output.stdout)
}

fn parse_nul_paths(bytes: &[u8]) -> Result<Vec<String>> {
    let mut paths = Vec::new();
    for raw in bytes.split(|byte| *byte == 0).filter(|raw| !raw.is_empty()) {
        let path = std::str::from_utf8(raw)
            .context("Git path is not valid UTF-8; dependency fan-in fails closed")?
            .to_string();
        validate_git_relative_path(&path)?;
        paths.push(path);
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn validate_git_relative_path(path: &str) -> Result<()> {
    let path = Path::new(path);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        bail!(
            "Git changed path escapes repository scope: {}",
            path.display()
        );
    }
    Ok(())
}

fn git_text(path: &Path, args: &[&str]) -> Result<String> {
    let output = git_output(path, args)?;
    if !output.status.success() {
        bail!(
            "git {:?} failed in {}: {}",
            args,
            path.display(),
            bounded_git_message(&output)
        );
    }
    String::from_utf8(output.stdout)
        .context("Git output is not valid UTF-8")
        .map(|value| value.trim().to_string())
}

fn git_output(path: &Path, args: &[&str]) -> Result<Output> {
    git_command(path)
        .args(args)
        .output()
        .with_context(|| format!("failed to invoke git {:?} in {}", args, path.display()))
}

fn git_command(path: &Path) -> Command {
    let path_environment = crate::brand::env_var_os("PATH").unwrap_or_else(default_git_path);
    let mut command = Command::new(trusted_git_program());
    command
        .env_clear()
        .env("PATH", path_environment)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_SYSTEM", git_null_config_path())
        .env("GIT_CONFIG_GLOBAL", git_null_config_path())
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .arg("-C")
        .arg(path)
        .arg("-c")
        .arg(format!("core.hooksPath={}", git_null_config_path()))
        .args(["-c", "core.fsmonitor=false", "-c", "commit.gpgsign=false"]);
    #[cfg(windows)]
    if let Some(system_root) = crate::brand::env_var_os("SystemRoot") {
        command.env("SystemRoot", system_root);
    }
    command
}

fn trusted_git_program() -> PathBuf {
    ["/usr/bin/git", "/bin/git", "/usr/local/bin/git"]
        .into_iter()
        .map(PathBuf::from)
        .find(|candidate| candidate.is_file())
        .and_then(|candidate| std::fs::canonicalize(candidate).ok())
        .unwrap_or_else(|| PathBuf::from("git"))
}

fn default_git_path() -> OsString {
    if cfg!(windows) {
        OsString::from("C:\\Windows\\System32")
    } else {
        OsString::from("/usr/local/bin:/usr/bin:/bin")
    }
}

fn git_null_config_path() -> &'static str {
    if cfg!(windows) {
        "NUL"
    } else {
        "/dev/null"
    }
}

fn bounded_git_message(output: &Output) -> String {
    let bytes = if output.stderr.is_empty() {
        &output.stdout
    } else {
        &output.stderr
    };
    let end = bytes.len().min(MAX_GIT_MESSAGE_BYTES);
    String::from_utf8_lossy(&bytes[..end]).trim().to_string()
}

fn canonical_path(path: &Path) -> Result<PathBuf> {
    std::fs::canonicalize(path)
        .with_context(|| format!("failed to canonicalize {}", path.display()))
}
