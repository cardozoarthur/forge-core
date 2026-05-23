use crate::graph::Workflow;
use anyhow::{bail, Result};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ContextPackage {
    pub workflow_id: String,
    pub task_id: String,
    pub context_bytes: usize,
    pub included_sections: Vec<String>,
    pub content: String,
}

pub fn build_context_package(
    workflow: &Workflow,
    task_id: &str,
    budget: usize,
) -> Result<ContextPackage> {
    let task = workflow
        .tasks
        .iter()
        .find(|candidate| candidate.id == task_id)
        .ok_or_else(|| anyhow::anyhow!("task not found: {task_id}"))?;
    if budget < 128 {
        bail!("context budget must be at least 128 bytes");
    }

    let mut sections = Vec::new();
    let mut content = String::new();
    push_section(
        &mut content,
        &mut sections,
        "local_objective",
        &format!(
            "Task {}: {}\nExpected output: {}\n",
            task.id, task.title, task.expected_output
        ),
        budget,
    );
    push_section(
        &mut content,
        &mut sections,
        "dependencies",
        &format!("Dependencies: {}\n", task.dependencies.join(", ")),
        budget,
    );
    push_section(
        &mut content,
        &mut sections,
        "validation_rules",
        &format!(
            "Validation rules: {}\n",
            serde_json::to_string(&task.validation_rules)?
        ),
        budget,
    );
    push_section(
        &mut content,
        &mut sections,
        "constraints",
        &format!("Constraints: {}\n", workflow.intent.constraints.join("; ")),
        budget,
    );

    if content.len() > budget {
        content.truncate(budget);
    }

    Ok(ContextPackage {
        workflow_id: workflow.id.clone(),
        task_id: task.id.clone(),
        context_bytes: content.len(),
        included_sections: sections,
        content,
    })
}

fn push_section(
    content: &mut String,
    sections: &mut Vec<String>,
    name: &str,
    value: &str,
    budget: usize,
) {
    if content.len() + value.len() <= budget {
        content.push_str(value);
        sections.push(name.to_string());
    }
}
