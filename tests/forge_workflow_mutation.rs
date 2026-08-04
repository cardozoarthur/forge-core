use assert_cmd::Command;
use forge_core::addon::{builtin_addon_catalog, resolve_goal_capabilities, CAP_PARALLEL_TEAMWORK};
use forge_core::event::{ingest_inbound_event, route_inbound_event, InboundEventIngestInput};
use forge_core::graph::{self, CoreOrchestrationSpec, ExecutorKind, TaskStatus, Workflow};
use forge_core::intent::parse_intent;
use forge_core::lease::acquire_task_lease;
use forge_core::mcp::{call_mcp_tool, mcp_tools_manifest};
use forge_core::storage::ForgeStore;
use forge_core::validation::validate_workflow_structure;
use forge_core::workflow::{
    add_workflow_task, add_workflow_task_dependency, clear_workflow_task_impediment,
    remove_workflow_task_dependency, set_workflow_task_impediment, set_workflow_task_priority,
    update_workflow_goal_with_expected_revision, update_workflow_task_with_expected_revision,
    WorkflowTaskAddInput, WorkflowTaskDependencyInput, WorkflowTaskImpedimentClearInput,
    WorkflowTaskImpedimentInput, WorkflowTaskPriorityInput, WorkflowTaskUpdateInput,
};
use rusqlite::Connection;
use serde_json::{json, Value};
use tempfile::tempdir;

fn workflow_task(id: &str, title: &str, dependencies: &[&str]) -> graph::AtomicTask {
    graph::task(
        id,
        title,
        dependencies,
        &[],
        Vec::new(),
        "validated output",
        (ExecutorKind::Ai, 0.0),
    )
}

fn seed_workflow(store: &ForgeStore, status: &str) -> Workflow {
    let mut workflow =
        graph::create_workflow(parse_intent("Coordinate a dynamic parallel delivery"));
    workflow.status = status.to_string();
    workflow.tasks = vec![
        workflow_task("task-a", "Prepare input", &[]),
        workflow_task("task-b", "Validate input", &["task-a"]),
    ];
    store.save_workflow(&workflow).unwrap();
    workflow
}

#[test]
fn core_orchestration_defaults_survive_legacy_deserialization() {
    let workflow = graph::create_workflow(parse_intent("Deserialize a legacy workflow"));
    let mut legacy_value = serde_json::to_value(&workflow).unwrap();
    legacy_value
        .as_object_mut()
        .unwrap()
        .remove("core_orchestration");

    let restored: Workflow = serde_json::from_value(legacy_value).unwrap();

    assert_eq!(
        restored.core_orchestration,
        CoreOrchestrationSpec::default()
    );
    assert_eq!(restored.core_orchestration.authority, "forge_core");
    assert!(restored.core_orchestration.dynamic_workflow);
    assert!(restored.core_orchestration.parallel_task_handoffs);
    assert!(restored.core_orchestration.parallel_agent_nodes);
    assert_eq!(restored.core_orchestration.max_parallel_tasks, 4);
    assert_eq!(restored.core_orchestration.max_parallel_agents_per_node, 4);

    let mut partial_value = serde_json::to_value(&workflow).unwrap();
    partial_value["core_orchestration"]
        .as_object_mut()
        .unwrap()
        .remove("fan_out_fan_in");
    partial_value["tasks"][0]
        .as_object_mut()
        .unwrap()
        .remove("active_impediments");
    let restored_partial: Workflow = serde_json::from_value(partial_value).unwrap();
    assert!(restored_partial.core_orchestration.fan_out_fan_in);
    assert!(restored_partial.tasks[0].active_impediments.is_empty());
}

#[test]
fn parallel_teamwork_is_a_universal_core_capability() {
    let catalog = builtin_addon_catalog();
    let core = catalog
        .addons
        .iter()
        .find(|addon| addon.id == "forge.core.kernel")
        .unwrap();
    assert!(core
        .capabilities
        .iter()
        .any(|capability| capability.id == CAP_PARALLEL_TEAMWORK));

    let resolution = resolve_goal_capabilities(
        "Produce one small deterministic output from a single input",
        &catalog,
    );
    let capability = resolution
        .required_capabilities
        .iter()
        .find(|capability| capability.id == CAP_PARALLEL_TEAMWORK)
        .unwrap();
    assert!(capability.required);
    assert_eq!(capability.source_addon, "forge.core.kernel");
    assert!(capability.matched_keywords.is_empty());
}

#[test]
fn structural_validation_rejects_corrupt_policy_duplicate_edges_and_cycles() {
    let mut workflow =
        graph::create_workflow(parse_intent("Reject an invalid dynamic workflow graph"));
    workflow.core_orchestration.authority = "external_scheduler".to_string();
    workflow.core_orchestration.dynamic_workflow = false;
    workflow.core_orchestration.max_parallel_tasks = 0;
    workflow.revisions.push(graph::WorkflowRevision {
        revision: 2,
        origin: "corrupt-fixture".to_string(),
        change_type: "invalid_revision".to_string(),
        summary: "Revision one is missing".to_string(),
        created_at: chrono::Utc::now(),
    });
    workflow.tasks = vec![
        workflow_task("task-a", "A", &["task-b", "task-b"]),
        workflow_task("task-b", "B", &["task-a"]),
        workflow_task("task-a", "Duplicate A", &["missing-task"]),
        workflow_task("task-self", "Self edge", &["task-self"]),
    ];

    let failures = validate_workflow_structure(&workflow);
    let messages = failures
        .iter()
        .map(|failure| failure.message.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(messages.contains("authority must be forge_core"));
    assert!(messages.contains("dynamic_workflow must be enabled"));
    assert!(messages.contains("max_parallel_tasks"));
    assert!(messages.contains("expected 1, found 2"));
    assert!(messages.contains("duplicate task id task-a"));
    assert!(messages.contains("duplicate dependency task-b"));
    assert!(messages.contains("missing dependency missing-task"));
    assert!(messages.contains("task task-self cannot depend on itself"));
    assert!(messages.contains("dependency cycle"));
}

#[test]
fn identical_goal_update_is_a_true_noop() {
    let temp = tempdir().unwrap();
    let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();
    let workflow = seed_workflow(&store, "running");
    let events_before = store.load_workflow_events(&workflow.id).unwrap().len();

    let report = update_workflow_goal_with_expected_revision(
        &store,
        &workflow.id,
        &workflow.goal,
        "test",
        Some(0),
    )
    .unwrap();

    assert_eq!(report.status, "workflow_goal_unchanged");
    assert_eq!(report.revision, 0);
    assert!(store
        .load_workflow(&workflow.id)
        .unwrap()
        .revisions
        .is_empty());
    assert_eq!(
        store.load_workflow_events(&workflow.id).unwrap().len(),
        events_before
    );
}

#[test]
fn add_task_noop_requires_the_complete_canonical_definition() {
    let temp = tempdir().unwrap();
    let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();
    let workflow = seed_workflow(&store, "running");

    let error = add_workflow_task(
        &store,
        &workflow.id,
        WorkflowTaskAddInput {
            task_id: Some("task-a".to_string()),
            description: "Prepare input".to_string(),
            priority: "medium".to_string(),
            origin: "test".to_string(),
            expected_revision: Some(0),
        },
    )
    .unwrap_err();

    assert!(error.to_string().contains("different definition"));
    assert!(store
        .load_workflow(&workflow.id)
        .unwrap()
        .revisions
        .is_empty());
    assert!(store.load_workflow_events(&workflow.id).unwrap().is_empty());
}

#[test]
fn running_workflow_supports_atomic_revisioned_mutation_sequence() {
    let temp = tempdir().unwrap();
    let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();
    let workflow = seed_workflow(&store, "running");

    let added = add_workflow_task(
        &store,
        &workflow.id,
        WorkflowTaskAddInput {
            task_id: Some("task-c".to_string()),
            description: "Implement dynamic branch".to_string(),
            priority: "medium".to_string(),
            origin: "test".to_string(),
            expected_revision: Some(0),
        },
    )
    .unwrap();
    assert_eq!(added.revision, 1);

    let updated = update_workflow_task_with_expected_revision(
        &store,
        &workflow.id,
        WorkflowTaskUpdateInput {
            task_id: "task-c",
            title: Some("Implement parallel branch"),
            goal: Some("Produce an independently validated branch"),
            expected_output: Some("branch receipt"),
            origin: "test",
        },
        Some(1),
    )
    .unwrap();
    assert_eq!(updated.revision, 2);

    let prioritized = set_workflow_task_priority(
        &store,
        &workflow.id,
        WorkflowTaskPriorityInput {
            task_id: "task-c".to_string(),
            priority: "p1".to_string(),
            origin: "test".to_string(),
            expected_revision: Some(2),
        },
    )
    .unwrap();
    assert_eq!(prioritized.priority.as_deref(), Some("high"));
    assert_eq!(prioritized.revision, 3);

    let dependency = add_workflow_task_dependency(
        &store,
        &workflow.id,
        WorkflowTaskDependencyInput {
            task_id: "task-c".to_string(),
            dependency_task_id: "task-a".to_string(),
            origin: "test".to_string(),
            expected_revision: Some(3),
        },
    )
    .unwrap();
    assert_eq!(dependency.revision, 4);

    let impediment = set_workflow_task_impediment(
        &store,
        &workflow.id,
        WorkflowTaskImpedimentInput {
            task_id: "task-c".to_string(),
            reason: "Await operator confirmation".to_string(),
            kind: "manual".to_string(),
            origin: "test".to_string(),
            expected_revision: Some(4),
        },
    )
    .unwrap();
    assert_eq!(impediment.revision, 5);

    let cleared = clear_workflow_task_impediment(
        &store,
        &workflow.id,
        WorkflowTaskImpedimentClearInput {
            task_id: "task-c".to_string(),
            impediment_id: None,
            origin: "test".to_string(),
            expected_revision: Some(5),
        },
    )
    .unwrap();
    assert_eq!(cleared.revision, 6);
    assert_eq!(cleared.cleared_impediment_ids.len(), 1);

    let removed = remove_workflow_task_dependency(
        &store,
        &workflow.id,
        WorkflowTaskDependencyInput {
            task_id: "task-c".to_string(),
            dependency_task_id: "task-a".to_string(),
            origin: "test".to_string(),
            expected_revision: Some(6),
        },
    )
    .unwrap();
    assert_eq!(removed.revision, 7);

    let events_before_noop = store.load_workflow_events(&workflow.id).unwrap().len();
    let unchanged = set_workflow_task_priority(
        &store,
        &workflow.id,
        WorkflowTaskPriorityInput {
            task_id: "task-c".to_string(),
            priority: "high".to_string(),
            origin: "test".to_string(),
            expected_revision: Some(7),
        },
    )
    .unwrap();
    assert!(!unchanged.changed);
    assert_eq!(unchanged.revision, 7);
    assert_eq!(
        store.load_workflow_events(&workflow.id).unwrap().len(),
        events_before_noop
    );

    let stale_error = set_workflow_task_priority(
        &store,
        &workflow.id,
        WorkflowTaskPriorityInput {
            task_id: "task-c".to_string(),
            priority: "low".to_string(),
            origin: "test".to_string(),
            expected_revision: Some(6),
        },
    )
    .unwrap_err();
    assert!(stale_error.to_string().contains("revision"));
    assert_eq!(
        store.load_workflow_events(&workflow.id).unwrap().len(),
        events_before_noop
    );

    let stored = store.load_workflow(&workflow.id).unwrap();
    let task = stored
        .tasks
        .iter()
        .find(|task| task.id == "task-c")
        .unwrap();
    assert_eq!(task.title, "Implement parallel branch");
    assert_eq!(task.work_item.priority, "high");
    assert!(task.dependencies.is_empty());
    assert!(task.active_impediments.is_empty());
    assert_eq!(task.status, TaskStatus::Pending);
    assert_eq!(stored.revisions.len(), 7);
}

#[test]
fn clear_all_only_clears_manual_impediments_and_kind_is_closed() {
    let temp = tempdir().unwrap();
    let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();
    let workflow = seed_workflow(&store, "pending");

    let invalid = set_workflow_task_impediment(
        &store,
        &workflow.id,
        WorkflowTaskImpedimentInput {
            task_id: "task-a".to_string(),
            reason: "Unknown source".to_string(),
            kind: "arbitrary".to_string(),
            origin: "test".to_string(),
            expected_revision: Some(0),
        },
    )
    .unwrap_err();
    assert!(invalid
        .to_string()
        .contains("manual, resource, authorization or policy"));

    let resource = set_workflow_task_impediment(
        &store,
        &workflow.id,
        WorkflowTaskImpedimentInput {
            task_id: "task-a".to_string(),
            reason: "Memory pressure".to_string(),
            kind: "RESOURCE".to_string(),
            origin: "resource_gate".to_string(),
            expected_revision: Some(0),
        },
    )
    .unwrap();
    let resource_id = resource.impediment.unwrap().id;

    set_workflow_task_impediment(
        &store,
        &workflow.id,
        WorkflowTaskImpedimentInput {
            task_id: "task-a".to_string(),
            reason: "Await operator".to_string(),
            kind: "manual".to_string(),
            origin: "test".to_string(),
            expected_revision: Some(1),
        },
    )
    .unwrap();

    let clear_manual = clear_workflow_task_impediment(
        &store,
        &workflow.id,
        WorkflowTaskImpedimentClearInput {
            task_id: "task-a".to_string(),
            impediment_id: None,
            origin: "test".to_string(),
            expected_revision: Some(2),
        },
    )
    .unwrap();
    assert_eq!(clear_manual.cleared_impediment_ids.len(), 1);

    let still_blocked = store.load_workflow(&workflow.id).unwrap();
    let task = still_blocked
        .tasks
        .iter()
        .find(|task| task.id == "task-a")
        .unwrap();
    assert_eq!(task.status, TaskStatus::Blocked);
    assert_eq!(task.active_impediments.len(), 1);
    assert_eq!(task.active_impediments[0].kind, "resource");
    assert_eq!(task.active_impediments[0].origin, "resource_gate");

    clear_workflow_task_impediment(
        &store,
        &workflow.id,
        WorkflowTaskImpedimentClearInput {
            task_id: "task-a".to_string(),
            impediment_id: Some(resource_id),
            origin: "test".to_string(),
            expected_revision: Some(3),
        },
    )
    .unwrap();
    let unblocked = store.load_workflow(&workflow.id).unwrap();
    let task = unblocked
        .tasks
        .iter()
        .find(|task| task.id == "task-a")
        .unwrap();
    assert_eq!(task.status, TaskStatus::Pending);
    assert!(task.active_impediments.is_empty());
}

#[test]
fn identical_impediments_from_distinct_authorities_remain_independent() {
    let temp = tempdir().unwrap();
    let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();
    let workflow = seed_workflow(&store, "running");

    let first = set_workflow_task_impediment(
        &store,
        &workflow.id,
        WorkflowTaskImpedimentInput {
            task_id: "task-a".to_string(),
            reason: "Shared safety condition".to_string(),
            kind: "policy".to_string(),
            origin: "policy-engine-a".to_string(),
            expected_revision: Some(0),
        },
    )
    .unwrap();
    let second = set_workflow_task_impediment(
        &store,
        &workflow.id,
        WorkflowTaskImpedimentInput {
            task_id: "task-a".to_string(),
            reason: "Shared safety condition".to_string(),
            kind: "policy".to_string(),
            origin: "policy-engine-b".to_string(),
            expected_revision: Some(1),
        },
    )
    .unwrap();

    assert!(first.changed);
    assert!(second.changed);
    assert_ne!(first.impediment.unwrap().id, second.impediment.unwrap().id);
    let stored = store.load_workflow(&workflow.id).unwrap();
    let task = stored
        .tasks
        .iter()
        .find(|task| task.id == "task-a")
        .unwrap();
    assert_eq!(task.active_impediments.len(), 2);
    assert_eq!(stored.revisions.len(), 2);
}

#[test]
fn mutation_rolls_back_when_event_insert_fails() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let store = ForgeStore::open(&store_path).unwrap();
    let workflow = seed_workflow(&store, "running");
    let connection = Connection::open(&store_path).unwrap();
    connection
        .execute_batch(
            r#"
            CREATE TRIGGER reject_dynamic_task_event
            BEFORE INSERT ON events
            WHEN NEW.kind = 'workflow_task_added'
            BEGIN
                SELECT RAISE(ABORT, 'test event rejection');
            END;
            "#,
        )
        .unwrap();

    let error = add_workflow_task(
        &store,
        &workflow.id,
        WorkflowTaskAddInput {
            task_id: Some("task-rollback".to_string()),
            description: "Must roll back".to_string(),
            priority: "medium".to_string(),
            origin: "test".to_string(),
            expected_revision: Some(0),
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("test event rejection"));

    let stored = store.load_workflow(&workflow.id).unwrap();
    assert!(stored.tasks.iter().all(|task| task.id != "task-rollback"));
    assert!(stored.revisions.is_empty());
    assert!(store.load_workflow_events(&workflow.id).unwrap().is_empty());
}

#[test]
fn dependency_mutation_rejects_self_edges_missing_tasks_and_cycles_without_revision() {
    let temp = tempdir().unwrap();
    let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();
    let workflow = seed_workflow(&store, "running");

    let duplicate = add_workflow_task_dependency(
        &store,
        &workflow.id,
        WorkflowTaskDependencyInput {
            task_id: "task-b".to_string(),
            dependency_task_id: "task-a".to_string(),
            origin: "test".to_string(),
            expected_revision: Some(0),
        },
    )
    .unwrap();
    assert!(!duplicate.changed);
    assert_eq!(duplicate.revision, 0);

    for (task_id, dependency_task_id, expected_message) in [
        ("task-a", "task-a", "cannot depend on itself"),
        ("task-a", "missing-task", "not found"),
        ("task-a", "task-b", "dependency cycle"),
    ] {
        let error = add_workflow_task_dependency(
            &store,
            &workflow.id,
            WorkflowTaskDependencyInput {
                task_id: task_id.to_string(),
                dependency_task_id: dependency_task_id.to_string(),
                origin: "test".to_string(),
                expected_revision: Some(0),
            },
        )
        .unwrap_err();
        assert!(
            error.to_string().contains(expected_message),
            "unexpected dependency error: {error:#}"
        );
    }

    let stored = store.load_workflow(&workflow.id).unwrap();
    assert!(stored.revisions.is_empty());
    assert!(store.load_workflow_events(&workflow.id).unwrap().is_empty());
    assert!(stored
        .tasks
        .iter()
        .find(|task| task.id == "task-a")
        .unwrap()
        .dependencies
        .is_empty());
}

#[test]
fn running_tasks_and_leased_tasks_reject_live_mutation() {
    let temp = tempdir().unwrap();
    let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();
    let workflow = seed_workflow(&store, "running");

    let mut running = store.load_workflow(&workflow.id).unwrap();
    running
        .tasks
        .iter_mut()
        .find(|task| task.id == "task-a")
        .unwrap()
        .status = TaskStatus::Running;
    store.save_workflow(&running).unwrap();
    let running_error = set_workflow_task_priority(
        &store,
        &workflow.id,
        WorkflowTaskPriorityInput {
            task_id: "task-a".to_string(),
            priority: "high".to_string(),
            origin: "test".to_string(),
            expected_revision: Some(0),
        },
    )
    .unwrap_err();
    assert!(running_error.to_string().contains("running"));

    running
        .tasks
        .iter_mut()
        .find(|task| task.id == "task-a")
        .unwrap()
        .status = TaskStatus::Pending;
    store.save_workflow(&running).unwrap();
    let lease = acquire_task_lease(&store, &workflow.id, "task-a", "codex", 60).unwrap();
    assert!(lease.allowed);

    let lease_error = set_workflow_task_priority(
        &store,
        &workflow.id,
        WorkflowTaskPriorityInput {
            task_id: "task-a".to_string(),
            priority: "high".to_string(),
            origin: "test".to_string(),
            expected_revision: Some(0),
        },
    )
    .unwrap_err();
    assert!(lease_error.to_string().contains("lease"));
}

#[test]
fn terminal_workflows_reject_goal_priority_and_impediment_mutations() {
    let temp = tempdir().unwrap();
    let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();
    let workflow = seed_workflow(&store, "cancelled");

    let goal_error = update_workflow_goal_with_expected_revision(
        &store,
        &workflow.id,
        "A goal that must not reopen a cancelled workflow",
        "test",
        Some(0),
    )
    .unwrap_err();
    assert!(goal_error.to_string().contains("terminal workflow"));

    let priority_error = set_workflow_task_priority(
        &store,
        &workflow.id,
        WorkflowTaskPriorityInput {
            task_id: "task-a".to_string(),
            priority: "high".to_string(),
            origin: "test".to_string(),
            expected_revision: Some(0),
        },
    )
    .unwrap_err();
    assert!(priority_error.to_string().contains("terminal workflow"));

    let impediment_error = set_workflow_task_impediment(
        &store,
        &workflow.id,
        WorkflowTaskImpedimentInput {
            task_id: "task-a".to_string(),
            reason: "Must not alter terminal state".to_string(),
            kind: "manual".to_string(),
            origin: "test".to_string(),
            expected_revision: Some(0),
        },
    )
    .unwrap_err();
    assert!(impediment_error.to_string().contains("terminal workflow"));

    assert!(store
        .load_workflow(&workflow.id)
        .unwrap()
        .revisions
        .is_empty());
    assert!(store.load_workflow_events(&workflow.id).unwrap().is_empty());
}

#[test]
fn corrupted_policy_and_mission_binding_reject_core_graph_mutation() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let store = ForgeStore::open(&store_path).unwrap();
    let workflow = seed_workflow(&store, "pending");

    let mut corrupted = store.load_workflow(&workflow.id).unwrap();
    corrupted.core_orchestration.mutations_revisioned = false;
    store.save_workflow(&corrupted).unwrap();
    let policy_error = add_workflow_task(
        &store,
        &workflow.id,
        WorkflowTaskAddInput {
            task_id: Some("task-policy".to_string()),
            description: "Must not bypass Core policy".to_string(),
            priority: "medium".to_string(),
            origin: "test".to_string(),
            expected_revision: Some(0),
        },
    )
    .unwrap_err();
    assert!(policy_error.to_string().contains("mutations_revisioned"));

    corrupted.core_orchestration = CoreOrchestrationSpec::default();
    store.save_workflow(&corrupted).unwrap();
    Connection::open(&store_path)
        .unwrap()
        .execute(
            r#"
            INSERT INTO forge_missions (
                id, workflow_id, squad_id, squad_version, status,
                data_json, created_at, updated_at
            ) VALUES (?1, ?2, 'test-squad', '1', 'running', '{}', ?3, ?3)
            "#,
            rusqlite::params![
                "mission-workflow-mutation-test",
                &workflow.id,
                chrono::Utc::now().to_rfc3339()
            ],
        )
        .unwrap();
    let mission_error = add_workflow_task(
        &store,
        &workflow.id,
        WorkflowTaskAddInput {
            task_id: Some("task-mission".to_string()),
            description: "Must use mission adapter".to_string(),
            priority: "medium".to_string(),
            origin: "test".to_string(),
            expected_revision: Some(0),
        },
    )
    .unwrap_err();
    assert!(mission_error.to_string().contains("mission"));

    let mission_goal_error = update_workflow_goal_with_expected_revision(
        &store,
        &workflow.id,
        "Must also use the mission-aware adapter",
        "test",
        Some(0),
    )
    .unwrap_err();
    assert!(mission_goal_error.to_string().contains("mission"));
}

#[test]
fn cli_and_mcp_expose_the_same_core_mutations() {
    let temp = tempdir().unwrap();
    let store_path = temp.path().join("forge.sqlite");
    let store = ForgeStore::open(&store_path).unwrap();
    let workflow = seed_workflow(&store, "running");

    let expected_tools = [
        "forge.workflow.add_task",
        "forge.workflow.update_task",
        "forge.workflow.set_priority",
        "forge.workflow.add_dependency",
        "forge.workflow.remove_dependency",
        "forge.workflow.set_impediment",
        "forge.workflow.clear_impediment",
    ];
    let manifest = mcp_tools_manifest();
    for name in expected_tools {
        let tool = manifest
            .tools
            .iter()
            .find(|tool| tool.name == name)
            .unwrap_or_else(|| panic!("missing MCP tool {name}"));
        assert!(tool.mutates_workflow);
    }

    let mcp = call_mcp_tool(
        &store,
        "forge.workflow.add_task",
        json!({
            "workflow_id": &workflow.id,
            "task_id": "task-mcp",
            "description": "Add task through MCP",
            "priority": "high",
            "origin": "mcp",
            "expected_revision": 0
        }),
    )
    .unwrap();
    assert_eq!(mcp.status, "ok");
    assert_eq!(mcp.result["schema_version"], "forge.workflow_mutation.v1");
    assert_eq!(mcp.result["revision"], 1);

    Command::cargo_bin("forge")
        .unwrap()
        .args([
            "--store",
            store_path.to_str().unwrap(),
            "workflow",
            "add-task",
            "--workflow",
            &workflow.id,
            "--task-id",
            "task-cli",
            "--description",
            "Add task through CLI",
            "--priority",
            "medium",
            "--expected-revision",
            "1",
            "--output",
            "json",
        ])
        .assert()
        .success();

    let stored = store.load_workflow(&workflow.id).unwrap();
    assert!(stored.tasks.iter().any(|task| task.id == "task-mcp"));
    assert!(stored.tasks.iter().any(|task| task.id == "task-cli"));
    assert_eq!(stored.revisions.len(), 2);

    let serialized = serde_json::to_value(manifest).unwrap();
    for name in expected_tools {
        let tool = serialized["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == Value::String(name.to_string()))
            .unwrap();
        assert_eq!(tool["mutates_workflow"], true);
        assert!(tool["input_schema"]["properties"]
            .get("expected_revision")
            .is_some());
    }
}

#[test]
fn modify_workflow_events_route_dynamic_mutations_and_keep_legacy_goal_updates() {
    let temp = tempdir().unwrap();
    let store = ForgeStore::open(temp.path().join("forge.sqlite")).unwrap();
    let workflow = seed_workflow(&store, "running");

    let add_event = ingest_inbound_event(
        &store,
        InboundEventIngestInput {
            origin: "api".to_string(),
            action: "modify_workflow".to_string(),
            data: json!({
                "workflow_id": &workflow.id,
                "mutation": "add_task",
                "task_id": "task-event",
                "description": "Add a branch from an inbound event",
                "priority": "high",
                "expected_revision": 0,
                "identity": {
                    "scope": "api",
                    "id": "workflow-mutation-test"
                }
            }),
        },
    )
    .unwrap();
    let routed = route_inbound_event(&store, &add_event.event.id, temp.path()).unwrap();
    assert!(routed.route_decision.contains("mutation add_task"));
    assert!(routed.route_decision.contains("revision 1"));
    assert_eq!(
        routed.route_result.as_ref().unwrap()["mutation"],
        "add_task"
    );

    let goal_event = ingest_inbound_event(
        &store,
        InboundEventIngestInput {
            origin: "api".to_string(),
            action: "modify_workflow".to_string(),
            data: json!({
                "workflow_id": &workflow.id,
                "goal": "Reoriented by a legacy goal-only event",
                "expected_revision": 1,
                "identity": {
                    "scope": "api",
                    "id": "workflow-mutation-test"
                }
            }),
        },
    )
    .unwrap();
    let routed = route_inbound_event(&store, &goal_event.event.id, temp.path()).unwrap();
    assert!(routed.route_decision.contains("mutation update_goal"));
    assert!(routed.route_decision.contains("revision 2"));
    assert_eq!(
        store.load_workflow(&workflow.id).unwrap().goal,
        "Reoriented by a legacy goal-only event"
    );
}
