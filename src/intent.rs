use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentSpec {
    pub goal: String,
    pub constraints: Vec<String>,
    pub deliverables: Vec<String>,
    pub risks: Vec<String>,
    pub unknowns: Vec<String>,
}

pub fn parse_intent(goal: &str) -> IntentSpec {
    let normalized = goal.trim();
    let mut deliverables = vec![
        "atomic task graph".to_string(),
        "validation plan".to_string(),
        "artifact manifest".to_string(),
    ];
    let lower = normalized.to_lowercase();

    if lower.contains("api") || lower.contains("platform") {
        deliverables.push("interface contract".to_string());
    }
    if lower.contains("dashboard") || lower.contains("docs") {
        deliverables.push("documentation artifact".to_string());
    }
    if lower.contains("runtime") || lower.contains("workflow") {
        deliverables.push("persistent runtime state".to_string());
    }
    if lower.contains("n8n") {
        deliverables.push("n8n primitive research catalog".to_string());
        deliverables.push("Forge primitive promotion recommendation".to_string());
    }

    let mut risks = vec![
        "ambiguous objective can create non-atomic tasks".to_string(),
        "invalid outputs must not be promoted".to_string(),
    ];
    let mut unknowns = vec![
        "provider adapter is selected at execution time".to_string(),
        "human approval rules may vary by workflow".to_string(),
    ];

    if lower.contains("n8n") {
        risks.push("external workflow concepts must not be copied blindly or promoted without Forge validation value".to_string());
        unknowns.push(
            "current n8n source and documentation must be checked during research execution"
                .to_string(),
        );
    }

    IntentSpec {
        goal: normalized.to_string(),
        constraints: vec![
            "context-bounded execution".to_string(),
            "validation before promotion".to_string(),
            "persistent operational state".to_string(),
        ],
        deliverables,
        risks,
        unknowns,
    }
}
