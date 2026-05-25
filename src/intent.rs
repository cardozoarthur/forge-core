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
    if requires_hackathon_factory(&lower) {
        deliverables.push("hackathon regulation compliance matrix".to_string());
        deliverables.push("idea viability decision".to_string());
        deliverables.push("final idea PDF artifact".to_string());
        deliverables.push("MVP backlog and software factory plan".to_string());
        deliverables.push("pitch package".to_string());
        deliverables.push("buffered deadline improvement loop".to_string());
        deliverables.push("Telegram delivery payload".to_string());
    }
    if requires_daily_goal_research(&lower) {
        deliverables.push("durable daily Goal research schedule".to_string());
        deliverables.push("explicit Goal loop node".to_string());
        deliverables.push("per-Goal research subflow lineage".to_string());
        deliverables.push("Markdown and PDF Goal reports".to_string());
        deliverables.push("Telegram delivery record".to_string());
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
    if requires_hackathon_factory(&lower) {
        risks.push(
            "user idea may be strategically useful but off-theme unless reframed against the regulation"
                .to_string(),
        );
        risks.push(
            "deadline buffer can be insufficient if the final pitch package is left too late"
                .to_string(),
        );
        risks.push(
            "MVP complexity must not crowd out pitch quality and judging criteria".to_string(),
        );
        unknowns.push(
            "exact final regulation deadline and preferred buffer hours are supplied per run"
                .to_string(),
        );
        unknowns.push("team size, skills and available implementation time must be confirmed before build scope is locked".to_string());
    }
    if requires_daily_goal_research(&lower) {
        risks.push("recurring research must remain Forge-owned instead of becoming an ad hoc terminal loop".to_string());
        risks.push("Telegram delivery records must not expose raw secrets".to_string());
        unknowns.push(
            "live DuckDuckGo and Playwright page availability can vary per daily run".to_string(),
        );
    }

    IntentSpec {
        goal: normalized.to_string(),
        constraints: {
            let mut constraints = vec![
                "context-bounded execution".to_string(),
                "validation before promotion".to_string(),
                "persistent operational state".to_string(),
            ];
            if requires_hackathon_factory(&lower) {
                constraints.push("regulation-first feasibility gate".to_string());
                constraints
                    .push("final package deadline buffer before official submission".to_string());
                constraints.push("PDF and explanation artifact delivered to Telegram".to_string());
            }
            if requires_daily_goal_research(&lower) {
                constraints
                    .push("cron and loop semantics remain native Forge graph state".to_string());
                constraints.push("deterministic code handles stable repeated work".to_string());
                constraints.push("AI is reserved for judgment and summarization".to_string());
            }
            constraints
        },
        deliverables,
        risks,
        unknowns,
    }
}

fn requires_hackathon_factory(lower_goal: &str) -> bool {
    (lower_goal.contains("hackathon")
        || lower_goal.contains("ideathon")
        || lower_goal.contains("maratona"))
        && (lower_goal.contains("mvp")
            || lower_goal.contains("software factory")
            || lower_goal.contains("fábrica")
            || lower_goal.contains("factory"))
}

fn requires_daily_goal_research(lower_goal: &str) -> bool {
    (lower_goal.contains("daily goal research")
        || lower_goal.contains("daily goal")
        || lower_goal.contains("goal research workflow"))
        && (lower_goal.contains("goal") || lower_goal.contains("goals"))
}
