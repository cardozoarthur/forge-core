use crate::graph::{TaskStatus, Workflow};
use crate::identity::{ensure_workflow_policy, evaluate_tenant_policy_for_action};
use crate::storage::{FoundryStore, TaskLeaseWrite};
use crate::worktree::{bound_worktree_mutation_claim, WorktreeMutationClaim};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskLease {
    pub lease_id: String,
    pub workflow_id: String,
    pub task_id: String,
    pub executor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_claim: Option<WorktreeMutationClaim>,
    pub acquired_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskLeaseWorkspaceConflict {
    pub requested_claim: WorktreeMutationClaim,
    pub held_claim: WorktreeMutationClaim,
    pub held_by_lease_id: String,
    pub held_by_workflow_id: String,
    pub held_by_task_id: String,
    pub held_by_executor: String,
    pub held_until: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskLeaseAcquireReport {
    pub status: String,
    pub allowed: bool,
    pub workflow_id: String,
    pub task_id: String,
    pub executor: String,
    pub lease: Option<TaskLease>,
    pub current_lease: Option<TaskLease>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_conflict: Option<TaskLeaseWorkspaceConflict>,
    pub reason: Option<String>,
    pub audit_event_status: String,
    pub audit_event_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskLeaseReleaseReport {
    pub status: String,
    pub released: bool,
    pub workflow_id: String,
    pub task_id: String,
    pub executor: String,
    pub lease_id: String,
    pub current_lease: Option<TaskLease>,
}

pub fn acquire_task_lease(
    store: &FoundryStore,
    workflow_id: &str,
    task_id: &str,
    executor: &str,
    ttl_seconds: u64,
) -> Result<TaskLeaseAcquireReport> {
    if executor.trim().is_empty() {
        bail!("executor cannot be empty");
    }
    if ttl_seconds == 0 {
        bail!("ttl seconds must be greater than zero");
    }
    let ttl_seconds = i64::try_from(ttl_seconds).context("ttl seconds exceeds supported range")?;
    let (task_status, lease, saved, workspace_conflict) = store.with_transaction(|| {
        let workflow = store.load_workflow(workflow_id)?;
        ensure_workflow_snapshot_policy(store, &workflow, "task lease acquire")?;
        let task_status = task_status_from_workflow(&workflow, task_id)?;
        if task_status != TaskStatus::Pending {
            return Ok((task_status, None, false, None));
        }

        let acquired_at = Utc::now();
        let expires_at = acquired_at + Duration::seconds(ttl_seconds);
        let workspace_claim = bound_worktree_mutation_claim(store, workflow_id, task_id)?;
        let lease = TaskLease {
            lease_id: format!("lease_{}", Uuid::new_v4().to_string().replace('-', "")),
            workflow_id: workflow_id.to_string(),
            task_id: task_id.to_string(),
            executor: executor.to_string(),
            workspace_claim,
            acquired_at,
            expires_at,
        };
        if let Some(conflict) = find_active_workspace_conflict(store, &lease, acquired_at)? {
            return Ok((task_status, Some(lease), false, Some(conflict)));
        }
        let lease_value = serde_json::to_value(&lease)?;
        let acquired_at_rfc3339 = acquired_at.to_rfc3339();
        let expires_at_rfc3339 = expires_at.to_rfc3339();
        let saved = store.try_save_task_lease(TaskLeaseWrite {
            workflow_id,
            task_id,
            lease_id: &lease.lease_id,
            executor,
            acquired_at: &acquired_at_rfc3339,
            expires_at: &expires_at_rfc3339,
            data: &lease_value,
        })?;
        Ok((task_status, Some(lease), saved, None))
    })?;

    if task_status != TaskStatus::Pending {
        let report = TaskLeaseAcquireReport {
            status: "lease_blocked_task_status".to_string(),
            allowed: false,
            workflow_id: workflow_id.to_string(),
            task_id: task_id.to_string(),
            executor: executor.to_string(),
            lease: None,
            current_lease: load_current_lease(store, workflow_id, task_id)?,
            workspace_conflict: None,
            reason: Some(format!(
                "task status is {}; lease acquisition requires pending status",
                task_status_name(&task_status)
            )),
            audit_event_status: "pending".to_string(),
            audit_event_error: None,
        };
        return Ok(record_task_lease_acquire_audit(
            store,
            workflow_id,
            "task_lease_blocked_task_status",
            report,
        ));
    }
    let lease = lease.context("pending task lease transaction did not return a lease")?;

    if let Some(conflict) = workspace_conflict {
        let reason = format!(
            "exclusive mutation claim for worktree {} conflicts with active lease {} held by workflow {} task {} executor {} until {}",
            conflict.requested_claim.worktree_id,
            conflict.held_by_lease_id,
            conflict.held_by_workflow_id,
            conflict.held_by_task_id,
            conflict.held_by_executor,
            conflict.held_until,
        );
        let report = TaskLeaseAcquireReport {
            status: "lease_blocked_workspace_conflict".to_string(),
            allowed: false,
            workflow_id: workflow_id.to_string(),
            task_id: task_id.to_string(),
            executor: executor.to_string(),
            lease: None,
            current_lease: load_current_lease(store, workflow_id, task_id)?,
            workspace_conflict: Some(conflict),
            reason: Some(reason),
            audit_event_status: "pending".to_string(),
            audit_event_error: None,
        };
        return Ok(record_task_lease_acquire_audit(
            store,
            workflow_id,
            "task_lease_blocked_workspace_conflict",
            report,
        ));
    }

    if saved {
        let report = TaskLeaseAcquireReport {
            status: "lease_acquired".to_string(),
            allowed: true,
            workflow_id: workflow_id.to_string(),
            task_id: task_id.to_string(),
            executor: executor.to_string(),
            lease: Some(lease),
            current_lease: None,
            workspace_conflict: None,
            reason: None,
            audit_event_status: "pending".to_string(),
            audit_event_error: None,
        };
        return Ok(record_task_lease_acquire_audit(
            store,
            workflow_id,
            "task_lease_acquired",
            report,
        ));
    }

    let current_lease = load_current_lease(store, workflow_id, task_id)?;
    let report = TaskLeaseAcquireReport {
        status: "lease_conflict".to_string(),
        allowed: false,
        workflow_id: workflow_id.to_string(),
        task_id: task_id.to_string(),
        executor: executor.to_string(),
        lease: None,
        current_lease,
        workspace_conflict: None,
        reason: Some("task already has an unexpired lease".to_string()),
        audit_event_status: "pending".to_string(),
        audit_event_error: None,
    };
    Ok(record_task_lease_acquire_audit(
        store,
        workflow_id,
        "task_lease_conflict",
        report,
    ))
}

pub fn validate_task_lease_for_execution(
    store: &FoundryStore,
    workflow_id: &str,
    task_id: &str,
    executor: &str,
    cwd: &Path,
) -> Result<TaskLease> {
    ensure_workflow_policy(store, workflow_id, "task lease execution")?;
    let lease = load_current_lease(store, workflow_id, task_id)?.with_context(|| {
        format!("active task lease is required for workflow {workflow_id} task {task_id}")
    })?;
    if lease.expires_at <= Utc::now() {
        bail!(
            "task lease {} expired at {}; acquire a fresh handoff before execution",
            lease.lease_id,
            lease.expires_at
        );
    }
    if lease.executor != executor {
        bail!(
            "task lease {} belongs to executor {}, not {}",
            lease.lease_id,
            lease.executor,
            executor
        );
    }

    let current_claim = bound_worktree_mutation_claim(store, workflow_id, task_id)?;
    match (&lease.workspace_claim, current_claim.as_ref()) {
        (Some(frozen), Some(current)) => {
            if frozen != current {
                bail!(
                    "task lease {} worktree claim drifted (leased worktree={} head={} config={} binding_revision={}; current worktree={} head={} config={} binding_revision={}); acquire a fresh handoff before execution",
                    lease.lease_id,
                    frozen.worktree_id,
                    frozen.head,
                    frozen.config_sha256,
                    frozen.binding_workflow_revision,
                    current.worktree_id,
                    current.head,
                    current.config_sha256,
                    current.binding_workflow_revision,
                );
            }
            let canonical_cwd = fs::canonicalize(cwd).with_context(|| {
                format!("failed to canonicalize execution cwd {}", cwd.display())
            })?;
            let claimed_root = Path::new(&frozen.worktree_root);
            if canonical_cwd != claimed_root {
                bail!(
                    "execution cwd {} conflicts with leased worktree {}",
                    canonical_cwd.display(),
                    claimed_root.display()
                );
            }
        }
        (Some(frozen), None) => {
            bail!(
                "task lease {} retains worktree claim {}, but the task no longer has a matching binding",
                lease.lease_id,
                frozen.worktree_id
            );
        }
        (None, Some(current)) => {
            bail!(
                "task lease {} predates the required worktree claim for {}; acquire a fresh handoff before execution",
                lease.lease_id,
                current.worktree_id
            );
        }
        (None, None) => {}
    }
    Ok(lease)
}

fn find_active_workspace_conflict(
    store: &FoundryStore,
    requested: &TaskLease,
    now: DateTime<Utc>,
) -> Result<Option<TaskLeaseWorkspaceConflict>> {
    let Some(requested_claim) = requested.workspace_claim.as_ref() else {
        return Ok(None);
    };
    for value in store.load_task_leases()? {
        let held = serde_json::from_value::<TaskLease>(value)
            .context("persisted task lease is invalid during workspace admission")?;
        if held.expires_at <= now
            || (held.workflow_id == requested.workflow_id && held.task_id == requested.task_id)
        {
            continue;
        }
        let held_claim = match held.workspace_claim.clone() {
            Some(claim) => Some(claim),
            None => bound_worktree_mutation_claim(store, &held.workflow_id, &held.task_id)?,
        };
        let Some(held_claim) = held_claim else {
            continue;
        };
        if same_worktree_identity(requested_claim, &held_claim) {
            return Ok(Some(TaskLeaseWorkspaceConflict {
                requested_claim: requested_claim.clone(),
                held_claim,
                held_by_lease_id: held.lease_id,
                held_by_workflow_id: held.workflow_id,
                held_by_task_id: held.task_id,
                held_by_executor: held.executor,
                held_until: held.expires_at,
            }));
        }
    }
    Ok(None)
}

fn same_worktree_identity(left: &WorktreeMutationClaim, right: &WorktreeMutationClaim) -> bool {
    (!left.worktree_identity_sha256.is_empty()
        && left.worktree_identity_sha256 == right.worktree_identity_sha256)
        || left.worktree_root == right.worktree_root
}

pub fn release_task_lease(
    store: &FoundryStore,
    workflow_id: &str,
    task_id: &str,
    lease_id: &str,
    executor: &str,
) -> Result<TaskLeaseReleaseReport> {
    ensure_workflow_policy(store, workflow_id, "task lease release")?;
    ensure_task_exists(store, workflow_id, task_id)?;
    let released = store.delete_task_lease(workflow_id, task_id, lease_id)?;
    let current_lease = load_current_lease(store, workflow_id, task_id)?;
    let report = TaskLeaseReleaseReport {
        status: if released {
            "lease_released".to_string()
        } else {
            "lease_not_found".to_string()
        },
        released,
        workflow_id: workflow_id.to_string(),
        task_id: task_id.to_string(),
        executor: executor.to_string(),
        lease_id: lease_id.to_string(),
        current_lease,
    };
    store.record_event(
        workflow_id,
        if released {
            "task_lease_released"
        } else {
            "task_lease_release_failed"
        },
        &serde_json::to_value(&report)?,
    )?;
    Ok(report)
}

fn ensure_task_exists(store: &FoundryStore, workflow_id: &str, task_id: &str) -> Result<()> {
    load_task_status(store, workflow_id, task_id).map(|_| ())
}

fn load_task_status(store: &FoundryStore, workflow_id: &str, task_id: &str) -> Result<TaskStatus> {
    let workflow = store.load_workflow(workflow_id)?;
    task_status_from_workflow(&workflow, task_id)
}

fn task_status_from_workflow(workflow: &Workflow, task_id: &str) -> Result<TaskStatus> {
    workflow
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .map(|task| task.status.clone())
        .ok_or_else(|| anyhow::anyhow!("task not found in workflow {}: {task_id}", workflow.id))
}

fn ensure_workflow_snapshot_policy(
    store: &FoundryStore,
    workflow: &Workflow,
    action: &str,
) -> Result<()> {
    if workflow.intent.operating_context.tenant_policy_mode != "enforce" {
        return Ok(());
    }
    let report = evaluate_tenant_policy_for_action(store, &workflow.id, "enforce", action)?;
    if report.allowed {
        return Ok(());
    }
    let denied_gates = report
        .decisions
        .iter()
        .filter(|decision| decision.status != "allowed")
        .map(|decision| format!("{}: {}", decision.gate, decision.reason))
        .collect::<Vec<_>>()
        .join("; ");
    bail!(
        "multi-tenant enforcement blocked {action}: workflow {} failed tenant policy ({denied_gates})",
        workflow.id
    );
}

fn record_task_lease_acquire_audit(
    store: &FoundryStore,
    workflow_id: &str,
    event_kind: &str,
    mut report: TaskLeaseAcquireReport,
) -> TaskLeaseAcquireReport {
    report.audit_event_status = "recorded".to_string();
    report.audit_event_error = None;
    let audit_result = serde_json::to_value(&report)
        .map_err(anyhow::Error::from)
        .and_then(|data| store.record_event(workflow_id, event_kind, &data));
    if let Err(error) = audit_result {
        report.audit_event_status = "failed".to_string();
        report.audit_event_error = Some(format!("{error:#}"));
    }
    report
}

fn task_status_name(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Pending => "pending",
        TaskStatus::Running => "running",
        TaskStatus::Completed => "completed",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Failed => "failed",
    }
}

fn load_current_lease(
    store: &FoundryStore,
    workflow_id: &str,
    task_id: &str,
) -> Result<Option<TaskLease>> {
    store
        .load_task_lease(workflow_id, task_id)?
        .map(serde_json::from_value)
        .transpose()
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::{acquire_task_lease, validate_task_lease_for_execution};
    use crate::graph::TaskStatus;
    use crate::intent::parse_intent;
    use crate::storage::{FoundryStore, IdentityMembershipWrite};
    use crate::worktree::{
        bind_worktree, create_worktree, register_worktree, WorktreeCreateOptions,
        WorktreeRegisterOptions,
    };
    use rusqlite::{params, Connection};
    use std::fs;
    use std::path::Path;
    use std::process::Command;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    fn git(repository: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(repository)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn initialize_repository(repository: &Path) {
        fs::create_dir_all(repository).unwrap();
        git(repository, &["init", "-q"]);
        git(
            repository,
            &["config", "user.email", "foundry-tests@example.invalid"],
        );
        git(repository, &["config", "user.name", "Foundry Tests"]);
        fs::write(repository.join("README.md"), "workspace claim fixture\n").unwrap();
        git(repository, &["add", "README.md"]);
        git(repository, &["commit", "-q", "-m", "fixture"]);
    }

    #[test]
    fn one_worktree_allows_only_one_active_mutating_lease_even_across_workflows() {
        let temp = tempfile::tempdir().unwrap();
        let repository = temp.path().join("repository");
        initialize_repository(&repository);
        let store = FoundryStore::open(temp.path().join("foundry.sqlite")).unwrap();

        let first = crate::graph::create_workflow(parse_intent("First isolated delivery"));
        let second = crate::graph::create_workflow(parse_intent("Second isolated delivery"));
        let first_task = first.tasks[0].id.clone();
        let second_task = second.tasks[0].id.clone();
        store.save_workflow(&first).unwrap();
        store.save_workflow(&second).unwrap();
        register_worktree(
            &store,
            WorktreeRegisterOptions {
                path: repository.clone(),
                id: None,
                workflow_id: Some(first.id.clone()),
                task_id: Some(first_task.clone()),
                origin: "lease-test".to_string(),
                created_by_foundry: false,
            },
        )
        .unwrap();
        register_worktree(
            &store,
            WorktreeRegisterOptions {
                path: repository,
                id: None,
                workflow_id: Some(second.id.clone()),
                task_id: Some(second_task.clone()),
                origin: "lease-test".to_string(),
                created_by_foundry: false,
            },
        )
        .unwrap();

        let first_lease = acquire_task_lease(&store, &first.id, &first_task, "agy", 300).unwrap();
        assert!(first_lease.allowed);
        assert_eq!(
            first_lease
                .lease
                .as_ref()
                .unwrap()
                .workspace_claim
                .as_ref()
                .unwrap()
                .binding_scope,
            "task"
        );
        let conflict = acquire_task_lease(&store, &second.id, &second_task, "codex", 300).unwrap();
        assert!(!conflict.allowed);
        assert_eq!(conflict.status, "lease_blocked_workspace_conflict");
        let conflict = conflict.workspace_conflict.unwrap();
        assert_eq!(conflict.held_by_workflow_id, first.id);
        assert_eq!(conflict.held_by_task_id, first_task);
        assert_eq!(conflict.held_by_executor, "agy");
    }

    #[test]
    fn distinct_task_worktrees_admit_parallel_leases_and_freeze_execution_cwd() {
        let temp = tempfile::tempdir().unwrap();
        let repository = temp.path().join("repository");
        initialize_repository(&repository);
        let store = FoundryStore::open(temp.path().join("foundry.sqlite")).unwrap();
        let workflow = crate::graph::create_workflow(parse_intent(
            "Run independent mutating agents in separate worktrees",
        ));
        let first_task = workflow.tasks[0].id.clone();
        let second_task = workflow.tasks[1].id.clone();
        store.save_workflow(&workflow).unwrap();

        let first_root = temp.path().join("agent-one");
        let second_root = temp.path().join("agent-two");
        let first_worktree = create_worktree(
            &store,
            WorktreeCreateOptions {
                repository: repository.clone(),
                path: first_root.clone(),
                branch: "foundry-test-agent-one".to_string(),
                start_point: Some("HEAD".to_string()),
                allow_repository_mutation: true,
                origin: "lease-test".to_string(),
            },
        )
        .unwrap();
        let second_worktree = create_worktree(
            &store,
            WorktreeCreateOptions {
                repository,
                path: second_root.clone(),
                branch: "foundry-test-agent-two".to_string(),
                start_point: Some("HEAD".to_string()),
                allow_repository_mutation: true,
                origin: "lease-test".to_string(),
            },
        )
        .unwrap();
        bind_worktree(
            &store,
            &first_worktree.worktree.id,
            &workflow.id,
            Some(&first_task),
            "lease-test",
        )
        .unwrap();
        bind_worktree(
            &store,
            &second_worktree.worktree.id,
            &workflow.id,
            Some(&second_task),
            "lease-test",
        )
        .unwrap();

        let first = acquire_task_lease(&store, &workflow.id, &first_task, "agy", 300).unwrap();
        let second = acquire_task_lease(&store, &workflow.id, &second_task, "codex", 300).unwrap();
        assert!(first.allowed && second.allowed);
        assert_ne!(
            first
                .lease
                .as_ref()
                .unwrap()
                .workspace_claim
                .as_ref()
                .unwrap()
                .worktree_id,
            second
                .lease
                .as_ref()
                .unwrap()
                .workspace_claim
                .as_ref()
                .unwrap()
                .worktree_id
        );
        let validated = validate_task_lease_for_execution(
            &store,
            &workflow.id,
            &first_task,
            "agy",
            &first_root,
        )
        .unwrap();
        assert_eq!(validated.lease_id, first.lease.unwrap().lease_id);
        let wrong_cwd = validate_task_lease_for_execution(
            &store,
            &workflow.id,
            &first_task,
            "agy",
            &second_root,
        )
        .unwrap_err();
        assert!(wrong_cwd
            .to_string()
            .contains("conflicts with leased worktree"));

        bind_worktree(
            &store,
            &second_worktree.worktree.id,
            &workflow.id,
            Some(&first_task),
            "lease-test-rebind",
        )
        .unwrap();
        let drift = validate_task_lease_for_execution(
            &store,
            &workflow.id,
            &first_task,
            "agy",
            &first_root,
        )
        .unwrap_err();
        assert!(drift.to_string().contains("worktree claim drifted"));
    }

    #[test]
    fn lease_status_check_and_insert_are_atomic_against_terminal_transition() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("atomic-task-lease.sqlite");
        let store = FoundryStore::open(&path).unwrap();
        let workflow = crate::graph::create_workflow(parse_intent(
            "Do not lease a task after its terminal transition commits",
        ));
        let workflow_id = workflow.id.clone();
        let task_id = workflow.tasks[0].id.clone();
        store.save_workflow(&workflow).unwrap();
        drop(store);

        let mut completed_workflow = workflow;
        completed_workflow.tasks[0].status = TaskStatus::Completed;
        completed_workflow.status = "completed".to_string();
        let completed_workflow_json = serde_json::to_string(&completed_workflow).unwrap();

        let blocker = Connection::open(&path).unwrap();
        blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
        blocker
            .execute(
                "UPDATE workflows SET status = 'completed', data_json = ?2 WHERE id = ?1",
                params![workflow_id, completed_workflow_json],
            )
            .unwrap();

        let (started_sender, started_receiver) = mpsc::channel();
        let (result_sender, result_receiver) = mpsc::channel();
        let worker_path = path.clone();
        let worker_workflow_id = workflow_id.clone();
        let worker_task_id = task_id.clone();
        let handle = thread::spawn(move || {
            let worker_store = FoundryStore::open(&worker_path).unwrap();
            started_sender.send(()).unwrap();
            result_sender
                .send(acquire_task_lease(
                    &worker_store,
                    &worker_workflow_id,
                    &worker_task_id,
                    "codex",
                    300,
                ))
                .unwrap();
        });

        started_receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        assert!(
            matches!(
                result_receiver.recv_timeout(Duration::from_millis(150)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ),
            "lease acquisition returned before the task transition committed"
        );
        blocker.execute_batch("COMMIT").unwrap();

        let report = result_receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        assert_eq!(report.status, "lease_blocked_task_status");
        assert!(!report.allowed);
        assert!(report.lease.is_none());
        handle.join().unwrap();

        let reopened = FoundryStore::open(&path).unwrap();
        assert!(reopened
            .load_task_lease(&workflow_id, &task_id)
            .unwrap()
            .is_none());
        assert_eq!(
            reopened
                .load_workflow(&workflow_id)
                .unwrap()
                .tasks
                .iter()
                .find(|task| task.id == task_id)
                .unwrap()
                .status,
            TaskStatus::Completed
        );
    }

    #[test]
    fn lease_policy_check_and_insert_are_atomic_against_membership_revocation() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("atomic-task-lease-policy.sqlite");
        let store = FoundryStore::open(&path).unwrap();
        let mut workflow = crate::graph::create_workflow(parse_intent(
            "Do not lease a task after tenant access is revoked",
        ));
        let workflow_id = workflow.id.clone();
        let task_id = workflow.tasks[0].id.clone();
        let context = &mut workflow.intent.operating_context;
        context.tenant_policy_mode = "enforce".to_string();
        context.organization.id = "org-lease-policy".to_string();
        context.brand.id = "brand-lease-policy".to_string();
        context.product.id = "product-lease-policy".to_string();
        context.user.id = "user-lease-policy".to_string();
        context.channel.id = "channel-lease-policy".to_string();
        store.save_workflow(&workflow).unwrap();

        let context = &workflow.intent.operating_context;
        let membership_data = serde_json::json!({"source": "lease-policy-test"});
        store
            .save_identity_membership(IdentityMembershipWrite {
                subject_scope: &context.user.scope,
                subject_id: &context.user.id,
                organization_id: &context.organization.id,
                brand_id: &context.brand.id,
                product_id: &context.product.id,
                role: "operator",
                status: "active",
                source: "lease-policy-test",
                data: &membership_data,
            })
            .unwrap();
        let subject_scope = context.user.scope.clone();
        let subject_id = context.user.id.clone();
        let organization_id = context.organization.id.clone();
        let brand_id = context.brand.id.clone();
        let product_id = context.product.id.clone();
        drop(store);

        let blocker = Connection::open(&path).unwrap();
        blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
        let changed = blocker
            .execute(
                r#"
                UPDATE identity_memberships
                SET status = 'revoked'
                WHERE subject_scope = ?1
                  AND subject_id = ?2
                  AND organization_id = ?3
                  AND brand_id = ?4
                  AND product_id = ?5
                "#,
                params![
                    subject_scope,
                    subject_id,
                    organization_id,
                    brand_id,
                    product_id
                ],
            )
            .unwrap();
        assert_eq!(changed, 1);

        let (started_sender, started_receiver) = mpsc::channel();
        let (result_sender, result_receiver) = mpsc::channel();
        let worker_path = path.clone();
        let worker_workflow_id = workflow_id.clone();
        let worker_task_id = task_id.clone();
        let handle = thread::spawn(move || {
            let worker_store = FoundryStore::open(&worker_path).unwrap();
            started_sender.send(()).unwrap();
            result_sender
                .send(acquire_task_lease(
                    &worker_store,
                    &worker_workflow_id,
                    &worker_task_id,
                    "codex",
                    300,
                ))
                .unwrap();
        });

        started_receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        assert!(
            matches!(
                result_receiver.recv_timeout(Duration::from_millis(150)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ),
            "lease acquisition returned before membership revocation committed"
        );

        blocker.execute_batch("COMMIT").unwrap();
        let error = result_receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap_err();
        assert!(
            format!("{error:#}").contains("no active membership"),
            "unexpected policy error: {error:#}"
        );
        handle.join().unwrap();

        let reopened = FoundryStore::open(&path).unwrap();
        assert!(reopened
            .load_task_lease(&workflow_id, &task_id)
            .unwrap()
            .is_none());
    }

    #[test]
    fn committed_lease_is_returned_when_audit_event_fails() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("task-lease-audit-failure.sqlite");
        let store = FoundryStore::open(&path).unwrap();
        let workflow = crate::graph::create_workflow(parse_intent(
            "Return a committed lease when its audit event fails",
        ));
        let workflow_id = workflow.id.clone();
        let task_id = workflow.tasks[0].id.clone();
        store.save_workflow(&workflow).unwrap();

        let audit_blocker = Connection::open(&path).unwrap();
        audit_blocker
            .execute_batch(
                r#"
                CREATE TRIGGER reject_task_lease_acquired_audit
                BEFORE INSERT ON events
                WHEN NEW.kind = 'task_lease_acquired'
                BEGIN
                    SELECT RAISE(ABORT, 'task lease audit blocked');
                END;
                "#,
            )
            .unwrap();
        drop(audit_blocker);

        let report = acquire_task_lease(&store, &workflow_id, &task_id, "codex", 300).unwrap();
        assert_eq!(report.status, "lease_acquired");
        assert!(report.allowed);
        assert_eq!(report.audit_event_status, "failed");
        assert!(report
            .audit_event_error
            .as_deref()
            .is_some_and(|error| error.contains("task lease audit blocked")));
        let lease = report.lease.as_ref().unwrap();
        let stored_lease = store
            .load_task_lease(&workflow_id, &task_id)
            .unwrap()
            .unwrap();
        assert_eq!(stored_lease["lease_id"], lease.lease_id);
    }
}
