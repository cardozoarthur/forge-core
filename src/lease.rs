use crate::graph::{TaskStatus, Workflow};
use crate::identity::{ensure_workflow_policy, evaluate_tenant_policy_for_action};
use crate::storage::{ForgeStore, TaskLeaseWrite};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskLease {
    pub lease_id: String,
    pub workflow_id: String,
    pub task_id: String,
    pub executor: String,
    pub acquired_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
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
    store: &ForgeStore,
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
    let (task_status, lease, saved) = store.with_transaction(|| {
        let workflow = store.load_workflow(workflow_id)?;
        ensure_workflow_snapshot_policy(store, &workflow, "task lease acquire")?;
        let task_status = task_status_from_workflow(&workflow, task_id)?;
        if task_status != TaskStatus::Pending {
            return Ok((task_status, None, false));
        }

        let acquired_at = Utc::now();
        let expires_at = acquired_at + Duration::seconds(ttl_seconds);
        let lease = TaskLease {
            lease_id: format!("lease_{}", Uuid::new_v4().to_string().replace('-', "")),
            workflow_id: workflow_id.to_string(),
            task_id: task_id.to_string(),
            executor: executor.to_string(),
            acquired_at,
            expires_at,
        };
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
        Ok((task_status, Some(lease), saved))
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

    if saved {
        let report = TaskLeaseAcquireReport {
            status: "lease_acquired".to_string(),
            allowed: true,
            workflow_id: workflow_id.to_string(),
            task_id: task_id.to_string(),
            executor: executor.to_string(),
            lease: Some(lease),
            current_lease: None,
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

pub fn release_task_lease(
    store: &ForgeStore,
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

fn ensure_task_exists(store: &ForgeStore, workflow_id: &str, task_id: &str) -> Result<()> {
    load_task_status(store, workflow_id, task_id).map(|_| ())
}

fn load_task_status(store: &ForgeStore, workflow_id: &str, task_id: &str) -> Result<TaskStatus> {
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
    store: &ForgeStore,
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
    store: &ForgeStore,
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
    store: &ForgeStore,
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
    use super::acquire_task_lease;
    use crate::graph::TaskStatus;
    use crate::intent::parse_intent;
    use crate::storage::{ForgeStore, IdentityMembershipWrite};
    use rusqlite::{params, Connection};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn lease_status_check_and_insert_are_atomic_against_terminal_transition() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("atomic-task-lease.sqlite");
        let store = ForgeStore::open(&path).unwrap();
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
            let worker_store = ForgeStore::open(&worker_path).unwrap();
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

        let reopened = ForgeStore::open(&path).unwrap();
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
        let store = ForgeStore::open(&path).unwrap();
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
            let worker_store = ForgeStore::open(&worker_path).unwrap();
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

        let reopened = ForgeStore::open(&path).unwrap();
        assert!(reopened
            .load_task_lease(&workflow_id, &task_id)
            .unwrap()
            .is_none());
    }

    #[test]
    fn committed_lease_is_returned_when_audit_event_fails() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("task-lease-audit-failure.sqlite");
        let store = ForgeStore::open(&path).unwrap();
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
