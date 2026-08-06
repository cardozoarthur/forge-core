use crate::memory::{search_memory, MemorySearchOptions};
use crate::storage::FoundryStore;
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_HISTORY_SOURCES: usize = 3;
const MAX_KNOWLEDGE_SOURCES: usize = 3;
const MAX_SKILL_SOURCES: usize = 2;
const MAX_DISCOVERED_FILES: usize = 96;
const MAX_FILE_BYTES: u64 = 512 * 1024;

#[derive(Debug, Clone)]
pub struct ChatContextRetrievalOptions<'a> {
    pub query: &'a str,
    pub conversation_history: &'a [String],
    pub recent_context_count: usize,
    pub declared_skills: &'a [String],
    pub project_root: &'a Path,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatContextRetrievalReport {
    pub schema_version: String,
    pub status: String,
    pub query: String,
    pub candidate_count: usize,
    pub selected_count: usize,
    pub recent_context_count: usize,
    pub sources: Vec<ChatContextSource>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatContextSource {
    pub source_type: String,
    pub source_id: String,
    pub score: f64,
    pub snippet: String,
    pub path: Option<String>,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
}

#[derive(Debug, Clone)]
struct RetrievalCandidate {
    source_type: &'static str,
    source_id: String,
    score: f64,
    snippet: String,
    path: Option<String>,
    start_line: Option<usize>,
    end_line: Option<usize>,
}

impl ChatContextRetrievalReport {
    pub fn not_run(query: &str) -> Self {
        Self {
            schema_version: "foundry.chat_context_retrieval.v1".to_string(),
            status: "not_run".to_string(),
            query: query.to_string(),
            candidate_count: 0,
            selected_count: 0,
            recent_context_count: 0,
            sources: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

pub fn retrieve_chat_context(
    store: &FoundryStore,
    options: ChatContextRetrievalOptions<'_>,
) -> ChatContextRetrievalReport {
    let query = options.query.trim();
    if query.is_empty() {
        return ChatContextRetrievalReport::not_run(query);
    }

    let mut warnings = Vec::new();
    let history = history_candidates(query, options.conversation_history);
    let mut knowledge = memory_candidates(store, query, options.project_root, &mut warnings);
    knowledge.extend(project_knowledge_candidates(
        query,
        options.project_root,
        &mut warnings,
    ));
    let skills = skill_candidates(
        query,
        options.declared_skills,
        options.project_root,
        &mut warnings,
    );
    let candidate_count = history.len() + knowledge.len() + skills.len();

    let mut selected = Vec::new();
    selected.extend(take_best(history, MAX_HISTORY_SOURCES));
    selected.extend(take_best(knowledge, MAX_KNOWLEDGE_SOURCES));
    selected.extend(take_best(skills, MAX_SKILL_SOURCES));
    selected.sort_by(compare_candidates);
    deduplicate_candidates(&mut selected);

    let sources = selected
        .into_iter()
        .map(|candidate| ChatContextSource {
            source_type: candidate.source_type.to_string(),
            source_id: candidate.source_id,
            score: rounded_score(candidate.score),
            snippet: candidate.snippet,
            path: candidate.path,
            start_line: candidate.start_line,
            end_line: candidate.end_line,
        })
        .collect::<Vec<_>>();

    ChatContextRetrievalReport {
        schema_version: "foundry.chat_context_retrieval.v1".to_string(),
        status: "context_retrieval_complete".to_string(),
        query: query.to_string(),
        candidate_count,
        selected_count: sources.len(),
        recent_context_count: options.recent_context_count,
        sources,
        warnings,
    }
}

fn history_candidates(query: &str, history: &[String]) -> Vec<RetrievalCandidate> {
    let older_count = history.len().saturating_sub(12);
    history
        .iter()
        .take(older_count)
        .enumerate()
        .filter_map(|(index, message)| {
            let score = relevance_score(query, message);
            (score > 0.0).then(|| RetrievalCandidate {
                source_type: "conversation_history",
                source_id: format!("conversation:{index}"),
                score: score + 0.12,
                snippet: compact_snippet(message, 520),
                path: None,
                start_line: None,
                end_line: None,
            })
        })
        .collect()
}

fn memory_candidates(
    store: &FoundryStore,
    query: &str,
    project_root: &Path,
    warnings: &mut Vec<String>,
) -> Vec<RetrievalCandidate> {
    let roots = [
        project_root.join(".foundry").join("memory"),
        project_root.join("memory"),
    ];
    let mut candidates = Vec::new();
    for root in roots.into_iter().filter(|root| root.is_dir()) {
        let report = search_memory(
            store,
            MemorySearchOptions {
                query: query.to_string(),
                workflow_id: None,
                scopes: vec!["project".to_string()],
                audience: Some("private".to_string()),
                visibility: None,
                memory_level: Some("standard".to_string()),
                run_id: None,
                organization_id: None,
                limit: 6,
                global_root: None,
                organization_root: None,
                project_root: Some(root.clone()),
                processing_root: None,
            },
        );
        match report {
            Ok(report) => {
                candidates.extend(report.results.into_iter().map(|result| RetrievalCandidate {
                    source_type: "memory",
                    source_id: format!(
                        "memory:{}:{}-{}",
                        result.path, result.start_line, result.end_line
                    ),
                    score: result.score + 0.2,
                    snippet: result.snippet,
                    path: Some(result.path),
                    start_line: Some(result.start_line),
                    end_line: Some(result.end_line),
                }))
            }
            Err(error) => warnings.push(format!(
                "memory root {} could not be searched: {error}",
                root.display()
            )),
        }
    }
    candidates
}

fn project_knowledge_candidates(
    query: &str,
    project_root: &Path,
    warnings: &mut Vec<String>,
) -> Vec<RetrievalCandidate> {
    let mut files = Vec::new();
    for file in [
        project_root.join("README.md"),
        project_root.join("PROJECT.md"),
    ] {
        if file.is_file() {
            files.push(file);
        }
    }
    for directory in [project_root.join("docs"), project_root.join("artifacts")] {
        if directory.is_dir() {
            if let Err(error) = collect_markdown_files(&directory, &mut files, 0) {
                warnings.push(format!(
                    "project knowledge root {} could not be scanned: {error}",
                    directory.display()
                ));
            }
        }
    }
    files.sort();
    files.dedup();
    files.truncate(MAX_DISCOVERED_FILES);

    files
        .into_iter()
        .flat_map(|file| file_candidates(query, project_root, &file, "project_knowledge", 0.08))
        .collect()
}

fn skill_candidates(
    query: &str,
    declared_skills: &[String],
    project_root: &Path,
    warnings: &mut Vec<String>,
) -> Vec<RetrievalCandidate> {
    let mut skill_files = Vec::new();
    for root in [
        project_root.join(".agents").join("skills"),
        project_root.join("skills"),
        project_root.join(".codex").join("skills"),
    ] {
        if root.is_dir() {
            if let Err(error) = collect_named_files(&root, "SKILL.md", &mut skill_files, 0) {
                warnings.push(format!(
                    "skill root {} could not be scanned: {error}",
                    root.display()
                ));
            }
        }
    }
    skill_files.sort();
    skill_files.dedup();
    skill_files.truncate(MAX_DISCOVERED_FILES);

    let mut candidates = skill_files
        .into_iter()
        .flat_map(|file| {
            let skill_name = file
                .parent()
                .and_then(Path::file_name)
                .and_then(|value| value.to_str())
                .unwrap_or("skill");
            let declared_boost = declared_skills
                .iter()
                .map(|declared| relevance_score(declared, skill_name))
                .fold(0.0_f64, f64::max)
                * 0.55;
            file_candidates(query, project_root, &file, "skill", declared_boost)
        })
        .collect::<Vec<_>>();

    for skill in declared_skills {
        let score = relevance_score(query, skill);
        if score > 0.0 {
            candidates.push(RetrievalCandidate {
                source_type: "skill",
                source_id: format!("declared-skill:{skill}"),
                score: score + 0.18,
                snippet: format!(
                    "Skill declarada para o agente: {skill}. Use esta capacidade para interpretar e recuperar o contexto relacionado ao pedido atual."
                ),
                path: None,
                start_line: None,
                end_line: None,
            });
        }
    }
    candidates
}

fn file_candidates(
    query: &str,
    project_root: &Path,
    file: &Path,
    source_type: &'static str,
    score_boost: f64,
) -> Vec<RetrievalCandidate> {
    if fs::metadata(file)
        .map(|metadata| metadata.len() > MAX_FILE_BYTES)
        .unwrap_or(true)
    {
        return Vec::new();
    }
    let Ok(content) = fs::read_to_string(file) else {
        return Vec::new();
    };
    let relative = file.strip_prefix(project_root).unwrap_or(file);
    chunk_text(&content, 1400)
        .into_iter()
        .enumerate()
        .filter_map(|(index, (start_line, end_line, text))| {
            let score = relevance_score(query, &text) + score_boost;
            (score > score_boost).then(|| RetrievalCandidate {
                source_type,
                source_id: format!("{}#{index}", relative.display()),
                score,
                snippet: compact_snippet(&text, 620),
                path: Some(file.display().to_string()),
                start_line: Some(start_line),
                end_line: Some(end_line),
            })
        })
        .collect()
}

fn collect_markdown_files(
    path: &Path,
    files: &mut Vec<PathBuf>,
    depth: usize,
) -> std::io::Result<()> {
    if depth > 5 || files.len() >= MAX_DISCOVERED_FILES {
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        if files.len() >= MAX_DISCOVERED_FILES {
            break;
        }
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_markdown_files(&path, files, depth + 1)?;
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
        {
            files.push(path);
        }
    }
    Ok(())
}

fn collect_named_files(
    path: &Path,
    file_name: &str,
    files: &mut Vec<PathBuf>,
    depth: usize,
) -> std::io::Result<()> {
    if depth > 4 || files.len() >= MAX_DISCOVERED_FILES {
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        if files.len() >= MAX_DISCOVERED_FILES {
            break;
        }
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_named_files(&path, file_name, files, depth + 1)?;
        } else if path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case(file_name))
        {
            files.push(path);
        }
    }
    Ok(())
}

fn chunk_text(content: &str, max_chars: usize) -> Vec<(usize, usize, String)> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut start_line = 1usize;
    let mut end_line = 1usize;
    for (index, line) in content.lines().enumerate() {
        let line_number = index + 1;
        if !current.is_empty() && current.len() + line.len() + 1 > max_chars {
            chunks.push((start_line, end_line, current.trim().to_string()));
            current.clear();
            start_line = line_number;
        }
        if current.is_empty() {
            start_line = line_number;
        } else {
            current.push('\n');
        }
        current.push_str(line);
        end_line = line_number;
    }
    if !current.trim().is_empty() {
        chunks.push((start_line, end_line, current.trim().to_string()));
    }
    chunks
}

fn take_best(mut candidates: Vec<RetrievalCandidate>, limit: usize) -> Vec<RetrievalCandidate> {
    candidates.sort_by(compare_candidates);
    candidates.truncate(limit);
    candidates
}

fn compare_candidates(a: &RetrievalCandidate, b: &RetrievalCandidate) -> Ordering {
    b.score
        .partial_cmp(&a.score)
        .unwrap_or(Ordering::Equal)
        .then_with(|| a.source_id.cmp(&b.source_id))
}

fn deduplicate_candidates(candidates: &mut Vec<RetrievalCandidate>) {
    let mut seen = BTreeSet::new();
    candidates.retain(|candidate| seen.insert(candidate.snippet.to_lowercase()));
}

fn relevance_score(query: &str, text: &str) -> f64 {
    let query_terms = expanded_terms(query);
    if query_terms.is_empty() {
        return 0.0;
    }
    let text_terms = expanded_terms(text);
    let text_set = text_terms.iter().collect::<BTreeSet<_>>();
    let mut exact = 0usize;
    let mut approximate = 0usize;
    for query_term in &query_terms {
        if text_set.contains(query_term) {
            exact += 1;
        } else if text_terms
            .iter()
            .any(|text_term| terms_are_close(query_term, text_term))
        {
            approximate += 1;
        }
    }
    if exact == 0 && approximate == 0 {
        return 0.0;
    }
    let denominator = query_terms.len().max(1) as f64;
    let coverage = (exact as f64 + approximate as f64 * 0.55) / denominator;
    let phrase_bonus = if text.to_lowercase().contains(&query.trim().to_lowercase()) {
        0.32
    } else {
        0.0
    };
    rounded_score(coverage + phrase_bonus)
}

fn expanded_terms(value: &str) -> Vec<String> {
    let mut terms = tokenize(value);
    let original = terms.clone();
    for term in original {
        for alias in aliases(&term) {
            if !terms.iter().any(|existing| existing == alias) {
                terms.push((*alias).to_string());
            }
        }
    }
    terms.sort();
    terms.dedup();
    terms
}

fn tokenize(value: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();
    for ch in value.chars() {
        if ch.is_alphanumeric() {
            current.extend(ch.to_lowercase());
        } else if !current.is_empty() {
            push_term(&mut terms, &mut current);
        }
    }
    if !current.is_empty() {
        push_term(&mut terms, &mut current);
    }
    terms
}

fn push_term(terms: &mut Vec<String>, current: &mut String) {
    if current.chars().count() >= 3 && !is_stopword(current) {
        terms.push(stem(current));
    }
    current.clear();
}

fn stem(value: &str) -> String {
    for suffix in [
        "amentos", "imentos", "mente", "ções", "ção", "idades", "idade", "ando", "endo", "indo",
        "ados", "adas", "idos", "idas", "es", "s",
    ] {
        if value.chars().count() > suffix.chars().count() + 3 && value.ends_with(suffix) {
            return value.trim_end_matches(suffix).to_string();
        }
    }
    value.to_string()
}

fn terms_are_close(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    let common = left
        .chars()
        .zip(right.chars())
        .take_while(|(a, b)| a == b)
        .count();
    common >= 5 && common + 2 >= left.chars().count().min(right.chars().count())
}

fn aliases(term: &str) -> &'static [&'static str] {
    match term {
        "agente" | "agent" => &["agente", "agent", "executor"],
        "contexto" | "context" => &[
            "contexto",
            "context",
            "memoria",
            "memory",
            "retrieval",
            "rag",
        ],
        "fluxo" | "workflow" => &["fluxo", "workflow", "automacao"],
        "mensagem" | "chat" => &["mensagem", "message", "chat", "conversa"],
        "projeto" | "project" => &["projeto", "project", "workspace"],
        "repositorio" | "repository" => &["repositorio", "repository", "repo", "git"],
        "tarefa" | "task" => &["tarefa", "task", "kanban", "cartao"],
        _ => &[],
    }
}

fn is_stopword(value: &str) -> bool {
    matches!(
        value,
        "que"
            | "com"
            | "para"
            | "por"
            | "uma"
            | "uns"
            | "das"
            | "dos"
            | "the"
            | "and"
            | "with"
            | "from"
            | "this"
            | "isso"
            | "essa"
            | "esse"
            | "como"
            | "mais"
            | "não"
            | "nao"
            | "tem"
            | "ter"
            | "ser"
            | "está"
            | "esta"
    )
}

fn compact_snippet(value: &str, max_chars: usize) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= max_chars {
        return compact;
    }
    let mut truncated = compact.chars().take(max_chars).collect::<String>();
    truncated.push('…');
    truncated
}

fn rounded_score(score: f64) -> f64 {
    (score * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retrieves_old_thematic_history_outside_recent_window() {
        let temp = tempfile::tempdir().unwrap();
        let store = FoundryStore::open(temp.path().join("foundry.sqlite")).unwrap();
        let mut history = vec![
            "Arthur: A importação do GitHub falha porque o token da organização não possui escopo repo."
                .to_string(),
        ];
        history
            .extend((0..15).map(|index| format!("Mensagem recente irrelevante número {index}.")));

        let report = retrieve_chat_context(
            &store,
            ChatContextRetrievalOptions {
                query: "Por que a importação dos repositórios GitHub falhou?",
                conversation_history: &history,
                recent_context_count: 12,
                declared_skills: &[],
                project_root: temp.path(),
            },
        );

        assert!(report.sources.iter().any(|source| {
            source.source_type == "conversation_history" && source.snippet.contains("escopo repo")
        }));
        assert_eq!(report.recent_context_count, 12);
    }

    #[test]
    fn retrieves_relevant_declared_skill_content() {
        let temp = tempfile::tempdir().unwrap();
        let skill_dir = temp.path().join(".agents/skills/context-routing");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "# Context Routing\nUse memory search and semantic retrieval before routing an agent.",
        )
        .unwrap();
        let store = FoundryStore::open(temp.path().join("foundry.sqlite")).unwrap();
        let declared = vec!["context-routing".to_string()];

        let report = retrieve_chat_context(
            &store,
            ChatContextRetrievalOptions {
                query: "Recupere contexto e memória relacionados antes de escolher o agente.",
                conversation_history: &[],
                recent_context_count: 0,
                declared_skills: &declared,
                project_root: temp.path(),
            },
        );

        assert!(report.sources.iter().any(|source| {
            source.source_type == "skill" && source.snippet.contains("semantic retrieval")
        }));
    }

    #[test]
    fn retrieves_governed_project_memory_with_lineage() {
        let temp = tempfile::tempdir().unwrap();
        let memory_dir = temp.path().join(".foundry/memory");
        fs::create_dir_all(&memory_dir).unwrap();
        fs::write(
            memory_dir.join("github-import.md"),
            "---\nvisibility: private\nshareability: thread_private\n---\nA organização exige autorização SSO antes de importar repositórios privados.",
        )
        .unwrap();
        let store = FoundryStore::open(temp.path().join("foundry.sqlite")).unwrap();

        let report = retrieve_chat_context(
            &store,
            ChatContextRetrievalOptions {
                query: "Qual autorização é necessária para importar repositórios privados?",
                conversation_history: &[],
                recent_context_count: 0,
                declared_skills: &[],
                project_root: temp.path(),
            },
        );

        let source = report
            .sources
            .iter()
            .find(|source| source.source_type == "memory")
            .expect("memory source");
        assert!(source.snippet.contains("autorização SSO"));
        assert!(source.path.is_some());
        assert!(source.start_line.is_some());
    }
}
