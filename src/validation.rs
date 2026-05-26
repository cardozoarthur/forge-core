use crate::graph::{
    AtomicTask, ChildSubflowRef, ExecutorKind, PersonaRoutingSpec, TaskStatus, Workflow,
};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct FailedRule {
    pub task_id: String,
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationReport {
    pub workflow_id: String,
    pub status: String,
    pub promotable: bool,
    pub failed_rules: Vec<FailedRule>,
    pub rework_tasks: Vec<ReworkTask>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReworkTask {
    pub task_id: String,
    pub goal: String,
    pub reason: String,
}

pub fn validate_workflow(workflow: &Workflow) -> ValidationReport {
    let mut failed_rules = Vec::new();
    let mut rework_tasks = Vec::new();

    for task in &workflow.tasks {
        for dependency in &task.dependencies {
            if !workflow
                .tasks
                .iter()
                .any(|candidate| &candidate.id == dependency)
            {
                failed_rules.push(FailedRule {
                    task_id: task.id.clone(),
                    kind: "graph".to_string(),
                    message: format!("missing dependency {dependency}"),
                });
            }
        }
        if task.status != TaskStatus::Completed {
            failed_rules.push(FailedRule {
                task_id: task.id.clone(),
                kind: "task_status".to_string(),
                message: format!("task {} is {:?}", task.id, task.status),
            });
        }
        if !task.work_item.goal_validation.definitively_ready {
            failed_rules.push(FailedRule {
                task_id: task.id.clone(),
                kind: "goal_readiness".to_string(),
                message: format!("task {} goal is not definitively ready", task.id),
            });
            rework_tasks.push(ReworkTask {
                task_id: task.id.clone(),
                goal: task.goal.clone(),
                reason: "goal evidence is missing or not definitively ready; return to work"
                    .to_string(),
            });
        }
        if let Some(persona) = &task.persona {
            let violations = persona_routing_violations(persona);
            if !violations.is_empty() {
                failed_rules.push(FailedRule {
                    task_id: task.id.clone(),
                    kind: "persona_routing".to_string(),
                    message: format!(
                        "task {} persona routing is not validation-ready: {}",
                        task.id,
                        violations.join("; ")
                    ),
                });
                rework_tasks.push(ReworkTask {
                    task_id: task.id.clone(),
                    goal: task.goal.clone(),
                    reason:
                        "persona routing contract is incomplete or not auditable; return to work"
                            .to_string(),
                });
            }
        }
        let exec_violations = execution_policy_violations(task);
        if !exec_violations.is_empty() {
            failed_rules.push(FailedRule {
                task_id: task.id.clone(),
                kind: "execution_policy".to_string(),
                message: format!(
                    "task {} execution policy is inconsistent: {}",
                    task.id,
                    exec_violations.join("; ")
                ),
            });
            rework_tasks.push(ReworkTask {
                task_id: task.id.clone(),
                goal: task.goal.clone(),
                reason:
                    "execution policy is inconsistent; fix policy configuration before promotion"
                        .to_string(),
            });
        }

        if task.version == 0 {
            failed_rules.push(FailedRule {
                task_id: task.id.clone(),
                kind: "version_boundary".to_string(),
                message: format!("task {} version is 0; must be >= 1", task.id),
            });
            rework_tasks.push(ReworkTask {
                task_id: task.id.clone(),
                goal: task.goal.clone(),
                reason: "task version is below minimum boundary; reset to >= 1".to_string(),
            });
        }

        if let Some(violation) = dependency_version_boundary_violation(task, &workflow.tasks) {
            let reason = violation.clone();
            failed_rules.push(FailedRule {
                task_id: task.id.clone(),
                kind: "version_boundary".to_string(),
                message: violation,
            });
            rework_tasks.push(ReworkTask {
                task_id: task.id.clone(),
                goal: task.goal.clone(),
                reason,
            });
        }

        for subflow in &task.child_subflows {
            let violations = child_subflow_validation_violations(subflow);
            if !violations.is_empty() {
                failed_rules.push(FailedRule {
                    task_id: task.id.clone(),
                    kind: "child_subflow_validation".to_string(),
                    message: format!(
                        "task {} child subflow {}/{} is not validation-ready: {}",
                        task.id,
                        subflow.workflow_id,
                        subflow.task_id,
                        violations.join("; ")
                    ),
                });
                rework_tasks.push(ReworkTask {
                    task_id: task.id.clone(),
                    goal: task.goal.clone(),
                    reason:
                        "child subflow binding is not validation-ready; validate or reschedule subflow before promotion"
                            .to_string(),
                });
            }
        }
    }

    let promotable = failed_rules.is_empty();
    ValidationReport {
        workflow_id: workflow.id.clone(),
        status: if promotable { "passed" } else { "blocked" }.to_string(),
        promotable,
        failed_rules,
        rework_tasks,
    }
}

fn child_subflow_validation_violations(subflow: &ChildSubflowRef) -> Vec<String> {
    let mut violations = Vec::new();
    if !matches!(subflow.binding_status.as_str(), "validated" | "reusable") {
        violations.push(format!(
            "binding status {} must be validated or reusable before promotion",
            subflow.binding_status
        ));
    }
    if !matches!(
        subflow.lifecycle_state.as_str(),
        "scaled_to_zero" | "reusable"
    ) {
        violations.push(format!(
            "lifecycle state {} is not promotion-ready",
            subflow.lifecycle_state
        ));
    }
    if subflow.validation_gate.trim().is_empty() {
        violations.push("validation gate must be explicit".to_string());
    }
    if subflow.reuse_key.trim().is_empty() {
        violations.push("reuse key must be explicit".to_string());
    }
    if subflow.context_lineage_sha256.len() != 64 {
        violations.push("context lineage hash must be content-addressed".to_string());
    }
    violations
}

fn execution_policy_violations(task: &crate::graph::AtomicTask) -> Vec<String> {
    let mut violations = Vec::new();
    let policy = &task.execution_policy;

    if matches!(task.executor, ExecutorKind::Ai) && !policy.ai_allowed {
        violations.push("AI executor with ai_allowed=false is contradictory".to_string());
    }
    if matches!(task.executor, ExecutorKind::Ai) && policy.deterministic {
        violations.push("AI executor with deterministic=true is contradictory".to_string());
    }
    if policy.mode == "local_code_node" {
        if !policy.deterministic {
            violations.push("local_code_node mode requires deterministic=true".to_string());
        }
        if policy.ai_allowed {
            violations.push("local_code_node mode requires ai_allowed=false".to_string());
        }
        if policy.code_runtime.is_none() {
            violations.push("local_code_node mode requires a code_runtime spec".to_string());
        }
    }
    if policy.mode == "executor_adapter" && !policy.deterministic && policy.ai_allowed {
        if let Some(runtime) = &policy.code_runtime {
            if !runtime.language.is_empty() {
                violations.push(
                    "AI-allowed executor_adapter should not specify a code_runtime".to_string(),
                );
            }
        }
    }
    if task.execution_policy.mode == "model_executor" {
        if policy.deterministic {
            violations
                .push("model_executor mode with deterministic=true is contradictory".to_string());
        }
        if !policy.ai_allowed {
            violations.push("model_executor mode requires ai_allowed=true".to_string());
        }
        if policy.code_runtime.is_some() {
            violations.push("model_executor mode should not specify a code_runtime".to_string());
        }
    }
    violations
}

fn persona_routing_violations(persona: &PersonaRoutingSpec) -> Vec<String> {
    let mut violations = Vec::new();
    if persona.mode.trim().is_empty() {
        violations.push("persona mode must be explicit".to_string());
    }
    if persona.scope != "node" {
        violations.push("persona routing must be node-scoped".to_string());
    }
    if persona.instruction_source != "forge_personality_soul_routing_v1" {
        violations.push("instruction source must be forge_personality_soul_routing_v1".to_string());
    }
    if persona.voice.trim().is_empty() {
        violations.push("voice must be explicit".to_string());
    }
    if persona.tone.trim().is_empty() {
        violations.push("tone must be explicit".to_string());
    }
    if persona.validation_gate != "persona_routing_required" {
        violations.push("validation gate must be persona_routing_required".to_string());
    }
    if !persona.auditable {
        violations.push("persona routing must be auditable".to_string());
    }
    if !persona
        .source_models
        .iter()
        .any(|model| model == "codex_developer_personality_instructions")
    {
        violations.push(
            "source models must include codex_developer_personality_instructions".to_string(),
        );
    }
    if !persona
        .source_models
        .iter()
        .any(|model| model == "paperclip_soul_voice_tone_persona")
    {
        violations.push("source models must include paperclip_soul_voice_tone_persona".to_string());
    }
    violations
}

fn dependency_version_boundary_violation(
    task: &AtomicTask,
    all_tasks: &[AtomicTask],
) -> Option<String> {
    for dep_id in &task.dependencies {
        if let Some(dep) = all_tasks.iter().find(|t| &t.id == dep_id) {
            if dep.version > task.version {
                return Some(format!(
                    "task {} version {} is lower than dependency {} version {}",
                    task.id, task.version, dep.id, dep.version
                ));
            }
        }
    }
    None
}

pub fn version_boundary(workflow: &Workflow) -> BTreeMap<String, u64> {
    workflow
        .tasks
        .iter()
        .map(|task| (task.id.clone(), task.version))
        .collect()
}

pub fn version_boundary_changed(
    previous: &BTreeMap<String, u64>,
    current: &BTreeMap<String, u64>,
) -> Vec<String> {
    let mut changes = Vec::new();
    for (task_id, current_version) in current {
        if let Some(prev_version) = previous.get(task_id) {
            if *prev_version != *current_version {
                changes.push(format!(
                    "task {task_id} version changed from {prev_version} to {current_version}"
                ));
            }
        } else {
            changes.push(format!(
                "task {task_id} is new at version {current_version}"
            ));
        }
    }
    for task_id in previous.keys() {
        if !current.contains_key(task_id) {
            changes.push(format!("task {task_id} was removed"));
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::*;
    use crate::intent;

    fn test_workflow() -> Workflow {
        let intent = intent::parse_intent("Test workflow");
        create_workflow(intent)
    }

    #[test]
    fn version_boundary_returns_task_id_version_map() {
        let wf = test_workflow();
        let boundary = version_boundary(&wf);
        assert_eq!(boundary.len(), wf.tasks.len());
        for task in &wf.tasks {
            assert_eq!(boundary.get(&task.id), Some(&1u64));
        }
    }

    #[test]
    fn version_boundary_changed_detects_new_versions() {
        let wf = test_workflow();
        let old = version_boundary(&wf);
        let mut updated = wf;
        if let Some(task) = updated.tasks.first_mut() {
            task.version = 2;
        }
        let current = version_boundary(&updated);
        let changes = version_boundary_changed(&old, &current);
        assert!(!changes.is_empty());
        assert!(changes[0].contains("version changed from 1 to 2"));
    }

    #[test]
    fn version_boundary_changed_detects_new_tasks() {
        let mut wf = test_workflow();
        let old = version_boundary(&wf);
        let new_id = "task-extra";
        wf.tasks.push(AtomicTask {
            id: new_id.to_string(),
            title: "extra".to_string(),
            goal: "extra goal".to_string(),
            dependencies: vec![],
            context_requirements: vec![],
            validation_rules: vec![],
            expected_output: "output".to_string(),
            executor: ExecutorKind::Command,
            human_required: false,
            schedule: None,
            loop_control: None,
            native_subflow: None,
            cost: CostEstimate {
                estimated_cost_usd: 0.0,
                cost_model: "test".to_string(),
            },
            notification: None,
            persona: None,
            work_item: WorkItemSpec {
                item_type: "execution_story".to_string(),
                backlog_state: "ready".to_string(),
                priority: "p1".to_string(),
                owner_role: "forge_runtime".to_string(),
                parent_id: None,
                subtasks: vec![],
                impediments: vec![],
                acceptance_criteria: vec![],
                goal_validation: GoalValidationSpec {
                    goal: "test".to_string(),
                    evidence_required: vec![],
                    definitively_ready: false,
                    rework_policy: "default".to_string(),
                },
            },
            async_policy: AsyncPolicy::default(),
            execution_policy: ExecutionPolicySpec::default(),
            child_subflows: vec![],
            human_interaction: None,
            status: TaskStatus::Pending,
            version: 1,
        });
        let current = version_boundary(&wf);
        let changes = version_boundary_changed(&old, &current);
        assert!(changes.iter().any(|c| c.contains("is new at version 1")));
    }

    #[test]
    fn validate_workflow_fails_on_zero_version() {
        let mut wf = test_workflow();
        if let Some(task) = wf.tasks.first_mut() {
            task.version = 0;
        }
        let report = validate_workflow(&wf);
        assert!(!report.promotable);
        assert!(report
            .failed_rules
            .iter()
            .any(|r| r.kind == "version_boundary"));
    }

    #[test]
    fn validate_workflow_fails_on_dependency_version_mismatch() {
        let intent = intent::parse_intent("Test workflow");
        let mut wf = create_workflow(intent);
        let first_id = wf.tasks[0].id.clone();
        let second_id = wf.tasks[1].id.clone();
        if let Some(task) = wf.tasks.iter_mut().find(|t| t.id == first_id) {
            task.version = 2;
        }
        if let Some(task) = wf.tasks.iter_mut().find(|t| t.id == second_id) {
            task.dependencies.push(first_id.clone());
            task.version = 1;
        }
        let report = validate_workflow(&wf);
        assert!(!report.promotable);
        assert!(report
            .failed_rules
            .iter()
            .any(|r| r.kind == "version_boundary"));
    }
}
