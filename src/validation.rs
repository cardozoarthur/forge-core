use crate::graph::{ChildSubflowRef, ExecutorKind, PersonaRoutingSpec, TaskStatus, Workflow};
use serde::Serialize;

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
